//! Fail-closed AWS deployment guards and `CloudFormation` change-set review.
#![forbid(unsafe_code)]

use minco_plugin_static_site::{StaticSitePublication, StaticSiteReleaseManifest};
use minco_release::{DeploymentOutcome, DeploymentReceipt, FileDigest, ReleaseEnvironment};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write as _,
    path::{Component, Path},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Add,
    Modify,
    Remove,
    Import,
    Dynamic,
    SyncWithActual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Replacement {
    Never,
    Conditional,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Delete,
    Retain,
    Snapshot,
    ReplaceAndDelete,
    ReplaceAndRetain,
    ReplaceAndSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeScope {
    Properties,
    Metadata,
    CreationPolicy,
    UpdatePolicy,
    DeletionPolicy,
    UpdateReplacePolicy,
    Tags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceChange {
    pub logical_id: String,
    pub resource_type: String,
    pub action: ChangeAction,
    pub replacement: Option<Replacement>,
    pub policy_action: Option<PolicyAction>,
    pub scope: Vec<ChangeScope>,
}

impl ResourceChange {
    pub fn new(
        logical_id: impl Into<String>,
        resource_type: impl Into<String>,
        action: ChangeAction,
        replacement: Option<Replacement>,
    ) -> Self {
        Self {
            logical_id: logical_id.into(),
            resource_type: resource_type.into(),
            action,
            replacement,
            policy_action: None,
            scope: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSetReview {
    pub additions: Vec<ResourceChange>,
    pub modifications: Vec<ResourceChange>,
    pub replacements: Vec<ResourceChange>,
    pub deletions: Vec<ResourceChange>,
    pub imports: Vec<ResourceChange>,
    pub indeterminate: Vec<ResourceChange>,
    pub metadata_syncs: Vec<ResourceChange>,
}

impl ChangeSetReview {
    pub fn classify(
        changes: impl IntoIterator<Item = ResourceChange>,
    ) -> Result<Self, ChangeReviewError> {
        let mut logical_ids = BTreeSet::new();
        let mut review = Self {
            additions: Vec::new(),
            modifications: Vec::new(),
            replacements: Vec::new(),
            deletions: Vec::new(),
            imports: Vec::new(),
            indeterminate: Vec::new(),
            metadata_syncs: Vec::new(),
        };

        for change in changes {
            if change.logical_id.is_empty() || change.resource_type.is_empty() {
                return Err(ChangeReviewError::MissingIdentity);
            }
            if !logical_ids.insert(change.logical_id.clone()) {
                return Err(ChangeReviewError::DuplicateLogicalId(change.logical_id));
            }
            match (change.action, change.replacement) {
                (ChangeAction::Add, None) => review.additions.push(change),
                (ChangeAction::Remove, None) => review.deletions.push(change),
                (ChangeAction::Modify, Some(Replacement::Conditional | Replacement::Always)) => {
                    review.replacements.push(change);
                }
                (ChangeAction::Modify, None | Some(Replacement::Never)) => {
                    review.modifications.push(change);
                }
                (ChangeAction::Import, None) => review.imports.push(change),
                (ChangeAction::Dynamic, None) => review.indeterminate.push(change),
                (ChangeAction::SyncWithActual, None) => review.metadata_syncs.push(change),
                (
                    ChangeAction::Add
                    | ChangeAction::Remove
                    | ChangeAction::Import
                    | ChangeAction::Dynamic
                    | ChangeAction::SyncWithActual,
                    Some(_),
                ) => {
                    return Err(ChangeReviewError::UnexpectedReplacement(change.logical_id));
                }
            }
        }

        for changes in [
            &mut review.additions,
            &mut review.modifications,
            &mut review.replacements,
            &mut review.deletions,
            &mut review.imports,
            &mut review.indeterminate,
            &mut review.metadata_syncs,
        ] {
            changes.sort_by(|left, right| {
                left.logical_id
                    .cmp(&right.logical_id)
                    .then_with(|| left.resource_type.cmp(&right.resource_type))
            });
        }
        Ok(review)
    }
}

#[derive(Debug, Error)]
pub enum ChangeReviewError {
    #[error("change-set resource is missing a logical ID or resource type")]
    MissingIdentity,
    #[error("change set contains duplicate logical ID {0}")]
    DuplicateLogicalId(String),
    #[error("non-modification change {0} unexpectedly declares replacement behavior")]
    UnexpectedReplacement(String),
    #[error("change-set entry has unsupported type {0}")]
    UnsupportedChangeType(String),
    #[error(
        "CloudFormation change-set response type {actual:?} contradicts guarded type {expected:?}"
    )]
    UnexpectedChangeSetType {
        expected: ChangeSetType,
        actual: ChangeSetType,
    },
    #[error("CloudFormation change-set response is invalid: {0}")]
    ProviderJson(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetType {
    Create,
    Update,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetStatus {
    CreatePending,
    CreateInProgress,
    CreateComplete,
    DeletePending,
    DeleteInProgress,
    DeleteComplete,
    DeleteFailed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Unavailable,
    Available,
    ExecuteInProgress,
    ExecuteComplete,
    ExecuteFailed,
    Obsolete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudFormationChangeSet {
    pub change_set_name: String,
    pub change_set_id: String,
    pub stack_id: String,
    pub stack_name: String,
    pub change_set_type: ChangeSetType,
    pub status: ChangeSetStatus,
    pub execution_status: ExecutionStatus,
    pub review: ChangeSetReview,
}

impl CloudFormationChangeSet {
    pub fn from_aws_json(
        source: &[u8],
        expected_change_set_type: ChangeSetType,
    ) -> Result<Self, ChangeReviewError> {
        let provider: ProviderChangeSet = serde_json::from_slice(source)?;
        if let Some(actual) = provider.change_set_type.map(Into::into)
            && actual != expected_change_set_type
        {
            return Err(ChangeReviewError::UnexpectedChangeSetType {
                expected: expected_change_set_type,
                actual,
            });
        }
        let changes = provider
            .changes
            .into_iter()
            .map(|change| {
                if change.kind != "Resource" {
                    return Err(ChangeReviewError::UnsupportedChangeType(change.kind));
                }
                let mut resource = ResourceChange::new(
                    change.resource_change.logical_resource_id,
                    change.resource_change.resource_type,
                    change.resource_change.action.into(),
                    change.resource_change.replacement.map(Into::into),
                );
                resource.policy_action = change.resource_change.policy_action.map(Into::into);
                resource.scope = change
                    .resource_change
                    .scope
                    .into_iter()
                    .map(Into::into)
                    .collect();
                resource.scope.sort();
                resource.scope.dedup();
                Ok(resource)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            change_set_name: provider.change_set_name,
            change_set_id: provider.change_set_id,
            stack_id: provider.stack_id,
            stack_name: provider.stack_name,
            change_set_type: expected_change_set_type,
            status: provider.status.into(),
            execution_status: provider.execution_status.into(),
            review: ChangeSetReview::classify(changes)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StackDrift {
    NotApplicableNewStack,
    InSync {
        detection_id: String,
        checked_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSetReceiptInput {
    pub source_change: String,
    pub release_manifest: FileDigest,
    pub release_id: String,
    pub release_digest: String,
    pub release_approval_digest: String,
    pub configuration_digest: String,
    pub environment: ReleaseEnvironment,
    pub expected_account_id: String,
    pub expected_role_arn: String,
    pub target_config: FileDigest,
    pub packaged_template: FileDigest,
    pub drift: StackDrift,
    pub change_set: CloudFormationChangeSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSetReceipt {
    pub schema_version: u32,
    pub receipt_digest: String,
    pub source_change: String,
    pub release_manifest: FileDigest,
    pub release_id: String,
    pub release_digest: String,
    pub release_approval_digest: String,
    pub configuration_digest: String,
    pub environment: ReleaseEnvironment,
    pub expected_account_id: String,
    pub expected_role_arn: String,
    pub target_config: FileDigest,
    pub packaged_template: FileDigest,
    pub drift: StackDrift,
    pub change_set: CloudFormationChangeSet,
}

impl ChangeSetReceipt {
    pub fn seal(input: ChangeSetReceiptInput) -> Result<Self, ChangeSetReceiptError> {
        let mut receipt = Self {
            schema_version: 1,
            receipt_digest: String::new(),
            source_change: input.source_change,
            release_manifest: input.release_manifest,
            release_id: input.release_id,
            release_digest: input.release_digest,
            release_approval_digest: input.release_approval_digest,
            configuration_digest: input.configuration_digest,
            environment: input.environment,
            expected_account_id: input.expected_account_id,
            expected_role_arn: input.expected_role_arn,
            target_config: input.target_config,
            packaged_template: input.packaged_template,
            drift: input.drift,
            change_set: input.change_set,
        };
        receipt.receipt_digest = receipt.calculate_digest()?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub fn verify_structure(&self) -> Result<(), ChangeSetReceiptError> {
        if self.schema_version != 1 {
            return Err(ChangeSetReceiptError::Invalid(
                "unsupported change-set receipt schema".into(),
            ));
        }
        if !source_change_is_valid(&self.source_change) {
            return Err(ChangeSetReceiptError::Invalid(
                "source change must be an exact lowercase VCS commit ID".into(),
            ));
        }
        if !release_identity_is_valid(&self.release_id, &self.release_digest)
            || self.release_approval_digest != self.release_digest
            || !sha256_is_valid(&self.configuration_digest)
        {
            return Err(ChangeSetReceiptError::Invalid(
                "release identity, configuration, or approval is invalid".into(),
            ));
        }
        for (file, label) in [
            (&self.release_manifest, "release manifest"),
            (&self.target_config, "target configuration"),
            (&self.packaged_template, "packaged template"),
        ] {
            validate_bound_file(file, label)?;
        }
        if !environment_is_valid(&self.environment.environment)
            || !region_is_valid(&self.environment.region)
            || self.environment.application.is_empty()
            || !account_id_is_valid(&self.expected_account_id)
            || !role_arn_is_valid(&self.expected_role_arn, &self.expected_account_id)
            || !stack_name_is_valid(&self.change_set.stack_name)
        {
            return Err(ChangeSetReceiptError::Invalid(
                "deployment environment identity is invalid".into(),
            ));
        }
        if self.change_set.status != ChangeSetStatus::CreateComplete
            || self.change_set.execution_status != ExecutionStatus::Available
        {
            return Err(ChangeSetReceiptError::Invalid(
                "change set is not complete and available".into(),
            ));
        }
        match (&self.change_set.change_set_type, &self.drift) {
            (ChangeSetType::Create, StackDrift::NotApplicableNewStack)
            | (ChangeSetType::Update, StackDrift::InSync { .. }) => {}
            _ => {
                return Err(ChangeSetReceiptError::Invalid(
                    "change-set type and drift evidence are inconsistent".into(),
                ));
            }
        }
        let role_partition = self
            .expected_role_arn
            .split(':')
            .nth(1)
            .ok_or_else(|| ChangeSetReceiptError::Invalid("role ARN is invalid".into()))?;
        for (arn, kind, name) in [
            (
                &self.change_set.change_set_id,
                "changeSet",
                self.change_set.change_set_name.as_str(),
            ),
            (
                &self.change_set.stack_id,
                "stack",
                self.change_set.stack_name.as_str(),
            ),
        ] {
            let (partition, region, account_id) = cloudformation_arn_identity(arn, kind, name)?;
            if region != self.environment.region || account_id != self.expected_account_id {
                return Err(ChangeSetReceiptError::Invalid(
                    "change-set ARN does not match the reviewed target".into(),
                ));
            }
            if partition != role_partition {
                return Err(ChangeSetReceiptError::Invalid(
                    "change-set ARN partition does not match the reviewed role".into(),
                ));
            }
        }
        if let StackDrift::InSync {
            detection_id,
            checked_at,
        } = &self.drift
            && (detection_id.is_empty() || checked_at.is_empty())
        {
            return Err(ChangeSetReceiptError::Invalid(
                "drift evidence is incomplete".into(),
            ));
        }
        if !sha256_is_valid(&self.receipt_digest) {
            return Err(ChangeSetReceiptError::Invalid(
                "change-set receipt digest is invalid".into(),
            ));
        }
        if self.calculate_digest()? != self.receipt_digest {
            return Err(ChangeSetReceiptError::DigestMismatch);
        }
        Ok(())
    }

    pub fn from_json(source: &[u8]) -> Result<Self, ChangeSetReceiptError> {
        let value: serde_json::Value = serde_json::from_slice(source)?;
        let receipt: Self = serde_json::from_value(value.clone())?;
        if serde_json::to_value(&receipt)? != value {
            return Err(ChangeSetReceiptError::Invalid(
                "change-set receipt contains unknown or non-canonical fields".into(),
            ));
        }
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), ChangeSetReceiptError> {
        self.verify_structure()?;
        let root = root.as_ref();
        for file in [
            &self.release_manifest,
            &self.target_config,
            &self.packaged_template,
        ] {
            file.verify_at(root).map_err(|error| {
                ChangeSetReceiptError::Invalid(format!("bound file verification failed: {error}"))
            })?;
        }
        let release =
            minco_release::ReleaseManifest::read_json(root.join(&self.release_manifest.path))
                .and_then(|release| release.verify_at(root).map(|()| release))
                .map_err(|error| {
                    ChangeSetReceiptError::Invalid(format!("release verification failed: {error}"))
                })?;
        if release.source_change != self.source_change
            || release.release_id != self.release_id
            || release.release_digest != self.release_digest
            || release.configuration_digest != self.configuration_digest
            || release.environment != self.environment
        {
            return Err(ChangeSetReceiptError::Invalid(
                "receipt does not match the verified release manifest".into(),
            ));
        }
        let catalog = DeploymentTargetCatalog::from_toml(&std::fs::read_to_string(
            root.join(&self.target_config.path),
        )?)
        .map_err(|error| {
            ChangeSetReceiptError::Invalid(format!(
                "deployment target verification failed: {error}"
            ))
        })?;
        let selected = catalog
            .select(Some(&self.environment.environment))
            .map_err(|error| {
                ChangeSetReceiptError::Invalid(format!(
                    "deployment target selection failed: {error}"
                ))
            })?;
        if !selected.target.enabled
            || selected.target.expected_account_id != self.expected_account_id
            || selected.target.expected_region != self.environment.region
            || selected.target.expected_role_arn != self.expected_role_arn
            || selected.target.stack_name != self.change_set.stack_name
        {
            return Err(ChangeSetReceiptError::Invalid(
                "receipt does not match the enabled reviewed target".into(),
            ));
        }
        Ok(())
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ChangeSetReceiptError> {
        self.verify_structure()?;
        let path = path.as_ref();
        let mut rendered = serde_json::to_vec_pretty(self)?;
        rendered.push(b'\n');
        if path.exists() {
            if std::fs::read(path)? == rendered {
                return Ok(());
            }
            return Err(ChangeSetReceiptError::Conflict(path.display().to_string()));
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        output.write_all(&rendered)?;
        output.sync_all()?;
        Ok(())
    }

    fn calculate_digest(&self) -> Result<String, ChangeSetReceiptError> {
        #[derive(Serialize)]
        struct DigestPayload<'a> {
            schema_version: u32,
            source_change: &'a str,
            release_manifest: &'a FileDigest,
            release_id: &'a str,
            release_digest: &'a str,
            release_approval_digest: &'a str,
            configuration_digest: &'a str,
            environment: &'a ReleaseEnvironment,
            expected_account_id: &'a str,
            expected_role_arn: &'a str,
            target_config: &'a FileDigest,
            packaged_template: &'a FileDigest,
            drift: &'a StackDrift,
            change_set: &'a CloudFormationChangeSet,
        }
        let payload = DigestPayload {
            schema_version: self.schema_version,
            source_change: &self.source_change,
            release_manifest: &self.release_manifest,
            release_id: &self.release_id,
            release_digest: &self.release_digest,
            release_approval_digest: &self.release_approval_digest,
            configuration_digest: &self.configuration_digest,
            environment: &self.environment,
            expected_account_id: &self.expected_account_id,
            expected_role_arn: &self.expected_role_arn,
            target_config: &self.target_config,
            packaged_template: &self.packaged_template,
            drift: &self.drift,
            change_set: &self.change_set,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&payload)?)
        ))
    }
}

#[derive(Debug, Error)]
pub enum ChangeSetReceiptError {
    #[error("change-set receipt is invalid: {0}")]
    Invalid(String),
    #[error("change-set receipt digest does not match its contents")]
    DigestMismatch,
    #[error("immutable change-set receipt already exists at {0}")]
    Conflict(String),
    #[error("change-set receipt JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("change-set receipt I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedCheckKind {
    Contract,
    Readiness,
    Authentication,
    Smoke,
    ArtifactIdentity,
}

impl HostedCheckKind {
    const REQUIRED: [Self; 5] = [
        Self::Contract,
        Self::Readiness,
        Self::Authentication,
        Self::Smoke,
        Self::ArtifactIdentity,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedCheckResult {
    pub kind: HostedCheckKind,
    pub passed: bool,
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedVerificationInput {
    pub endpoint: String,
    pub expected_artifact_digest: String,
    pub executed_artifact_digest: String,
    pub executed_version: String,
    pub checks: Vec<HostedCheckResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedVerificationReport {
    pub endpoint: String,
    pub executed_artifact_digest: String,
    pub executed_version: String,
    pub checks: Vec<HostedCheckResult>,
}

impl HostedVerificationReport {
    pub fn complete(mut input: HostedVerificationInput) -> Result<Self, HostedVerificationError> {
        let endpoint = url::Url::parse(&input.endpoint)
            .ok()
            .filter(|endpoint| {
                endpoint.scheme() == "https"
                    && endpoint.host_str().is_some()
                    && endpoint.username().is_empty()
                    && endpoint.password().is_none()
                    && endpoint.query().is_none()
                    && endpoint.fragment().is_none()
            })
            .ok_or(HostedVerificationError::InvalidField { field: "endpoint" })?;
        if !input
            .executed_version
            .parse::<u64>()
            .ok()
            .is_some_and(|version| version > 0 && version.to_string() == input.executed_version)
        {
            return Err(HostedVerificationError::InvalidField {
                field: "executed_version",
            });
        }
        if !sha256_is_valid(&input.expected_artifact_digest)
            || !sha256_is_valid(&input.executed_artifact_digest)
        {
            return Err(HostedVerificationError::InvalidField {
                field: "artifact_digest",
            });
        }
        for kind in HostedCheckKind::REQUIRED {
            let count = input
                .checks
                .iter()
                .filter(|check| check.kind == kind)
                .count();
            if count == 0 {
                return Err(HostedVerificationError::MissingRequiredCheck { kind });
            }
            if count > 1 {
                return Err(HostedVerificationError::DuplicateRequiredCheck { kind });
            }
        }
        for check in &input.checks {
            let valid = if check.kind == HostedCheckKind::ArtifactIdentity {
                check.request_id.is_none() && check.status_code.is_none()
            } else {
                let structural = check.request_id.as_deref().is_some_and(request_id_is_valid)
                    && check
                        .status_code
                        .is_some_and(|status| (100..=599).contains(&status));
                let expected_status = match check.kind {
                    HostedCheckKind::Contract
                    | HostedCheckKind::Readiness
                    | HostedCheckKind::Smoke => 200,
                    HostedCheckKind::Authentication => 401,
                    HostedCheckKind::ArtifactIdentity => unreachable!(),
                };
                structural && (!check.passed || check.status_code == Some(expected_status))
            };
            if !valid {
                return Err(HostedVerificationError::InvalidCheck { kind: check.kind });
            }
        }
        if let Some(check) = input.checks.iter().find(|check| !check.passed) {
            return Err(HostedVerificationError::RequiredCheckFailed { kind: check.kind });
        }
        if input.executed_artifact_digest != input.expected_artifact_digest {
            return Err(HostedVerificationError::ArtifactMismatch);
        }
        input.checks.sort_by_key(|check| check.kind);
        Ok(Self {
            endpoint: endpoint.to_string().trim_end_matches('/').to_owned(),
            executed_artifact_digest: input.executed_artifact_digest,
            executed_version: input.executed_version,
            checks: input.checks,
        })
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), HostedVerificationError> {
        let path = path.as_ref();
        let mut rendered = serde_json::to_vec_pretty(self)
            .map_err(|error| HostedVerificationError::Serialization(error.to_string()))?;
        rendered.push(b'\n');
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(&rendered)
                    .map_err(|error| HostedVerificationError::Io(error.to_string()))?;
                file.sync_all()
                    .map_err(|error| HostedVerificationError::Io(error.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = Self::read_json(path, &self.executed_artifact_digest)?;
                if existing == *self {
                    Ok(())
                } else {
                    Err(HostedVerificationError::Conflict(
                        path.display().to_string(),
                    ))
                }
            }
            Err(error) => Err(HostedVerificationError::Io(error.to_string())),
        }
    }

    pub fn read_json(
        path: impl AsRef<Path>,
        expected_artifact_digest: &str,
    ) -> Result<Self, HostedVerificationError> {
        let bytes =
            std::fs::read(path).map_err(|error| HostedVerificationError::Io(error.to_string()))?;
        let report: Self = serde_json::from_slice(&bytes)
            .map_err(|error| HostedVerificationError::Serialization(error.to_string()))?;
        Self::complete(HostedVerificationInput {
            endpoint: report.endpoint,
            expected_artifact_digest: expected_artifact_digest.to_owned(),
            executed_artifact_digest: report.executed_artifact_digest,
            executed_version: report.executed_version,
            checks: report.checks,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticSiteDistributionStatus {
    Deployed,
    InProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticSiteInvalidationStatus {
    Completed,
    InProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudFrontBillingModel {
    RequestAndTransfer,
    FlatRate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlatRateEligibility {
    Ineligible,
    EligibleNotSelected,
    EligibleSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteCertificateObservation {
    pub arn: String,
    pub status: String,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteDnsObservation {
    pub hosted_zone_id: String,
    pub hosted_zone_name: String,
    pub private_zone: bool,
    pub a_target: String,
    pub aaaa_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteObjectObservation {
    pub path: String,
    pub s3_bytes: u64,
    pub s3_sha256: String,
    pub s3_content_type: String,
    pub s3_cache_control: String,
    pub cloudfront_bytes: u64,
    pub cloudfront_sha256: String,
    pub cloudfront_content_type: String,
    pub cloudfront_cache_control: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSitePricingEvidence {
    pub checked_on: String,
    pub source: String,
    pub billing_model: CloudFrontBillingModel,
    pub price_class: String,
    pub flat_rate_eligibility: FlatRateEligibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteProviderObservation {
    pub bucket: String,
    pub distribution_id: String,
    pub distribution_domain: String,
    pub distribution_status: StaticSiteDistributionStatus,
    pub distribution_aliases: Vec<String>,
    pub distribution_certificate_arn: Option<String>,
    pub origin_domain: String,
    pub origin_access_control_id: String,
    pub invalidation_id: String,
    pub invalidation_status: StaticSiteInvalidationStatus,
    pub certificate: Option<StaticSiteCertificateObservation>,
    pub dns: Option<StaticSiteDnsObservation>,
    pub objects: Vec<StaticSiteObjectObservation>,
    pub pricing: StaticSitePricingEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticSiteVerificationInput {
    pub release_digest: String,
    pub expected_account_id: String,
    pub deployment_region: String,
    pub manifest: StaticSiteReleaseManifest,
    pub observation: StaticSiteProviderObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteVerificationReport {
    pub schema_version: u32,
    pub release_digest: String,
    pub expected_account_id: String,
    pub deployment_region: String,
    pub static_site_manifest_digest: String,
    pub manifest: StaticSiteReleaseManifest,
    pub observation: StaticSiteProviderObservation,
}

impl StaticSiteVerificationReport {
    pub fn complete(
        mut input: StaticSiteVerificationInput,
    ) -> Result<Self, StaticSiteVerificationError> {
        if !sha256_is_valid(&input.release_digest)
            || !account_id_is_valid(&input.expected_account_id)
            || !region_is_valid(&input.deployment_region)
            || input.manifest.schema_version != 1
            || input.manifest.plan.validate().is_err()
        {
            return Err(StaticSiteVerificationError::InvalidIdentity);
        }
        let manifest_digest = input
            .manifest
            .digest_sha256()
            .map_err(|_| StaticSiteVerificationError::ManifestInvalid)?;
        validate_static_site_provider_identity(&input.observation, &input.deployment_region)?;
        validate_static_site_pricing(&input.manifest, &input.observation.pricing)?;

        input.observation.distribution_aliases.sort_unstable();
        if input
            .observation
            .distribution_aliases
            .windows(2)
            .any(|aliases| aliases[0] == aliases[1])
        {
            return Err(StaticSiteVerificationError::DistributionMismatch);
        }
        validate_static_site_domain(
            &input.manifest,
            &input.expected_account_id,
            &input.observation,
        )?;

        input
            .observation
            .objects
            .sort_by(|left, right| left.path.cmp(&right.path));
        if input.observation.objects.len() != input.manifest.assets.len()
            || input
                .observation
                .objects
                .windows(2)
                .any(|objects| objects[0].path == objects[1].path)
        {
            return Err(StaticSiteVerificationError::AssetSetMismatch);
        }
        for (expected, observed) in input.manifest.assets.iter().zip(&input.observation.objects) {
            let s3_matches = expected.bytes == observed.s3_bytes
                && expected.sha256 == observed.s3_sha256
                && expected.content_type == observed.s3_content_type
                && expected.cache_control == observed.s3_cache_control;
            let cloudfront_matches = expected.bytes == observed.cloudfront_bytes
                && expected.sha256 == observed.cloudfront_sha256
                && expected.content_type == observed.cloudfront_content_type
                && expected.cache_control == observed.cloudfront_cache_control;
            if expected.path != observed.path || !s3_matches || !cloudfront_matches {
                return Err(StaticSiteVerificationError::AssetMismatch {
                    path: expected.path.clone(),
                });
            }
        }

        Ok(Self {
            schema_version: 1,
            release_digest: input.release_digest,
            expected_account_id: input.expected_account_id,
            deployment_region: input.deployment_region,
            static_site_manifest_digest: manifest_digest,
            manifest: input.manifest,
            observation: input.observation,
        })
    }

    pub fn verify_structure(&self) -> Result<(), StaticSiteVerificationError> {
        if self.schema_version != 1 {
            return Err(StaticSiteVerificationError::UnsupportedSchema);
        }
        let rebuilt = Self::complete(StaticSiteVerificationInput {
            release_digest: self.release_digest.clone(),
            expected_account_id: self.expected_account_id.clone(),
            deployment_region: self.deployment_region.clone(),
            manifest: self.manifest.clone(),
            observation: self.observation.clone(),
        })?;
        if &rebuilt != self {
            return Err(StaticSiteVerificationError::ManifestInvalid);
        }
        Ok(())
    }
}

fn validate_static_site_provider_identity(
    observation: &StaticSiteProviderObservation,
    deployment_region: &str,
) -> Result<(), StaticSiteVerificationError> {
    let distribution_id = provider_identifier_is_valid(&observation.distribution_id, 'E');
    let invalidation_id = provider_identifier_is_valid(&observation.invalidation_id, 'I');
    let distribution_domain = normalized_dns_name(&observation.distribution_domain)
        .is_some_and(|domain| domain.ends_with(".cloudfront.net"));
    let origin_is_private_s3 = private_s3_origin_is_valid(
        &observation.origin_domain,
        &observation.bucket,
        deployment_region,
    );
    if !bucket_name_is_valid(&observation.bucket)
        || !distribution_id
        || !invalidation_id
        || !distribution_domain
        || !origin_is_private_s3
        || !provider_identifier_is_valid(&observation.origin_access_control_id, 'E')
        || observation.distribution_status != StaticSiteDistributionStatus::Deployed
        || observation.invalidation_status != StaticSiteInvalidationStatus::Completed
    {
        return Err(StaticSiteVerificationError::DistributionMismatch);
    }
    Ok(())
}

fn private_s3_origin_is_valid(origin: &str, bucket: &str, region: &str) -> bool {
    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    normalized_dns_name(origin) == Some(format!("{bucket}.s3.{region}.{suffix}").as_str())
}

fn validate_static_site_domain(
    manifest: &StaticSiteReleaseManifest,
    account_id: &str,
    observation: &StaticSiteProviderObservation,
) -> Result<(), StaticSiteVerificationError> {
    let custom_domain = manifest.plan.custom_domain.as_deref();
    let expected_aliases = custom_domain.map_or_else(Vec::new, |domain| vec![domain.to_owned()]);
    if observation.distribution_aliases != expected_aliases {
        return Err(StaticSiteVerificationError::DistributionMismatch);
    }
    match (custom_domain, &observation.certificate) {
        (None, None) if observation.distribution_certificate_arn.is_none() => {}
        (Some(domain), Some(certificate))
            if cloudfront_certificate_arn_is_valid(&certificate.arn, account_id)
                && observation.distribution_certificate_arn.as_deref()
                    == Some(certificate.arn.as_str())
                && certificate.status == "ISSUED"
                && certificate
                    .names
                    .iter()
                    .any(|name| certificate_name_covers(name, domain)) => {}
        _ => return Err(StaticSiteVerificationError::CertificateMismatch),
    }
    match (
        manifest.plan.manage_dns_alias,
        custom_domain,
        &observation.dns,
    ) {
        (false, _, None) => {}
        (true, Some(domain), Some(dns))
            if dns_observation_matches(
                dns,
                domain,
                &observation.distribution_domain,
                manifest.plan.ipv6_enabled,
            ) => {}
        _ => return Err(StaticSiteVerificationError::DnsMismatch),
    }
    Ok(())
}

fn validate_static_site_pricing(
    manifest: &StaticSiteReleaseManifest,
    pricing: &StaticSitePricingEvidence,
) -> Result<(), StaticSiteVerificationError> {
    let source = url::Url::parse(&pricing.source).ok();
    let official_source = source.as_ref().is_some_and(|source| {
        source.scheme() == "https"
            && matches!(
                source.host_str(),
                Some("aws.amazon.com" | "docs.aws.amazon.com")
            )
            && source.username().is_empty()
            && source.password().is_none()
    });
    let selection_valid = match pricing.billing_model {
        CloudFrontBillingModel::RequestAndTransfer => {
            pricing.flat_rate_eligibility != FlatRateEligibility::EligibleSelected
        }
        CloudFrontBillingModel::FlatRate => {
            pricing.flat_rate_eligibility == FlatRateEligibility::EligibleSelected
        }
    };
    if pricing.price_class != manifest.plan.price_class
        || !iso_date_is_valid(&pricing.checked_on)
        || !official_source
        || !selection_valid
    {
        return Err(StaticSiteVerificationError::PricingEvidenceInvalid);
    }
    Ok(())
}

fn provider_identifier_is_valid(value: &str, prefix: char) -> bool {
    value.len() >= 8
        && value.starts_with(prefix)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn normalized_dns_name(value: &str) -> Option<&str> {
    let value = value.strip_suffix('.').unwrap_or(value);
    (!value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        }))
    .then_some(value)
}

fn cloudfront_certificate_arn_is_valid(value: &str, account_id: &str) -> bool {
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    parts.len() == 6
        && parts[0] == "arn"
        && parts[1].starts_with("aws")
        && parts[2] == "acm"
        && parts[3] == "us-east-1"
        && parts[4] == account_id
        && parts[5]
            .strip_prefix("certificate/")
            .is_some_and(|id| !id.is_empty() && !id.chars().any(char::is_control))
}

fn certificate_name_covers(name: &str, domain: &str) -> bool {
    if name == domain {
        return true;
    }
    name.strip_prefix("*.").is_some_and(|suffix| {
        domain.strip_suffix(suffix).is_some_and(|prefix| {
            prefix.ends_with('.') && !prefix[..prefix.len() - 1].contains('.')
        })
    })
}

fn dns_observation_matches(
    dns: &StaticSiteDnsObservation,
    domain: &str,
    distribution_domain: &str,
    ipv6_enabled: bool,
) -> bool {
    let zone = normalized_dns_name(&dns.hosted_zone_name);
    let distribution = normalized_dns_name(distribution_domain);
    let zone_owns_domain = zone.is_some_and(|zone| {
        domain == zone
            || domain
                .strip_suffix(zone)
                .is_some_and(|prefix| prefix.ends_with('.'))
    });
    let target_matches = normalized_dns_name(&dns.a_target) == distribution;
    let ipv6_matches = match (&dns.aaaa_target, ipv6_enabled) {
        (Some(target), true) => normalized_dns_name(target) == distribution,
        (None, false) => true,
        _ => false,
    };
    !dns.private_zone
        && dns.hosted_zone_id.starts_with('Z')
        && dns.hosted_zone_id.len() >= 8
        && dns
            .hosted_zone_id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        && zone_owns_domain
        && target_matches
        && ipv6_matches
}

fn iso_date_is_valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| !matches!(index, 4 | 7) && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u16>().ok();
    let month = value[5..7].parse::<u8>().ok();
    let day = value[8..10].parse::<u8>().ok();
    let Some((year, month, day)) = year.zip(month).zip(day).map(|((y, m), d)| (y, m, d)) else {
        return false;
    };
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StaticSiteVerificationError {
    #[error("static-site verification report uses an unsupported schema")]
    UnsupportedSchema,
    #[error("static-site verification identity is invalid")]
    InvalidIdentity,
    #[error("static-site release manifest is invalid")]
    ManifestInvalid,
    #[error("static-site distribution or invalidation does not match")]
    DistributionMismatch,
    #[error("static-site certificate does not cover the custom domain")]
    CertificateMismatch,
    #[error("static-site DNS evidence does not own or target the custom domain")]
    DnsMismatch,
    #[error("static-site provider object set does not match the release")]
    AssetSetMismatch,
    #[error("static-site provider object {path} does not match the release")]
    AssetMismatch { path: String },
    #[error("static-site pricing evidence is invalid")]
    PricingEvidenceInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticSitePublicationReceiptInput {
    pub release_digest: String,
    pub manifest_file: FileDigest,
    pub bucket: String,
    pub distribution_id: String,
    pub distribution_domain: String,
    pub publication: StaticSitePublication,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSitePublicationReceipt {
    pub schema_version: u32,
    pub receipt_digest: String,
    pub release_digest: String,
    pub manifest_file: FileDigest,
    pub bucket: String,
    pub distribution_id: String,
    pub distribution_domain: String,
    pub publication: StaticSitePublication,
}

impl StaticSitePublicationReceipt {
    pub fn seal(
        input: StaticSitePublicationReceiptInput,
    ) -> Result<Self, StaticSitePublicationReceiptError> {
        let mut receipt = Self {
            schema_version: 1,
            receipt_digest: String::new(),
            release_digest: input.release_digest,
            manifest_file: input.manifest_file,
            bucket: input.bucket,
            distribution_id: input.distribution_id,
            distribution_domain: input.distribution_domain,
            publication: input.publication,
        };
        receipt.receipt_digest = receipt.calculate_digest()?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub fn verify_structure(&self) -> Result<(), StaticSitePublicationReceiptError> {
        validate_bound_file(&self.manifest_file, "static-site release manifest")
            .map_err(|error| StaticSitePublicationReceiptError::Invalid(error.to_string()))?;
        let distribution_domain = normalized_dns_name(&self.distribution_domain);
        let url = url::Url::parse(&self.publication.url).ok();
        if self.schema_version != 1
            || !sha256_is_valid(&self.release_digest)
            || !sha256_is_valid(&self.receipt_digest)
            || !sha256_is_valid(&self.publication.release_manifest_digest)
            || !bucket_name_is_valid(&self.bucket)
            || !provider_identifier_is_valid(&self.distribution_id, 'E')
            || !distribution_domain.is_some_and(|domain| domain.ends_with(".cloudfront.net"))
            || self.publication.assets.is_empty()
            || self.publication.uploaded != self.publication.assets.len()
            || !self
                .publication
                .invalidation_id
                .as_deref()
                .is_some_and(|id| provider_identifier_is_valid(id, 'I'))
            || !self.publication.invalidation_completed
            || url.as_ref().is_none_or(|url| {
                url.scheme() != "https"
                    || url.username() != ""
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || url.path() != "/"
            })
            || self
                .publication
                .assets
                .windows(2)
                .any(|assets| assets[0].path >= assets[1].path)
        {
            return Err(StaticSitePublicationReceiptError::Invalid(
                "publication identity or completion evidence is invalid".into(),
            ));
        }
        if self.calculate_digest()? != self.receipt_digest {
            return Err(StaticSitePublicationReceiptError::DigestMismatch);
        }
        Ok(())
    }

    pub fn verify_at(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<(), StaticSitePublicationReceiptError> {
        self.verify_structure()?;
        let root = root.as_ref();
        self.manifest_file
            .verify_at(root)
            .map_err(|error| StaticSitePublicationReceiptError::Invalid(error.to_string()))?;
        let source = std::fs::read(root.join(&self.manifest_file.path))?;
        let manifest: StaticSiteReleaseManifest = serde_json::from_slice(&source)?;
        manifest
            .verify_at(root)
            .map_err(|error| StaticSitePublicationReceiptError::Invalid(error.to_string()))?;
        let public_host = url::Url::parse(&self.publication.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        let expected_host = manifest
            .plan
            .custom_domain
            .as_ref()
            .unwrap_or(&self.distribution_domain);
        if public_host.as_deref() != Some(expected_host.as_str())
            || manifest
                .digest_sha256()
                .map_err(|error| StaticSitePublicationReceiptError::Invalid(error.to_string()))?
                != self.publication.release_manifest_digest
            || manifest.assets != self.publication.assets
        {
            return Err(StaticSitePublicationReceiptError::Invalid(
                "publication does not match the exact static-site release manifest".into(),
            ));
        }
        Ok(())
    }

    pub fn write_json(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(), StaticSitePublicationReceiptError> {
        self.verify_structure()?;
        let path = path.as_ref();
        let mut rendered = serde_json::to_vec_pretty(self)?;
        rendered.push(b'\n');
        if path.exists() {
            if std::fs::read(path)? == rendered {
                return Ok(());
            }
            return Err(StaticSitePublicationReceiptError::Conflict(
                path.display().to_string(),
            ));
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        output.write_all(&rendered)?;
        output.sync_all()?;
        Ok(())
    }

    pub fn read_json(path: impl AsRef<Path>) -> Result<Self, StaticSitePublicationReceiptError> {
        let source = std::fs::read(path)?;
        let receipt: Self = serde_json::from_slice(&source)?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    fn calculate_digest(&self) -> Result<String, StaticSitePublicationReceiptError> {
        #[derive(Serialize)]
        struct DigestPayload<'a> {
            schema_version: u32,
            release_digest: &'a str,
            manifest_file: &'a FileDigest,
            bucket: &'a str,
            distribution_id: &'a str,
            distribution_domain: &'a str,
            publication: &'a StaticSitePublication,
        }
        let payload = DigestPayload {
            schema_version: self.schema_version,
            release_digest: &self.release_digest,
            manifest_file: &self.manifest_file,
            bucket: &self.bucket,
            distribution_id: &self.distribution_id,
            distribution_domain: &self.distribution_domain,
            publication: &self.publication,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&payload)?)
        ))
    }
}

#[derive(Debug, Error)]
pub enum StaticSitePublicationReceiptError {
    #[error("static-site publication receipt is invalid: {0}")]
    Invalid(String),
    #[error("static-site publication receipt digest does not match its contents")]
    DigestMismatch,
    #[error("immutable static-site publication receipt already exists at {0}")]
    Conflict(String),
    #[error("static-site publication receipt JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("static-site publication receipt I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn verify_promotion_boundary(
    change_set: &CloudFormationChangeSet,
    expected_stack_name: &str,
    live_alias_logical_id: &str,
) -> Result<(), PromotionBoundaryError> {
    if change_set.change_set_type != ChangeSetType::Update
        || change_set.status != ChangeSetStatus::CreateComplete
        || change_set.execution_status != ExecutionStatus::Available
        || change_set.stack_name != expected_stack_name
    {
        return Err(PromotionBoundaryError::InvalidProviderState);
    }
    let review = &change_set.review;
    let only_live_alias = match review.modifications.as_slice() {
        [change] => {
            change.logical_id == live_alias_logical_id
                && change.resource_type == "AWS::Lambda::Alias"
                && change.action == ChangeAction::Modify
                && matches!(change.replacement, None | Some(Replacement::Never))
                && change.policy_action.is_none()
                && change.scope.as_slice() == [ChangeScope::Properties]
        }
        _ => false,
    };
    if !only_live_alias
        || !review.additions.is_empty()
        || !review.replacements.is_empty()
        || !review.deletions.is_empty()
        || !review.imports.is_empty()
        || !review.indeterminate.is_empty()
        || !review.metadata_syncs.is_empty()
    {
        return Err(PromotionBoundaryError::NonRoutingChange);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PromotionBoundaryError {
    #[error("promotion change set is not a complete available update for the expected stack")]
    InvalidProviderState,
    #[error("promotion change set contains a non-routing resource change")]
    NonRoutingChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionOutcome {
    Started,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReceiptInput {
    pub attempt_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub environment: ReleaseEnvironment,
    pub deployment_receipt: FileDigest,
    pub hosted_verification: FileDigest,
    pub operator_approval_digest: String,
    pub stack_name: String,
    pub live_alias_logical_id: String,
    pub previous_version: String,
    pub promoted_version: String,
    pub change_set: CloudFormationChangeSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionReceipt {
    pub schema_version: u32,
    pub receipt_digest: String,
    pub attempt_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub environment: ReleaseEnvironment,
    pub deployment_receipt: FileDigest,
    pub hosted_verification: FileDigest,
    pub operator_approval_digest: String,
    pub stack_name: String,
    pub live_alias_logical_id: String,
    pub previous_version: String,
    pub promoted_version: String,
    pub change_set: CloudFormationChangeSet,
    outcome: PromotionOutcome,
    failure_code: Option<String>,
}

impl PromotionReceipt {
    pub fn start(input: PromotionReceiptInput) -> Result<Self, PromotionReceiptError> {
        verify_promotion_boundary(
            &input.change_set,
            &input.stack_name,
            &input.live_alias_logical_id,
        )?;
        let mut receipt = Self {
            schema_version: 1,
            receipt_digest: String::new(),
            attempt_id: input.attempt_id,
            release_id: input.release_id,
            release_digest: input.release_digest,
            environment: input.environment,
            deployment_receipt: input.deployment_receipt,
            hosted_verification: input.hosted_verification,
            operator_approval_digest: input.operator_approval_digest,
            stack_name: input.stack_name,
            live_alias_logical_id: input.live_alias_logical_id,
            previous_version: input.previous_version,
            promoted_version: input.promoted_version,
            change_set: input.change_set,
            outcome: PromotionOutcome::Started,
            failure_code: None,
        };
        receipt.refresh_digest()?;
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub const fn outcome(&self) -> PromotionOutcome {
        self.outcome
    }

    pub fn succeed(&mut self) -> Result<(), PromotionReceiptError> {
        self.require_started()?;
        self.outcome = PromotionOutcome::Succeeded;
        self.refresh_digest()?;
        self.verify_structure()
    }

    pub fn fail(&mut self, code: impl Into<String>) -> Result<(), PromotionReceiptError> {
        self.require_started()?;
        let code = code.into();
        if !identifier_is_valid(&code) {
            return Err(PromotionReceiptError::Invalid(
                "promotion failure code is invalid".into(),
            ));
        }
        self.outcome = PromotionOutcome::Failed;
        self.failure_code = Some(code);
        self.refresh_digest()?;
        self.verify_structure()
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), PromotionReceiptError> {
        self.verify_structure()?;
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PromotionReceiptError::Invalid("receipt path is invalid".into()))?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join(format!(".{name}.lock")))?;
        lock.lock()?;
        let mut rendered = serde_json::to_vec_pretty(self)?;
        rendered.push(b'\n');
        if !path.exists() {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(&rendered)?;
            file.sync_all()?;
            return Ok(());
        }
        let existing = Self::read_json(path)?;
        if existing == *self {
            return Ok(());
        }
        if existing.attempt_id != self.attempt_id || !existing.same_binding(self) {
            return Err(PromotionReceiptError::Conflict(self.attempt_id.clone()));
        }
        if existing.outcome != PromotionOutcome::Started
            || self.outcome == PromotionOutcome::Started
        {
            return Err(PromotionReceiptError::Terminal {
                attempt_id: existing.attempt_id,
                outcome: existing.outcome,
            });
        }
        let temporary = parent.join(format!(".{name}.{}.tmp", self.receipt_digest));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&rendered)?;
        file.sync_all()?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn read_json(path: impl AsRef<Path>) -> Result<Self, PromotionReceiptError> {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let receipt: Self = serde_json::from_value(value.clone())?;
        if serde_json::to_value(&receipt)? != value {
            return Err(PromotionReceiptError::Invalid(
                "promotion receipt contains unknown or non-canonical fields".into(),
            ));
        }
        receipt.verify_structure()?;
        Ok(receipt)
    }

    pub fn verify_at(&self, root: impl AsRef<Path>) -> Result<(), PromotionReceiptError> {
        self.verify_structure()?;
        let root = root.as_ref();
        self.deployment_receipt
            .verify_at(root)
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        self.hosted_verification
            .verify_at(root)
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;

        let deployment_path = root.join(&self.deployment_receipt.path);
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&deployment_path)?)?;
        let deployment: DeploymentReceipt = serde_json::from_value(value.clone())?;
        if serde_json::to_value(&deployment)? != value {
            return Err(PromotionReceiptError::Invalid(
                "deployment receipt contains unknown or non-canonical fields".into(),
            ));
        }
        deployment
            .verify_at(root)
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        if deployment.outcome() != DeploymentOutcome::Succeeded
            || deployment.release_id != self.release_id
            || deployment.release_digest != self.release_digest
            || deployment.environment != self.environment
        {
            return Err(PromotionReceiptError::Invalid(
                "promotion does not bind a successful deployment of the exact release".into(),
            ));
        }
        let hosted = deployment
            .verification()
            .iter()
            .filter(|verification| verification.kind == "hosted_verification")
            .collect::<Vec<_>>();
        let [verification] = hosted.as_slice() else {
            return Err(PromotionReceiptError::Invalid(
                "deployment must bind exactly one hosted verification report".into(),
            ));
        };
        if verification.file != self.hosted_verification {
            return Err(PromotionReceiptError::Invalid(
                "deployment hosted verification binding does not match promotion".into(),
            ));
        }
        for verification in deployment.verification() {
            match verification.kind.as_str() {
                "hosted_verification" => {}
                "static_site_verification" => {
                    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(
                        root.join(&verification.file.path),
                    )?)?;
                    let report: StaticSiteVerificationReport =
                        serde_json::from_value(value.clone())?;
                    if serde_json::to_value(&report)? != value
                        || report.verify_structure().is_err()
                        || report.release_digest != self.release_digest
                    {
                        return Err(PromotionReceiptError::Invalid(
                            "static-site verification does not bind the promoted release".into(),
                        ));
                    }
                }
                _ => {
                    return Err(PromotionReceiptError::Invalid(
                        "deployment contains an unsupported verification kind".into(),
                    ));
                }
            }
        }
        let release_value: serde_json::Value = serde_json::from_slice(&std::fs::read(
            root.join(&deployment.release_manifest.path),
        )?)?;
        let release: minco_release::ReleaseManifest =
            serde_json::from_value(release_value.clone())?;
        if serde_json::to_value(&release)? != release_value {
            return Err(PromotionReceiptError::Invalid(
                "release manifest contains unknown or non-canonical fields".into(),
            ));
        }
        release
            .verify_at(root)
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        let api_artifacts = release
            .artifacts
            .iter()
            .filter(|artifact| artifact.function_id == "api")
            .collect::<Vec<_>>();
        let [api_artifact] = api_artifacts.as_slice() else {
            return Err(PromotionReceiptError::Invalid(
                "release must bind exactly one API artifact".into(),
            ));
        };
        let report = HostedVerificationReport::read_json(
            root.join(&self.hosted_verification.path),
            &api_artifact.file.sha256,
        )
        .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        if report.executed_version != self.promoted_version {
            return Err(PromotionReceiptError::Invalid(
                "hosted verification version does not match promotion".into(),
            ));
        }
        Ok(())
    }

    fn require_started(&self) -> Result<(), PromotionReceiptError> {
        if self.outcome == PromotionOutcome::Started {
            Ok(())
        } else {
            Err(PromotionReceiptError::Terminal {
                attempt_id: self.attempt_id.clone(),
                outcome: self.outcome,
            })
        }
    }

    fn same_binding(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.attempt_id == other.attempt_id
            && self.release_id == other.release_id
            && self.release_digest == other.release_digest
            && self.environment == other.environment
            && self.deployment_receipt == other.deployment_receipt
            && self.hosted_verification == other.hosted_verification
            && self.operator_approval_digest == other.operator_approval_digest
            && self.stack_name == other.stack_name
            && self.live_alias_logical_id == other.live_alias_logical_id
            && self.previous_version == other.previous_version
            && self.promoted_version == other.promoted_version
            && self.change_set == other.change_set
    }

    fn verify_structure(&self) -> Result<(), PromotionReceiptError> {
        if self.schema_version != 1
            || !identifier_is_valid(&self.attempt_id)
            || !release_identity_is_valid(&self.release_id, &self.release_digest)
            || self.operator_approval_digest != self.hosted_verification.sha256
            || !stack_name_is_valid(&self.stack_name)
            || !self
                .live_alias_logical_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
            || !(self.previous_version == "candidate"
                || published_version_is_valid(&self.previous_version))
            || !published_version_is_valid(&self.promoted_version)
            || self.previous_version == self.promoted_version
        {
            return Err(PromotionReceiptError::Invalid(
                "promotion receipt binding is invalid".into(),
            ));
        }
        validate_bound_file(&self.deployment_receipt, "deployment receipt")
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        validate_bound_file(&self.hosted_verification, "hosted verification")
            .map_err(|error| PromotionReceiptError::Invalid(error.to_string()))?;
        verify_promotion_boundary(
            &self.change_set,
            &self.stack_name,
            &self.live_alias_logical_id,
        )?;
        match self.outcome {
            PromotionOutcome::Started | PromotionOutcome::Succeeded
                if self.failure_code.is_none() => {}
            PromotionOutcome::Failed
                if self
                    .failure_code
                    .as_deref()
                    .is_some_and(identifier_is_valid) => {}
            _ => {
                return Err(PromotionReceiptError::Invalid(
                    "promotion outcome and failure are inconsistent".into(),
                ));
            }
        }
        let actual = self.calculate_digest()?;
        if self.receipt_digest != actual {
            return Err(PromotionReceiptError::DigestMismatch);
        }
        Ok(())
    }

    fn refresh_digest(&mut self) -> Result<(), PromotionReceiptError> {
        self.receipt_digest = self.calculate_digest()?;
        Ok(())
    }

    fn calculate_digest(&self) -> Result<String, PromotionReceiptError> {
        #[derive(Serialize)]
        struct DigestPayload<'a> {
            schema_version: u32,
            attempt_id: &'a str,
            release_id: &'a str,
            release_digest: &'a str,
            environment: &'a ReleaseEnvironment,
            deployment_receipt: &'a FileDigest,
            hosted_verification: &'a FileDigest,
            operator_approval_digest: &'a str,
            stack_name: &'a str,
            live_alias_logical_id: &'a str,
            previous_version: &'a str,
            promoted_version: &'a str,
            change_set: &'a CloudFormationChangeSet,
            outcome: PromotionOutcome,
            failure_code: &'a Option<String>,
        }
        let payload = DigestPayload {
            schema_version: self.schema_version,
            attempt_id: &self.attempt_id,
            release_id: &self.release_id,
            release_digest: &self.release_digest,
            environment: &self.environment,
            deployment_receipt: &self.deployment_receipt,
            hosted_verification: &self.hosted_verification,
            operator_approval_digest: &self.operator_approval_digest,
            stack_name: &self.stack_name,
            live_alias_logical_id: &self.live_alias_logical_id,
            previous_version: &self.previous_version,
            promoted_version: &self.promoted_version,
            change_set: &self.change_set,
            outcome: self.outcome,
            failure_code: &self.failure_code,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&payload)?)
        ))
    }
}

#[derive(Debug, Error)]
pub enum PromotionReceiptError {
    #[error("invalid promotion receipt: {0}")]
    Invalid(String),
    #[error("promotion receipt digest does not match its contents")]
    DigestMismatch,
    #[error("promotion receipt {0} conflicts with an existing attempt")]
    Conflict(String),
    #[error("promotion receipt {attempt_id} is already terminal as {outcome:?}")]
    Terminal {
        attempt_id: String,
        outcome: PromotionOutcome,
    },
    #[error(transparent)]
    Boundary(#[from] PromotionBoundaryError),
    #[error("promotion receipt JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("promotion receipt I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

fn identifier_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn published_version_is_valid(value: &str) -> bool {
    value
        .parse::<u64>()
        .ok()
        .is_some_and(|version| version > 0 && version.to_string() == value)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostedVerificationError {
    #[error("hosted verification field {field} is invalid")]
    InvalidField { field: &'static str },
    #[error("required hosted {kind:?} check is missing")]
    MissingRequiredCheck { kind: HostedCheckKind },
    #[error("required hosted {kind:?} check is duplicated")]
    DuplicateRequiredCheck { kind: HostedCheckKind },
    #[error("hosted {kind:?} check evidence is invalid")]
    InvalidCheck { kind: HostedCheckKind },
    #[error("required hosted {kind:?} check failed")]
    RequiredCheckFailed { kind: HostedCheckKind },
    #[error("executed artifact does not match the verified release")]
    ArtifactMismatch,
    #[error("hosted verification JSON is invalid: {0}")]
    Serialization(String),
    #[error("hosted verification I/O failed: {0}")]
    Io(String),
    #[error("immutable hosted verification report already exists at {0}")]
    Conflict(String),
}

fn request_id_is_valid(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    let value_without_padding = value
        .strip_suffix("==")
        .or_else(|| value.strip_suffix('='))
        .unwrap_or(value);
    !value_without_padding.is_empty()
        && !value_without_padding.contains('=')
        && value_without_padding
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn source_change_is_valid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_bound_file(
    file: &FileDigest,
    label: &'static str,
) -> Result<(), ChangeSetReceiptError> {
    let path = Path::new(&file.path);
    if file.bytes == 0
        || !sha256_is_valid(&file.sha256)
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(ChangeSetReceiptError::Invalid(format!(
            "{label} binding is invalid"
        )));
    }
    Ok(())
}

fn cloudformation_arn_identity<'a>(
    value: &'a str,
    expected_kind: &str,
    expected_name: &str,
) -> Result<(&'a str, &'a str, &'a str), ChangeSetReceiptError> {
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    let resource_is_valid = parts.get(5).is_some_and(|resource| {
        resource
            .strip_prefix(&format!("{expected_kind}/"))
            .and_then(|resource| resource.split_once('/'))
            .is_some_and(|(name, id)| name == expected_name && !id.is_empty() && !id.contains('/'))
    });
    if parts.len() != 6
        || parts[0] != "arn"
        || !parts[1].starts_with("aws")
        || parts[2] != "cloudformation"
        || !region_is_valid(parts[3])
        || !account_id_is_valid(parts[4])
        || !resource_is_valid
    {
        return Err(ChangeSetReceiptError::Invalid(
            "CloudFormation ARN is invalid".into(),
        ));
    }
    Ok((parts[1], parts[3], parts[4]))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProviderChangeSet {
    change_set_name: String,
    change_set_id: String,
    stack_id: String,
    stack_name: String,
    change_set_type: Option<ProviderChangeSetType>,
    status: ProviderChangeSetStatus,
    execution_status: ProviderExecutionStatus,
    #[serde(default)]
    changes: Vec<ProviderChange>,
}

#[derive(Debug, Deserialize)]
struct ProviderChange {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "ResourceChange")]
    resource_change: ProviderResourceChange,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ProviderResourceChange {
    action: ProviderChangeAction,
    logical_resource_id: String,
    resource_type: String,
    replacement: Option<ProviderReplacement>,
    policy_action: Option<ProviderPolicyAction>,
    #[serde(default)]
    scope: Vec<ProviderChangeScope>,
}

macro_rules! provider_enum {
    ($provider:ident => $public:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Deserialize)]
        enum $provider {
            $($variant),+
        }

        impl From<$provider> for $public {
            fn from(value: $provider) -> Self {
                match value {
                    $($provider::$variant => Self::$variant),+
                }
            }
        }
    };
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProviderChangeSetType {
    Create,
    Update,
    Import,
}

impl From<ProviderChangeSetType> for ChangeSetType {
    fn from(value: ProviderChangeSetType) -> Self {
        match value {
            ProviderChangeSetType::Create => Self::Create,
            ProviderChangeSetType::Update => Self::Update,
            ProviderChangeSetType::Import => Self::Import,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProviderChangeSetStatus {
    CreatePending,
    CreateInProgress,
    CreateComplete,
    DeletePending,
    DeleteInProgress,
    DeleteComplete,
    DeleteFailed,
    Failed,
}

impl From<ProviderChangeSetStatus> for ChangeSetStatus {
    fn from(value: ProviderChangeSetStatus) -> Self {
        match value {
            ProviderChangeSetStatus::CreatePending => Self::CreatePending,
            ProviderChangeSetStatus::CreateInProgress => Self::CreateInProgress,
            ProviderChangeSetStatus::CreateComplete => Self::CreateComplete,
            ProviderChangeSetStatus::DeletePending => Self::DeletePending,
            ProviderChangeSetStatus::DeleteInProgress => Self::DeleteInProgress,
            ProviderChangeSetStatus::DeleteComplete => Self::DeleteComplete,
            ProviderChangeSetStatus::DeleteFailed => Self::DeleteFailed,
            ProviderChangeSetStatus::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ProviderExecutionStatus {
    Unavailable,
    Available,
    ExecuteInProgress,
    ExecuteComplete,
    ExecuteFailed,
    Obsolete,
}

impl From<ProviderExecutionStatus> for ExecutionStatus {
    fn from(value: ProviderExecutionStatus) -> Self {
        match value {
            ProviderExecutionStatus::Unavailable => Self::Unavailable,
            ProviderExecutionStatus::Available => Self::Available,
            ProviderExecutionStatus::ExecuteInProgress => Self::ExecuteInProgress,
            ProviderExecutionStatus::ExecuteComplete => Self::ExecuteComplete,
            ProviderExecutionStatus::ExecuteFailed => Self::ExecuteFailed,
            ProviderExecutionStatus::Obsolete => Self::Obsolete,
        }
    }
}
provider_enum!(ProviderChangeAction => ChangeAction {
    Add,
    Modify,
    Remove,
    Import,
    Dynamic,
    SyncWithActual,
});
provider_enum!(ProviderPolicyAction => PolicyAction {
    Delete,
    Retain,
    Snapshot,
    ReplaceAndDelete,
    ReplaceAndRetain,
    ReplaceAndSnapshot,
});
provider_enum!(ProviderChangeScope => ChangeScope {
    Properties,
    Metadata,
    CreationPolicy,
    UpdatePolicy,
    DeletionPolicy,
    UpdateReplacePolicy,
    Tags,
});

#[derive(Debug, Deserialize)]
enum ProviderReplacement {
    True,
    False,
    Conditional,
}

impl From<ProviderReplacement> for Replacement {
    fn from(value: ProviderReplacement) -> Self {
        match value {
            ProviderReplacement::True => Self::Always,
            ProviderReplacement::False => Self::Never,
            ProviderReplacement::Conditional => Self::Conditional,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftState {
    Clean,
    Drifted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MigrationState {
    NotRequired,
    Verified { plan_digest: String },
    Missing,
    Drifted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Clean,
    Dirty,
    Conflicted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentExpectation {
    pub account_id: String,
    pub region: String,
    pub environment: String,
    pub role_arn: String,
    pub release_id: String,
    pub release_digest: String,
    pub configuration_digest: String,
    pub migration_plan_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentObservation {
    pub account_id: String,
    pub region: String,
    pub environment: String,
    pub role_arn: String,
    pub release_id: String,
    pub release_digest: String,
    pub release_verified: bool,
    pub configuration_digest: String,
    pub drift: DriftState,
    pub migration: MigrationState,
    pub source: SourceState,
    pub operator_approval_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEnvironment {
    pub account_id: String,
    pub region: String,
    pub environment: String,
    pub role_arn: String,
    pub release_id: String,
    pub release_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("caller identity is not an IAM role or STS assumed-role session")]
pub struct CallerIdentityError;

pub fn caller_role_arn(caller_arn: &str) -> Result<String, CallerIdentityError> {
    let parts = caller_arn.splitn(6, ':').collect::<Vec<_>>();
    if parts.len() != 6
        || parts[0] != "arn"
        || !parts[1].starts_with("aws")
        || !parts[3].is_empty()
        || !account_id_is_valid(parts[4])
    {
        return Err(CallerIdentityError);
    }
    if parts[2] == "iam" && parts[5].starts_with("role/") {
        return role_arn_is_valid(caller_arn, parts[4])
            .then(|| caller_arn.to_owned())
            .ok_or(CallerIdentityError);
    }
    if parts[2] != "sts" {
        return Err(CallerIdentityError);
    }
    let assumed_role = parts[5]
        .strip_prefix("assumed-role/")
        .ok_or(CallerIdentityError)?;
    let (role, session) = assumed_role.split_once('/').ok_or(CallerIdentityError)?;
    if role.is_empty() || session.is_empty() || session.contains('/') {
        return Err(CallerIdentityError);
    }
    let role_arn = format!("arn:{}:iam::{}:role/{role}", parts[1], parts[4]);
    role_arn_is_valid(&role_arn, parts[4])
        .then_some(role_arn)
        .ok_or(CallerIdentityError)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardFailureCode {
    AccountInvalid,
    RegionInvalid,
    EnvironmentInvalid,
    RoleInvalid,
    ReleaseInvalid,
    ConfigurationInvalid,
    MigrationInvalid,
    OperatorApprovalInvalid,
    AccountMismatch,
    RegionMismatch,
    EnvironmentMismatch,
    RoleMismatch,
    ReleaseMismatch,
    ReleaseUnverified,
    ConfigurationMismatch,
    DriftUnproved,
    MigrationUnproved,
    SourceDirty,
    OperatorApprovalMissing,
    OperatorApprovalMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("deployment guards failed: {codes:?}")]
pub struct GuardFailure {
    codes: Vec<GuardFailureCode>,
}

impl GuardFailure {
    pub fn codes(&self) -> Vec<GuardFailureCode> {
        self.codes.clone()
    }
}

pub fn verify_guards(
    expected: &EnvironmentExpectation,
    observed: &EnvironmentObservation,
) -> Result<VerifiedEnvironment, GuardFailure> {
    let mut codes = Vec::new();
    if !account_id_is_valid(&expected.account_id) || !account_id_is_valid(&observed.account_id) {
        codes.push(GuardFailureCode::AccountInvalid);
    }
    if !region_is_valid(&expected.region) || !region_is_valid(&observed.region) {
        codes.push(GuardFailureCode::RegionInvalid);
    }
    if !environment_is_valid(&expected.environment) || !environment_is_valid(&observed.environment)
    {
        codes.push(GuardFailureCode::EnvironmentInvalid);
    }
    if !role_arn_is_valid(&expected.role_arn, &expected.account_id)
        || !role_arn_is_valid(&observed.role_arn, &observed.account_id)
    {
        codes.push(GuardFailureCode::RoleInvalid);
    }
    if !release_identity_is_valid(&expected.release_id, &expected.release_digest)
        || !release_identity_is_valid(&observed.release_id, &observed.release_digest)
    {
        codes.push(GuardFailureCode::ReleaseInvalid);
    }
    if !sha256_is_valid(&expected.configuration_digest)
        || !sha256_is_valid(&observed.configuration_digest)
    {
        codes.push(GuardFailureCode::ConfigurationInvalid);
    }
    if expected
        .migration_plan_digest
        .as_deref()
        .is_some_and(|digest| !sha256_is_valid(digest))
        || matches!(
            &observed.migration,
            MigrationState::Verified { plan_digest } if !sha256_is_valid(plan_digest)
        )
    {
        codes.push(GuardFailureCode::MigrationInvalid);
    }
    if observed
        .operator_approval_digest
        .as_deref()
        .is_some_and(|digest| !sha256_is_valid(digest))
    {
        codes.push(GuardFailureCode::OperatorApprovalInvalid);
    }
    if observed.account_id != expected.account_id {
        codes.push(GuardFailureCode::AccountMismatch);
    }
    if observed.region != expected.region {
        codes.push(GuardFailureCode::RegionMismatch);
    }
    if observed.environment != expected.environment {
        codes.push(GuardFailureCode::EnvironmentMismatch);
    }
    if observed.role_arn != expected.role_arn {
        codes.push(GuardFailureCode::RoleMismatch);
    }
    if observed.release_id != expected.release_id
        || observed.release_digest != expected.release_digest
    {
        codes.push(GuardFailureCode::ReleaseMismatch);
    }
    if !observed.release_verified {
        codes.push(GuardFailureCode::ReleaseUnverified);
    }
    if observed.configuration_digest != expected.configuration_digest {
        codes.push(GuardFailureCode::ConfigurationMismatch);
    }
    if observed.drift != DriftState::Clean {
        codes.push(GuardFailureCode::DriftUnproved);
    }
    let migration_matches = match (
        expected.migration_plan_digest.as_deref(),
        &observed.migration,
    ) {
        (None, MigrationState::NotRequired) => true,
        (Some(expected), MigrationState::Verified { plan_digest }) => plan_digest == expected,
        _ => false,
    };
    if !migration_matches {
        codes.push(GuardFailureCode::MigrationUnproved);
    }
    if observed.source != SourceState::Clean {
        codes.push(GuardFailureCode::SourceDirty);
    }
    match observed.operator_approval_digest.as_deref() {
        None => codes.push(GuardFailureCode::OperatorApprovalMissing),
        Some(digest) if digest != expected.release_digest => {
            codes.push(GuardFailureCode::OperatorApprovalMismatch);
        }
        Some(_) => {}
    }

    if !codes.is_empty() {
        return Err(GuardFailure { codes });
    }

    Ok(VerifiedEnvironment {
        account_id: observed.account_id.clone(),
        region: observed.region.clone(),
        environment: observed.environment.clone(),
        role_arn: observed.role_arn.clone(),
        release_id: observed.release_id.clone(),
        release_digest: observed.release_digest.clone(),
    })
}

fn account_id_is_valid(value: &str) -> bool {
    value.len() == 12 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn region_is_valid(value: &str) -> bool {
    let mut segments = value.split('-');
    let Some(first) = segments.next() else {
        return false;
    };
    let remaining = segments.collect::<Vec<_>>();
    !first.is_empty()
        && first.bytes().all(|byte| byte.is_ascii_lowercase())
        && remaining.len() >= 2
        && remaining[..remaining.len() - 1].iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
        && remaining.last().is_some_and(|segment| {
            !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit())
        })
}

fn environment_is_valid(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= 64
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn role_arn_is_valid(value: &str, account_id: &str) -> bool {
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    parts.len() == 6
        && parts[0] == "arn"
        && parts[1].starts_with("aws")
        && parts[2] == "iam"
        && parts[3].is_empty()
        && parts[4] == account_id
        && parts[5].strip_prefix("role/").is_some_and(|role| {
            !role.is_empty()
                && role.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'/' | b'+' | b'=' | b',' | b'.' | b'@' | b'_' | b'-')
                })
        })
}

fn release_identity_is_valid(release_id: &str, release_digest: &str) -> bool {
    sha256_is_valid(release_digest) && release_id == format!("minco.{}", &release_digest[..24])
}

fn sha256_is_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentTargetCatalog {
    pub schema_version: u32,
    pub default_environment: String,
    pub environments: BTreeMap<String, DeploymentTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentTarget {
    pub enabled: bool,
    pub expected_account_id: String,
    pub expected_region: String,
    pub expected_role_arn: String,
    pub stack_name: String,
    pub artifact_bucket: String,
    pub database_url_parameter_name: String,
    pub database_kms_key_arn: Option<String>,
    #[serde(default)]
    pub static_site_certificate_arn: Option<String>,
    #[serde(default)]
    pub static_site_hosted_zone_id: Option<String>,
    #[serde(default)]
    pub static_site_pricing_checked_on: Option<String>,
    #[serde(default)]
    pub static_site_pricing_source: Option<String>,
    #[serde(default)]
    pub static_site_billing_model: Option<CloudFrontBillingModel>,
    #[serde(default)]
    pub static_site_flat_rate_eligibility: Option<FlatRateEligibility>,
    #[serde(default)]
    pub lambda_subnet_ids: Vec<String>,
    #[serde(default)]
    pub lambda_security_group_ids: Vec<String>,
    #[serde(default)]
    pub stack_tags: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectedDeploymentTarget {
    pub environment: String,
    pub target: DeploymentTarget,
}

#[derive(Debug, Error)]
pub enum DeploymentTargetError {
    #[error("deployment target configuration is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unsupported deployment target schema {0}")]
    UnsupportedSchema(u32),
    #[error("deployment target catalog has no environment {0}")]
    UnknownEnvironment(String),
    #[error("deployment target {environment} has invalid {field}")]
    InvalidField {
        environment: String,
        field: &'static str,
    },
    #[error("enabled deployment target {0} still uses a disabled placeholder account")]
    EnabledPlaceholder(String),
}

impl DeploymentTargetCatalog {
    pub fn from_toml(source: &str) -> Result<Self, DeploymentTargetError> {
        let catalog: Self = toml::from_str(source)?;
        if catalog.schema_version != 1 {
            return Err(DeploymentTargetError::UnsupportedSchema(
                catalog.schema_version,
            ));
        }
        if !environment_is_valid(&catalog.default_environment) {
            return Err(DeploymentTargetError::InvalidField {
                environment: catalog.default_environment,
                field: "default_environment",
            });
        }
        for (environment, target) in &catalog.environments {
            validate_deployment_target(environment, target)?;
        }
        if !catalog
            .environments
            .contains_key(&catalog.default_environment)
        {
            return Err(DeploymentTargetError::UnknownEnvironment(
                catalog.default_environment,
            ));
        }
        Ok(catalog)
    }

    pub fn select(
        &self,
        environment: Option<&str>,
    ) -> Result<SelectedDeploymentTarget, DeploymentTargetError> {
        let environment = environment.unwrap_or(&self.default_environment);
        let target = self
            .environments
            .get(environment)
            .ok_or_else(|| DeploymentTargetError::UnknownEnvironment(environment.to_owned()))?;
        Ok(SelectedDeploymentTarget {
            environment: environment.to_owned(),
            target: target.clone(),
        })
    }
}

fn validate_deployment_target(
    environment: &str,
    target: &DeploymentTarget,
) -> Result<(), DeploymentTargetError> {
    let valid = [
        ("environment", environment_is_valid(environment)),
        (
            "expected_account_id",
            account_id_is_valid(&target.expected_account_id),
        ),
        ("expected_region", region_is_valid(&target.expected_region)),
        (
            "expected_role_arn",
            role_arn_is_valid(&target.expected_role_arn, &target.expected_account_id),
        ),
        ("stack_name", stack_name_is_valid(&target.stack_name)),
        (
            "artifact_bucket",
            bucket_name_is_valid(&target.artifact_bucket),
        ),
        (
            "database_url_parameter_name",
            parameter_name_is_valid(&target.database_url_parameter_name),
        ),
        (
            "database_kms_key_arn",
            target.database_kms_key_arn.as_deref().is_none_or(|arn| {
                kms_key_arn_is_valid(
                    arn,
                    &target.expected_account_id,
                    &target.expected_region,
                    &target.expected_role_arn,
                )
            }),
        ),
        (
            "static_site_certificate_arn",
            target
                .static_site_certificate_arn
                .as_deref()
                .is_none_or(|arn| {
                    cloudfront_certificate_arn_is_valid(arn, &target.expected_account_id)
                }),
        ),
        (
            "static_site_hosted_zone_id",
            target
                .static_site_hosted_zone_id
                .as_deref()
                .is_none_or(hosted_zone_id_is_valid),
        ),
        (
            "static_site_pricing",
            static_site_target_pricing_is_valid(target),
        ),
        (
            "lambda_network",
            target.lambda_subnet_ids.is_empty() == target.lambda_security_group_ids.is_empty()
                && resource_ids_are_valid(&target.lambda_subnet_ids, "subnet-")
                && resource_ids_are_valid(&target.lambda_security_group_ids, "sg-"),
        ),
        (
            "stack_tags",
            deployment_stack_tags_are_valid(&target.stack_tags),
        ),
    ];
    if let Some((field, _)) = valid.into_iter().find(|(_, valid)| !valid) {
        return Err(DeploymentTargetError::InvalidField {
            environment: environment.to_owned(),
            field,
        });
    }
    if target.enabled && target.expected_account_id == "000000000000" {
        return Err(DeploymentTargetError::EnabledPlaceholder(
            environment.to_owned(),
        ));
    }
    Ok(())
}

fn deployment_stack_tags_are_valid(tags: &BTreeMap<String, String>) -> bool {
    tags.len() <= 47
        && tags.iter().all(|(key, value)| {
            !key.is_empty()
                && key.len() <= 128
                && value.len() <= 256
                && key.chars().all(|character| !character.is_control())
                && value.chars().all(|character| !character.is_control())
                && !key.to_ascii_lowercase().starts_with("aws:")
                && !matches!(
                    key.as_str(),
                    "MincoEnvironment" | "MincoReleaseId" | "MincoReleaseDigest"
                )
        })
}

fn stack_name_is_valid(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= 128
        && bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn hosted_zone_id_is_valid(value: &str) -> bool {
    value.len() >= 8
        && value.starts_with('Z')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn static_site_target_pricing_is_valid(target: &DeploymentTarget) -> bool {
    let values_present = [
        target.static_site_pricing_checked_on.is_some(),
        target.static_site_pricing_source.is_some(),
        target.static_site_billing_model.is_some(),
        target.static_site_flat_rate_eligibility.is_some(),
    ];
    if values_present.iter().all(|present| !present) {
        return true;
    }
    if !values_present.iter().all(|present| *present) {
        return false;
    }
    let source = target
        .static_site_pricing_source
        .as_deref()
        .and_then(|source| url::Url::parse(source).ok());
    let source_is_official = source.is_some_and(|source| {
        source.scheme() == "https"
            && matches!(
                source.host_str(),
                Some("aws.amazon.com" | "docs.aws.amazon.com")
            )
            && source.username().is_empty()
            && source.password().is_none()
    });
    let selection_is_consistent = match (
        target.static_site_billing_model,
        target.static_site_flat_rate_eligibility,
    ) {
        (Some(CloudFrontBillingModel::FlatRate), Some(FlatRateEligibility::EligibleSelected)) => {
            true
        }
        (Some(CloudFrontBillingModel::RequestAndTransfer), Some(eligibility)) => {
            eligibility != FlatRateEligibility::EligibleSelected
        }
        _ => false,
    };
    target
        .static_site_pricing_checked_on
        .as_deref()
        .is_some_and(iso_date_is_valid)
        && source_is_official
        && selection_is_consistent
}

fn bucket_name_is_valid(value: &str) -> bool {
    (3..=63).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !value.contains("..")
        && value.parse::<std::net::Ipv4Addr>().is_err()
}

fn parameter_name_is_valid(value: &str) -> bool {
    (2..=1011).contains(&value.len())
        && value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("//")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_'))
}

fn kms_key_arn_is_valid(value: &str, account_id: &str, region: &str, role_arn: &str) -> bool {
    let parts = value.splitn(6, ':').collect::<Vec<_>>();
    let partition = role_arn.split(':').nth(1);
    parts.len() == 6
        && parts[0] == "arn"
        && Some(parts[1]) == partition
        && parts[2] == "kms"
        && parts[3] == region
        && parts[4] == account_id
        && parts[5].strip_prefix("key/").is_some_and(|key_id| {
            !key_id.is_empty()
                && key_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn resource_ids_are_valid(values: &[String], prefix: &str) -> bool {
    let unique = values.iter().collect::<BTreeSet<_>>();
    unique.len() == values.len()
        && values.iter().all(|value| {
            value.strip_prefix(prefix).is_some_and(|id| {
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
        })
}
