//! Fail-closed AWS deployment guards and `CloudFormation` change-set review.
#![forbid(unsafe_code)]

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

pub fn verify_promotion_boundary(
    change_set: &CloudFormationChangeSet,
    expected_stack_name: &str,
    live_stage_logical_id: &str,
) -> Result<(), PromotionBoundaryError> {
    if change_set.change_set_type != ChangeSetType::Update
        || change_set.status != ChangeSetStatus::CreateComplete
        || change_set.execution_status != ExecutionStatus::Available
        || change_set.stack_name != expected_stack_name
    {
        return Err(PromotionBoundaryError::InvalidProviderState);
    }
    let review = &change_set.review;
    let only_live_stage = match review.modifications.as_slice() {
        [change] => {
            change.logical_id == live_stage_logical_id
                && change.resource_type == "AWS::ApiGatewayV2::Stage"
                && change.action == ChangeAction::Modify
                && matches!(change.replacement, None | Some(Replacement::Never))
                && change.policy_action.is_none()
                && change.scope.as_slice() == [ChangeScope::Properties]
        }
        _ => false,
    };
    if !only_live_stage
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
    pub live_stage_logical_id: String,
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
    pub live_stage_logical_id: String,
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
            &input.live_stage_logical_id,
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
            live_stage_logical_id: input.live_stage_logical_id,
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
        let [verification] = deployment.verification() else {
            return Err(PromotionReceiptError::Invalid(
                "deployment must bind exactly one hosted verification report".into(),
            ));
        };
        if verification.kind != "hosted_verification"
            || verification.file != self.hosted_verification
        {
            return Err(PromotionReceiptError::Invalid(
                "deployment hosted verification binding does not match promotion".into(),
            ));
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
            && self.live_stage_logical_id == other.live_stage_logical_id
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
                .live_stage_logical_id
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
            &self.live_stage_logical_id,
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
            live_stage_logical_id: &'a str,
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
            live_stage_logical_id: &self.live_stage_logical_id,
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
    !value.is_empty()
        && value.len() <= 128
        && value
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
