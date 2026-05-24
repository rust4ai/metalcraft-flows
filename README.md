# metalcraft-flows

[![crates.io](https://img.shields.io/crates/v/metalcraft-flows.svg)](https://crates.io/crates/metalcraft-flows)
[![docs.rs](https://docs.rs/metalcraft-flows/badge.svg)](https://docs.rs/metalcraft-flows)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A serializable, human-authored DAG format for AI agent workflows.

**Flows** are JSON documents describing a graph of nodes (prompts, branches,
custom steps) connected by edges. They are designed to be edited in a visual
editor, persisted on disk, and executed by an external runtime such as
[`metalcraft`](https://crates.io/crates/metalcraft).

This crate provides:

- The reference Rust types (`FlowDefinition`, `FlowNode`, `FlowEdge`,
  `SavedFlow`) that parse and emit the wire format losslessly.
- A generic BFS walker over a flow graph.
- Validation for spec conformance.
- Optional filesystem-backed CRUD (`fs` feature, on by default).
- Optional flow execution logging (`log` feature).

See [`SPEC.md`](./SPEC.md) for the formal wire-format specification.

## Quick start

```toml
[dependencies]
metalcraft-flows = "0.1"
```

```rust
use metalcraft_flows::{FlowDefinition, FlowNode, FlowEdge, FlowNodeType, CoreNodeType};
use serde_json::json;

let flow = FlowDefinition {
    nodes: vec![
        FlowNode {
            id: "entry".into(),
            node_type: FlowNodeType::Core(CoreNodeType::Entry),
            data: json!({ "schedule_type": "manual" }),
            position: [0.0, 0.0],
        },
        FlowNode {
            id: "say-hello".into(),
            node_type: FlowNodeType::Core(CoreNodeType::Prompt),
            data: json!({ "prompt": "Say hello to the user." }),
            position: [250.0, 0.0],
        },
    ],
    edges: vec![
        FlowEdge {
            id: "e1".into(),
            source: "entry".into(),
            target: "say-hello".into(),
            source_handle: None,
            target_handle: None,
        },
    ],
};

let json = serde_json::to_string_pretty(&flow).unwrap();
println!("{json}");
```

## Custom node types

The spec supports vendor-namespaced custom node types so third-party tooling
can add new step kinds without forking:

```rust
use metalcraft_flows::FlowNodeType;

let nt: FlowNodeType = serde_json::from_str("\"slack:send_message\"").unwrap();
assert!(matches!(nt, FlowNodeType::Custom(_)));
```

See [`SPEC.md` §5.2](./SPEC.md) for the vendor namespace rules.

## Features

| Feature  | Default | Description                                              |
| -------- | ------- | -------------------------------------------------------- |
| `fs`     | yes     | Enables `save_flow`, `load_flow`, `list_flows`, `delete_flow`. |
| `log`    | no      | Enables `FlowLogEntry` plus `append_flow_log` / `load_flow_logs`. |

To depend on the spec types only, opt out of defaults:

```toml
metalcraft-flows = { version = "0.1", default-features = false }
```

## License

MIT — see [LICENSE](./LICENSE).
