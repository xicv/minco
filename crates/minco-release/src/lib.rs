//! Immutable release manifests and artifact verification.
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDigest {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

impl FileDigest {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ReleaseError> {
        let path = path.as_ref();
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut bytes = 0_u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            bytes += read as u64;
        }
        Ok(Self {
            path: path.display().to_string(),
            sha256: format!("{:x}", hasher.finalize()),
            bytes,
        })
    }

    pub fn verify(&self) -> Result<(), ReleaseError> {
        let actual = Self::from_path(&self.path)?;
        if actual.sha256 != self.sha256 || actual.bytes != self.bytes {
            return Err(ReleaseError::DigestMismatch {
                path: self.path.clone(),
                expected: self.sha256.clone(),
                actual: actual.sha256,
            });
        }
        Ok(())
    }

    pub fn from_rooted_path(
        root: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<Self, ReleaseError> {
        let root = root.as_ref().canonicalize()?;
        let path = path.as_ref().canonicalize()?;
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| ReleaseError::PathOutsideRoot(path.display().to_string()))?;
        let mut digest = Self::from_path(&path)?;
        digest.path = relative
            .iter()
            .map(|component| {
                component
                    .to_str()
                    .ok_or_else(|| ReleaseError::NonUtf8Path(path.display().to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        Ok(digest)
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), ReleaseError> {
        let root = root.as_ref().canonicalize()?;
        let path = Path::new(&self.path);
        if path.is_absolute()
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(ReleaseError::InvalidManifestPath(self.path.clone()));
        }
        let path = root.join(path).canonicalize()?;
        path.strip_prefix(&root)
            .map_err(|_| ReleaseError::PathOutsideRoot(path.display().to_string()))?;
        let actual = Self::from_path(path)?;
        if actual.sha256 != self.sha256 || actual.bytes != self.bytes {
            return Err(ReleaseError::DigestMismatch {
                path: self.path.clone(),
                expected: self.sha256.clone(),
                actual: actual.sha256,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub release_id: String,
    pub created_at: DateTime<Utc>,
    pub source_change: String,
    pub rust_version: String,
    pub minco_version: String,
    pub artifact: FileDigest,
    pub contract: FileDigest,
    pub migration_set: Vec<FileDigest>,
    pub cargo_lock: Option<FileDigest>,
    pub deployment_plan: FileDigest,
    pub deployment_template: FileDigest,
}

impl ReleaseManifest {
    pub fn verify(&self) -> Result<(), ReleaseError> {
        self.verify_at(".")
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), ReleaseError> {
        if self.schema_version != 2 {
            return Err(ReleaseError::UnsupportedSchemaVersion(self.schema_version));
        }
        let root = root.as_ref();
        self.artifact.verify_at(root)?;
        self.contract.verify_at(root)?;
        self.deployment_plan.verify_at(root)?;
        self.deployment_template.verify_at(root)?;
        if let Some(lock) = &self.cargo_lock {
            lock.verify_at(root)?;
        }
        for migration in &self.migration_set {
            migration.verify_at(root)?;
        }
        Ok(())
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ReleaseError> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn read_json(path: impl AsRef<Path>) -> Result<Self, ReleaseError> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}

#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("release JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("release input is outside the repository root: {0}")]
    PathOutsideRoot(String),
    #[error("release manifest path must be a normalized repository-relative UTF-8 path: {0}")]
    InvalidManifestPath(String),
    #[error("release input path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
    #[error("unsupported release manifest schema version {0}; expected 2")]
    UnsupportedSchemaVersion(u32),
    #[error("digest mismatch for {path}: expected {expected}, found {actual}")]
    DigestMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_file_changes() {
        let directory =
            std::env::temp_dir().join(format!("minco-release-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("artifact");
        std::fs::write(&path, b"one").unwrap();
        let digest = FileDigest::from_rooted_path(&directory, &path).unwrap();
        std::fs::write(&path, b"two").unwrap();
        assert!(matches!(
            digest.verify_at(&directory),
            Err(ReleaseError::DigestMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn rooted_digests_are_portable_and_verify_from_the_repository_root() {
        let directory =
            std::env::temp_dir().join(format!("minco-release-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(directory.join("target")).unwrap();
        let path = directory.join("target/artifact.zip");
        std::fs::write(&path, b"artifact").unwrap();

        let digest = FileDigest::from_rooted_path(&directory, &path).unwrap();
        assert_eq!(digest.path, "target/artifact.zip");
        digest.verify_at(&directory).unwrap();

        let outside = std::env::temp_dir().join(format!("minco-outside-{}", uuid::Uuid::new_v4()));
        std::fs::write(&outside, b"outside").unwrap();
        assert!(matches!(
            FileDigest::from_rooted_path(&directory, &outside),
            Err(ReleaseError::PathOutsideRoot(_))
        ));
        let outside_digest = FileDigest::from_path(&outside).unwrap();
        assert!(matches!(
            outside_digest.verify_at(&directory),
            Err(ReleaseError::InvalidManifestPath(_))
        ));
        let parent_digest = FileDigest {
            path: "../artifact.zip".into(),
            sha256: digest.sha256.clone(),
            bytes: digest.bytes,
        };
        assert!(matches!(
            parent_digest.verify_at(&directory),
            Err(ReleaseError::InvalidManifestPath(_))
        ));

        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(directory);
    }
}
