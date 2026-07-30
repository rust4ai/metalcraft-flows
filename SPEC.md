# Flow Specification

**Version:** 2 (supersets v1)
**Status:** Draft
**Date:** 2026-07-27

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

Types marked **(v2)** require `spec_version = "2"` (see §6).

| `node_type`    | `data` schema                                                                                                                                          | Purpose                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `entry`        | `{ "schedule_type": "manual"\|"minutes"\|"hours"\|"cron", "interval"?: number, "cron"?: string, "inputs"?: { [name]: InputSpec } }`                    | Marks the flow's start. At most one per `FlowDefinition` (see §5.3). **(v2)** `inputs` declares typed invocation parameters that seed flow state. |
| `prompt`       | `{ "prompt": string, "persona"?: string, "model"?: string, "output_var"?: string, "output_schema"?: object }`                                          | A natural-language instruction run by an LLM agent. **(v2)** may store its answer and emit `ok`/`error` handles. |
| `conditional`  | **(v2)** `{ "conditions": [{ "handle": string, "variable": string, "operator": Operator, "value"?: any }], "default_handle"?: string }`                | Deterministic routing: first matching predicate's handle wins (§5.5).    |
| `branch`       | **(v2)** `{ "query": string, "outputs": [{ "handle": string, "description"?: string, "schema"?: object, "var"?: string }], "persona"?: string, "default_handle"?: string, "timeout"?: number }` | LLM classifier: the model picks exactly one typed output handle and fills its args, which become that edge's payload (§5.4). |
| `set_variable` | **(v2)** `{ "variable": string, "value"?: any, "from"?: string }`                                                                                      | Assign into flow state (literal/template `value`, or dotted `from` path into `_last`). |
| `tool`         | **(v2)** `{ "tool_name": string, "args"?: object, "output_var"?: string }`                                                                             | Call one registered tool directly (no agent loop). Emits `ok`/`error`.   |
| `http`         | **(v2)** `{ "method": string, "url": string, "headers"?: object, "body"?: any, "output_var"?: string }`                                                | Direct HTTP request. Emits `ok`/`error`.                                 |
| `sub_agent`    | **(v2)** `{ "task": string, "persona"?: string, "tool_set"?: string, "pack"?: string, "output_var"?: string }`                                         | Delegate a subtask to a scoped sub-agent.                               |
| `approval`     | **(v2)** `{ "message": string, "choices"?: [string], "timeout"?: number }`                                                                             | Pause for human input; resume on a decision handle.                     |
| `wait`         | **(v2)** `{ "duration"?: string, "until"?: string }`                                                                                                   | Pause for a durable delay; resume via `after`.                          |
| `foreach`      | **(v2)** `{ "list": string, "item_var": string, "mode": "sequential"\|"concurrent", "body_entry": string }`                                            | Fan out over a list into a sub-body.                                     |
| `end`          | **(v2)** `{ "status"?: string, "outputs"?: object }`                                                                                                   | Explicit terminal; may publish flow outputs.                            |
| `branch_tool`  | **Deprecated (v1).** `{ "tool_name": string, "branches": { [tool_outcome: string]: string } }`                                                         | Superseded by `conditional` + `branch`; retained for round-trip.        |

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

The bare core-type names (`entry`, `prompt`, `conditional`, `branch`,
`set_variable`, `tool`, `http`, `sub_agent`, `approval`, `wait`, `foreach`,
`end`, `branch_tool`) are reserved and MUST NOT be redefined by vendors.

> **Wire-name note (v1 → v2).** In v1, `branch` named an opaque
> `{ "condition": string }` stub. In v2, `branch` is **reassigned** to the LLM
> classifier and the deterministic node is the new `conditional`. The two `data`
> shapes are disjoint; parsers distinguish them by `spec_version`.

### 5.3 Entry node rules

- A `FlowDefinition` MAY have zero or one `entry` nodes.
- Flows with zero `entry` nodes are valid as templates / fragments, but
  cannot be executed.
- Flows with two or more `entry` nodes are a validation error.

### 5.4 State and typed edge payloads (v2)

A run carries a single JSON **state** object (`variables`). It is runtime state —
**not** part of a `SavedFlow` — but the wire format references it:

- `entry.data.inputs` declares typed parameters seeded into state at run start.
- Reserved keys: `_last` (the payload of the edge just traversed into the current
  node — its typed input), `_inputs` (immutable copy of seeded inputs), `_run`
  (reserved metadata).
- String fields (`prompt`, `tool.args`, `http.url`/`body`, `set_variable.value`,
  `approval.message`, …) support `{{path}}` interpolation against `variables`.

**Output handles may be typed.** When a node's chosen handle carries a payload,
that payload becomes the traversed edge's value and is delivered to the target
node as `_last`. For a `branch`, each `outputs[]` entry is a tool definition: the
model selects exactly one handle and fills its `schema`-typed arguments, which
become the payload. `prompt`/`tool`/`http` emit `ok` (result) / `error` handles.

**The reserved `error` handle.** Every executable node shares one failure
convention: on a **protocol failure** it routes an `error` handle whose payload
(`_last`) is a string reason. For `prompt`/`tool`/`http` this is the failure of
the single operation. For `branch` a protocol failure is an LLM/API error, a
timeout, the classifier exhausting its step budget without selecting a handle, or
a chosen handle whose payload does not satisfy its declared `schema` — distinct
from any *semantic* outcome the model deliberately picks. The `error` rail is
always available and **optional to wire**: a runtime routes to it on failure, and
if nothing is wired to it (and, for `branch`, no `default_handle` is set) the run
**fails** rather than reporting success. A `branch` MAY also declare `error` in
its `outputs[]` so the model can select it explicitly; because the rail carries a
string reason, such a declaration's `schema` MUST be omitted or `{"type":
"string"}`. `branch.default_handle` is a legacy fallback that, when set, absorbs
protocol failures in place of the `error` rail.

### 5.5 Conditional operators (v2)

`conditional.conditions[].operator` is one of: `equals`, `not_equals`,
`contains`, `starts_with`, `ends_with`, `gt`, `lt`, `exists`, `truthy`,
`matches` (regex). `gt`/`lt` compare **numerically** when both operands parse as
numbers. The first matching condition's `handle` is taken; otherwise
`default_handle`; otherwise the node's unlabeled outgoing edge.

---

## 6. Versioning

The spec is versioned via the optional top-level `spec_version` field.

- The current version is `"2"`; parsers in this crate accept `"1"` and `"2"`.
- When the field is absent, parsers MUST treat the document as `"1"` (this is a
  back-compat rule, distinct from the version the crate *emits*).
- Any v2-only core node type in a document declaring `"1"` is a validation error:
  using v2 features requires setting `spec_version` to `"2"` explicitly.
- Parsers MUST refuse documents with a `spec_version` they don't understand.

Additive, non-breaking changes (new optional fields) may be introduced without a
version bump and announced in the changelog.

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
