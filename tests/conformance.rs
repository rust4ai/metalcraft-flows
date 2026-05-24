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
