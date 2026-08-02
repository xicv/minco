use async_trait::async_trait;
use minco_plugin_static_site::{
    StaticSiteError, StaticSitePlan, StaticSitePublication, StaticSitePublisher,
    StaticSitePublisherService, StaticSiteReleaseManifest,
};
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn plan() -> StaticSitePlan {
    StaticSitePlan {
        source_directory: "dist".into(),
        index_document: "index.html".into(),
        spa_fallback: true,
        immutable_cache_seconds: 31_536_000,
        html_cache_seconds: 0,
        price_class: "PriceClass_100".into(),
        ipv6_enabled: true,
        custom_domain: None,
        manage_dns_alias: false,
    }
}

#[test]
fn release_manifest_binds_the_exact_published_bytes() {
    let root = tempfile::tempdir().expect("temporary project");
    fs::create_dir_all(root.path().join("dist/assets")).expect("asset directory");
    fs::write(root.path().join("dist/index.html"), b"<h1>Minco</h1>\n").expect("index asset");
    fs::write(
        root.path().join("dist/assets/app.0123abcd.js"),
        b"console.log('minco');\n",
    )
    .expect("fingerprinted asset");

    let first =
        StaticSiteReleaseManifest::build(&plan(), root.path()).expect("build release manifest");
    let second =
        StaticSiteReleaseManifest::build(&plan(), root.path()).expect("rebuild release manifest");

    assert_eq!(first, second);
    assert_eq!(
        first.digest_sha256().expect("first digest"),
        second.digest_sha256().expect("second digest")
    );
    assert_eq!(
        first
            .assets
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>(),
        ["assets/app.0123abcd.js", "index.html"]
    );
    assert_eq!(
        first.assets[0].cache_control,
        "public,max-age=31536000,immutable"
    );
    assert_eq!(first.assets[0].content_type, "text/javascript");
    assert_eq!(
        first.assets[1].cache_control,
        "public,max-age=0,must-revalidate"
    );
    assert_eq!(first.assets[1].content_type, "text/html");
    first.verify_at(root.path()).expect("unchanged assets");

    fs::write(root.path().join("dist/index.html"), b"tampered\n").expect("tamper asset");
    let changed =
        StaticSiteReleaseManifest::build(&plan(), root.path()).expect("changed release manifest");
    assert_ne!(
        first.digest_sha256().expect("original digest"),
        changed.digest_sha256().expect("changed digest")
    );
    let error = first
        .verify_at(root.path())
        .expect_err("changed bytes must fail verification");
    assert!(error.to_string().contains("index.html"));
}

#[derive(Debug)]
struct RecordingPublisher {
    called: Arc<AtomicBool>,
}

#[async_trait]
impl StaticSitePublisher for RecordingPublisher {
    async fn publish_manifest(
        &self,
        _manifest: &StaticSiteReleaseManifest,
        _repository_root: &Path,
    ) -> Result<StaticSitePublication, StaticSiteError> {
        self.called.store(true, Ordering::SeqCst);
        unreachable!("a changed release must fail before the provider is called")
    }
}

#[tokio::test]
async fn changed_release_fails_before_the_provider_is_called() {
    let root = tempfile::tempdir().expect("temporary project");
    fs::create_dir(root.path().join("dist")).expect("asset directory");
    fs::write(root.path().join("dist/index.html"), b"original\n").expect("index asset");
    let manifest =
        StaticSiteReleaseManifest::build(&plan(), root.path()).expect("release manifest");
    fs::write(root.path().join("dist/index.html"), b"changed\n").expect("tamper asset");

    let called = Arc::new(AtomicBool::new(false));
    let service = StaticSitePublisherService::new(Arc::new(RecordingPublisher {
        called: called.clone(),
    }));
    let error = service
        .publish_manifest(&manifest, root.path())
        .await
        .expect_err("changed bytes must block publication");

    assert!(error.to_string().contains("index.html"));
    assert!(!called.load(Ordering::SeqCst));
}

#[test]
fn release_assets_cannot_claim_the_provider_control_prefix() {
    let root = tempfile::tempdir().expect("temporary project");
    fs::create_dir_all(root.path().join("dist/.minco")).expect("reserved directory");
    fs::write(root.path().join("dist/index.html"), b"index\n").expect("index asset");
    fs::write(
        root.path().join("dist/.minco/deployment-lock"),
        b"collision\n",
    )
    .expect("reserved asset");

    let error = StaticSiteReleaseManifest::build(&plan(), root.path())
        .expect_err("provider control prefix must be reserved");
    assert!(error.to_string().contains(".minco"));
}

#[test]
fn release_manifest_structure_rejects_noncanonical_digests_and_metadata() {
    let mut manifest = StaticSiteReleaseManifest {
        schema_version: 1,
        plan: plan(),
        assets: vec![minco_plugin_static_site::StaticSiteAsset {
            path: "index.html".into(),
            bytes: 5,
            sha256: "A".repeat(64),
            content_type: "text/html\r\nX-Injected: true".into(),
            cache_control: "public,max-age=0".into(),
        }],
    };
    assert!(manifest.validate_structure().is_err());

    manifest.assets[0].sha256 = "a".repeat(64);
    manifest.assets[0].content_type = "text/html".into();
    manifest
        .validate_structure()
        .expect("canonical release metadata");
}
