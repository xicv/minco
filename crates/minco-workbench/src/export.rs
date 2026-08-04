use minco_project_view::ProjectView;
use serde::Serialize;
use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Mermaid,
    Static,
}

#[derive(Debug, Clone, Copy)]
pub struct ExportRequest<'a> {
    pub root: &'a Path,
    pub destination: &'a Path,
    pub canonical_inputs: &'a [PathBuf],
    pub format: ExportFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportReport {
    pub schema_version: u32,
    pub status: &'static str,
    pub format: ExportFormat,
    pub destination: PathBuf,
    pub files: Vec<String>,
    pub source_digest: String,
}

#[derive(Debug, Error)]
pub enum WorkbenchError {
    #[error("workbench export root must be an explicit canonical absolute directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("workbench export destination must be a new normalized project-relative path: {0}")]
    InvalidDestination(PathBuf),
    #[error("workbench export destination overlaps canonical input {input}: {destination}")]
    CanonicalInputOverlap {
        destination: PathBuf,
        input: PathBuf,
    },
    #[error("workbench export destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("safe atomic no-clobber directory installation is unsupported on this platform")]
    SafeInstallationUnsupported,
    #[error("workbench export serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("workbench export I/O failed during {operation} at {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn export_project_view(
    view: &ProjectView,
    request: ExportRequest<'_>,
) -> Result<ExportReport, WorkbenchError> {
    validate_request(&request)?;
    let artifacts = match request.format {
        ExportFormat::Json => vec![(
            PathBuf::from("project-view.json"),
            serde_json::to_vec(view)?,
        )],
        ExportFormat::Mermaid => vec![(
            PathBuf::from("project-view.mmd"),
            crate::render_mermaid(view).into_bytes(),
        )],
        ExportFormat::Static => vec![
            (
                PathBuf::from("index.html"),
                include_bytes!("../assets/index.html").to_vec(),
            ),
            (
                PathBuf::from("project-view.json"),
                serde_json::to_vec(view)?,
            ),
            (
                PathBuf::from("project-view.mmd"),
                crate::render_mermaid(view).into_bytes(),
            ),
            (
                PathBuf::from("workbench.css"),
                include_bytes!("../assets/workbench.css").to_vec(),
            ),
            (
                PathBuf::from("workbench.js"),
                include_bytes!("../assets/workbench.js").to_vec(),
            ),
        ],
    };
    let files = artifacts
        .iter()
        .map(|(path, _)| path.display().to_string())
        .collect::<Vec<_>>();

    secure::publish(request.root, request.destination, &artifacts)?;

    Ok(ExportReport {
        schema_version: 1,
        status: "ok",
        format: request.format,
        destination: request.destination.to_path_buf(),
        files,
        source_digest: view.project.source_digest.clone(),
    })
}

fn validate_request(request: &ExportRequest<'_>) -> Result<(), WorkbenchError> {
    let canonical_root = request
        .root
        .canonicalize()
        .map_err(|source| WorkbenchError::Io {
            operation: "canonicalize root",
            path: request.root.to_path_buf(),
            source,
        })?;
    if !request.root.is_absolute() || canonical_root != request.root || !request.root.is_dir() {
        return Err(WorkbenchError::InvalidRoot(request.root.to_path_buf()));
    }
    if request.destination.as_os_str().is_empty()
        || request.destination.is_absolute()
        || !request
            .destination
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(WorkbenchError::InvalidDestination(
            request.destination.to_path_buf(),
        ));
    }
    for input in request.canonical_inputs {
        if request.destination.starts_with(input) || input.starts_with(request.destination) {
            return Err(WorkbenchError::CanonicalInputOverlap {
                destination: request.destination.to_path_buf(),
                input: input.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
mod secure {
    use super::{Component, OsStr, OsString, Path, PathBuf, WorkbenchError};
    use rustix::{
        fd::OwnedFd,
        fs::{
            AtFlags, Mode, OFlags, RenameFlags, fstat, fsync, mkdirat, open, openat, renameat_with,
            statat, unlinkat,
        },
        io::Errno,
    };
    use std::{fs::File, io::Write};
    use uuid::Uuid;

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    pub(super) fn publish(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
    ) -> Result<(), WorkbenchError> {
        let mut staging_names = || {
            Some(OsString::from(format!(
                ".minco-workbench-{}.staging",
                Uuid::new_v4().simple()
            )))
        };
        publish_inner(
            root,
            destination,
            artifacts,
            &mut staging_names,
            || {},
            None,
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn publish_with_before_install<F>(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
        before_install: F,
    ) -> Result<(), WorkbenchError>
    where
        F: FnOnce(),
    {
        let mut staging_names = || {
            Some(OsString::from(format!(
                ".minco-workbench-{}.staging",
                Uuid::new_v4().simple()
            )))
        };
        publish_inner(
            root,
            destination,
            artifacts,
            &mut staging_names,
            before_install,
            None,
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn publish_with_staging_names<I>(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
        names: I,
    ) -> Result<(), WorkbenchError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut names = names.into_iter();
        publish_inner(
            root,
            destination,
            artifacts,
            &mut || names.next(),
            || {},
            None,
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn publish_with_install_error(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
        install_error: Errno,
    ) -> Result<(), WorkbenchError> {
        let mut staging_names = || Some(OsString::from(".owned.staging"));
        publish_inner(
            root,
            destination,
            artifacts,
            &mut staging_names,
            || {},
            Some(install_error),
            None,
        )
    }

    #[cfg(test)]
    pub(super) fn publish_with_post_install_error(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
        post_install_error: Errno,
    ) -> Result<(), WorkbenchError> {
        let mut staging_names = || Some(OsString::from(".owned.staging"));
        publish_inner(
            root,
            destination,
            artifacts,
            &mut staging_names,
            || {},
            None,
            Some(post_install_error),
        )
    }

    fn publish_inner<F, N>(
        root: &Path,
        destination: &Path,
        artifacts: &[(PathBuf, Vec<u8>)],
        staging_names: &mut N,
        before_install: F,
        forced_install_error: Option<Errno>,
        forced_post_install_error: Option<Errno>,
    ) -> Result<(), WorkbenchError>
    where
        F: FnOnce(),
        N: FnMut() -> Option<OsString>,
    {
        let parent_path = destination.parent().unwrap_or_else(|| Path::new(""));
        let destination_name = destination
            .file_name()
            .ok_or_else(|| WorkbenchError::InvalidDestination(destination.to_path_buf()))?;
        let parent = open_directory_chain(root, parent_path)?;
        let parent_identity = identity(&parent, parent_path)?;
        ensure_absent(&parent, destination_name, destination)?;
        let staging_name = create_private_staging(&parent, destination, staging_names)?;
        let staging = openat(&parent, &staging_name, DIRECTORY_FLAGS, Mode::empty())
            .map_err(|source| io_error("open private staging directory", destination, source))?;
        let staging_identity = identity(&staging, destination)?;
        let mut installed = false;

        let result = (|| {
            for (relative, contents) in artifacts {
                write_artifact(&staging, relative, contents, destination)?;
            }
            fsync(&staging).map_err(|source| {
                io_error("sync private staging directory", destination, source)
            })?;
            before_install();

            let restaged =
                statat(&parent, &staging_name, AtFlags::SYMLINK_NOFOLLOW).map_err(|source| {
                    io_error("verify private staging identity", destination, source)
                })?;
            if (restaged.st_dev, restaged.st_ino) != staging_identity {
                return Err(WorkbenchError::Io {
                    operation: "verify private staging identity",
                    path: destination.to_path_buf(),
                    source: std::io::Error::other("staging directory identity changed"),
                });
            }

            let resolved_parent = open_directory_chain(root, parent_path)?;
            if identity(&resolved_parent, parent_path)? != parent_identity {
                return Err(WorkbenchError::Io {
                    operation: "verify destination parent identity",
                    path: destination.to_path_buf(),
                    source: std::io::Error::other("destination parent identity changed"),
                });
            }
            ensure_absent(&parent, destination_name, destination)?;
            let installation = forced_install_error.map_or_else(
                || {
                    renameat_with(
                        &parent,
                        &staging_name,
                        &parent,
                        destination_name,
                        RenameFlags::NOREPLACE,
                    )
                },
                Err,
            );
            match installation {
                Ok(()) => installed = true,
                Err(Errno::EXIST) => {
                    return Err(WorkbenchError::DestinationExists(destination.to_path_buf()));
                }
                Err(source)
                    if [Errno::NOSYS, Errno::NOTSUP, Errno::OPNOTSUPP].contains(&source) =>
                {
                    return Err(WorkbenchError::SafeInstallationUnsupported);
                }
                Err(source) => {
                    return Err(io_error(
                        "atomically install export without replacement",
                        destination,
                        source,
                    ));
                }
            }
            if let Some(source) = forced_post_install_error {
                return Err(io_error("sync destination parent", destination, source));
            }
            fsync(&parent)
                .map_err(|source| io_error("sync destination parent", destination, source))?;
            Ok(())
        })();

        if result.is_err() {
            for (relative, _) in artifacts.iter().rev() {
                let _ = unlinkat(&staging, relative, AtFlags::empty());
            }
            let owned_name = if installed {
                destination_name
            } else {
                staging_name.as_os_str()
            };
            let published_name_still_owned = statat(&parent, owned_name, AtFlags::SYMLINK_NOFOLLOW)
                .is_ok_and(|stat| (stat.st_dev, stat.st_ino) == staging_identity);
            drop(staging);
            if published_name_still_owned {
                let _ = unlinkat(&parent, owned_name, AtFlags::REMOVEDIR);
            }
        }
        result
    }

    fn open_directory_chain(root: &Path, relative: &Path) -> Result<OwnedFd, WorkbenchError> {
        let mut current = open(root, DIRECTORY_FLAGS, Mode::empty())
            .map_err(|source| io_error("open canonical project root", root, source))?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(WorkbenchError::InvalidDestination(relative.to_path_buf()));
            };
            current = openat(&current, name, DIRECTORY_FLAGS, Mode::empty()).map_err(|source| {
                io_error("open destination parent without symlinks", relative, source)
            })?;
        }
        Ok(current)
    }

    fn create_private_staging<N>(
        parent: &OwnedFd,
        destination: &Path,
        staging_names: &mut N,
    ) -> Result<OsString, WorkbenchError>
    where
        N: FnMut() -> Option<OsString>,
    {
        for _ in 0..32 {
            let Some(name) = staging_names() else {
                break;
            };
            match mkdirat(parent, &name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                Ok(()) => return Ok(name),
                Err(Errno::EXIST) => {}
                Err(source) => {
                    return Err(io_error(
                        "exclusively create private staging directory",
                        destination,
                        source,
                    ));
                }
            }
        }
        Err(WorkbenchError::Io {
            operation: "exclusively create private staging directory",
            path: destination.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "staging name collision limit exceeded",
            ),
        })
    }

    fn write_artifact(
        staging: &OwnedFd,
        relative: &Path,
        contents: &[u8],
        destination: &Path,
    ) -> Result<(), WorkbenchError> {
        if relative
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(WorkbenchError::InvalidDestination(relative.to_path_buf()));
        }
        let fd = openat(
            staging,
            relative,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|source| io_error("create staged artifact", destination, source))?;
        let mut file = File::from(fd);
        file.write_all(contents)
            .map_err(|source| WorkbenchError::Io {
                operation: "write staged artifact",
                path: destination.join(relative),
                source,
            })?;
        file.sync_all().map_err(|source| WorkbenchError::Io {
            operation: "sync staged artifact",
            path: destination.join(relative),
            source,
        })?;
        Ok(())
    }

    fn ensure_absent(
        parent: &OwnedFd,
        name: &OsStr,
        destination: &Path,
    ) -> Result<(), WorkbenchError> {
        match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Err(WorkbenchError::DestinationExists(destination.to_path_buf())),
            Err(Errno::NOENT) => Ok(()),
            Err(source) => Err(io_error("check export destination", destination, source)),
        }
    }

    fn identity(fd: &OwnedFd, path: &Path) -> Result<(rustix::fs::Dev, u64), WorkbenchError> {
        let stat =
            fstat(fd).map_err(|source| io_error("read filesystem identity", path, source))?;
        Ok((stat.st_dev, stat.st_ino))
    }

    fn io_error(operation: &'static str, path: &Path, source: Errno) -> WorkbenchError {
        WorkbenchError::Io {
            operation,
            path: path.to_path_buf(),
            source: source.into(),
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_vendor = "apple")))]
mod race_tests {
    use super::*;
    use std::fs;

    #[test]
    fn concurrently_created_destination_is_not_replaced_and_staging_is_removed() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        fs::create_dir(canonical_root.join("parent")).expect("destination parent");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];

        let error =
            secure::publish_with_before_install(&canonical_root, destination, &artifacts, || {
                fs::create_dir(canonical_root.join(destination)).expect("concurrent destination");
                fs::write(
                    canonical_root.join(destination).join("sentinel"),
                    "owned elsewhere",
                )
                .expect("concurrent sentinel");
            })
            .expect_err("concurrent destination must fail closed");

        assert!(matches!(error, WorkbenchError::DestinationExists(_)));
        assert_eq!(
            fs::read_to_string(canonical_root.join(destination).join("sentinel"))
                .expect("concurrent sentinel retained"),
            "owned elsewhere"
        );
        let entries = fs::read_dir(canonical_root.join("parent"))
            .expect("destination parent entries")
            .map(|entry| entry.expect("parent entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![OsString::from("workbench")]);
    }

    #[test]
    fn destination_parent_identity_swap_fails_closed_and_cleans_only_owned_staging() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        fs::create_dir(canonical_root.join("parent")).expect("destination parent");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];

        let error =
            secure::publish_with_before_install(&canonical_root, destination, &artifacts, || {
                fs::rename(
                    canonical_root.join("parent"),
                    canonical_root.join("moved-parent"),
                )
                .expect("swap original parent");
                fs::create_dir(canonical_root.join("parent")).expect("replacement parent");
            })
            .expect_err("parent identity swap must fail closed");

        assert!(
            error
                .to_string()
                .contains("destination parent identity changed")
        );
        assert!(!canonical_root.join(destination).exists());
        assert!(!canonical_root.join("moved-parent/workbench").exists());
        assert!(
            fs::read_dir(canonical_root.join("moved-parent"))
                .expect("original parent entries")
                .next()
                .is_none()
        );
    }

    #[test]
    fn preexisting_staging_entry_is_never_adopted_and_name_collision_is_retried() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        let parent = canonical_root.join("parent");
        fs::create_dir(&parent).expect("destination parent");
        let occupied = parent.join(".occupied.staging");
        fs::create_dir(&occupied).expect("preexisting staging entry");
        fs::write(occupied.join("sentinel"), "owned elsewhere")
            .expect("preexisting staging sentinel");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];

        secure::publish_with_staging_names(
            &canonical_root,
            destination,
            &artifacts,
            [
                OsString::from(".occupied.staging"),
                OsString::from(".owned.staging"),
            ],
        )
        .expect("collision should retry with an exclusively created staging name");

        assert_eq!(
            fs::read_to_string(occupied.join("sentinel")).expect("sentinel retained"),
            "owned elsewhere"
        );
        assert!(!parent.join(".owned.staging").exists());
        assert_eq!(
            fs::read_to_string(parent.join("workbench/project-view.json"))
                .expect("published artifact"),
            "{}"
        );
    }

    #[test]
    fn unsupported_no_clobber_primitive_fails_closed_and_removes_owned_staging() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        let parent = canonical_root.join("parent");
        fs::create_dir(&parent).expect("destination parent");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];

        for source in [
            rustix::io::Errno::NOSYS,
            rustix::io::Errno::NOTSUP,
            rustix::io::Errno::OPNOTSUPP,
        ] {
            let error = secure::publish_with_install_error(
                &canonical_root,
                destination,
                &artifacts,
                source,
            )
            .expect_err("unsupported no-clobber primitive must fail closed");

            assert!(matches!(error, WorkbenchError::SafeInstallationUnsupported));
            assert!(!canonical_root.join(destination).exists());
            assert!(
                fs::read_dir(&parent)
                    .expect("destination parent entries")
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn post_install_failure_removes_the_owned_destination() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        let parent = canonical_root.join("parent");
        fs::create_dir(&parent).expect("destination parent");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];

        let error = secure::publish_with_post_install_error(
            &canonical_root,
            destination,
            &artifacts,
            rustix::io::Errno::IO,
        )
        .expect_err("post-install failure must fail closed");

        assert!(error.to_string().contains("sync destination parent"));
        assert!(!canonical_root.join(destination).exists());
        assert!(
            fs::read_dir(parent)
                .expect("destination parent entries")
                .next()
                .is_none()
        );
    }

    #[test]
    fn staging_identity_swap_never_removes_the_unrelated_replacement_entry() {
        let root = tempfile::tempdir().expect("export root");
        let canonical_root = root.path().canonicalize().expect("canonical export root");
        let parent = canonical_root.join("parent");
        fs::create_dir(&parent).expect("destination parent");
        let destination = Path::new("parent/workbench");
        let artifacts = vec![(PathBuf::from("project-view.json"), b"{}".to_vec())];
        let replacement_name = std::cell::RefCell::new(None);

        let error =
            secure::publish_with_before_install(&canonical_root, destination, &artifacts, || {
                let staging_name = fs::read_dir(&parent)
                    .expect("staging entries")
                    .map(|entry| entry.expect("staging entry").file_name())
                    .find(|name| name.to_string_lossy().ends_with(".staging"))
                    .expect("created staging name");
                fs::rename(
                    parent.join(&staging_name),
                    parent.join("moved-owned-staging"),
                )
                .expect("move owned staging");
                fs::create_dir(parent.join(&staging_name)).expect("unrelated replacement staging");
                replacement_name.replace(Some(staging_name));
            })
            .expect_err("staging identity swap must fail closed");

        assert!(
            error
                .to_string()
                .contains("staging directory identity changed")
        );
        let replacement_name = replacement_name
            .into_inner()
            .expect("replacement staging name");
        assert!(
            parent.join(replacement_name).is_dir(),
            "cleanup must not remove the unrelated replacement entry"
        );
        assert!(parent.join("moved-owned-staging").is_dir());
        assert!(
            !parent
                .join("moved-owned-staging/project-view.json")
                .exists()
        );
        assert!(!canonical_root.join(destination).exists());
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
mod secure {
    use super::{Path, PathBuf, WorkbenchError};

    pub(super) fn publish(
        _root: &Path,
        _destination: &Path,
        _artifacts: &[(PathBuf, Vec<u8>)],
    ) -> Result<(), WorkbenchError> {
        Err(WorkbenchError::SafeInstallationUnsupported)
    }
}
