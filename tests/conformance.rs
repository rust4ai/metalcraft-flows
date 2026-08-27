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

#[test]
fn requires_demo_parses_validates_and_derives() {
    use metalcraft_flows::{check_requirements, derive_requires, AvailablePack, Unmet};

    let raw = read_example("requires_demo.json");
    let flow: SavedFlow = serde_json::from_str(&raw).expect("should parse");
    assert_eq!(flow.spec_version, "2");

    // The declared block is well-formed.
    let errs = validate(&flow);
    assert!(errs.is_empty(), "expected no validation errors, got: {errs:?}");

    // Derivation recovers the tool surface from the graph. It does NOT recover
    // the `cloudflare` pack id here: a bare `tool_name` can't be mapped back to a
    // pack without a registry, so that pack entry was stamped by the authoring
    // host. (A `sub_agent` `data.pack` or a `vendor:` node WOULD be derivable.)
    let declared = flow.requires.clone().unwrap();
    let derived = derive_requires(&flow);
    assert_eq!(declared.tools, derived.tools);
    assert_eq!(derived.tools, vec!["cloudflare_purge_cache".to_string()]);
    assert!(
        derived.packs.is_empty(),
        "tool_name is not registry-mappable in-crate: {:?}",
        derived.packs
    );
    assert_eq!(declared.packs[0].id, "cloudflare");

    // A host with a compatible cloudflare pack satisfies it; one without does not.
    let ok = check_requirements(
        &declared,
        &[AvailablePack {
            id: "cloudflare".into(),
            version: "1.3.1".into(),
            content_sha256: None,
        }],
    );
    assert!(ok.is_empty(), "{ok:?}");

    let missing = check_requirements(&declared, &[]);
    assert!(matches!(missing.as_slice(), [Unmet::MissingPack { id, .. }] if id == "cloudflare"));

    // Round-trips losslessly (including the requires block).
    let again: SavedFlow =
        serde_json::from_str(&serde_json::to_string(&flow).unwrap()).unwrap();
    assert_eq!(flow, again);
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

// ---- v3: the flow and its schedule are two documents -----------------------

#[test]
fn v3_flow_carries_no_scheduling() {
    let raw = read_example("morning_brief.json");
    let parsed: SavedFlow = serde_json::from_str(&raw).expect("should parse");

    assert_eq!(parsed.spec_version, "3");
    assert!(validate(&parsed).is_empty());

    // Nothing in the document says when it runs — including the entry node,
    // which is where v1 kept it.
    let entry = parsed.flow.nodes.iter().find(|n| n.id == "entry").unwrap();
    assert!(entry.data.get("schedule_type").is_none());
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(json.get("schedules").is_none());
    assert!(json.get("enabled").is_none());
}

#[test]
fn scheduled_flow_example_parses_and_validates() {
    let raw = std::fs::read_to_string(examples_dir().join("scheduled/morning_brief.json"))
        .expect("read scheduled example");
    let sf: metalcraft_flows::ScheduledFlow = serde_json::from_str(&raw).expect("should parse");

    assert_eq!(sf.flow_id, "morning-brief");
    assert!(sf.enabled);
    assert!(metalcraft_flows::validate_scheduled(&sf).is_empty());
    assert_eq!(
        sf.schedule.describe(),
        "Cron `0 0 8 * * *` (America/Detroit)"
    );

    // Round-trips without loss.
    let again: metalcraft_flows::ScheduledFlow =
        serde_json::from_str(&serde_json::to_string(&sf).unwrap()).unwrap();
    assert_eq!(sf, again);
}

#[test]
fn a_v1_example_migrates_to_v3_plus_its_schedule() {
    // `linear_task_worker` is the legacy shape: `enabled` on the document and the
    // trigger on the entry node. It is kept in the corpus precisely because that
    // is what a host upgrading from v1 has on disk.
    let raw = read_example("linear_task_worker.json");
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let out = metalcraft_flows::extract(&doc).expect("extract");

    assert!(out.changed);
    assert_eq!(out.flow.spec_version, "3");
    assert!(validate(&out.flow).is_empty());

    assert_eq!(out.schedules.len(), 1);
    let s = &out.schedules[0];
    assert_eq!(s.key, "default");
    // The example ships disabled, so the migrated schedule is off. Migration
    // never starts something that was not already running.
    assert!(!s.enabled);
    assert!(matches!(
        s.schedule.trigger,
        metalcraft_flows::ScheduleTrigger::Minutes { .. }
    ));

    // The entry node keeps everything except the dead scheduling keys.
    let entry = out.flow.flow.nodes.iter().find(|n| n.id == "entry").unwrap();
    assert!(entry.data.get("schedule_type").is_none());
    assert!(entry.data.get("interval").is_none());
}
