use serde::Deserialize;
use serde_yaml_ng::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct Bundle {
    schema_version: u32,
    minco_version: String,
    skills: Vec<BundleSkill>,
    scenarios: String,
}

#[derive(Debug, Deserialize)]
struct BundleSkill {
    name: String,
    path: String,
    mode: String,
    documentation: Vec<String>,
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
        assert!(matches!(
            skill.mode.as_str(),
            "application" | "framework" | "shared"
        ));
        assert!(!skill.documentation.is_empty());
        assert!(
            skill
                .documentation
                .iter()
                .all(|identifier| identifier.starts_with("minco-1.0.0:"))
        );
        for identifier in &skill.documentation {
            let relative = identifier
                .strip_prefix("minco-1.0.0:")
                .expect("versioned documentation identifier");
            assert!(
                Path::new(relative)
                    .components()
                    .all(|component| matches!(component, Component::Normal(_)))
            );
            let documentation = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("docs-site/1.0.0")
                .join(format!("{relative}.md"));
            assert!(
                documentation.is_file(),
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
