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
}

impl ReleaseManifest {
    pub fn verify(&self) -> Result<(), ReleaseError> {
        self.artifact.verify()?;
        self.contract.verify()?;
        self.deployment_plan.verify()?;
        if let Some(lock) = &self.cargo_lock {
            lock.verify()?;
        }
        for migration in &self.migration_set {
            migration.verify()?;
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
        let digest = FileDigest::from_path(&path).unwrap();
        std::fs::write(&path, b"two").unwrap();
        assert!(matches!(
            digest.verify(),
            Err(ReleaseError::DigestMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(directory);
    }
}
