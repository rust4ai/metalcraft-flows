# Changelog

All notable changes to `metalcraft-flows` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0]

Spec **v2**: flows become a stateful state machine (shared variables, typed edge
payloads, deterministic + LLM-driven routing). v2 is a **superset of v1** —
`spec_version` `"1"` and `"2"` both validate, and a document that omits the field
still defaults to `"1"`. Using any v2 node type requires `spec_version = "2"`.

### Added

- New core node types (v2): `conditional` (deterministic predicate routing),
  `branch` (LLM classifier with typed output handles — reassigned from the v1
  stub), `set_variable`, `tool`, `http`, `sub_agent`, `approval`, `wait`,
  `foreach`, `end`. `branch_tool` retained but **deprecated**.
- `nodes` module: typed `data` views for every core node type (`EntryData`,
  `ConditionalData`/`Condition`, `BranchData`/`BranchOutput`, `PromptData`,
  `SetVariableData`, `ToolData`, `HttpData`, `SubAgentData`, `ApprovalData`,
  `WaitData`, `EndData`, `InputSpec`).
- `eval` module: `Operator` + `evaluate` for `conditional` predicates, with
  **numeric coercion** for `gt`/`lt` (fixes lexicographic comparison) and an
  optional regex `matches` operator behind the default `regex` feature.
- `template` module: `{{path}}` interpolation for string fields.
- `state` module: `Variables` bag with dotted-path get/set, `_last`, and
  `seed_from_inputs` for typed entry `inputs`.
- `walk::next_by_handle`: handle-aware single-step routing (prefer matching
  `source_handle`, fall back to the unlabeled edge).
- Validation: v2-node-in-v1-document, invalid node data, unknown operator, and
  missing-handle-edge checks; `SUPPORTED_SPEC_VERSIONS`, `DEFAULT_SPEC_VERSION`.
- Example `examples/madrid_weather.json` + conformance tests proving branch and
  conditional routing (incl. the numeric-comparison regression guard).

### Changed

- `SPEC_VERSION` is now `"2"` (the version the crate emits); the missing-field
  default remains `"1"` per SPEC §6.
- **MSRV → 1.91**, edition 2024. Let-chains used internally.

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
