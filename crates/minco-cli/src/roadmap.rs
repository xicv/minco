use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Roadmap {
    pub schema: u32,
    pub product: String,
    pub milestones: Vec<Milestone>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Milestone {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub outcome: String,
    #[serde(default)]
    pub exit_criteria: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub milestone: String,
    pub status: String,
    pub priority: String,
    pub area: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
    #[serde(default)]
    pub owned_paths: Vec<String>,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub body: String,
}

pub fn load_roadmap(path: &Path) -> Result<Roadmap> {
    let roadmap: Roadmap = serde_yaml_ng::from_str(&std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?)?;
    if roadmap.schema != 1 {
        bail!("unsupported roadmap schema {}", roadmap.schema);
    }
    Ok(roadmap)
}

pub fn load_tasks(root: &Path) -> Result<Vec<Task>> {
    let mut tasks = Vec::new();
    if !root.is_dir() {
        return Ok(tasks);
    }
    let mut paths = collect_markdown(root)?;
    paths.sort();
    for path in paths {
        let source = std::fs::read_to_string(&path)?;
        let Some(rest) = source.strip_prefix("---\n") else {
            bail!("task {} has no YAML front matter", path.display());
        };
        let Some((front, body)) = rest.split_once("\n---\n") else {
            bail!("task {} has unterminated YAML front matter", path.display());
        };
        let mut task: Task = serde_yaml_ng::from_str(front).with_context(|| format!("parse {}", path.display()))?;
        task.path = path;
        task.body = body.trim().to_owned();
        tasks.push(task);
    }
    Ok(tasks)
}

pub fn ready_tasks(tasks: &[Task]) -> Vec<&Task> {
    let complete = tasks
        .iter()
        .filter(|task| task.status == "complete")
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    tasks
        .iter()
        .filter(|task| task.status == "ready" || task.status == "active")
        .filter(|task| task.depends_on.iter().all(|dependency| complete.contains(dependency.as_str())))
        .collect()
}

pub fn render_roadmap_mermaid(roadmap: &Roadmap) -> String {
    let mut output = String::from("flowchart LR\n");
    for milestone in &roadmap.milestones {
        output.push_str(&format!(
            "    {}[\"{}<br/>{}\"]\n",
            safe_id(&milestone.id),
            escape(&milestone.id),
            escape(&milestone.name)
        ));
    }
    for milestone in &roadmap.milestones {
        for dependency in &milestone.depends_on {
            output.push_str(&format!("    {} --> {}\n", safe_id(dependency), safe_id(&milestone.id)));
        }
    }
    output
}

pub fn render_task_mermaid(tasks: &[Task]) -> String {
    let mut output = String::from("flowchart LR\n");
    for task in tasks {
        output.push_str(&format!(
            "    {}[\"{}<br/>{}\"]\n",
            safe_id(&task.id),
            escape(&task.id),
            escape(&task.title)
        ));
    }
    for task in tasks {
        for dependency in &task.depends_on {
            output.push_str(&format!("    {} --> {}\n", safe_id(dependency), safe_id(&task.id)));
        }
    }
    output
}

pub fn validate_task_graph(tasks: &[Task]) -> Result<()> {
    let by_id = tasks.iter().map(|task| (task.id.as_str(), task)).collect::<BTreeMap<_, _>>();
    if by_id.len() != tasks.len() {
        bail!("duplicate task IDs");
    }
    for task in tasks {
        for dependency in &task.depends_on {
            if !by_id.contains_key(dependency.as_str()) {
                bail!("task {} depends on unknown task {dependency}", task.id);
            }
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for task in tasks {
        visit(task.id.as_str(), &by_id, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit<'a>(
    id: &'a str,
    tasks: &BTreeMap<&'a str, &'a Task>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        bail!("task dependency cycle includes {id}");
    }
    let task = tasks.get(id).context("task disappeared")?;
    for dependency in &task.depends_on {
        visit(dependency, tasks, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn collect_markdown(root: &Path) -> Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            output.extend(collect_markdown(&path)?);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
            output.push(path);
        }
    }
    Ok(output)
}

fn safe_id(value: &str) -> String {
    format!("N{}", value.chars().filter(char::is_ascii_alphanumeric).collect::<String>())
}

fn escape(value: &str) -> String {
    value.replace('"', "&quot;")
}
