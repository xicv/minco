use minco_deploy_aws::{
    HostedCheckKind, HostedCheckResult, HostedVerificationError, HostedVerificationInput,
    HostedVerificationReport,
};

const ARTIFACT_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn passed(kind: HostedCheckKind, request_id: &str) -> HostedCheckResult {
    HostedCheckResult {
        kind,
        passed: true,
        request_id: Some(request_id.into()),
        status_code: Some(if kind == HostedCheckKind::Authentication {
            401
        } else {
            200
        }),
    }
}

const fn artifact_identity() -> HostedCheckResult {
    HostedCheckResult {
        kind: HostedCheckKind::ArtifactIdentity,
        passed: true,
        request_id: None,
        status_code: None,
    }
}

#[test]
fn failed_readiness_cannot_authorize_promotion() {
    let checks = vec![
        passed(HostedCheckKind::Contract, "request-contract"),
        HostedCheckResult {
            kind: HostedCheckKind::Readiness,
            passed: false,
            request_id: Some("request-readiness".into()),
            status_code: Some(503),
        },
        passed(HostedCheckKind::Authentication, "request-authentication"),
        passed(HostedCheckKind::Smoke, "request-smoke"),
        artifact_identity(),
    ];

    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks,
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::RequiredCheckFailed {
            kind: HostedCheckKind::Readiness,
        })
    );
}

#[test]
fn every_required_hosted_check_must_be_present() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::MissingRequiredCheck {
            kind: HostedCheckKind::Smoke,
        })
    );
}

#[test]
fn executed_artifact_must_match_the_verified_release() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: "b".repeat(64),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(result, Err(HostedVerificationError::ArtifactMismatch));
}

#[test]
fn required_hosted_checks_cannot_be_duplicated() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
            passed(HostedCheckKind::Smoke, "request-smoke-again"),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::DuplicateRequiredCheck {
            kind: HostedCheckKind::Smoke,
        })
    );
}

#[test]
fn verification_endpoint_cannot_embed_credentials_or_query_values() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate?token=secret".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::InvalidField { field: "endpoint" })
    );
}

#[test]
fn hosted_http_checks_require_request_ids_and_status_codes() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            HostedCheckResult {
                kind: HostedCheckKind::Readiness,
                passed: true,
                request_id: None,
                status_code: Some(200),
            },
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::InvalidCheck {
            kind: HostedCheckKind::Readiness,
        })
    );
}

#[test]
fn passed_hosted_checks_require_their_exact_expected_status() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            HostedCheckResult {
                kind: HostedCheckKind::Readiness,
                passed: true,
                request_id: Some("request-readiness".into()),
                status_code: Some(503),
            },
            HostedCheckResult {
                kind: HostedCheckKind::Authentication,
                passed: true,
                request_id: Some("request-authentication".into()),
                status_code: Some(401),
            },
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::InvalidCheck {
            kind: HostedCheckKind::Readiness,
        })
    );
}

#[test]
fn hosted_verification_requires_a_published_numeric_version() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "$LATEST".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::InvalidField {
            field: "executed_version",
        })
    );
}

#[test]
fn successful_report_is_immutable_and_round_trips_strictly() {
    let report = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    })
    .expect("successful report");
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("hosted-verification.json");

    report.write_json(&path).expect("write report");
    assert_eq!(
        HostedVerificationReport::read_json(&path, ARTIFACT_DIGEST).expect("read report"),
        report
    );
    report.write_json(&path).expect("idempotent exact rewrite");
}

#[test]
fn artifact_digests_must_be_lowercase_sha256_values() {
    let result = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: "not-a-sha256".into(),
        executed_artifact_digest: "not-a-sha256".into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    });

    assert_eq!(
        result,
        Err(HostedVerificationError::InvalidField {
            field: "artifact_digest",
        })
    );
}

#[test]
fn persisted_reports_reject_unknown_fields_at_every_level() {
    let report = HostedVerificationReport::complete(HostedVerificationInput {
        endpoint: "https://api.example.test/candidate".into(),
        expected_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_artifact_digest: ARTIFACT_DIGEST.into(),
        executed_version: "42".into(),
        checks: vec![
            passed(HostedCheckKind::Contract, "request-contract"),
            passed(HostedCheckKind::Readiness, "request-readiness"),
            passed(HostedCheckKind::Authentication, "request-authentication"),
            passed(HostedCheckKind::Smoke, "request-smoke"),
            artifact_identity(),
        ],
    })
    .expect("successful report");
    let directory = tempfile::tempdir().expect("temporary directory");
    let root_unknown = directory.path().join("root-unknown.json");
    let nested_unknown = directory.path().join("nested-unknown.json");
    let mut value = serde_json::to_value(&report).expect("serialize report");
    value
        .as_object_mut()
        .expect("report object")
        .insert("authorization".into(), serde_json::json!("secret"));
    std::fs::write(
        &root_unknown,
        serde_json::to_vec(&value).expect("serialize root mutation"),
    )
    .expect("write root mutation");

    let mut value = serde_json::to_value(&report).expect("serialize report");
    value["checks"][0]["body"] = serde_json::json!("secret");
    std::fs::write(
        &nested_unknown,
        serde_json::to_vec(&value).expect("serialize nested mutation"),
    )
    .expect("write nested mutation");

    assert!(matches!(
        HostedVerificationReport::read_json(&root_unknown, ARTIFACT_DIGEST),
        Err(HostedVerificationError::Serialization(_))
    ));
    assert!(matches!(
        HostedVerificationReport::read_json(&nested_unknown, ARTIFACT_DIGEST),
        Err(HostedVerificationError::Serialization(_))
    ));
}
