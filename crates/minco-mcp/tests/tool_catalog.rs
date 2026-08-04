use minco_mcp::MincoMcp;
use minco_project_view::{NodeKind, load_project_view};
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use std::path::Path;

fn packaged_project_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project")
        .canonicalize()
        .expect("canonical packaged project fixture")
}

#[test]
fn exposes_only_the_bounded_read_only_project_tools() {
    let tools = MincoMcp::tool_catalog();
    let names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "minco.evidence",
            "minco.feedback_context",
            "minco.operation_explain",
            "minco.project_summary",
            "minco.project_view",
            "minco.task_readiness",
        ]
    );

    for tool in tools {
        let annotations = tool.annotations.expect("read-only annotations");
        assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
        assert_eq!(annotations.destructive_hint, Some(false), "{}", tool.name);
        assert_eq!(annotations.idempotent_hint, Some(true), "{}", tool.name);
        assert_eq!(annotations.open_world_hint, Some(false), "{}", tool.name);

        let schema = serde_json::to_string(&tool.input_schema).expect("input schema JSON");
        for forbidden in ["path", "command", "shell", "sql", "url"] {
            assert!(
                !schema.to_ascii_lowercase().contains(forbidden),
                "{} accepts forbidden input category {forbidden}: {schema}",
                tool.name
            );
        }
    }
}

#[tokio::test]
async fn serves_schema_versioned_structured_results_over_the_mcp_transport() {
    let root = packaged_project_fixture();
    let view = load_project_view(&root).expect("packaged fixture ProjectView");
    let operation_id = view
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Operation)
        .and_then(|node| node.properties.get("operation_id"))
        .and_then(serde_json::Value::as_str)
        .expect("declared operation")
        .to_owned();
    let root_text = root.to_string_lossy().into_owned();

    let (server_transport, client_transport) = tokio::io::duplex(4 * 1024 * 1024);
    let server = tokio::spawn(async move {
        let service = MincoMcp::new(view)
            .serve(server_transport)
            .await
            .expect("serve MCP");
        service.waiting().await.expect("MCP server completion");
    });
    let client = ().serve(client_transport).await.expect("connect MCP client");

    assert_eq!(client.list_all_tools().await.expect("list tools").len(), 6);

    let summary_result = client
        .call_tool(CallToolRequestParams::new("minco.project_summary"))
        .await
        .expect("project summary call");
    assert!(
        serde_json::to_vec(&summary_result)
            .expect("MCP result JSON")
            .len()
            + minco_mcp::DEFAULT_MAX_MCP_MESSAGE_BYTES
            <= 2 * 1024 * 1024
    );
    let summary = summary_result
        .structured_content
        .expect("structured project summary");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["kind"], "project_summary");
    assert_eq!(summary["data"]["project"]["name"], "fixture");
    assert!(!summary.to_string().contains(&root_text));

    let operation = client
        .call_tool(
            CallToolRequestParams::new("minco.operation_explain").with_arguments(
                serde_json::from_value(json!({ "operation_id": operation_id }))
                    .expect("operation arguments"),
            ),
        )
        .await
        .expect("operation explanation call")
        .structured_content
        .expect("structured operation explanation");
    assert_eq!(operation["schema_version"], 1);
    assert_eq!(operation["data"]["found"], true);

    let injection = client
        .call_tool(
            CallToolRequestParams::new("minco.project_view").with_arguments(
                serde_json::from_value(json!({ "path": "/tmp/outside" }))
                    .expect("injected arguments"),
            ),
        )
        .await
        .expect("invalid tool input is returned as a bounded MCP tool error");
    assert_eq!(injection.is_error, Some(true));
    assert!(injection.structured_content.is_none());

    client.cancel().await.expect("cancel MCP client");
    server.await.expect("join MCP server");
}

#[tokio::test]
async fn enforces_the_project_view_response_limit_at_every_tool_boundary() {
    let root = packaged_project_fixture();
    let mut view = load_project_view(&root).expect("packaged fixture ProjectView");
    view.limits.max_response_bytes = 32;

    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        let service = MincoMcp::new(view)
            .serve(server_transport)
            .await
            .expect("serve MCP");
        service.waiting().await.expect("MCP server completion");
    });
    let client = ().serve(client_transport).await.expect("connect MCP client");

    let error = client
        .call_tool(CallToolRequestParams::new("minco.project_summary"))
        .await
        .expect_err("oversized response must fail closed");

    assert!(!error.to_string().contains(root.to_string_lossy().as_ref()));
    client.cancel().await.expect("cancel MCP client");
    server.await.expect("join MCP server");
}
