use serde::Deserialize;
use serde_yaml_ng::Value;
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

const RELEASE_FEATURE_COVERAGE_BASELINE: &str = "1.2.0";

#[derive(Debug, Deserialize)]
struct Bundle {
    schema_version: u32,
    minco_version: String,
    skills: Vec<BundleSkill>,
    scenarios: String,
    #[serde(default)]
    release_feature_coverage: Option<ReleaseFeatureCoverage>,
}

#[derive(Debug, Deserialize)]
struct BundleSkill {
    name: String,
    path: String,
    mode: String,
    documentation: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReleaseFeatureCoverage {
    schema_version: u32,
    releases: Vec<CoveredRelease>,
    features: Vec<ReleaseFeature>,
}

#[derive(Debug, Deserialize)]
struct CoveredRelease {
    version: String,
    changelog_sha256: String,
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReleaseFeature {
    id: String,
    release_version: String,
    skill_marker: String,
    skills: Vec<String>,
    documentation: Vec<String>,
    release_note_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EvaluationSuite {
    schema_version: u32,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    skill: String,
    kind: String,
    prompt: String,
    required_concepts: Vec<String>,
    forbidden_actions: Vec<String>,
}

fn asset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/agent")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn changelog_release_section<'a>(source: &'a str, version: &str) -> &'a str {
    let marker = format!("## [{version}] - ");
    let start = source.find(&marker).expect("covered changelog release");
    let remaining = &source[start..];
    let end = remaining[marker.len()..]
        .find("\n## [")
        .map_or(remaining.len(), |offset| marker.len() + offset + 1);
    &remaining[..end]
}

fn sha256(source: &str) -> String {
    hex::encode(Sha256::digest(source.as_bytes()))
}

fn release_tuple(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.').map(|part| {
        part.parse::<u64>()
            .expect("covered release is exact semver")
    });
    let value = (
        parts.next().expect("major version"),
        parts.next().expect("minor version"),
        parts.next().expect("patch version"),
    );
    assert!(parts.next().is_none(), "covered release is exact semver");
    value
}

fn regular_file_without_symlinks(base: &Path, relative: &Path) -> bool {
    let mut current = base.to_path_buf();
    let Ok(metadata) = fs::symlink_metadata(&current) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return false;
        };
        current.push(part);
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    current.is_file()
}

#[test]
fn portable_skill_bundle_is_bounded_versioned_and_complete() {
    let root = asset_root();
    let bundle: Bundle = serde_json::from_slice(
        &fs::read(root.join("bundle.json")).expect("versioned agent bundle"),
    )
    .expect("valid agent bundle JSON");

    assert_eq!(bundle.schema_version, 1);
    assert_eq!(bundle.minco_version, env!("CARGO_PKG_VERSION"));

    let expected = [
        "minco-diagnose",
        "minco-framework-task",
        "minco-lifecycle",
        "minco-operation",
        "minco-plugin",
        "minco-release",
        "minco-review",
        "minco-waffo-payments",
        "minco-web-application",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    let actual = bundle
        .skills
        .iter()
        .map(|skill| skill.name.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
    let directories = fs::read_dir(root.join("skills"))
        .expect("skill asset directory")
        .map(|entry| {
            entry
                .expect("skill asset entry")
                .file_name()
                .into_string()
                .expect("UTF-8 skill directory")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(directories, expected);

    for skill in &bundle.skills {
        let version = env!("CARGO_PKG_VERSION");
        let prefix = format!("minco-{version}:");
        assert!(matches!(
            skill.mode.as_str(),
            "application" | "framework" | "shared"
        ));
        assert!(!skill.documentation.is_empty());
        assert!(
            skill
                .documentation
                .iter()
                .all(|identifier| identifier.starts_with(&prefix))
        );
        for identifier in &skill.documentation {
            let relative = identifier
                .strip_prefix(&prefix)
                .expect("versioned documentation identifier");
            assert!(
                Path::new(relative)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
            );
            let documentation_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(format!("docs-site/{version}"));
            assert!(
                regular_file_without_symlinks(
                    &documentation_root,
                    &PathBuf::from(format!("{relative}.md")),
                ),
                "missing documentation {identifier}"
            );
        }

        let skill_root = root.join(&skill.path);
        assert!(
            Path::new(&skill.path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        );
        assert_eq!(
            skill_root.file_name().and_then(|name| name.to_str()),
            Some(skill.name.as_str())
        );
        assert!(
            !fs::symlink_metadata(&skill_root)
                .expect("skill directory metadata")
                .file_type()
                .is_symlink()
        );

        let source = fs::read_to_string(skill_root.join("SKILL.md")).expect("skill instructions");
        assert!(source.lines().count() < 500, "{} is too large", skill.name);
        let rest = source.strip_prefix("---\n").expect("front matter start");
        let (front, body) = rest.split_once("\n---\n").expect("front matter end");
        let metadata: Value = serde_yaml_ng::from_str(front).expect("portable YAML front matter");
        let mapping = metadata.as_mapping().expect("front matter mapping");
        assert_eq!(
            mapping.len(),
            2,
            "{} has non-portable front matter",
            skill.name
        );
        assert_eq!(metadata["name"].as_str(), Some(skill.name.as_str()));
        let description = metadata["description"].as_str().expect("skill description");
        assert!(
            description.contains("Use when"),
            "{} has no trigger text",
            skill.name
        );
        assert!(description.len() <= 1024);

        let openai: Value = serde_yaml_ng::from_slice(
            &fs::read(skill_root.join("agents/openai.yaml")).expect("Codex skill metadata"),
        )
        .expect("valid Codex skill metadata");
        let short_description = openai["interface"]["short_description"]
            .as_str()
            .expect("short skill description");
        assert!((25..=64).contains(&short_description.len()));
        assert!(
            openai["interface"]["default_prompt"]
                .as_str()
                .expect("default skill prompt")
                .contains(&format!("${}", skill.name))
        );
        if skill.name == "minco-release" {
            assert_eq!(
                openai["policy"]["allow_implicit_invocation"].as_bool(),
                Some(false)
            );
        }

        for forbidden in [
            "raw.githubusercontent.com",
            "curl ",
            "wget ",
            "npx add-skill",
            "~/.claude",
            "~/.codex",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} contains {forbidden}",
                skill.name
            );
        }

        let references = skill_root.join("references");
        for entry in fs::read_dir(&references).expect("skill references") {
            let entry = entry.expect("reference entry");
            let metadata = fs::symlink_metadata(entry.path()).expect("reference metadata");
            assert!(metadata.is_file(), "references must be one level deep");
            let name = entry.file_name();
            let name = name.to_str().expect("UTF-8 reference name");
            assert!(
                body.contains(name),
                "{} does not route to {name}",
                skill.name
            );
        }

        if skill.mode != "framework" {
            for framework_only in ["task-start.sh", "task-finish.sh", "scripts/jj/"] {
                assert!(
                    !source.contains(framework_only),
                    "{} leaks framework workflow {framework_only}",
                    skill.name
                );
            }
        }
    }

    let release =
        fs::read_to_string(root.join("skills/minco-release/SKILL.md")).expect("release skill");
    assert!(release.contains("explicit user request"));
    assert!(release.contains("Stop before"));
}

#[test]
fn packaged_waffo_skill_matches_the_projected_bundle() {
    let bundled = asset_root().join("skills/minco-waffo-payments");
    let packaged = repository_root()
        .join("plugins/minco-plugin-payments-waffo/agent/skills/minco-waffo-payments");
    for relative in ["SKILL.md", "agents/openai.yaml", "references/workflow.md"] {
        assert_eq!(
            fs::read(bundled.join(relative)).expect("bundled Waffo skill asset"),
            fs::read(packaged.join(relative)).expect("package-local Waffo skill asset"),
            "Waffo skill copies differ at {relative}"
        );
    }
}

#[test]
fn release_feature_coverage_matches_changelog_and_every_skill() {
    let root = asset_root();
    let bundle: Bundle = serde_json::from_slice(
        &fs::read(root.join("bundle.json")).expect("versioned agent bundle"),
    )
    .expect("valid agent bundle JSON");
    let coverage = bundle
        .release_feature_coverage
        .expect("bundle release feature coverage");
    assert_eq!(coverage.schema_version, 1);

    let known_skills = bundle
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<BTreeSet<_>>();
    let features = coverage
        .features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        features.len(),
        coverage.features.len(),
        "duplicate feature ID"
    );

    let changelog =
        fs::read_to_string(repository_root().join("CHANGELOG.md")).expect("repository changelog");
    let mut releases = BTreeSet::new();
    let mut covered_skills = BTreeSet::new();
    for release in &coverage.releases {
        assert!(
            releases.insert(release.version.as_str()),
            "duplicate release"
        );
        let section = changelog_release_section(&changelog, &release.version);
        assert_eq!(sha256(section), release.changelog_sha256);
        assert!(!release.features.is_empty());

        let release_features = release
            .features
            .iter()
            .map(|id| *features.get(id.as_str()).expect("covered feature ID"))
            .collect::<Vec<_>>();
        assert!(
            release_features
                .iter()
                .all(|feature| feature.release_version == release.version)
        );
        let bullets = section
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        assert!(!bullets.is_empty(), "release has no top-level notes");
        for bullet in bullets {
            assert!(
                release_features.iter().any(|feature| {
                    feature
                        .release_note_markers
                        .iter()
                        .any(|marker| bullet.contains(marker))
                }),
                "uncovered release note: {bullet}"
            );
        }
        for feature in release_features {
            assert!(!feature.skill_marker.trim().is_empty());
            assert!(!feature.skills.is_empty());
            assert!(!feature.documentation.is_empty());
            assert!(!feature.release_note_markers.is_empty());
            for marker in &feature.release_note_markers {
                assert!(section.contains(marker), "missing release marker {marker}");
            }
            for identifier in &feature.documentation {
                let prefix = format!("minco-{}:", bundle.minco_version);
                assert!(
                    identifier.starts_with(&prefix),
                    "feature documentation is not version matched: {identifier}"
                );
                let relative = identifier
                    .strip_prefix(&prefix)
                    .expect("feature documentation prefix");
                let documentation = PathBuf::from(format!("{relative}.md"));
                assert!(
                    regular_file_without_symlinks(
                        &repository_root()
                            .join("docs-site")
                            .join(&bundle.minco_version),
                        &documentation,
                    ),
                    "missing feature documentation: {identifier}"
                );
            }
            for skill in &feature.skills {
                assert!(
                    known_skills.contains(skill.as_str()),
                    "unknown skill {skill}"
                );
                covered_skills.insert(skill.as_str());
                let skill_root = root.join("skills").join(skill);
                let combined = format!(
                    "{}\n{}",
                    fs::read_to_string(skill_root.join("SKILL.md")).expect("skill source"),
                    fs::read_to_string(skill_root.join("references/workflow.md"))
                        .expect("skill workflow")
                )
                .to_lowercase();
                assert!(
                    combined.contains(&feature.skill_marker.to_lowercase()),
                    "{skill} lacks feature marker {}",
                    feature.skill_marker
                );
            }
        }
    }

    let baseline = release_tuple(RELEASE_FEATURE_COVERAGE_BASELINE);
    let current = release_tuple(env!("CARGO_PKG_VERSION"));
    let expected_releases = changelog
        .lines()
        .filter_map(|line| {
            line.strip_prefix("## [")
                .and_then(|remaining| remaining.split_once("] - "))
                .map(|(version, _)| version)
        })
        .filter(|version| {
            let value = release_tuple(version);
            value >= baseline && value <= current
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(releases, expected_releases);
    assert_eq!(covered_skills, known_skills);
}

#[test]
fn scenario_contracts_cover_triggering_and_forbidden_actions() {
    let root = asset_root();
    let bundle: Bundle = serde_json::from_slice(
        &fs::read(root.join("bundle.json")).expect("versioned agent bundle"),
    )
    .expect("valid agent bundle JSON");
    let suite: EvaluationSuite = serde_json::from_slice(
        &fs::read(root.join(&bundle.scenarios)).expect("agent scenario contracts"),
    )
    .expect("valid scenario JSON");

    assert_eq!(suite.schema_version, 1);
    let skill_names = bundle
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();

    for scenario in &suite.scenarios {
        assert!(ids.insert(scenario.id.as_str()), "duplicate scenario ID");
        assert!(skill_names.contains(scenario.skill.as_str()));
        assert!(matches!(scenario.kind.as_str(), "trigger" | "boundary"));
        assert!(!scenario.prompt.trim().is_empty());
        assert!(!scenario.required_concepts.is_empty());
        assert!(!scenario.forbidden_actions.is_empty());
    }

    for skill in skill_names {
        for kind in ["trigger", "boundary"] {
            assert!(
                suite
                    .scenarios
                    .iter()
                    .any(|scenario| scenario.skill == skill && scenario.kind == kind),
                "{skill} lacks a {kind} scenario"
            );
        }
    }
}
