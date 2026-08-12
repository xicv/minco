//! Immutable release manifests and artifact verification.
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
            sha256: hex::encode(hasher.finalize()),
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
        if !is_normalized_relative_path(path) {
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
pub struct ReleaseEnvironment {
    pub application: String,
    pub environment: String,
    pub region: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainIdentity {
    pub rustc: String,
    pub cargo_minco: String,
    pub artifact_builder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionArtifact {
    pub function_id: String,
    pub file: FileDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseSourceDigests {
    pub migration_catalog: String,
    pub seed_catalog: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseManifestInput {
    pub source_change: String,
    pub environment: ReleaseEnvironment,
    pub toolchain: ToolchainIdentity,
    pub artifacts: Vec<FunctionArtifact>,
    pub contract: FileDigest,
    pub configuration_digest: String,
    pub database_sources: DatabaseSourceDigests,
    pub cargo_lock: Option<FileDigest>,
    pub deployment_plan: FileDigest,
    pub deployment_template: FileDigest,
    pub attestations: Vec<FileDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub release_id: String,
    pub release_digest: String,
    pub source_change: String,
    pub environment: ReleaseEnvironment,
    pub toolchain: ToolchainIdentity,
    pub artifacts: Vec<FunctionArtifact>,
    pub contract: FileDigest,
    pub configuration_digest: String,
    pub database_sources: DatabaseSourceDigests,
    pub cargo_lock: Option<FileDigest>,
    pub deployment_plan: FileDigest,
    pub deployment_template: FileDigest,
    pub attestations: Vec<FileDigest>,
}

impl ReleaseManifest {
    pub fn seal(mut input: ReleaseManifestInput) -> Result<Self, ReleaseError> {
        input
            .artifacts
            .sort_by(|left, right| left.function_id.cmp(&right.function_id));
        input
            .attestations
            .sort_by(|left, right| left.path.cmp(&right.path));
        validate_release_input(&input)?;
        let mut manifest = Self {
            schema_version: 3,
            release_id: String::new(),
            release_digest: String::new(),
            source_change: input.source_change,
            environment: input.environment,
            toolchain: input.toolchain,
            artifacts: input.artifacts,
            contract: input.contract,
            configuration_digest: input.configuration_digest,
            database_sources: input.database_sources,
            cargo_lock: input.cargo_lock,
            deployment_plan: input.deployment_plan,
            deployment_template: input.deployment_template,
            attestations: input.attestations,
        };
        manifest.release_digest = manifest.calculate_release_digest()?;
        manifest.release_id = format!("minco.{}", &manifest.release_digest[..24]);
        Ok(manifest)
    }

    pub fn verify(&self) -> Result<(), ReleaseError> {
        self.verify_at(".")
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), ReleaseError> {
        self.verify_structure()?;
        let root = root.as_ref();
        for artifact in &self.artifacts {
            artifact.file.verify_at(root)?;
        }
        self.contract.verify_at(root)?;
        self.deployment_plan.verify_at(root)?;
        self.deployment_template.verify_at(root)?;
        if let Some(lock) = &self.cargo_lock {
            lock.verify_at(root)?;
        }
        for attestation in &self.attestations {
            attestation.verify_at(root)?;
        }
        Ok(())
    }

    fn verify_structure(&self) -> Result<(), ReleaseError> {
        if self.schema_version != 3 {
            return Err(ReleaseError::UnsupportedSchemaVersion(self.schema_version));
        }
        validate_release_manifest(self)?;
        let actual_digest = self.calculate_release_digest()?;
        if actual_digest != self.release_digest {
            return Err(ReleaseError::ReleaseDigestMismatch {
                expected: self.release_digest.clone(),
                actual: actual_digest,
            });
        }
        let expected_id = format!("minco.{}", &self.release_digest[..24]);
        if self.release_id != expected_id {
            return Err(ReleaseError::InvalidRelease(format!(
                "release ID {} does not match digest-derived ID {expected_id}",
                self.release_id
            )));
        }
        Ok(())
    }

    fn calculate_release_digest(&self) -> Result<String, ReleaseError> {
        #[derive(Serialize)]
        struct DigestPayload<'a> {
            schema_version: u32,
            source_change: &'a str,
            environment: &'a ReleaseEnvironment,
            toolchain: &'a ToolchainIdentity,
            artifacts: &'a [FunctionArtifact],
            contract: &'a FileDigest,
            configuration_digest: &'a str,
            database_sources: &'a DatabaseSourceDigests,
            cargo_lock: &'a Option<FileDigest>,
            deployment_plan: &'a FileDigest,
            deployment_template: &'a FileDigest,
            attestations: &'a [FileDigest],
        }
        let payload = DigestPayload {
            schema_version: self.schema_version,
            source_change: &self.source_change,
            environment: &self.environment,
            toolchain: &self.toolchain,
            artifacts: &self.artifacts,
            contract: &self.contract,
            configuration_digest: &self.configuration_digest,
            database_sources: &self.database_sources,
            cargo_lock: &self.cargo_lock,
            deployment_plan: &self.deployment_plan,
            deployment_template: &self.deployment_template,
            attestations: &self.attestations,
        };
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(&payload)?)))
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ReleaseError> {
        self.verify_structure()?;
        let path = path.as_ref();
        let mut rendered = serde_json::to_vec_pretty(self)?;
        rendered.push(b'\n');
        if path.exists() {
            if std::fs::read(path)? == rendered {
                return Ok(());
            }
            return Err(ReleaseError::ReleaseManifestConflict(
                path.display().to_string(),
            ));
        }
        write_new_file(path, &rendered)
    }

    pub fn read_json(path: impl AsRef<Path>) -> Result<Self, ReleaseError> {
        let source = std::fs::read(path)?;
        let value: serde_json::Value = serde_json::from_slice(&source)?;
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| {
                ReleaseError::InvalidRelease(
                    "release manifest has no supported integer schema_version".into(),
                )
            })?;
        if schema_version != 3 {
            return Err(ReleaseError::UnsupportedSchemaVersion(schema_version));
        }
        Ok(serde_json::from_value(value)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabasePlanKind {
    Migration,
    Seed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DatabasePlanBinding {
    pub kind: DatabasePlanKind,
    pub schema_version: u32,
    pub catalog_digest: String,
    pub plan_digest: String,
    pub file: FileDigest,
    pub selected_set: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentReceiptInput {
    pub attempt_id: String,
    pub release_manifest: FileDigest,
    pub release_id: String,
    pub release_digest: String,
    pub environment: ReleaseEnvironment,
    pub configuration_digest: String,
    pub database_plans: Vec<DatabasePlanBinding>,
    pub attestations: Vec<FileDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidence {
    pub kind: String,
    pub file: FileDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentOutcome {
    Started,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentReceipt {
    pub schema_version: u32,
    pub receipt_digest: String,
    pub attempt_id: String,
    pub release_manifest: FileDigest,
    pub release_id: String,
    pub release_digest: String,
    pub environment: ReleaseEnvironment,
    pub configuration_digest: String,
    pub database_plans: Vec<DatabasePlanBinding>,
    pub attestations: Vec<FileDigest>,
    outcome: DeploymentOutcome,
    failure_code: Option<String>,
    verification: Vec<VerificationEvidence>,
}

impl DeploymentReceipt {
    pub fn start(mut input: DeploymentReceiptInput) -> Result<Self, ReleaseError> {
        input.database_plans.sort();
        input
            .attestations
            .sort_by(|left, right| left.path.cmp(&right.path));
        validate_identifier(&input.attempt_id, "deployment attempt ID")?;
        validate_sha256(&input.release_manifest.sha256, "release manifest digest")?;
        validate_sha256(&input.release_digest, "release digest")?;
        validate_sha256(&input.configuration_digest, "configuration digest")?;
        for plan in &input.database_plans {
            if plan.schema_version == 0 {
                return Err(ReleaseError::InvalidDeploymentReceipt(
                    "database plan schema version must be greater than zero".into(),
                ));
            }
            validate_sha256(&plan.catalog_digest, "database catalog digest")?;
            validate_sha256(&plan.plan_digest, "database plan digest")?;
        }
        let mut receipt = Self {
            schema_version: 1,
            receipt_digest: String::new(),
            attempt_id: input.attempt_id,
            release_manifest: input.release_manifest,
            release_id: input.release_id,
            release_digest: input.release_digest,
            environment: input.environment,
            configuration_digest: input.configuration_digest,
            database_plans: input.database_plans,
            attestations: input.attestations,
            outcome: DeploymentOutcome::Started,
            failure_code: None,
            verification: Vec::new(),
        };
        receipt.refresh_digest()?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub const fn outcome(&self) -> DeploymentOutcome {
        self.outcome
    }

    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }

    pub fn verification(&self) -> &[VerificationEvidence] {
        &self.verification
    }

    pub fn fail(&mut self, failure_code: impl Into<String>) -> Result<(), ReleaseError> {
        self.require_started()?;
        let failure_code = failure_code.into();
        validate_identifier(&failure_code, "deployment failure code")?;
        self.outcome = DeploymentOutcome::Failed;
        self.failure_code = Some(failure_code);
        self.refresh_digest()?;
        self.verify_structure()
    }

    pub fn succeed(
        &mut self,
        mut verification: Vec<VerificationEvidence>,
    ) -> Result<(), ReleaseError> {
        self.require_started()?;
        if verification.is_empty() {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "successful deployment requires verification evidence".into(),
            ));
        }
        verification.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.file.path.cmp(&right.file.path))
        });
        for evidence in &verification {
            validate_identifier(&evidence.kind, "verification evidence kind")?;
            validate_sha256(&evidence.file.sha256, "verification evidence digest")?;
        }
        self.outcome = DeploymentOutcome::Succeeded;
        self.verification = verification;
        self.refresh_digest()?;
        self.verify_structure()
    }

    pub fn verify(&self) -> Result<(), ReleaseError> {
        self.verify_at(".")
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), ReleaseError> {
        self.verify_structure()?;
        let root = root.as_ref();
        self.release_manifest.verify_at(root)?;
        let release = ReleaseManifest::read_json(root.join(&self.release_manifest.path))?;
        release.verify_at(root)?;
        if release.release_id != self.release_id
            || release.release_digest != self.release_digest
            || release.environment != self.environment
            || release.configuration_digest != self.configuration_digest
        {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "deployment binding does not match the verified release manifest".into(),
            ));
        }
        for plan in &self.database_plans {
            plan.file.verify_at(root)?;
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.join(&plan.file.path))?)?;
            let schema_version = value
                .get("schema_version")
                .and_then(serde_json::Value::as_u64);
            let catalog_digest = value
                .get("catalog_digest")
                .and_then(serde_json::Value::as_str);
            let plan_digest = value.get("digest").and_then(serde_json::Value::as_str);
            if schema_version != Some(u64::from(plan.schema_version))
                || catalog_digest != Some(plan.catalog_digest.as_str())
                || plan_digest != Some(plan.plan_digest.as_str())
            {
                return Err(ReleaseError::InvalidDeploymentReceipt(format!(
                    "{:?} plan binding does not match {}",
                    plan.kind, plan.file.path
                )));
            }
        }
        for attestation in &self.attestations {
            attestation.verify_at(root)?;
        }
        for evidence in &self.verification {
            evidence.file.verify_at(root)?;
        }
        Ok(())
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ReleaseError> {
        self.verify_structure()?;
        let path = path.as_ref();
        let _transition_lock = lock_deployment_receipt(path)?;
        let mut rendered = serde_json::to_vec_pretty(self)?;
        rendered.push(b'\n');
        if !path.exists() {
            return write_new_file(path, &rendered);
        }
        let existing = Self::read_json(path)?;
        if existing == *self {
            return Ok(());
        }
        if existing.attempt_id != self.attempt_id || !existing.same_binding(self) {
            return Err(ReleaseError::DeploymentReceiptConflict(
                self.attempt_id.clone(),
            ));
        }
        if existing.outcome != DeploymentOutcome::Started
            || self.outcome == DeploymentOutcome::Started
        {
            return Err(ReleaseError::TerminalDeploymentReceipt {
                attempt_id: existing.attempt_id,
                outcome: existing.outcome,
            });
        }
        replace_file(path, &rendered, &self.receipt_digest)
    }

    pub fn read_json(path: impl AsRef<Path>) -> Result<Self, ReleaseError> {
        let receipt: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    fn require_started(&self) -> Result<(), ReleaseError> {
        if self.outcome != DeploymentOutcome::Started {
            return Err(ReleaseError::TerminalDeploymentReceipt {
                attempt_id: self.attempt_id.clone(),
                outcome: self.outcome,
            });
        }
        Ok(())
    }

    fn same_binding(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.attempt_id == other.attempt_id
            && self.release_manifest == other.release_manifest
            && self.release_id == other.release_id
            && self.release_digest == other.release_digest
            && self.environment == other.environment
            && self.configuration_digest == other.configuration_digest
            && self.database_plans == other.database_plans
            && self.attestations == other.attestations
    }

    fn refresh_digest(&mut self) -> Result<(), ReleaseError> {
        self.receipt_digest = self.calculate_receipt_digest()?;
        Ok(())
    }

    fn verify_structure(&self) -> Result<(), ReleaseError> {
        if self.schema_version != 1 {
            return Err(ReleaseError::UnsupportedDeploymentReceiptSchema(
                self.schema_version,
            ));
        }
        validate_identifier(&self.attempt_id, "deployment attempt ID")?;
        validate_sha256(&self.receipt_digest, "deployment receipt digest")?;
        validate_sha256(&self.release_manifest.sha256, "release manifest digest")?;
        validate_sha256(&self.release_digest, "release digest")?;
        validate_sha256(&self.configuration_digest, "configuration digest")?;
        validate_file_digest(&self.release_manifest, "release manifest")?;
        let expected_release_id = format!("minco.{}", &self.release_digest[..24]);
        if self.release_id != expected_release_id {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "release ID does not match its digest".into(),
            ));
        }
        for plan in &self.database_plans {
            if plan.schema_version == 0 {
                return Err(ReleaseError::InvalidDeploymentReceipt(
                    "database plan schema version must be greater than zero".into(),
                ));
            }
            validate_sha256(&plan.catalog_digest, "database catalog digest")?;
            validate_sha256(&plan.plan_digest, "database plan digest")?;
            validate_file_digest(&plan.file, "database plan")?;
            if let Some(selected_set) = &plan.selected_set {
                validate_identifier(selected_set, "database plan selected set")?;
            }
            if let Some(environment) = &plan.environment {
                validate_identifier(environment, "database plan environment")?;
            }
        }
        for attestation in &self.attestations {
            validate_file_digest(attestation, "attestation")?;
        }
        for evidence in &self.verification {
            validate_identifier(&evidence.kind, "verification evidence kind")?;
            validate_file_digest(&evidence.file, "verification evidence")?;
        }
        if self
            .database_plans
            .windows(2)
            .any(|plans| plans[0] == plans[1])
        {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "deployment receipt repeats a database plan binding".into(),
            ));
        }
        if self
            .attestations
            .windows(2)
            .any(|attestations| attestations[0].path == attestations[1].path)
        {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "deployment receipt repeats an attestation path".into(),
            ));
        }
        let state_is_valid = match self.outcome {
            DeploymentOutcome::Started => {
                self.failure_code.is_none() && self.verification.is_empty()
            }
            DeploymentOutcome::Failed => {
                self.failure_code.is_some() && self.verification.is_empty()
            }
            DeploymentOutcome::Succeeded => {
                self.failure_code.is_none() && !self.verification.is_empty()
            }
        };
        if !state_is_valid {
            return Err(ReleaseError::InvalidDeploymentReceipt(
                "deployment outcome, failure, and verification fields are inconsistent".into(),
            ));
        }
        if let Some(failure_code) = &self.failure_code {
            validate_identifier(failure_code, "deployment failure code")?;
        }
        let actual = self.calculate_receipt_digest()?;
        if actual != self.receipt_digest {
            return Err(ReleaseError::DeploymentReceiptDigestMismatch {
                expected: self.receipt_digest.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn calculate_receipt_digest(&self) -> Result<String, ReleaseError> {
        #[derive(Serialize)]
        struct DigestPayload<'a> {
            schema_version: u32,
            attempt_id: &'a str,
            release_manifest: &'a FileDigest,
            release_id: &'a str,
            release_digest: &'a str,
            environment: &'a ReleaseEnvironment,
            configuration_digest: &'a str,
            database_plans: &'a [DatabasePlanBinding],
            attestations: &'a [FileDigest],
            outcome: DeploymentOutcome,
            failure_code: &'a Option<String>,
            verification: &'a [VerificationEvidence],
        }
        let payload = DigestPayload {
            schema_version: self.schema_version,
            attempt_id: &self.attempt_id,
            release_manifest: &self.release_manifest,
            release_id: &self.release_id,
            release_digest: &self.release_digest,
            environment: &self.environment,
            configuration_digest: &self.configuration_digest,
            database_plans: &self.database_plans,
            attestations: &self.attestations,
            outcome: self.outcome,
            failure_code: &self.failure_code,
            verification: &self.verification,
        };
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(&payload)?)))
    }
}

fn lock_deployment_receipt(path: &Path) -> Result<File, ReleaseError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ReleaseError::NonUtf8Path(path.display().to_string()))?;
    let lock_path = parent.join(format!(".{name}.lock"));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock.lock()?;
    Ok(lock)
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), ReleaseError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

fn replace_file(path: &Path, contents: &[u8], digest: &str) -> Result<(), ReleaseError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ReleaseError::NonUtf8Path(path.display().to_string()))?;
    let temporary = parent.join(format!(".{name}.{}.tmp", &digest[..16]));
    write_new_file(&temporary, contents)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn validate_release_input(input: &ReleaseManifestInput) -> Result<(), ReleaseError> {
    if !matches!(input.source_change.len(), 40 | 64)
        || !input
            .source_change
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ReleaseError::InvalidRelease(
            "source change must be a lowercase 40- or 64-character VCS commit ID".into(),
        ));
    }
    for (value, label) in [
        (input.environment.application.as_str(), "application"),
        (input.environment.environment.as_str(), "environment"),
        (input.environment.region.as_str(), "region"),
        (input.toolchain.rustc.as_str(), "rustc toolchain"),
        (
            input.toolchain.cargo_minco.as_str(),
            "cargo-minco toolchain",
        ),
    ] {
        validate_release_text(value, label)?;
    }
    if let Some(builder) = &input.toolchain.artifact_builder {
        validate_release_text(builder, "artifact builder toolchain")?;
    }
    if input.artifacts.is_empty() {
        return Err(ReleaseError::InvalidRelease(
            "release must bind at least one function artifact".into(),
        ));
    }
    let function_ids = input
        .artifacts
        .iter()
        .map(|artifact| artifact.function_id.as_str())
        .collect::<BTreeSet<_>>();
    if function_ids.len() != input.artifacts.len() {
        return Err(ReleaseError::InvalidRelease(
            "release repeats a function artifact ID".into(),
        ));
    }
    for artifact in &input.artifacts {
        validate_release_text(&artifact.function_id, "function artifact ID")?;
        validate_file_digest(&artifact.file, "function artifact")?;
    }
    validate_file_digest(&input.contract, "contract")?;
    validate_file_digest(&input.deployment_plan, "deployment plan")?;
    validate_file_digest(&input.deployment_template, "deployment template")?;
    if let Some(lock) = &input.cargo_lock {
        validate_file_digest(lock, "Cargo.lock")?;
    }
    for attestation in &input.attestations {
        validate_file_digest(attestation, "attestation")?;
    }
    if input
        .attestations
        .windows(2)
        .any(|attestations| attestations[0].path == attestations[1].path)
    {
        return Err(ReleaseError::InvalidRelease(
            "release repeats an attestation path".into(),
        ));
    }
    validate_sha256(&input.configuration_digest, "configuration digest")?;
    validate_sha256(
        &input.database_sources.migration_catalog,
        "migration catalog digest",
    )?;
    validate_sha256(&input.database_sources.seed_catalog, "seed catalog digest")?;
    Ok(())
}

fn validate_release_text(value: &str, label: &str) -> Result<(), ReleaseError> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|character| character.is_control() || character == '\u{7f}')
    {
        return Err(ReleaseError::InvalidRelease(format!(
            "{label} must contain 1 to 256 non-control characters"
        )));
    }
    Ok(())
}

fn validate_file_digest(file: &FileDigest, label: &str) -> Result<(), ReleaseError> {
    if !is_normalized_relative_path(Path::new(&file.path)) {
        return Err(ReleaseError::InvalidManifestPath(file.path.clone()));
    }
    validate_sha256(&file.sha256, &format!("{label} digest"))
}

fn is_normalized_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn validate_release_manifest(manifest: &ReleaseManifest) -> Result<(), ReleaseError> {
    validate_release_input(&ReleaseManifestInput {
        source_change: manifest.source_change.clone(),
        environment: manifest.environment.clone(),
        toolchain: manifest.toolchain.clone(),
        artifacts: manifest.artifacts.clone(),
        contract: manifest.contract.clone(),
        configuration_digest: manifest.configuration_digest.clone(),
        database_sources: manifest.database_sources.clone(),
        cargo_lock: manifest.cargo_lock.clone(),
        deployment_plan: manifest.deployment_plan.clone(),
        deployment_template: manifest.deployment_template.clone(),
        attestations: manifest.attestations.clone(),
    })?;
    validate_sha256(&manifest.release_digest, "release digest")
}

fn validate_sha256(value: &str, label: &str) -> Result<(), ReleaseError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ReleaseError::InvalidRelease(format!(
            "{label} must be a lowercase SHA-256 hex digest"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(ReleaseError::InvalidRelease(format!(
            "{label} must be a lowercase SHA-256 hex digest"
        )));
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str) -> Result<(), ReleaseError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ReleaseError::InvalidDeploymentReceipt(format!(
            "{label} must contain 1 to 128 ASCII letters, digits, dots, dashes, or underscores"
        )));
    }
    Ok(())
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
    #[error("unsupported release manifest schema version {0}; expected 3")]
    UnsupportedSchemaVersion(u32),
    #[error("invalid release manifest: {0}")]
    InvalidRelease(String),
    #[error("refusing to replace a different release manifest at {0}")]
    ReleaseManifestConflict(String),
    #[error("invalid deployment receipt: {0}")]
    InvalidDeploymentReceipt(String),
    #[error("unsupported deployment receipt schema version {0}; expected 1")]
    UnsupportedDeploymentReceiptSchema(u32),
    #[error("deployment receipt {0} conflicts with an existing attempt binding")]
    DeploymentReceiptConflict(String),
    #[error("deployment receipt digest mismatch: expected {expected}, found {actual}")]
    DeploymentReceiptDigestMismatch { expected: String, actual: String },
    #[error("deployment attempt {attempt_id} is already terminal with outcome {outcome:?}")]
    TerminalDeploymentReceipt {
        attempt_id: String,
        outcome: DeploymentOutcome,
    },
    #[error("release digest mismatch: expected {expected}, found {actual}")]
    ReleaseDigestMismatch { expected: String, actual: String },
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
    fn sealed_release_is_deterministic_and_detects_manifest_tampering() {
        let directory =
            std::env::temp_dir().join(format!("minco-release-seal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(directory.join("target")).unwrap();
        std::fs::create_dir_all(directory.join("openapi")).unwrap();
        std::fs::create_dir_all(directory.join("infra")).unwrap();
        for (path, contents) in [
            ("target/api.zip", b"artifact".as_slice()),
            ("openapi/openapi.yaml", b"openapi: 3.1.0".as_slice()),
            ("infra/plan.json", b"{}".as_slice()),
            ("infra/template.yaml", b"Resources: {}".as_slice()),
        ] {
            std::fs::write(directory.join(path), contents).unwrap();
        }

        let input = ReleaseManifestInput {
            source_change: "a".repeat(40),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "staging".into(),
                region: "ap-southeast-2".into(),
            },
            toolchain: ToolchainIdentity {
                rustc: "rustc 1.97.1".into(),
                cargo_minco: "0.3.1".into(),
                artifact_builder: Some("cargo-lambda 1.9.1".into()),
            },
            artifacts: vec![FunctionArtifact {
                function_id: "api".into(),
                file: FileDigest::from_rooted_path(&directory, directory.join("target/api.zip"))
                    .unwrap(),
            }],
            contract: FileDigest::from_rooted_path(
                &directory,
                directory.join("openapi/openapi.yaml"),
            )
            .unwrap(),
            configuration_digest: "b".repeat(64),
            database_sources: DatabaseSourceDigests {
                migration_catalog: "c".repeat(64),
                seed_catalog: "d".repeat(64),
            },
            cargo_lock: None,
            deployment_plan: FileDigest::from_rooted_path(
                &directory,
                directory.join("infra/plan.json"),
            )
            .unwrap(),
            deployment_template: FileDigest::from_rooted_path(
                &directory,
                directory.join("infra/template.yaml"),
            )
            .unwrap(),
            attestations: Vec::new(),
        };

        let first = ReleaseManifest::seal(input.clone()).unwrap();
        let second = ReleaseManifest::seal(input.clone()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.schema_version, 3);
        assert!(first.release_id.starts_with("minco."));
        first.verify_at(&directory).unwrap();

        let mut tampered = first.clone();
        tampered.configuration_digest = "e".repeat(64);
        assert!(matches!(
            tampered.verify_at(&directory),
            Err(ReleaseError::ReleaseDigestMismatch { .. })
        ));

        let manifest_path = directory.join("release.json");
        first.write_json(&manifest_path).unwrap();
        let mut replacement_input = input;
        replacement_input.configuration_digest = "f".repeat(64);
        let replacement = ReleaseManifest::seal(replacement_input).unwrap();
        assert!(matches!(
            replacement.write_json(&manifest_path),
            Err(ReleaseError::ReleaseManifestConflict(_))
        ));
        assert_eq!(ReleaseManifest::read_json(&manifest_path).unwrap(), first);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn failed_deployment_receipt_is_terminal() {
        let receipt = FileDigest {
            path: "target/minco/release.json".into(),
            sha256: "a".repeat(64),
            bytes: 512,
        };
        let mut deployment = DeploymentReceipt::start(DeploymentReceiptInput {
            attempt_id: "attempt-001".into(),
            release_manifest: receipt,
            release_id: format!("minco.{}", "b".repeat(24)),
            release_digest: "b".repeat(64),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "staging".into(),
                region: "ap-southeast-2".into(),
            },
            configuration_digest: "c".repeat(64),
            database_plans: vec![DatabasePlanBinding {
                kind: DatabasePlanKind::Migration,
                schema_version: 1,
                catalog_digest: "d".repeat(64),
                plan_digest: "e".repeat(64),
                file: FileDigest {
                    path: "target/minco/migration-plan.json".into(),
                    sha256: "e".repeat(64),
                    bytes: 256,
                },
                selected_set: Some("orders-postgres".into()),
                environment: None,
            }],
            attestations: Vec::new(),
        })
        .unwrap();

        deployment.fail("cloudformation_apply_failed").unwrap();
        assert_eq!(deployment.outcome(), DeploymentOutcome::Failed);
        assert!(deployment.succeed(Vec::new()).is_err());
        assert_eq!(deployment.outcome(), DeploymentOutcome::Failed);
        assert_eq!(
            deployment.failure_code(),
            Some("cloudformation_apply_failed")
        );
    }

    #[test]
    fn recorded_failed_deployment_cannot_be_replaced_with_success() {
        let directory =
            std::env::temp_dir().join(format!("minco-deployment-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("attempt-001.json");
        let input = DeploymentReceiptInput {
            attempt_id: "attempt-001".into(),
            release_manifest: FileDigest {
                path: "target/minco/release.json".into(),
                sha256: "a".repeat(64),
                bytes: 512,
            },
            release_id: format!("minco.{}", "b".repeat(24)),
            release_digest: "b".repeat(64),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "staging".into(),
                region: "ap-southeast-2".into(),
            },
            configuration_digest: "c".repeat(64),
            database_plans: vec![DatabasePlanBinding {
                kind: DatabasePlanKind::Migration,
                schema_version: 1,
                catalog_digest: "d".repeat(64),
                plan_digest: "e".repeat(64),
                file: FileDigest {
                    path: "target/minco/migration-plan.json".into(),
                    sha256: "e".repeat(64),
                    bytes: 256,
                },
                selected_set: Some("orders-postgres".into()),
                environment: None,
            }],
            attestations: Vec::new(),
        };
        let mut failed = DeploymentReceipt::start(input.clone()).unwrap();
        failed.write_json(&path).unwrap();
        failed.fail("cloudformation_apply_failed").unwrap();
        failed.write_json(&path).unwrap();

        let mut forged_success = DeploymentReceipt::start(input).unwrap();
        forged_success
            .succeed(vec![VerificationEvidence {
                kind: "http_smoke".into(),
                file: FileDigest {
                    path: "target/minco/http-smoke.json".into(),
                    sha256: "f".repeat(64),
                    bytes: 128,
                },
            }])
            .unwrap();
        assert!(matches!(
            forged_success.write_json(&path),
            Err(ReleaseError::TerminalDeploymentReceipt { .. })
        ));
        assert_eq!(
            DeploymentReceipt::read_json(&path).unwrap().outcome(),
            DeploymentOutcome::Failed
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn deployment_receipt_terminal_transition_is_serialized_across_writers() {
        let directory = std::env::temp_dir().join(format!(
            "minco-deployment-serialized-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("attempt-001.json");
        let lock_path = directory.join(".attempt-001.json.lock");
        let input = DeploymentReceiptInput {
            attempt_id: "attempt-001".into(),
            release_manifest: FileDigest {
                path: "target/minco/release.json".into(),
                sha256: "a".repeat(64),
                bytes: 512,
            },
            release_id: format!("minco.{}", "b".repeat(24)),
            release_digest: "b".repeat(64),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "staging".into(),
                region: "ap-southeast-2".into(),
            },
            configuration_digest: "c".repeat(64),
            database_plans: Vec::new(),
            attestations: Vec::new(),
        };
        let started = DeploymentReceipt::start(input.clone()).unwrap();
        started.write_json(&path).unwrap();

        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        lock_file.lock().unwrap();

        let mut failed = DeploymentReceipt::start(input.clone()).unwrap();
        failed.fail("cloudformation_apply_failed").unwrap();
        let writer_path = path.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            sender.send(failed.write_json(writer_path)).unwrap();
        });

        assert!(matches!(
            receiver.recv_timeout(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        lock_file.unlock().unwrap();
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        writer.join().unwrap();

        let mut forged_success = DeploymentReceipt::start(input).unwrap();
        forged_success
            .succeed(vec![VerificationEvidence {
                kind: "http_smoke".into(),
                file: FileDigest {
                    path: "target/minco/http-smoke.json".into(),
                    sha256: "f".repeat(64),
                    bytes: 128,
                },
            }])
            .unwrap();
        assert!(matches!(
            forged_success.write_json(&path),
            Err(ReleaseError::TerminalDeploymentReceipt { .. })
        ));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn deployment_receipt_independently_verifies_release_plan_and_evidence() {
        let directory =
            std::env::temp_dir().join(format!("minco-deployment-verify-{}", uuid::Uuid::new_v4()));
        for relative in ["target/minco", "target/lambda", "openapi"] {
            std::fs::create_dir_all(directory.join(relative)).unwrap();
        }
        for (path, contents) in [
            ("target/lambda/api.zip", b"artifact".as_slice()),
            ("openapi/openapi.yaml", b"openapi: 3.1.0".as_slice()),
            ("target/minco/plan.json", b"{}".as_slice()),
            ("target/minco/template.yaml", b"Resources: {}".as_slice()),
            (
                "target/minco/http-smoke.json",
                b"{\"verified\":true}".as_slice(),
            ),
        ] {
            std::fs::write(directory.join(path), contents).unwrap();
        }
        let environment = ReleaseEnvironment {
            application: "orders".into(),
            environment: "staging".into(),
            region: "ap-southeast-2".into(),
        };
        let release = ReleaseManifest::seal(ReleaseManifestInput {
            source_change: "a".repeat(40),
            environment: environment.clone(),
            toolchain: ToolchainIdentity {
                rustc: "rustc 1.97.1".into(),
                cargo_minco: "0.3.1".into(),
                artifact_builder: None,
            },
            artifacts: vec![FunctionArtifact {
                function_id: "api".into(),
                file: FileDigest::from_rooted_path(
                    &directory,
                    directory.join("target/lambda/api.zip"),
                )
                .unwrap(),
            }],
            contract: FileDigest::from_rooted_path(
                &directory,
                directory.join("openapi/openapi.yaml"),
            )
            .unwrap(),
            configuration_digest: "b".repeat(64),
            database_sources: DatabaseSourceDigests {
                migration_catalog: "c".repeat(64),
                seed_catalog: "d".repeat(64),
            },
            cargo_lock: None,
            deployment_plan: FileDigest::from_rooted_path(
                &directory,
                directory.join("target/minco/plan.json"),
            )
            .unwrap(),
            deployment_template: FileDigest::from_rooted_path(
                &directory,
                directory.join("target/minco/template.yaml"),
            )
            .unwrap(),
            attestations: Vec::new(),
        })
        .unwrap();
        let release_path = directory.join("target/minco/release.json");
        release.write_json(&release_path).unwrap();

        let migration_plan_path = directory.join("target/minco/migration-plan.json");
        std::fs::write(
            &migration_plan_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 1,
                "catalog_digest": "c".repeat(64),
                "digest": "e".repeat(64),
                "sets": [],
            }))
            .unwrap(),
        )
        .unwrap();
        let mut receipt = DeploymentReceipt::start(DeploymentReceiptInput {
            attempt_id: "attempt-verified".into(),
            release_manifest: FileDigest::from_rooted_path(&directory, &release_path).unwrap(),
            release_id: release.release_id.clone(),
            release_digest: release.release_digest.clone(),
            environment,
            configuration_digest: release.configuration_digest,
            database_plans: vec![DatabasePlanBinding {
                kind: DatabasePlanKind::Migration,
                schema_version: 1,
                catalog_digest: "c".repeat(64),
                plan_digest: "e".repeat(64),
                file: FileDigest::from_rooted_path(&directory, &migration_plan_path).unwrap(),
                selected_set: None,
                environment: None,
            }],
            attestations: Vec::new(),
        })
        .unwrap();
        receipt
            .succeed(vec![VerificationEvidence {
                kind: "http_smoke".into(),
                file: FileDigest::from_rooted_path(
                    &directory,
                    directory.join("target/minco/http-smoke.json"),
                )
                .unwrap(),
            }])
            .unwrap();
        receipt.verify_at(&directory).unwrap();

        std::fs::write(migration_plan_path, b"{\"tampered\":true}").unwrap();
        assert!(matches!(
            receipt.verify_at(&directory),
            Err(ReleaseError::DigestMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(directory);
    }

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
    fn file_digest_rejects_independent_hash_and_size_mismatches() {
        let directory =
            std::env::temp_dir().join(format!("minco-release-digest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("artifact");
        std::fs::write(&path, b"one").unwrap();
        let digest = FileDigest::from_path(&path).unwrap();

        let mut wrong_hash = digest.clone();
        wrong_hash.sha256 = "a".repeat(64);
        assert!(matches!(
            wrong_hash.verify(),
            Err(ReleaseError::DigestMismatch { .. })
        ));

        let mut wrong_size = digest;
        wrong_size.bytes += 1;
        assert!(matches!(
            wrong_size.verify(),
            Err(ReleaseError::DigestMismatch { .. })
        ));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn deployment_start_rejects_every_non_lowercase_sha256_shape() {
        let valid = DeploymentReceiptInput {
            attempt_id: "attempt-digest".into(),
            release_manifest: FileDigest {
                path: "target/minco/release.json".into(),
                sha256: "a".repeat(64),
                bytes: 512,
            },
            release_id: format!("minco.{}", "b".repeat(24)),
            release_digest: "b".repeat(64),
            environment: ReleaseEnvironment {
                application: "orders".into(),
                environment: "staging".into(),
                region: "ap-southeast-2".into(),
            },
            configuration_digest: "c".repeat(64),
            database_plans: Vec::new(),
            attestations: Vec::new(),
        };

        for invalid in ["c".repeat(63), "g".repeat(64), "C".repeat(64)] {
            let mut input = valid.clone();
            input.configuration_digest = invalid;
            assert!(matches!(
                DeploymentReceipt::start(input),
                Err(ReleaseError::InvalidRelease(_))
            ));
        }
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
