# Flow Specification

**Version:** 1
**Status:** Draft
**Date:** 2026-05-24

A **Flow** is a serializable, human-authored directed graph that describes an
agent workflow. Flows are designed to be:

1. **Editable** in a visual editor (one position per node).
2. **Persistable** as plain JSON on disk or in a database.
3. **Executable** by an external runtime (e.g. `metalcraft`) that interprets
   each node type's `data` payload.

This document defines the on-disk JSON wire format. The Rust reference types in
the [`metalcraft-flows`](https://crates.io/crates/metalcraft-flows) crate
parse and emit this format losslessly.

---

## 1. Document shape (`SavedFlow`)

A Flow saved to disk is a single JSON object:

```json
{
  "spec_version": "1",
  "id": "template-linear-task-worker",
  "name": "Linear Task Worker",
  "created_at": "2026-05-19T00:00:00Z",
  "updated_at": "2026-05-19T00:00:00Z",
  "enabled": false,
  "flow": {
    "nodes": [ ... ],
    "edges": [ ... ]
  }
}
```

| Field          | Type    | Required | Description                                                                  |
| -------------- | ------- | -------- | ---------------------------------------------------------------------------- |
| `spec_version` | string  | no       | Specification version. Defaults to `"1"` when absent. See §6.                |
| `id`           | string  | yes      | Stable identifier. Must match `^[A-Za-z0-9-]{1,64}$` (see §1.1).             |
| `name`         | string  | yes      | Human-readable label.                                                        |
| `created_at`   | string  | yes      | ISO-8601 / RFC-3339 timestamp.                                               |
| `updated_at`   | string  | yes      | ISO-8601 / RFC-3339 timestamp.                                               |
| `enabled`      | boolean | no       | Defaults to `false`. Whether the flow should be executed by a scheduler.     |
| `flow`         | object  | yes      | The `FlowDefinition` — see §2.                                               |

### 1.1 `id` constraints

The `id` is used as a filename in the `fs` storage backend and must therefore
be safe across filesystems:

- Length 1–64 characters.
- Characters `[A-Za-z0-9-]` only.
- Empty strings are rejected.

---

## 2. `FlowDefinition`

```json
{
  "nodes": [ ... ],
  "edges": [ ... ]
}
```

| Field   | Type             | Required | Description           |
| ------- | ---------------- | -------- | --------------------- |
| `nodes` | array of `FlowNode` | yes   | The graph's vertices. |
| `edges` | array of `FlowEdge` | yes   | The graph's arcs.     |

---

## 3. `FlowNode`

```json
{
  "id": "task-worker",
  "node_type": "prompt",
  "data": { "prompt": "You are a..." },
  "position": [250.0, 0.0]
}
```

| Field       | Type             | Required | Description                                                                  |
| ----------- | ---------------- | -------- | ---------------------------------------------------------------------------- |
| `id`        | string           | yes      | Unique within the enclosing `FlowDefinition`.                                |
| `node_type` | string           | yes      | A core or custom node type. See §5.                                          |
| `data`      | object           | yes      | Free-form per-node configuration. Schema is defined per `node_type` in §5.   |
| `position`  | `[number, number]` | no     | `[x, y]` coordinates for visual editors. Defaults to `[0.0, 0.0]`.           |

Node `id`s must be unique within their `FlowDefinition`. Duplicates are a
validation error.

---

## 4. `FlowEdge`

```json
{
  "id": "edge-entry-to-worker",
  "source": "entry",
  "target": "task-worker",
  "source_handle": null,
  "target_handle": null
}
```

| Field           | Type             | Required | Description                                                                 |
| --------------- | ---------------- | -------- | --------------------------------------------------------------------------- |
| `id`            | string           | yes      | Unique within the enclosing `FlowDefinition`.                               |
| `source`        | string           | yes      | The `id` of the source node. Must exist.                                    |
| `target`        | string           | yes      | The `id` of the target node. Must exist.                                    |
| `source_handle` | string \| null   | no       | Named output port on the source node (used by multi-output nodes like Branch). |
| `target_handle` | string \| null   | no       | Named input port on the target node.                                        |

### 4.1 Graph semantics

- Edges are **directed**: traversal follows `source` → `target`.
- Cycles are **allowed**; runtimes must track a visited set to terminate.
- A node may have any number of incoming and outgoing edges.
- Edges pointing at unknown node IDs are a validation error.
- Disconnected nodes (not reachable from the entry node) are **silently
  ignored** by reference traversal, but are not a validation error — visual
  editors may legitimately persist them as scratch nodes during editing.

---

## 5. Node types

### 5.1 Core node types

The spec defines these as a closed set; reference implementations MUST
understand them.

| `node_type`    | `data` schema                                                                                                                                          | Purpose                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `entry`        | `{ "schedule_type": "manual" \| "minutes" \| "hours" \| "cron", "interval"?: number, "cron"?: string }`                                                | Marks the flow's start. At most one per `FlowDefinition` (see §5.3).     |
| `prompt`       | `{ "prompt": string }`                                                                                                                                 | A natural-language instruction to be passed to an LLM agent.             |
| `branch`       | `{ "condition": string }`                                                                                                                              | Splits flow execution. Edges should use `source_handle` of `"true"` / `"false"` (or other named outputs). |
| `branch_tool`  | `{ "tool_name": string, "branches": { [tool_outcome: string]: string } }`                                                                              | Branches based on the outcome of a tool call.                            |

### 5.2 Custom (vendor) node types

Any `node_type` containing a colon is a **custom** type, namespaced by a
vendor prefix:

```
node_type: "slack:send_message"
node_type: "github:open_pr"
node_type: "mycompany:internal_step"
```

Rules:

- The prefix before the first colon is the **vendor namespace**.
- The vendor namespace MUST match `^[a-z][a-z0-9_-]{0,31}$`.
- The portion after the colon is opaque to the spec (vendor-defined).
- Reference parsers MUST accept any well-formed custom `node_type` and
  preserve its `data` payload verbatim.
- Reference runtimes MAY refuse to execute unknown custom node types but
  MUST NOT corrupt or drop them when round-tripping the JSON.

The bare names `entry`, `prompt`, `branch`, `branch_tool` are reserved for
core types and MUST NOT be redefined by vendors.

### 5.3 Entry node rules

- A `FlowDefinition` MAY have zero or one `entry` nodes.
- Flows with zero `entry` nodes are valid as templates / fragments, but
  cannot be executed.
- Flows with two or more `entry` nodes are a validation error.

---

## 6. Versioning

The spec is versioned via the optional top-level `spec_version` field.

- The current version is `"1"`.
- When the field is absent, parsers MUST treat the document as `"1"`.
- Future breaking changes will increment to `"2"` and parsers will be
  expected to read both, or to refuse documents with newer `spec_version`
  values they don't understand.

Additive, non-breaking changes (new optional fields, new core node types)
will be introduced without a version bump and announced in the changelog.

---

## 7. Storage (informative)

The reference `fs` backend stores one `SavedFlow` per file in a directory:

```
flows/
  template-linear-task-worker.json
  my-other-flow.json
```

The filename is `{id}.json`. This is a reference convention only — the spec
does not mandate how flows are stored.

---

## 8. Conformance

A conformant parser:

1. Accepts every example in `examples/` of the
   [`metalcraft-flows`](https://github.com/rust4ai/metalcraft-flows) repo.
2. Round-trips any conformant document via parse → serialize → parse without
   loss.
3. Rejects documents that violate the rules in §1.1, §3, §4, §5.3.
4. Defaults missing optional fields per their documented defaults.
5. Preserves unknown vendor `node_type` strings and their `data` payloads.
