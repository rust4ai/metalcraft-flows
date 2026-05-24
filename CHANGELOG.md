# Changelog

All notable changes to `metalcraft-flows` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-24

### Added

- Initial release of the Flows specification (`SPEC.md`, v1).
- Core types: `FlowDefinition`, `FlowNode`, `FlowEdge`, `SavedFlow`.
- `FlowNodeType` as an open enum (`Core(CoreNodeType) | Custom(String)`) with
  custom serde, supporting third-party node types via `vendor:name` strings.
- Built-in core node types: `entry`, `prompt`, `branch`, `branch_tool`.
- Generic BFS traversal (`walk_bfs`) over a `FlowDefinition`.
- Spec validation (`validate`) for entry-node count, ID format, edge
  references, and reserved namespace rules.
- Optional `fs` feature: directory-backed CRUD (`save_flow`, `load_flow`,
  `list_flows`, `delete_flow`).
- Optional `log` feature: `FlowLogEntry` plus `append_flow_log` /
  `load_flow_logs`.
- Example `examples/linear_task_worker.json`.
- Conformance test suite (`tests/conformance.rs`).
