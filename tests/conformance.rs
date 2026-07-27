//! End-to-end conformance tests against the bundled `examples/` fixtures.

use metalcraft_flows::{validate, walk_bfs, CoreNodeType, FlowNodeType, SavedFlow};
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn read_example(name: &str) -> String {
    std::fs::read_to_string(examples_dir().join(name))
        .unwrap_or_else(|e| panic!("failed to read example {name}: {e}"))
}

#[test]
fn linear_task_worker_parses_validates_round_trips() {
    let raw = read_example("linear_task_worker.json");
    let parsed: SavedFlow = serde_json::from_str(&raw).expect("should parse");

    assert_eq!(parsed.spec_version, "1");
    assert_eq!(parsed.id, "template-linear-task-worker");
    assert_eq!(parsed.flow.nodes.len(), 2);
    assert_eq!(parsed.flow.edges.len(), 1);

    // Validate.
    let errs = validate(&parsed);
    assert!(errs.is_empty(), "expected no validation errors, got: {errs:?}");

    // Round-trip.
    let reserialized = serde_json::to_string(&parsed).expect("serialize");
    let again: SavedFlow = serde_json::from_str(&reserialized).expect("re-parse");
    assert_eq!(parsed, again);

    // Walk reaches both nodes.
    let mut visited: Vec<String> = vec![];
    walk_bfs(&parsed.flow, |n| visited.push(n.id.clone()));
    assert_eq!(visited, vec!["entry", "task-worker"]);

    // The entry node is a Core::Entry.
    let entry = parsed
        .flow
        .nodes
        .iter()
        .find(|n| n.id == "entry")
        .unwrap();
    assert_eq!(entry.node_type, FlowNodeType::Core(CoreNodeType::Entry));
}

#[test]
fn missing_spec_version_defaults_and_validates() {
    let raw = r#"{
        "id": "no-version",
        "name": "No Version",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "flow": {
            "nodes": [
                {"id": "e", "node_type": "entry", "data": {}, "position": [0,0]}
            ],
            "edges": []
        }
    }"#;
    let parsed: SavedFlow = serde_json::from_str(raw).expect("should parse without spec_version");
    assert_eq!(parsed.spec_version, "1");
    assert!(validate(&parsed).is_empty());
}

#[test]
fn vendor_node_type_preserved_through_round_trip() {
    let raw = r##"{
        "id": "vendor-flow",
        "name": "Vendor",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "flow": {
            "nodes": [
                {"id": "e", "node_type": "entry", "data": {}},
                {"id": "s", "node_type": "slack:send_message", "data": {"channel": "#ops"}}
            ],
            "edges": [
                {"id": "x", "source": "e", "target": "s"}
            ]
        }
    }"##;
    let parsed: SavedFlow = serde_json::from_str(raw).expect("parse");
    let slack = parsed.flow.nodes.iter().find(|n| n.id == "s").unwrap();
    assert_eq!(
        slack.node_type,
        FlowNodeType::Custom("slack:send_message".into())
    );
    let reserialized = serde_json::to_string(&parsed).unwrap();
    assert!(reserialized.contains("slack:send_message"));
    assert!(reserialized.contains("\"channel\":\"#ops\""));
}

// --- v2: the Madrid weather user story (deterministic routing) ---------------

use metalcraft_flows::{evaluate, next_by_handle, nodes::ConditionalData, Operator, Variables};

/// Given the `conditional` node's data and a `_last` value, reproduce the
/// runtime's routing decision using only the crate's public helpers, and return
/// the id of the node it would advance to.
fn route_conditional(flow: &SavedFlow, node_id: &str, last: serde_json::Value) -> String {
    let node = flow.flow.nodes.iter().find(|n| n.id == node_id).unwrap();
    let data: ConditionalData = serde_json::from_value(node.data.clone()).unwrap();

    let mut vars = Variables::new();
    vars.set_last(last);

    let handle = data
        .conditions
        .iter()
        .find(|c| {
            let op = Operator::from_wire(&c.operator).expect("known operator");
            evaluate(op, vars.get(&c.variable), c.value.as_ref())
        })
        .map(|c| c.handle.clone())
        .or(data.default_handle.clone());

    next_by_handle(&flow.flow, node_id, handle.as_deref())
        .unwrap_or_else(|| panic!("no route for handle {handle:?}"))
}

#[test]
fn madrid_weather_parses_and_validates_as_v2() {
    let raw = read_example("madrid_weather.json");
    let flow: SavedFlow = serde_json::from_str(&raw).expect("should parse");
    assert_eq!(flow.spec_version, "2");

    let errs = validate(&flow);
    assert!(errs.is_empty(), "expected no validation errors, got: {errs:?}");

    // The classifier node is a Core::Branch with two typed outputs.
    let get_temp = flow.flow.nodes.iter().find(|n| n.id == "get_temp").unwrap();
    assert_eq!(get_temp.node_type, FlowNodeType::Core(CoreNodeType::Branch));

    // Round-trips losslessly.
    let again: SavedFlow =
        serde_json::from_str(&serde_json::to_string(&flow).unwrap()).unwrap();
    assert_eq!(flow, again);
}

#[test]
fn madrid_weather_routes_by_branch_and_conditional() {
    let raw = read_example("madrid_weather.json");
    let flow: SavedFlow = serde_json::from_str(&raw).expect("parse");

    // 1. The branch's typed handles route to the right successors.
    assert_eq!(
        next_by_handle(&flow.flow, "get_temp", Some("report_temp")).as_deref(),
        Some("check_hot")
    );
    assert_eq!(
        next_by_handle(&flow.flow, "get_temp", Some("error")).as_deref(),
        Some("handle_err")
    );

    // 2. The conditional compares the incoming i64 payload NUMERICALLY.
    //    A cold day (18°F) must NOT satisfy `_last > 50`.
    assert_eq!(route_conditional(&flow, "check_hot", serde_json::json!(18)), "say_cold");
    //    A warm day (75°F) must satisfy it.
    assert_eq!(route_conditional(&flow, "check_hot", serde_json::json!(75)), "say_hot");
    //    Regression guard against the vix lexicographic bug: as strings,
    //    "18" > "50" would be true and wrongly route to say_hot.
    assert_eq!(route_conditional(&flow, "check_hot", serde_json::json!("18")), "say_cold");
}
