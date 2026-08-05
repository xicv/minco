//! Optional local presentation surfaces for bounded Minco project views.
#![forbid(unsafe_code)]

mod export;
mod server;

use minco_project_view::{
    DerivedSummary, EdgeKind, InputUsage, ProjectIdentity, ProjectView, ViewLimits,
};
use serde::Serialize;
use std::{collections::BTreeMap, fmt::Write as _};

pub use export::{ExportFormat, ExportReport, ExportRequest, WorkbenchError, export_project_view};
pub use server::{bind_loopback, serve_loopback};

pub const WORKBENCH_SCHEMA_VERSION: u32 = 1;

pub(crate) fn render_mermaid(view: &ProjectView) -> String {
    let mut rendered = String::from("flowchart TD\n");
    let mut node_indexes = BTreeMap::new();
    for (index, node) in view.nodes.iter().enumerate() {
        node_indexes.insert(node.id.as_str(), index);
        let label = escape_mermaid_label(&node.label);
        writeln!(&mut rendered, "  n{index}[{label}]").expect("writing to a String is infallible");
    }
    for edge in &view.edges {
        let (Some(from), Some(to)) = (
            node_indexes.get(edge.from.as_str()),
            node_indexes.get(edge.to.as_str()),
        ) else {
            continue;
        };
        writeln!(
            &mut rendered,
            "  n{from} -->|{}| n{to}",
            edge_kind(edge.kind)
        )
        .expect("writing to a String is infallible");
    }
    rendered
}

fn escape_mermaid_label(value: &str) -> String {
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    serde_json::to_string(&escaped).expect("serializing a String is infallible")
}

const fn edge_kind(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Contains => "contains",
        EdgeKind::DependsOn => "depends_on",
        EdgeKind::BelongsTo => "belongs_to",
        EdgeKind::Implements => "implements",
        EdgeKind::Exposes => "exposes",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkbenchCheckReport {
    pub schema_version: u32,
    pub status: &'static str,
    pub mode: &'static str,
    pub read_only: bool,
    pub listening_sockets: usize,
    pub writes: usize,
    pub project: ProjectIdentity,
    pub summary: DerivedSummary,
    pub limits: ViewLimits,
    pub input_usage: InputUsage,
}

#[must_use]
pub fn check_report(view: &ProjectView) -> WorkbenchCheckReport {
    WorkbenchCheckReport {
        schema_version: WORKBENCH_SCHEMA_VERSION,
        status: "ok",
        mode: "check",
        read_only: true,
        listening_sockets: 0,
        writes: 0,
        project: view.project.clone(),
        summary: view.summary.clone(),
        limits: view.limits,
        input_usage: view.input_usage.clone(),
    }
}
