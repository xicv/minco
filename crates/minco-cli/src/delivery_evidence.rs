use anyhow::{Context, Result, bail};
use minco_release::FileDigest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

#[cfg(any(test, not(any(target_os = "linux", target_vendor = "apple"))))]
use std::fs;

#[cfg(test)]
type SourceWalkTestHook = Box<dyn FnMut(&Path)>;

#[cfg(test)]
thread_local! {
    static SOURCE_WALK_TEST_HOOK: std::cell::RefCell<Option<SourceWalkTestHook>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn run_source_walk_test_hook(path: &Path) {
    SOURCE_WALK_TEST_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(path);
        }
    });
}

#[cfg(not(test))]
const fn run_source_walk_test_hook(_path: &Path) {}

const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
const EXCLUDED_EXACT: &[&str] = &[
    "verification/source-manifest.json",
    "verification/adoption-measurements.json",
    "verification/1.0-candidate-load.json",
    "verification/1.0-candidate-recovery.json",
    "verification/1.0-candidate-release-gates.json",
    "verification/1.2-candidate-load.json",
    "verification/1.2-candidate-recovery.json",
    "verification/1.2-candidate-release-gates.json",
    "verification/1.2-performance-baseline.json",
    "verification/1.3-candidate-load.json",
    "verification/1.3-candidate-recovery.json",
    "verification/1.3-candidate-release-gates.json",
    "verification/1.3-performance-baseline.json",
    "verification/1.4-candidate-load.json",
    "verification/1.4-candidate-recovery.json",
    "verification/1.4-candidate-release-gates.json",
    "verification/1.4-performance-baseline.json",
    "verification/1.5-candidate-load.json",
    "verification/1.5-candidate-recovery.json",
    "verification/1.5-candidate-release-gates.json",
    "verification/1.5-performance-baseline.json",
    "verification/1.6-candidate-load.json",
    "verification/1.6-candidate-recovery.json",
    "verification/1.6-candidate-release-gates.json",
    "verification/1.6-performance-baseline.json",
    "verification/1.7-candidate-load.json",
    "verification/1.7-candidate-recovery.json",
    "verification/1.7-candidate-release-gates.json",
    "verification/1.7-performance-baseline.json",
    "verification/1.8-candidate-load.json",
    "verification/1.8-candidate-recovery.json",
    "verification/1.8-candidate-release-gates.json",
    "verification/1.8-performance-baseline.json",
    "verification/1.9-performance-baseline.json",
    "verification/1.10-candidate-load.json",
    "verification/1.10-candidate-recovery.json",
    "verification/1.10-candidate-release-gates.json",
    "verification/1.12-candidate-load.json",
    "verification/1.12-candidate-recovery.json",
    "verification/1.12-candidate-release-gates.json",
    "verification/operational-evidence-validation.json",
    "verification/static-validation.json",
    "verification/deep-review.json",
    "verification/publish-validation.json",
    "verification/quality-assurance.json",
    "verification/handover.json",
    "verification/handover.md",
];
const EXCLUDED_PREFIXES: &[&str] = &[
    "verification/feedback-task-receipts",
    "verification/handover",
    "verification/provider-evidence-receipts",
    "docs-site/.vitepress/cache",
    "docs-site/.vitepress/dist",
    "proofs/realtime-pusher/appsync-plan/generated",
];
const EXCLUDED_PARTS: &[&str] = &[
    ".git",
    ".jj",
    ".venv",
    "target",
    "node_modules",
    "__pycache__",
    ".mimosa",
];
const EXCLUDED_SUFFIXES: &[&str] = &["pyc", "zip", "db", "sqlite"];
const FORBIDDEN_SECRET_PATTERNS: &[&str] = &[
    "authorization: bearer",
    "bearer eyj",
    "database_url=",
    "postgres://",
    "postgresql://",
    "mysql://",
    "aws_secret_access_key",
    "x-amz-credential=",
    "akia",
];

#[derive(Debug, Clone)]
pub struct OutputSpec {
    pub relative: PathBuf,
    pub contents: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationResult {
    pub created: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSourceManifest {
    pub source_tree_sha256: String,
    pub file: FileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactInput {
    pub relative: PathBuf,
    pub bytes: Vec<u8>,
    pub file: FileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEntry {
    path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct SourceManifest {
    schema_version: u32,
    artifact: String,
    version: String,
    source_tree_sha256: String,
    source_tree_exclusions: Vec<String>,
    file_count: usize,
    total_size_bytes: u64,
    files: Vec<SourceEntry>,
}

pub fn sha256(contents: &[u8]) -> String {
    hex::encode(Sha256::digest(contents))
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn relative_utf8(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    relative
        .to_str()
        .map(|value| value.replace('\\', "/"))
        .context("project paths must be UTF-8")
}

pub fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        bail!("delivery evidence path must be normalized and project-relative");
    }
    Ok(())
}

fn source_exclusions() -> Vec<String> {
    let mut exclusions = EXCLUDED_EXACT
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    exclusions.extend(EXCLUDED_PREFIXES.iter().map(|path| format!("{path}/**")));
    exclusions.sort();
    exclusions
}

fn source_path_included(relative: &Path, extra_exclusions: &BTreeSet<PathBuf>) -> bool {
    if extra_exclusions.contains(relative) {
        return false;
    }
    let rendered = relative.to_string_lossy().replace('\\', "/");
    if EXCLUDED_EXACT.contains(&rendered.as_str())
        || EXCLUDED_PREFIXES
            .iter()
            .any(|prefix| rendered == *prefix || rendered.starts_with(&format!("{prefix}/")))
        || relative.components().any(|component| {
            EXCLUDED_PARTS.contains(&component.as_os_str().to_string_lossy().as_ref())
        })
        || relative.file_name().is_some_and(|name| name == ".env")
        || relative.extension().is_some_and(|extension| {
            EXCLUDED_SUFFIXES.contains(&extension.to_string_lossy().as_ref())
        })
    {
        return false;
    }
    true
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
fn collect_source_files_path(
    root: &Path,
    directory: &Path,
    extra_exclusions: &BTreeSet<PathBuf>,
    files: &mut Vec<SourceEntry>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .context("source traversal escaped project root")?;
        if !source_path_included(relative, extra_exclusions) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            bail!(
                "source authority refuses symbolic link {}",
                relative.display()
            );
        }
        run_source_walk_test_hook(relative);
        if metadata.is_dir() {
            collect_source_files_path(root, &path, extra_exclusions, files)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path)?;
            files.push(SourceEntry {
                path: relative
                    .to_str()
                    .context("source paths must be UTF-8")?
                    .replace('\\', "/"),
                sha256: sha256(&bytes),
                size_bytes: u64::try_from(bytes.len()).context("source file size overflow")?,
            });
        }
    }
    Ok(())
}

fn current_source_entries(root: &Path, extra_exclusions: &[PathBuf]) -> Result<Vec<SourceEntry>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("resolve project root {}", root.display()))?;
    let extra = extra_exclusions.iter().cloned().collect::<BTreeSet<_>>();
    for path in &extra {
        validate_relative(path)?;
    }
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    let mut files = secure::collect_source_entries(&root, &extra)?;
    #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
    let mut files = {
        let mut files = Vec::new();
        collect_source_files_path(&root, &root, &extra, &mut files)?;
        files
    };
    // Match pathlib's component-wise ordering used by scripts/source_manifest.py.
    // A flat string sort orders `minco-cli` before `minco/`; pathlib orders the
    // complete `minco` component first, so this distinction is authoritative.
    files.sort_by(|left, right| Path::new(&left.path).cmp(Path::new(&right.path)));
    Ok(files)
}

fn aggregate_source_digest(files: &[SourceEntry]) -> Result<String> {
    let values = files
        .iter()
        .map(|file| {
            serde_json::json!({
                "path": file.path,
                "sha256": file.sha256,
                "size_bytes": file.size_bytes,
            })
        })
        .collect::<Vec<_>>();
    Ok(sha256(&serde_json::to_vec(&values)?))
}

pub fn current_source_digest_excluding(
    root: &Path,
    extra_exclusions: &[PathBuf],
) -> Result<String> {
    aggregate_source_digest(&current_source_entries(root, extra_exclusions)?)
}

pub fn verify_current_source_manifest(root: &Path) -> Result<VerifiedSourceManifest> {
    let root = root.canonicalize()?;
    let manifest_input = read_exact_input(&root, Path::new("verification/source-manifest.json"))
        .context("read current source manifest")?;
    let cargo_input =
        read_exact_input(&root, Path::new("Cargo.toml")).context("read workspace manifest")?;
    let bytes = &manifest_input.bytes;
    let manifest: SourceManifest =
        serde_json::from_slice(bytes).context("parse source manifest")?;
    let files = current_source_entries(&root, &[])?;
    let digest = aggregate_source_digest(&files)?;
    let cargo: toml::Value = toml::from_str(
        std::str::from_utf8(&cargo_input.bytes).context("workspace manifest must be UTF-8")?,
    )?;
    let version = cargo
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .context("workspace package version is missing")?;
    let total_size_bytes = files.iter().map(|file| file.size_bytes).sum::<u64>();
    if manifest.schema_version != 2
        || manifest.artifact != "minco-cargo-ready-source"
        || manifest.version != version
        || manifest.source_tree_sha256 != digest
        || manifest.source_tree_exclusions != source_exclusions()
        || manifest.file_count != files.len()
        || manifest.total_size_bytes != total_size_bytes
        || manifest.files != files
    {
        bail!("current source manifest is stale or does not match canonical source authority");
    }
    verify_exact_inputs(&root, &[manifest_input.clone(), cargo_input])
        .context("source authority inputs changed during verification")?;
    Ok(VerifiedSourceManifest {
        source_tree_sha256: digest,
        file: manifest_input.file,
    })
}

pub fn reject_secret_text(label: &str, value: &str, exact_secrets: &[&str]) -> Result<()> {
    if exact_secrets
        .iter()
        .any(|secret| !secret.is_empty() && value.contains(secret))
    {
        bail!("{label} contains an operator credential");
    }
    let lower = value.to_ascii_lowercase();
    if FORBIDDEN_SECRET_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
        || (lower.contains("-----begin ") && lower.contains("private key-----"))
    {
        bail!("{label} contains secret-like material and cannot enter delivery evidence");
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
mod secure {
    use super::{
        ExactInput, MAX_OUTPUT_BYTES, OutputSpec, PublicationResult, Result, SourceEntry, bail,
        run_source_walk_test_hook, sha256, source_path_included, validate_relative,
    };
    use anyhow::Context;
    use rustix::{
        fd::OwnedFd,
        fs::{
            AtFlags, Dir, FileType, Mode, OFlags, RenameFlags, Stat, fstat, fsync, mkdirat, open,
            openat, renameat_with, statat, unlinkat,
        },
        io::Errno,
    };
    use std::{
        collections::BTreeSet,
        ffi::{OsStr, OsString},
        fs::File,
        io::{Read, Write},
        os::unix::ffi::OsStringExt,
        path::{Component, Path, PathBuf},
    };
    use uuid::Uuid;

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    const FILE_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Identity {
        device: rustix::fs::Dev,
        inode: u64,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum State {
        Existing,
        Staged,
        Created,
    }

    struct Prepared {
        index: usize,
        path: PathBuf,
        parent_path: PathBuf,
        parent: OwnedFd,
        parent_identity: Identity,
        name: OsString,
        expected_contents: Vec<u8>,
        expected_identity: Option<Identity>,
        staging_name: Option<OsString>,
        staged_identity: Option<Identity>,
        state: State,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct StableStat {
        identity: Identity,
        mode: rustix::fs::RawMode,
        size: i64,
        modified_seconds: i64,
        modified_nanoseconds: i128,
        changed_seconds: i64,
        changed_nanoseconds: i128,
    }

    pub(super) fn collect_source_entries(
        root: &Path,
        extra_exclusions: &BTreeSet<PathBuf>,
    ) -> Result<Vec<SourceEntry>> {
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())
            .with_context(|| format!("open source authority root {}", root.display()))?;
        let root_before = stable_stat(&fstat(&root_fd)?);
        let mut files = Vec::new();
        collect_source_directory(&root_fd, Path::new(""), extra_exclusions, &mut files)?;
        let root_after = stable_stat(&fstat(&root_fd)?);
        let reopened = open(root, DIRECTORY_FLAGS, Mode::empty())?;
        if root_before != root_after || root_before.identity != identity_fd(&reopened)? {
            bail!("source authority root changed during traversal");
        }
        Ok(files)
    }

    fn collect_source_directory(
        directory: &OwnedFd,
        relative_directory: &Path,
        extra_exclusions: &BTreeSet<PathBuf>,
        files: &mut Vec<SourceEntry>,
    ) -> Result<()> {
        let directory_before = stable_stat(&fstat(directory)?);
        let mut names = Vec::new();
        for entry in Dir::read_from(directory)? {
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if name != b"." && name != b".." {
                names.push(OsString::from_vec(name.to_vec()));
            }
        }
        names.sort();
        for name in names {
            let relative = relative_directory.join(&name);
            if !source_path_included(&relative, extra_exclusions) {
                continue;
            }
            let before = statat(directory, &name, AtFlags::SYMLINK_NOFOLLOW)?;
            let before = stable_stat(&before);
            if FileType::from_raw_mode(before.mode).is_symlink() {
                bail!(
                    "source authority refuses symbolic link {}",
                    relative.display()
                );
            }
            run_source_walk_test_hook(&relative);
            let flags = if FileType::from_raw_mode(before.mode).is_dir() {
                DIRECTORY_FLAGS
            } else {
                FILE_FLAGS
            };
            let child = openat(directory, &name, flags, Mode::empty())?;
            let opened = stable_stat(&fstat(&child)?);
            if opened.identity != before.identity || opened.mode != before.mode {
                bail!(
                    "source authority entry changed while opening {}",
                    relative.display()
                );
            }
            if FileType::from_raw_mode(opened.mode).is_dir() {
                collect_source_directory(&child, &relative, extra_exclusions, files)?;
            } else if FileType::from_raw_mode(opened.mode).is_file() {
                let mut bytes = Vec::new();
                File::from(child.try_clone()?).read_to_end(&mut bytes)?;
                files.push(SourceEntry {
                    path: relative
                        .to_str()
                        .context("source paths must be UTF-8")?
                        .replace('\\', "/"),
                    sha256: sha256(&bytes),
                    size_bytes: u64::try_from(bytes.len()).context("source file size overflow")?,
                });
            } else {
                bail!(
                    "source authority refuses non-file entry {}",
                    relative.display()
                );
            }
            let after = stable_stat(&fstat(&child)?);
            let linked = stable_stat(&statat(directory, &name, AtFlags::SYMLINK_NOFOLLOW)?);
            if after != opened || linked.identity != opened.identity || linked.mode != opened.mode {
                bail!(
                    "source authority entry changed during traversal {}",
                    relative.display()
                );
            }
        }
        if stable_stat(&fstat(directory)?) != directory_before {
            bail!(
                "source authority directory changed during traversal {}",
                relative_directory.display()
            );
        }
        Ok(())
    }

    fn stable_stat(stat: &Stat) -> StableStat {
        StableStat {
            identity: Identity {
                device: stat.st_dev,
                inode: stat.st_ino,
            },
            mode: stat.st_mode,
            size: stat.st_size,
            modified_seconds: stat.st_mtime,
            modified_nanoseconds: stat.st_mtime_nsec.into(),
            changed_seconds: stat.st_ctime,
            changed_nanoseconds: stat.st_ctime_nsec.into(),
        }
    }

    pub(super) fn inspect_exact(root: &Path, relative: &Path, expected: &[u8]) -> Result<bool> {
        validate_relative(relative)?;
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())?;
        let Some((parent, name)) = open_existing_parent(&root_fd, relative)? else {
            return Ok(false);
        };
        match read_regular(&parent, name, relative)? {
            None => Ok(false),
            Some((contents, _)) if contents == expected => Ok(true),
            Some(_) => bail!(
                "refusing to overwrite conflicting file {}",
                relative.display()
            ),
        }
    }

    pub(super) fn read_input(root: &Path, relative: &Path) -> Result<Vec<u8>> {
        validate_relative(relative)?;
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())?;
        let (parent, name) = open_existing_parent(&root_fd, relative)?
            .context("delivery evidence input parent is missing")?;
        let (contents, identity) =
            read_regular(&parent, name, relative)?.context("delivery evidence input is missing")?;
        verify_identity(&parent, name, identity, relative)?;
        Ok(contents)
    }

    pub(super) fn publish(root: &Path, outputs: Vec<OutputSpec>) -> Result<PublicationResult> {
        publish_inner(root, &[], outputs, || Ok(()), |_| {})
    }

    pub(super) fn publish_guarded_checked<F>(
        root: &Path,
        inputs: &[ExactInput],
        outputs: Vec<OutputSpec>,
        check: F,
    ) -> Result<PublicationResult>
    where
        F: FnMut() -> Result<()>,
    {
        publish_inner(root, inputs, outputs, check, |_| {})
    }

    #[cfg(test)]
    pub(super) fn publish_with_hook<F>(
        root: &Path,
        outputs: Vec<OutputSpec>,
        hook: F,
    ) -> Result<PublicationResult>
    where
        F: FnMut(usize),
    {
        publish_inner(root, &[], outputs, || Ok(()), hook)
    }

    #[cfg(test)]
    pub(super) fn publish_guarded_with_hook<F>(
        root: &Path,
        inputs: &[ExactInput],
        outputs: Vec<OutputSpec>,
        hook: F,
    ) -> Result<PublicationResult>
    where
        F: FnMut(usize),
    {
        publish_inner(root, inputs, outputs, || Ok(()), hook)
    }

    #[cfg(test)]
    pub(super) fn publish_guarded_checked_with_hook<F, H>(
        root: &Path,
        inputs: &[ExactInput],
        outputs: Vec<OutputSpec>,
        check: F,
        hook: H,
    ) -> Result<PublicationResult>
    where
        F: FnMut() -> Result<()>,
        H: FnMut(usize),
    {
        publish_inner(root, inputs, outputs, check, hook)
    }

    fn publish_inner<F, H>(
        root: &Path,
        inputs: &[ExactInput],
        outputs: Vec<OutputSpec>,
        mut check: F,
        mut hook: H,
    ) -> Result<PublicationResult>
    where
        F: FnMut() -> Result<()>,
        H: FnMut(usize),
    {
        let root_fd = open(root, DIRECTORY_FLAGS, Mode::empty())
            .with_context(|| format!("open canonical project root {}", root.display()))?;
        let root_identity = identity_fd(&root_fd)?;
        check()?;
        let mut paths = std::collections::BTreeSet::new();
        let mut prepared = Vec::with_capacity(outputs.len());
        for (index, output) in outputs.into_iter().enumerate() {
            validate_relative(&output.relative)?;
            if output.contents.len() as u64 > MAX_OUTPUT_BYTES {
                cleanup(&mut prepared);
                bail!("delivery evidence output exceeds the managed size limit");
            }
            if !paths.insert(output.relative.clone()) {
                cleanup(&mut prepared);
                bail!("delivery evidence output paths must be unique");
            }
            match prepare(&root_fd, index, output) {
                Ok(entry) => prepared.push(entry),
                Err(error) => {
                    cleanup(&mut prepared);
                    return Err(error);
                }
            }
        }
        for index in 0..prepared.len() {
            hook(index);
            if let Err(error) = verify_inputs(&root_fd, inputs) {
                let rollback_errors = rollback(&mut prepared);
                cleanup(&mut prepared);
                if rollback_errors.is_empty() {
                    return Err(error);
                }
                bail!(
                    "{error}; rollback incomplete: {}",
                    rollback_errors.join("; ")
                );
            }
            if let Err(error) = install_one(root, &root_fd, root_identity, &mut prepared[index]) {
                let rollback_errors = rollback(&mut prepared);
                cleanup(&mut prepared);
                if rollback_errors.is_empty() {
                    return Err(error);
                }
                bail!(
                    "{error}; rollback incomplete: {}",
                    rollback_errors.join("; ")
                );
            }
        }
        hook(prepared.len());
        if let Err(error) = check().and_then(|()| verify_inputs(&root_fd, inputs)) {
            let rollback_errors = rollback(&mut prepared);
            cleanup(&mut prepared);
            if rollback_errors.is_empty() {
                return Err(error);
            }
            bail!(
                "{error}; rollback incomplete: {}",
                rollback_errors.join("; ")
            );
        }
        let mut created = vec![false; prepared.len()];
        for entry in &prepared {
            created[entry.index] = entry.state == State::Created;
        }
        cleanup(&mut prepared);
        Ok(PublicationResult { created })
    }

    fn verify_inputs(root: &OwnedFd, inputs: &[ExactInput]) -> Result<()> {
        for input in inputs {
            validate_relative(&input.relative)?;
            let (parent, name) = open_existing_parent(root, &input.relative)?
                .context("delivery evidence guarded input parent is missing")?;
            let (contents, identity) = read_regular(&parent, name, &input.relative)?
                .context("delivery evidence guarded input is missing")?;
            verify_identity(&parent, name, identity, &input.relative)?;
            let rendered = input
                .relative
                .to_str()
                .context("delivery evidence guarded input path must be UTF-8")?
                .replace('\\', "/");
            if contents != input.bytes
                || input.file.path != rendered
                || input.file.sha256 != sha256(&contents)
                || input.file.bytes != contents.len() as u64
            {
                bail!(
                    "delivery evidence guarded input {} changed during publication",
                    input.relative.display()
                );
            }
        }
        Ok(())
    }

    fn prepare(root: &OwnedFd, index: usize, output: OutputSpec) -> Result<Prepared> {
        let (parent, name) = open_or_create_parent(root, &output.relative)?;
        let name = name.to_os_string();
        let parent_identity = identity_fd(&parent)?;
        let parent_path = output
            .relative
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        if let Some((contents, identity)) = read_regular(&parent, &name, &output.relative)? {
            if contents != output.contents {
                bail!(
                    "refusing to overwrite conflicting file {}",
                    output.relative.display()
                );
            }
            return Ok(Prepared {
                index,
                path: output.relative,
                parent_path,
                parent,
                parent_identity,
                name,
                expected_contents: output.contents,
                expected_identity: Some(identity),
                staging_name: None,
                staged_identity: None,
                state: State::Existing,
            });
        }
        let staging_name = OsString::from(format!(
            ".minco-delivery-{}.staging",
            Uuid::new_v4().simple()
        ));
        let fd = openat(
            &parent,
            &staging_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::ROTH,
        )?;
        let created = identity_fd(&fd)?;
        let write_result = (|| -> Result<()> {
            let mut file = File::from(fd);
            file.write_all(&output.contents)?;
            file.sync_all()?;
            verify_identity(&parent, &staging_name, created, &output.relative)
        })();
        if let Err(error) = write_result {
            if identity_at(&parent, &staging_name) == Some(created) {
                let _ = unlinkat(&parent, &staging_name, AtFlags::empty());
            }
            return Err(error);
        }
        Ok(Prepared {
            index,
            path: output.relative,
            parent_path,
            parent,
            parent_identity,
            name,
            expected_contents: output.contents,
            expected_identity: None,
            staging_name: Some(staging_name),
            staged_identity: Some(created),
            state: State::Staged,
        })
    }

    fn install_one(
        root_path: &Path,
        root: &OwnedFd,
        root_identity: Identity,
        entry: &mut Prepared,
    ) -> Result<()> {
        let reopened_root = open(root_path, DIRECTORY_FLAGS, Mode::empty())?;
        if identity_fd(&reopened_root)? != root_identity {
            bail!("delivery evidence project root changed identity during publication");
        }
        let reopened_parent = open_parent(root, &entry.parent_path)?;
        if identity_fd(&reopened_parent)? != entry.parent_identity {
            bail!(
                "delivery evidence parent {} changed identity during publication",
                entry.parent_path.display()
            );
        }
        match entry.state {
            State::Existing => {
                let Some((contents, identity)) =
                    read_regular(&entry.parent, &entry.name, &entry.path)?
                else {
                    bail!(
                        "delivery evidence {} changed after planning",
                        entry.path.display()
                    );
                };
                if Some(identity) != entry.expected_identity || contents != entry.expected_contents
                {
                    bail!(
                        "delivery evidence {} changed after planning",
                        entry.path.display()
                    );
                }
            }
            State::Staged => {
                if statat(&entry.parent, &entry.name, AtFlags::SYMLINK_NOFOLLOW).is_ok() {
                    bail!(
                        "delivery evidence {} appeared after planning",
                        entry.path.display()
                    );
                }
                let staging = entry.staging_name.as_ref().expect("staged name");
                let identity = entry.staged_identity.expect("staged identity");
                verify_identity(&entry.parent, staging, identity, &entry.path)?;
                renameat_with(
                    &entry.parent,
                    staging,
                    &entry.parent,
                    &entry.name,
                    RenameFlags::NOREPLACE,
                )
                .map_err(|error| anyhow::anyhow!(error))?;
                entry.state = State::Created;
                verify_identity(&entry.parent, &entry.name, identity, &entry.path)?;
                fsync(&entry.parent)?;
            }
            State::Created => unreachable!("outputs install once"),
        }
        Ok(())
    }

    fn rollback(entries: &mut [Prepared]) -> Vec<String> {
        let mut errors = Vec::new();
        for entry in entries.iter_mut().rev() {
            if entry.state != State::Created {
                continue;
            }
            let identity = entry.staged_identity.expect("created identity");
            if identity_at(&entry.parent, &entry.name) != Some(identity) {
                errors.push(format!("{} changed identity", entry.path.display()));
                continue;
            }
            if let Err(error) = unlinkat(&entry.parent, &entry.name, AtFlags::empty()) {
                errors.push(format!("{}: {error}", entry.path.display()));
                continue;
            }
            entry.state = State::Staged;
            let _ = fsync(&entry.parent);
        }
        errors
    }

    fn cleanup(entries: &mut [Prepared]) {
        for entry in entries {
            let Some(name) = entry.staging_name.as_ref() else {
                continue;
            };
            let Some(identity) = entry.staged_identity else {
                continue;
            };
            if identity_at(&entry.parent, name) == Some(identity) {
                let _ = unlinkat(&entry.parent, name, AtFlags::empty());
                let _ = fsync(&entry.parent);
            }
        }
    }

    fn open_or_create_parent<'a>(
        root: &OwnedFd,
        relative: &'a Path,
    ) -> Result<(OwnedFd, &'a OsStr)> {
        let mut current = root.try_clone()?;
        for component in relative
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .components()
        {
            let Component::Normal(name) = component else {
                bail!("delivery evidence path must be normalized and project-relative");
            };
            match mkdirat(
                &current,
                name,
                Mode::RUSR
                    | Mode::WUSR
                    | Mode::XUSR
                    | Mode::RGRP
                    | Mode::XGRP
                    | Mode::ROTH
                    | Mode::XOTH,
            ) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            current =
                openat(&current, name, DIRECTORY_FLAGS, Mode::empty()).with_context(|| {
                    format!(
                        "open delivery evidence parent {}",
                        Path::new(name).display()
                    )
                })?;
        }
        Ok((
            current,
            relative.file_name().expect("validated path has a name"),
        ))
    }

    fn open_existing_parent<'a>(
        root: &OwnedFd,
        relative: &'a Path,
    ) -> Result<Option<(OwnedFd, &'a OsStr)>> {
        let mut current = root.try_clone()?;
        for component in relative
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .components()
        {
            let Component::Normal(name) = component else {
                bail!("delivery evidence path must be normalized and project-relative");
            };
            match openat(&current, name, DIRECTORY_FLAGS, Mode::empty()) {
                Ok(next) => current = next,
                Err(Errno::NOENT) => return Ok(None),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Some((
            current,
            relative.file_name().expect("validated path has a name"),
        )))
    }

    fn open_parent(root: &OwnedFd, relative: &Path) -> Result<OwnedFd> {
        let mut current = root.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                bail!("delivery evidence path must be normalized and project-relative");
            };
            current = openat(&current, name, DIRECTORY_FLAGS, Mode::empty())?;
        }
        Ok(current)
    }

    fn read_regular(
        parent: &OwnedFd,
        name: &OsStr,
        path: &Path,
    ) -> Result<Option<(Vec<u8>, Identity)>> {
        let stat = match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(Errno::NOENT) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !FileType::from_raw_mode(stat.st_mode).is_file() {
            bail!(
                "delivery evidence output is a symlink or non-regular file: {}",
                path.display()
            );
        }
        if u64::try_from(stat.st_size)
            .ok()
            .is_none_or(|size| size > MAX_OUTPUT_BYTES)
        {
            bail!(
                "delivery evidence output exceeds the managed size limit: {}",
                path.display()
            );
        }
        let fd = openat(parent, name, FILE_FLAGS, Mode::empty())?;
        let identity = identity_fd(&fd)?;
        if identity
            != (Identity {
                device: stat.st_dev,
                inode: stat.st_ino,
            })
        {
            bail!(
                "delivery evidence output changed identity while opening: {}",
                path.display()
            );
        }
        let mut contents = Vec::new();
        File::from(fd)
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_end(&mut contents)?;
        if contents.len() as u64 > MAX_OUTPUT_BYTES {
            bail!(
                "delivery evidence output changed size while reading: {}",
                path.display()
            );
        }
        Ok(Some((contents, identity)))
    }

    fn verify_identity(
        parent: &OwnedFd,
        name: &OsStr,
        expected: Identity,
        path: &Path,
    ) -> Result<()> {
        if identity_at(parent, name) != Some(expected) {
            bail!("delivery evidence {} changed identity", path.display());
        }
        Ok(())
    }

    fn identity_at(parent: &OwnedFd, name: &OsStr) -> Option<Identity> {
        let stat = statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
        Some(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
        })
    }

    fn identity_fd(fd: &OwnedFd) -> Result<Identity> {
        let stat = fstat(fd)?;
        Ok(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
        })
    }
}

#[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
mod secure {
    use super::{ExactInput, OutputSpec, PublicationResult, Result};
    use anyhow::bail;
    use std::path::Path;

    pub(super) fn inspect_exact(_root: &Path, _relative: &Path, _expected: &[u8]) -> Result<bool> {
        bail!("secure delivery evidence publication is unsupported on this platform")
    }

    pub(super) fn publish_guarded_checked<F>(
        _root: &Path,
        _inputs: &[ExactInput],
        _outputs: Vec<OutputSpec>,
        _check: F,
    ) -> Result<PublicationResult>
    where
        F: FnMut() -> Result<()>,
    {
        bail!("secure delivery evidence publication is unsupported on this platform")
    }

    pub(super) fn read_input(_root: &Path, _relative: &Path) -> Result<Vec<u8>> {
        bail!("secure delivery evidence input reads are unsupported on this platform")
    }

    pub(super) fn publish(_root: &Path, _outputs: Vec<OutputSpec>) -> Result<PublicationResult> {
        bail!("secure delivery evidence publication is unsupported on this platform")
    }
}

pub fn inspect_exact(root: &Path, relative: &Path, expected: &[u8]) -> Result<bool> {
    secure::inspect_exact(root, relative, expected)
}

pub fn read_exact_input(root: &Path, relative: &Path) -> Result<ExactInput> {
    validate_relative(relative)?;
    let bytes = secure::read_input(root, relative)?;
    Ok(ExactInput {
        relative: relative.to_path_buf(),
        file: FileDigest {
            path: relative_utf8(Path::new(""), relative)?,
            sha256: sha256(&bytes),
            bytes: u64::try_from(bytes.len()).context("delivery input size overflow")?,
        },
        bytes,
    })
}

pub fn verify_exact_inputs(root: &Path, inputs: &[ExactInput]) -> Result<()> {
    for input in inputs {
        let current = read_exact_input(root, &input.relative)?;
        if current.bytes != input.bytes || current.file != input.file {
            bail!(
                "delivery evidence input {} changed after planning",
                input.relative.display()
            );
        }
    }
    Ok(())
}

pub fn publish_create_only(root: &Path, outputs: Vec<OutputSpec>) -> Result<PublicationResult> {
    secure::publish(root, outputs)
}

pub fn publish_create_only_guarded_checked<F>(
    root: &Path,
    inputs: &[ExactInput],
    outputs: Vec<OutputSpec>,
    check: F,
) -> Result<PublicationResult>
where
    F: FnMut() -> Result<()>,
{
    secure::publish_guarded_checked(root, inputs, outputs, check)
}

#[cfg(all(test, any(target_os = "linux", target_vendor = "apple")))]
pub fn publish_create_only_with_hook<F>(
    root: &Path,
    outputs: Vec<OutputSpec>,
    hook: F,
) -> Result<PublicationResult>
where
    F: FnMut(usize),
{
    secure::publish_with_hook(root, outputs, hook)
}

#[cfg(all(test, any(target_os = "linux", target_vendor = "apple")))]
pub fn publish_create_only_guarded_with_hook<F>(
    root: &Path,
    inputs: &[ExactInput],
    outputs: Vec<OutputSpec>,
    hook: F,
) -> Result<PublicationResult>
where
    F: FnMut(usize),
{
    secure::publish_guarded_with_hook(root, inputs, outputs, hook)
}

#[cfg(all(test, any(target_os = "linux", target_vendor = "apple")))]
pub fn publish_create_only_guarded_checked_with_hook<F, H>(
    root: &Path,
    inputs: &[ExactInput],
    outputs: Vec<OutputSpec>,
    check: F,
    hook: H,
) -> Result<PublicationResult>
where
    F: FnMut() -> Result<()>,
    H: FnMut(usize),
{
    secure::publish_guarded_checked_with_hook(root, inputs, outputs, check, hook)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_exclusions_match_the_canonical_policy() {
        assert_eq!(source_exclusions().len(), 53);
        assert!(!source_path_included(
            Path::new("verification/1.3-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.4-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.5-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.6-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.7-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.8-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.10-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/1.12-candidate-release-gates.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/handover.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/static-validation.json"),
            &BTreeSet::new()
        ));
        assert!(!source_path_included(
            Path::new("verification/quality-assurance.json"),
            &BTreeSet::new()
        ));
        assert!(source_path_included(
            Path::new("verification/performance-policy.toml"),
            &BTreeSet::new()
        ));
        assert!(source_path_included(
            Path::new("scripts/validate_static.py"),
            &BTreeSet::new()
        ));
    }

    #[test]
    fn rust_source_authority_matches_the_generated_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let bytes = fs::read(root.join("verification/source-manifest.json")).unwrap();
        let manifest: SourceManifest = serde_json::from_slice(&bytes).unwrap();
        let files = current_source_entries(&root, &[]).unwrap();
        assert_eq!(manifest.source_tree_exclusions, source_exclusions());
        assert_eq!(manifest.file_count, files.len());
        let expected_paths = manifest
            .files
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<BTreeSet<_>>();
        let actual_paths = files
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            expected_paths.difference(&actual_paths).collect::<Vec<_>>(),
            actual_paths.difference(&expected_paths).collect::<Vec<_>>(),
            "Python and Rust source traversal selected different paths"
        );
        for (index, (expected, actual)) in manifest.files.iter().zip(&files).enumerate() {
            assert_eq!(
                expected, actual,
                "source authority differs at index {index}"
            );
        }
        assert_eq!(
            manifest.source_tree_sha256,
            aggregate_source_digest(&files).unwrap()
        );
    }

    #[test]
    fn verified_source_manifest_rejects_any_stale_included_file() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("verification")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\n\n[workspace.package]\nversion = \"1.2.0\"\n",
        )
        .unwrap();
        fs::write(root.join("source.txt"), "current\n").unwrap();
        let files = current_source_entries(root, &[]).unwrap();
        let digest = aggregate_source_digest(&files).unwrap();
        let manifest = serde_json::json!({
            "schema_version": 2,
            "artifact": "minco-cargo-ready-source",
            "version": "1.2.0",
            "source_tree_sha256": digest,
            "source_tree_exclusions": source_exclusions(),
            "file_count": files.len(),
            "total_size_bytes": files.iter().map(|file| file.size_bytes).sum::<u64>(),
            "files": files,
        });
        fs::write(
            root.join("verification/source-manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(verify_current_source_manifest(root).is_ok());

        fs::write(root.join("source.txt"), "stale\n").unwrap();
        assert!(verify_current_source_manifest(root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn verified_source_manifest_rejects_a_symlinked_manifest() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("verification")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\n\n[workspace.package]\nversion = \"1.2.0\"\n",
        )
        .unwrap();
        fs::write(root.join("source.txt"), "current\n").unwrap();
        let files = current_source_entries(root, &[]).unwrap();
        let manifest = serde_json::json!({
            "schema_version": 2,
            "artifact": "minco-cargo-ready-source",
            "version": "1.2.0",
            "source_tree_sha256": aggregate_source_digest(&files).unwrap(),
            "source_tree_exclusions": source_exclusions(),
            "file_count": files.len(),
            "total_size_bytes": files.iter().map(|file| file.size_bytes).sum::<u64>(),
            "files": files,
        });
        let outside_manifest = outside.path().join("source-manifest.json");
        fs::write(
            outside_manifest.as_path(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        symlink(
            outside_manifest,
            root.join("verification/source-manifest.json"),
        )
        .unwrap();

        assert!(verify_current_source_manifest(root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn source_authority_rejects_a_file_replaced_after_entry_inspection() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::write(root.join("source.txt"), b"trusted\n").unwrap();
        fs::write(outside.path().join("replacement.txt"), b"trusted\n").unwrap();

        SOURCE_WALK_TEST_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new({
                let root = root.to_path_buf();
                let replacement = outside.path().join("replacement.txt");
                move |relative| {
                    if relative == Path::new("source.txt") {
                        fs::rename(root.join("source.txt"), root.join("source.original")).unwrap();
                        symlink(&replacement, root.join("source.txt")).unwrap();
                    }
                }
            }));
        });
        let result = current_source_entries(root, &[]);
        SOURCE_WALK_TEST_HOOK.with(|hook| *hook.borrow_mut() = None);

        assert!(
            result.is_err(),
            "source authority followed a replacement symlink"
        );
    }

    #[test]
    fn secret_scanner_rejects_operator_and_credential_shaped_values() {
        assert!(reject_secret_text("test", "safe evidence", &["operator-secret"]).is_ok());
        assert!(reject_secret_text("test", "operator-secret", &["operator-secret"]).is_err());
        assert!(reject_secret_text("test", "postgres://user:pass@host/db", &[]).is_err());
        assert!(reject_secret_text("test", "Authorization: Bearer eyJtoken", &[]).is_err());
        let private_key_marker = format!("-----begin {}-----", "private key");
        assert!(reject_secret_text("test", &private_key_marker, &[]).is_err());
    }

    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    #[test]
    fn second_install_failure_rolls_back_the_first_created_output() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let outputs = vec![
            OutputSpec {
                relative: PathBuf::from("tasks/M14/task.md"),
                contents: b"task\n".to_vec(),
            },
            OutputSpec {
                relative: PathBuf::from("verification/receipt.json"),
                contents: b"receipt\n".to_vec(),
            },
        ];
        let result = publish_create_only_with_hook(root, outputs, |index| {
            if index == 1 {
                fs::write(root.join("verification/receipt.json"), b"conflict\n").unwrap();
            }
        });
        assert!(result.is_err());
        assert!(!root.join("tasks/M14/task.md").exists());
        assert_eq!(
            fs::read(root.join("verification/receipt.json")).unwrap(),
            b"conflict\n"
        );
    }

    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    #[test]
    fn post_install_input_race_rolls_back_both_created_outputs() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("verification")).unwrap();
        fs::write(root.join("verification/input.json"), b"exact\n").unwrap();
        let input = read_exact_input(root, Path::new("verification/input.json")).unwrap();
        let outputs = vec![
            OutputSpec {
                relative: PathBuf::from("tasks/M14/task.md"),
                contents: b"task\n".to_vec(),
            },
            OutputSpec {
                relative: PathBuf::from("verification/receipt.json"),
                contents: b"receipt\n".to_vec(),
            },
        ];
        let result = publish_create_only_guarded_with_hook(root, &[input], outputs, |index| {
            if index == 2 {
                fs::write(root.join("verification/input.json"), b"changed\n").unwrap();
            }
        });
        assert!(result.is_err());
        assert!(!root.join("tasks/M14/task.md").exists());
        assert!(!root.join("verification/receipt.json").exists());
    }

    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    #[test]
    fn post_install_source_authority_race_rolls_back_both_outputs() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("verification")).unwrap();
        fs::write(root.join("source.txt"), b"exact\n").unwrap();
        let outputs = vec![
            OutputSpec {
                relative: PathBuf::from("tasks/M14/task.md"),
                contents: b"task\n".to_vec(),
            },
            OutputSpec {
                relative: PathBuf::from("verification/receipt.json"),
                contents: b"receipt\n".to_vec(),
            },
        ];
        let result = publish_create_only_guarded_checked_with_hook(
            root,
            &[],
            outputs,
            || {
                if fs::read(root.join("source.txt"))? != b"exact\n" {
                    bail!("source authority changed during publication");
                }
                Ok(())
            },
            |index| {
                if index == 2 {
                    fs::write(root.join("source.txt"), b"changed\n").unwrap();
                }
            },
        );
        assert!(result.is_err());
        assert!(!root.join("tasks/M14/task.md").exists());
        assert!(!root.join("verification/receipt.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn parent_replacement_cannot_escape_and_rolls_back_an_installed_peer() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let outputs = vec![
            OutputSpec {
                relative: PathBuf::from("tasks/M14/task.md"),
                contents: b"task\n".to_vec(),
            },
            OutputSpec {
                relative: PathBuf::from("verification/receipt.json"),
                contents: b"receipt\n".to_vec(),
            },
        ];
        let result = publish_create_only_with_hook(root, outputs, |index| {
            if index == 1 {
                fs::rename(
                    root.join("verification"),
                    root.join("verification-original"),
                )
                .unwrap();
                symlink(outside.path(), root.join("verification")).unwrap();
            }
        });
        assert!(result.is_err());
        assert!(!root.join("tasks/M14/task.md").exists());
        assert!(!outside.path().join("receipt.json").exists());
    }
}
