//! Provider-neutral static-site deployment intent for Minco applications.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use minco_core::{
    CapabilityProvision, ConfigurationField, ConfigurationValueKind, IdleCostClass, Plugin,
    PluginContext, PluginDescriptor, PluginError, PluginId, PluginStability, ResourceIntent,
    ResourceKind, WakeSource,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudFrontPriceClass {
    #[default]
    PriceClass100,
    PriceClass200,
    PriceClassAll,
}

impl CloudFrontPriceClass {
    #[must_use]
    pub const fn as_aws_value(self) -> &'static str {
        match self {
            Self::PriceClass100 => "PriceClass_100",
            Self::PriceClass200 => "PriceClass_200",
            Self::PriceClassAll => "PriceClass_All",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteConfig {
    #[serde(default = "default_source_directory")]
    pub source_directory: String,
    #[serde(default = "default_index_document")]
    pub index_document: String,
    #[serde(default = "default_true")]
    pub spa_fallback: bool,
    #[serde(default = "default_immutable_cache_seconds")]
    pub immutable_cache_seconds: u32,
    #[serde(default)]
    pub html_cache_seconds: u32,
    #[serde(default)]
    pub price_class: CloudFrontPriceClass,
    #[serde(default = "default_true")]
    pub ipv6_enabled: bool,
    #[serde(default)]
    pub custom_domain: Option<String>,
    #[serde(default)]
    pub manage_dns_alias: bool,
}

impl Default for StaticSiteConfig {
    fn default() -> Self {
        Self {
            source_directory: default_source_directory(),
            index_document: default_index_document(),
            spa_fallback: true,
            immutable_cache_seconds: default_immutable_cache_seconds(),
            html_cache_seconds: 0,
            price_class: CloudFrontPriceClass::PriceClass100,
            ipv6_enabled: true,
            custom_domain: None,
            manage_dns_alias: false,
        }
    }
}

impl StaticSiteConfig {
    pub fn validate(&self) -> Result<(), StaticSiteError> {
        validate_relative_path("source_directory", &self.source_directory)?;
        validate_relative_path("index_document", &self.index_document)?;
        if self.immutable_cache_seconds > 31_536_000 || self.html_cache_seconds > 86_400 {
            return Err(StaticSiteError::InvalidConfiguration(
                "immutable cache must not exceed one year and HTML cache must not exceed one day"
                    .into(),
            ));
        }
        if self.manage_dns_alias && self.custom_domain.is_none() {
            return Err(StaticSiteError::InvalidConfiguration(
                "manage_dns_alias requires custom_domain".into(),
            ));
        }
        if let Some(domain) = self.custom_domain.as_deref() {
            validate_domain(domain)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSitePlan {
    pub source_directory: String,
    pub index_document: String,
    pub spa_fallback: bool,
    pub immutable_cache_seconds: u32,
    pub html_cache_seconds: u32,
    pub price_class: String,
    pub ipv6_enabled: bool,
    pub custom_domain: Option<String>,
    pub manage_dns_alias: bool,
}

impl From<StaticSiteConfig> for StaticSitePlan {
    fn from(value: StaticSiteConfig) -> Self {
        Self {
            source_directory: value.source_directory,
            index_document: value.index_document,
            spa_fallback: value.spa_fallback,
            immutable_cache_seconds: value.immutable_cache_seconds,
            html_cache_seconds: value.html_cache_seconds,
            price_class: value.price_class.as_aws_value().into(),
            ipv6_enabled: value.ipv6_enabled,
            custom_domain: value.custom_domain,
            manage_dns_alias: value.manage_dns_alias,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSitePublication {
    pub url: String,
    pub release_manifest_digest: String,
    pub assets: Vec<StaticSiteAsset>,
    pub uploaded: usize,
    pub removed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalidation_id: Option<String>,
    pub invalidation_completed: bool,
}

#[async_trait]
pub trait StaticSitePublisher: Send + Sync + std::fmt::Debug {
    async fn publish_manifest(
        &self,
        manifest: &StaticSiteReleaseManifest,
        repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError>;
}

#[derive(Clone)]
pub struct StaticSitePublisherService(pub Arc<dyn StaticSitePublisher>);

impl std::fmt::Debug for StaticSitePublisherService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("StaticSitePublisherService").finish()
    }
}

impl StaticSitePublisherService {
    pub fn new(publisher: Arc<dyn StaticSitePublisher>) -> Self {
        Self(publisher)
    }

    pub async fn publish(
        &self,
        plan: &StaticSitePlan,
        repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        let manifest = StaticSiteReleaseManifest::build(plan, repository_root)?;
        self.publish_manifest(&manifest, repository_root).await
    }

    pub async fn publish_manifest(
        &self,
        manifest: &StaticSiteReleaseManifest,
        repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        manifest.verify_at(repository_root)?;
        self.0.publish_manifest(manifest, repository_root).await
    }
}

impl StaticSitePlan {
    pub fn validate(&self) -> Result<(), StaticSiteError> {
        self.clone().try_into_config()?.validate()
    }

    fn try_into_config(self) -> Result<StaticSiteConfig, StaticSiteError> {
        let price_class = match self.price_class.as_str() {
            "PriceClass_100" => CloudFrontPriceClass::PriceClass100,
            "PriceClass_200" => CloudFrontPriceClass::PriceClass200,
            "PriceClass_All" => CloudFrontPriceClass::PriceClassAll,
            _ => {
                return Err(StaticSiteError::InvalidConfiguration(
                    "price_class is not a supported CloudFront price class".into(),
                ));
            }
        };
        Ok(StaticSiteConfig {
            source_directory: self.source_directory,
            index_document: self.index_document,
            spa_fallback: self.spa_fallback,
            immutable_cache_seconds: self.immutable_cache_seconds,
            html_cache_seconds: self.html_cache_seconds,
            price_class,
            ipv6_enabled: self.ipv6_enabled,
            custom_domain: self.custom_domain,
            manage_dns_alias: self.manage_dns_alias,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteAsset {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub content_type: String,
    pub cache_control: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSiteReleaseManifest {
    pub schema_version: u32,
    pub plan: StaticSitePlan,
    pub assets: Vec<StaticSiteAsset>,
}

impl StaticSiteReleaseManifest {
    pub fn build(plan: &StaticSitePlan, repository_root: &Path) -> Result<Self, StaticSiteError> {
        plan.validate()?;
        let repository_root = fs::canonicalize(repository_root)
            .map_err(|error| manifest_error("repository root", &error))?;
        let source = fs::canonicalize(repository_root.join(&plan.source_directory))
            .map_err(|error| manifest_error("static-site source", &error))?;
        if !source.starts_with(&repository_root) {
            return Err(StaticSiteError::Publish(
                "static-site source resolves outside the repository root".into(),
            ));
        }

        let mut paths = Vec::new();
        collect_release_files(&source, &source, &mut paths)?;
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        if paths
            .iter()
            .any(|(relative, _)| relative == ".minco" || relative.starts_with(".minco/"))
        {
            return Err(StaticSiteError::Publish(
                "static-site assets cannot use the reserved .minco provider-control prefix".into(),
            ));
        }
        if !paths
            .iter()
            .any(|(relative, _)| relative == &plan.index_document)
        {
            return Err(StaticSiteError::Publish(format!(
                "static-site index document {} is missing",
                plan.index_document
            )));
        }

        let assets = paths
            .into_iter()
            .map(|(relative, absolute)| release_asset(plan, relative, &absolute))
            .collect::<Result<Vec<_>, _>>()?;
        let manifest = Self {
            schema_version: 1,
            plan: plan.clone(),
            assets,
        };
        manifest.validate_structure()?;
        Ok(manifest)
    }

    pub fn verify_at(&self, repository_root: &Path) -> Result<(), StaticSiteError> {
        self.validate_structure()?;
        let actual = Self::build(&self.plan, repository_root)?;
        if &actual == self {
            return Ok(());
        }
        let changed = self
            .assets
            .iter()
            .zip(&actual.assets)
            .find_map(|(expected, actual)| (expected != actual).then(|| expected.path.clone()))
            .or_else(|| {
                self.assets
                    .get(actual.assets.len())
                    .or_else(|| actual.assets.get(self.assets.len()))
                    .map(|asset| asset.path.clone())
            })
            .unwrap_or_else(|| "manifest".into());
        Err(StaticSiteError::Publish(format!(
            "static-site release asset {changed} no longer matches the manifest"
        )))
    }

    pub fn validate_structure(&self) -> Result<(), StaticSiteError> {
        if self.schema_version != 1 {
            return Err(StaticSiteError::Publish(format!(
                "unsupported static-site release manifest schema {}",
                self.schema_version
            )));
        }
        self.plan.validate()?;
        if self.assets.is_empty()
            || self
                .assets
                .windows(2)
                .any(|assets| assets[0].path >= assets[1].path)
            || !self
                .assets
                .iter()
                .any(|asset| asset.path == self.plan.index_document)
        {
            return Err(StaticSiteError::Publish(
                "static-site release asset set is empty, unsorted, duplicated, or missing its index"
                    .into(),
            ));
        }
        for asset in &self.assets {
            validate_relative_path("static-site asset path", &asset.path)?;
            if asset.path.len() > 1_024
                || asset.path == ".minco"
                || asset.path.starts_with(".minco/")
                || asset.sha256.len() != 64
                || !asset
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                || !safe_metadata_value(&asset.content_type, 255)
                || !safe_metadata_value(&asset.cache_control, 1_024)
            {
                return Err(StaticSiteError::Publish(format!(
                    "static-site release asset {} has invalid path, digest, or metadata",
                    asset.path
                )));
            }
        }
        Ok(())
    }

    pub fn digest_sha256(&self) -> Result<String, StaticSiteError> {
        self.validate_structure()?;
        let encoded = serde_json::to_vec(self).map_err(|error| {
            StaticSiteError::Publish(format!(
                "static-site release manifest cannot be encoded: {error}"
            ))
        })?;
        Ok(hex::encode(Sha256::digest(encoded)))
    }
}

fn safe_metadata_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.bytes().all(|byte| matches!(byte, b' '..=b'~'))
}

fn collect_release_files(
    source: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), StaticSiteError> {
    let entries = fs::read_dir(directory)
        .map_err(|error| manifest_error("static-site source directory", &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| manifest_error("static-site directory entry", &error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| manifest_error("static-site asset", &error))?;
        if metadata.file_type().is_symlink() {
            return Err(StaticSiteError::Publish(
                "static-site source contains a symlink".into(),
            ));
        }
        if metadata.is_dir() {
            collect_release_files(source, &path, output)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(StaticSiteError::Publish(
                "static-site source contains a non-regular file".into(),
            ));
        }
        let canonical =
            fs::canonicalize(&path).map_err(|error| manifest_error("static-site asset", &error))?;
        if !canonical.starts_with(source) {
            return Err(StaticSiteError::Publish(
                "static-site asset resolves outside the source directory".into(),
            ));
        }
        let relative = canonical
            .strip_prefix(source)
            .map_err(|_| StaticSiteError::Publish("static-site asset prefix changed".into()))?;
        output.push((slash_path(relative)?, canonical));
    }
    Ok(())
}

fn release_asset(
    plan: &StaticSitePlan,
    relative: String,
    absolute: &Path,
) -> Result<StaticSiteAsset, StaticSiteError> {
    let file = fs::File::open(absolute).map_err(|error| manifest_error(&relative, &error))?;
    let bytes = file
        .metadata()
        .map_err(|error| manifest_error(&relative, &error))?
        .len();
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| manifest_error(&relative, &error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(StaticSiteAsset {
        content_type: asset_content_type(&relative).into(),
        cache_control: asset_cache_control(plan, &relative),
        path: relative,
        bytes,
        sha256: hex::encode(hasher.finalize()),
    })
}

fn asset_content_type(relative: &str) -> &'static str {
    let extension = Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("json" | "map" | "webmanifest") => "application/json",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("avif") => "image/avif",
        Some("wasm") => "application/wasm",
        Some("pdf") => "application/pdf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

fn slash_path(path: &Path) -> Result<String, StaticSiteError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_str().ok_or_else(|| {
                StaticSiteError::Publish("static-site asset paths must be valid UTF-8".into())
            })?),
            _ => {
                return Err(StaticSiteError::Publish(
                    "static-site asset path is not relative".into(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(StaticSiteError::Publish(
            "static-site asset path is empty".into(),
        ));
    }
    Ok(parts.join("/"))
}

fn asset_cache_control(plan: &StaticSitePlan, relative: &str) -> String {
    let fingerprinted = relative
        .split(|character: char| !character.is_ascii_hexdigit())
        .any(|token| token.len() >= 8);
    if fingerprinted {
        format!("public,max-age={},immutable", plan.immutable_cache_seconds)
    } else {
        format!("public,max-age={},must-revalidate", plan.html_cache_seconds)
    }
}

fn manifest_error(context: &str, error: &std::io::Error) -> StaticSiteError {
    StaticSiteError::Publish(format!("{context} is unavailable: {error}"))
}

#[derive(Debug)]
pub struct MemoryStaticSitePublisher {
    public_url: String,
    publications: Mutex<Vec<StaticSitePlan>>,
}

impl Default for MemoryStaticSitePublisher {
    fn default() -> Self {
        Self {
            public_url: "http://localhost/static".into(),
            publications: Mutex::new(Vec::new()),
        }
    }
}

impl MemoryStaticSitePublisher {
    pub fn published(&self) -> Result<Vec<StaticSitePlan>, StaticSiteError> {
        Ok(self
            .publications
            .lock()
            .map_err(|_| StaticSiteError::Publish("static-site memory lock was poisoned".into()))?
            .clone())
    }
}

#[async_trait]
impl StaticSitePublisher for MemoryStaticSitePublisher {
    async fn publish_manifest(
        &self,
        manifest: &StaticSiteReleaseManifest,
        _repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        self.publications
            .lock()
            .map_err(|_| StaticSiteError::Publish("static-site memory lock was poisoned".into()))?
            .push(manifest.plan.clone());
        Ok(StaticSitePublication {
            url: self.public_url.clone(),
            release_manifest_digest: manifest.digest_sha256()?,
            assets: manifest.assets.clone(),
            uploaded: 0,
            removed: 0,
            invalidation_id: None,
            invalidation_completed: false,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticSitePlugin {
    publisher: Option<StaticSitePublisherService>,
}

impl StaticSitePlugin {
    #[must_use]
    pub fn with_publisher(mut self, publisher: Arc<dyn StaticSitePublisher>) -> Self {
        self.publisher = Some(StaticSitePublisherService::new(publisher));
        self
    }
}

impl Plugin for StaticSitePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        let mut descriptor = PluginDescriptor::new(
            PluginId::new("static-site").expect("static plugin ID"),
            Version::new(1, 0, 0),
            "Private static assets distributed through a CDN without assuming a frontend framework",
        );
        descriptor.documentation = Some("https://docs.rs/minco-plugin-static-site".into());
        descriptor.core_compatibility =
            VersionReq::parse(concat!("^", env!("CARGO_PKG_VERSION"))).expect("package version");
        descriptor.stability = PluginStability::Beta;
        descriptor.provides.push(CapabilityProvision {
            name: "static-site.publish".into(),
            version: Version::new(1, 0, 0),
        });
        if self.publisher.is_some() {
            descriptor.provides.push(CapabilityProvision {
                name: "static-site.provider".into(),
                version: Version::new(1, 0, 0),
            });
        }
        descriptor.configuration = configuration_fields();
        descriptor
    }

    fn configure_descriptor(
        &self,
        descriptor: &mut PluginDescriptor,
        configuration: Option<&serde_json::Value>,
    ) -> Result<(), PluginError> {
        let configuration = configuration
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let configuration =
            serde_json::from_value::<StaticSiteConfig>(configuration).map_err(|source| {
                PluginError::InvalidConfiguration {
                    plugin: descriptor.id.clone(),
                    source,
                }
            })?;
        configuration
            .validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        descriptor
            .resources
            .extend(static_site_resources(&configuration));
        Ok(())
    }

    fn install(&self, context: &mut PluginContext<'_>) -> Result<(), PluginError> {
        let configuration = context.configuration::<StaticSiteConfig>()?;
        configuration
            .validate()
            .map_err(|error| PluginError::Installation(error.to_string()))?;
        context
            .services()
            .insert(Arc::new(StaticSitePlan::from(configuration)))?;
        if let Some(publisher) = &self.publisher {
            context.services().insert(Arc::new(publisher.clone()))?;
        }
        Ok(())
    }
}

fn static_site_resources(configuration: &StaticSiteConfig) -> Vec<ResourceIntent> {
    let mut resources = vec![
        ResourceIntent {
            id: "static-site-bucket".into(),
            kind: ResourceKind::S3Bucket,
            idle_cost: IdleCostClass::StorageOnly,
            wake_sources: Vec::new(),
            dependencies: Vec::new(),
        },
        ResourceIntent {
            id: "static-site-cdn".into(),
            kind: ResourceKind::CloudFrontDistribution,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: vec![WakeSource::HttpRequest],
            dependencies: vec!["static-site-bucket".into()],
        },
    ];
    if configuration.custom_domain.is_some() {
        resources.push(ResourceIntent {
            id: "static-site-certificate".into(),
            kind: ResourceKind::AcmCertificate,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: Vec::new(),
            dependencies: Vec::new(),
        });
        if let Some(cdn) = resources
            .iter_mut()
            .find(|resource| resource.id == "static-site-cdn")
        {
            cdn.dependencies.push("static-site-certificate".into());
        }
    }
    if configuration.manage_dns_alias {
        resources.push(ResourceIntent {
            id: "static-site-dns".into(),
            kind: ResourceKind::Route53Alias,
            idle_cost: IdleCostClass::ProviderManaged,
            wake_sources: Vec::new(),
            dependencies: vec!["static-site-cdn".into()],
        });
    }
    resources
}

fn configuration_fields() -> Vec<ConfigurationField> {
    vec![
        string_field(
            "source_directory",
            "dist",
            "Directory containing the built static artifact",
        ),
        string_field(
            "index_document",
            "index.html",
            "Default document served at the site root",
        ),
        boolean_field(
            "spa_fallback",
            true,
            "Rewrite missing browser routes to the index document",
        ),
        integer_field(
            "immutable_cache_seconds",
            31_536_000,
            "Cache lifetime for fingerprinted immutable assets",
        ),
        integer_field(
            "html_cache_seconds",
            0,
            "Cache lifetime for HTML entrypoints",
        ),
        string_field(
            "price_class",
            "price_class100",
            "CloudFront price class: price_class100, price_class200, or price_class_all",
        ),
        boolean_field("ipv6_enabled", true, "Enable IPv6 on the CDN distribution"),
        ConfigurationField {
            key: "custom_domain".into(),
            kind: ConfigurationValueKind::String,
            required: false,
            secret: false,
            description: "Optional application hostname".into(),
            default: None,
        },
        boolean_field(
            "manage_dns_alias",
            false,
            "Create a DNS alias for custom_domain in the selected deployment renderer",
        ),
    ]
}

fn string_field(key: &str, default: &str, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::String,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::Value::String(default.into())),
    }
}

fn boolean_field(key: &str, default: bool, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::Boolean,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::Value::Bool(default)),
    }
}

fn integer_field(key: &str, default: u32, description: &str) -> ConfigurationField {
    ConfigurationField {
        key: key.into(),
        kind: ConfigurationValueKind::Integer,
        required: false,
        secret: false,
        description: description.into(),
        default: Some(serde_json::json!(default)),
    }
}

fn validate_relative_path(field: &str, value: &str) -> Result<(), StaticSiteError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.chars().any(char::is_control)
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(StaticSiteError::InvalidConfiguration(format!(
            "{field} must contain only normal relative path components"
        )));
    }
    Ok(())
}

fn validate_domain(value: &str) -> Result<(), StaticSiteError> {
    if value.len() > 253
        || value.starts_with('.')
        || value.ends_with('.')
        || value.split('.').count() < 2
        || value.bytes().any(|byte| byte.is_ascii_uppercase())
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(StaticSiteError::InvalidConfiguration(
            "custom_domain must be a valid lower-ASCII DNS name".into(),
        ));
    }
    Ok(())
}

fn default_source_directory() -> String {
    "dist".into()
}

fn default_index_document() -> String {
    "index.html".into()
}

const fn default_true() -> bool {
    true
}

const fn default_immutable_cache_seconds() -> u32 {
    31_536_000
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StaticSiteError {
    #[error("invalid static-site configuration: {0}")]
    InvalidConfiguration(String),
    #[error("static-site publication failed: {0}")]
    Publish(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_core::{PluginManager, PluginSelection};

    #[test]
    fn default_profile_declares_private_storage_and_cdn() {
        let mut manager = PluginManager::default();
        manager.register(StaticSitePlugin::default()).unwrap();
        let mut selection = PluginSelection::default();
        selection
            .enabled
            .insert(PluginId::new("static-site").unwrap());
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .resources
                .contains_key("static-site-bucket")
        );
        assert!(application.graph.resources.contains_key("static-site-cdn"));
        assert!(!application.graph.resources.contains_key("static-site-dns"));
    }

    #[test]
    fn custom_domain_configuration_changes_the_validated_resource_graph() {
        let mut manager = PluginManager::default();
        manager.register(StaticSitePlugin::default()).unwrap();
        let id = PluginId::new("static-site").unwrap();
        let mut selection = PluginSelection::default();
        selection.enabled.insert(id.clone());
        selection.configuration.insert(
            id,
            serde_json::json!({
                "custom_domain": "app.example.test",
                "manage_dns_alias": true
            }),
        );
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .resources
                .contains_key("static-site-certificate")
        );
        assert!(application.graph.resources.contains_key("static-site-dns"));
    }

    #[test]
    fn dns_management_requires_a_custom_domain() {
        let configuration = StaticSiteConfig {
            manage_dns_alias: true,
            ..StaticSiteConfig::default()
        };
        assert!(configuration.validate().is_err());
    }

    #[test]
    fn provider_neutral_paths_reject_components_the_publisher_cannot_accept() {
        for source_directory in [".", "./dist", "../dist"] {
            let configuration = StaticSiteConfig {
                source_directory: source_directory.into(),
                ..StaticSiteConfig::default()
            };
            assert!(configuration.validate().is_err(), "{source_directory}");
        }
    }

    #[tokio::test]
    async fn publisher_is_an_explicit_injected_service() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("dist")).unwrap();
        fs::write(root.path().join("dist/index.html"), b"<!doctype html>").unwrap();
        let publisher = Arc::new(MemoryStaticSitePublisher::default());
        let mut manager = PluginManager::default();
        manager
            .register(StaticSitePlugin::default().with_publisher(publisher.clone()))
            .unwrap();
        let mut selection = PluginSelection::default();
        selection
            .enabled
            .insert(PluginId::new("static-site").unwrap());
        let application = manager.compose(&selection).unwrap();
        assert!(
            application
                .graph
                .capabilities
                .contains_key("static-site.provider")
        );
        let service = application
            .services
            .get::<StaticSitePublisherService>()
            .unwrap();
        let plan = application.services.get::<StaticSitePlan>().unwrap();
        service.publish(&plan, root.path()).await.unwrap();
        assert_eq!(publisher.published().unwrap().len(), 1);
    }
}
