# Flow `requires` Plan — declaring integration-pack dependencies

Status: approved, implementing.
Repo: `metalcraft-flows` crate (published to crates.io; consumed by `metalcraft-agent`,
`metalcraft-flows-web`, `metalcraft-workshop`).

## Goal

Let a flow document declare the integration packs / tool surface it depends on, so that
missing or incompatible dependencies are caught at **import / enable / preflight** time
with a clear message, instead of failing silently mid-run (today a `tool` node whose pack
is disabled routes to the `error` handle or fails the whole run with a generic error).

This crate owns:
1. The **schema** — a typed, optional `requires` envelope field on `SavedFlow`.
2. **Derivation** — one canonical `derive_requires()` that folds the duplicated
   `scan_packs` (agent `flow_install.rs`) and `derive_required_packs` (flows-web
   `models/flow.rs`) logic, and adds tool-name scanning.
3. **Well-formedness validation** — semver ranges parse, ids are safe, hashes are 64 hex.
4. A **pure satisfaction check** — `check_requirements(requires, available)` taking the
   caller's pack inventory as input (no IO), reusable by agent + flows-web.

This crate does **not** know pack versions or what's installed/enabled (it has no registry
or filesystem view). Versions/hashes are stamped by the environment that has that info
(agent / flows-web) at save/publish time; satisfaction is evaluated by passing the
inventory into `check_requirements`.

## Current reality (from code)

- `SavedFlow` (`src/model.rs:201`) has `spec_version, id, name, created_at, updated_at,
  enabled, flow`. **No `requires`.** No `#[non_exhaustive]`, no `Default`, so struct-literal
  sites must list every field.
- `validate()` (`src/validate.rs:132`) checks id safety, then
  `SUPPORTED_SPEC_VERSIONS.contains(spec_version)`, then `validate_definition(...)`.
  Errors are the `ValidationError` enum (`src/validate.rs:13`). **This is the hook point.**
- Node data of interest: `ToolData.tool_name` (`src/nodes.rs:161`), `SubAgentData.pack` +
  `tool_set` (`src/nodes.rs:197`), and `FlowNodeType::Custom("vendor:action")` where the
  `vendor` prefix is the pack id.
- Deps (`Cargo.toml`): `serde`, `serde_json`, optional `regex`. **No `semver` crate.**
  Crate is `#![deny(missing_docs)]` — every new public item needs a doc comment.
  Edition 2024, rust-version 1.91, current version `0.2.2`.
- **Duplicated derivation already exists downstream** and is byte-for-byte equivalent:
  agent `flow_install.rs::scan_packs`/`dependency_report`, flows-web
  `models/flow.rs::derive_required_packs`. Neither derives *tools* — that's greenfield.
- Consumers pin the published version (`metalcraft-flows = "0.2.2"`), so this ships as a
  new minor and each consumer bumps to adopt.

## Schema

New module `src/requires.rs` (re-exported from `lib.rs`):

```rust
/// Dependencies a flow declares on integration packs and their tool surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Requires {
    /// Integration packs this flow needs, by id, with an optional version/hash contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<PackRequirement>,
    /// Flat tool names the flow invokes (the actual API surface). Auto-derived.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

/// A single pack dependency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackRequirement {
    /// Pack id / slug (stable identity). Must match `^[a-z0-9][a-z0-9_-]{0,63}$`.
    pub id: String,
    /// Semver range the pack must satisfy, e.g. ">=1.2.0, <2.0.0". None = any version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional integrity lock: exact canonical content hash (64 lowercase hex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    /// Human reason, shown in install/enable prompts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// If true, an unmet requirement is a warning, not a hard failure (flow degrades).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// The version the author's environment resolved to (lock hint for reproducibility).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
}
```

`SavedFlow` gains:
```rust
    /// Declared pack / tool dependencies. Optional; absent on legacy documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Requires>,
```
`Option<Requires>` (not bare `Requires`) so a doc with no dependencies serializes to
nothing and round-trips byte-identically to today.

### spec_version interaction

`requires` is envelope metadata, not a node type. **Allow it under any `spec_version`**
(ignoring it is always safe); tooling emits it under `"2"`. Do **not** add a
"requires-in-v1" error — that would break additive adoption. (Documented decision;
revisit only if we want strict gating.)

## Derivation

```rust
/// Derive the pack ids and tool names a flow references, from its graph alone.
/// Fills `packs[].id` and `tools`; leaves version/hash None (this crate has no registry).
pub fn derive_requires(flow: &SavedFlow) -> Requires;
```

Rules (union of the existing downstream logic + new tool scan):
- **Packs**: for each node — `data.pack` (string, sub_agent scoping) and, when
  `node_type` is `Custom("vendor:action")`, the `vendor` prefix. Sorted, de-duplicated.
- **Tools**: each `ToolData.tool_name` (parse node `data` when `node_type == Tool`).
  Sorted, de-duplicated. This is the real binding (`flow_exec` calls tools by flat name).
- The crate cannot map a bare `tool_name` back to a pack id (no registry); it only lists
  the names. The **caller** (agent/flows-web) enriches: map tool→pack, stamp
  `packs[].version`/`resolved_version`/`content_sha256` from its pack inventory before
  saving. So `derive_requires` gives the *shape*; the environment adds the *contract*.

Downstream then deletes its bespoke `scan_packs` / `derive_required_packs` in favor of
this (follow-up, out of scope here but the reason the logic moves into the crate).

## Validation

Add to `ValidationError` (`src/validate.rs:13`) + a `Display` arm:
```rust
/// A `requires` entry is malformed (bad id, unparseable semver range, or bad hash).
InvalidRequires { message: String },
```
New `validate_requires(flow, &mut errors)` called from `validate()` **right after** the
spec_version check (`src/validate.rs:~139`). It checks *well-formedness only*:
- each `packs[].id` matches the pack-id regex;
- if `version` is `Some`, it parses as `semver::VersionReq`;
- if `content_sha256` is `Some`, it's exactly 64 lowercase hex chars;
- `resolved_version`, if `Some`, parses as `semver::Version`;
- no duplicate pack ids.

It does **not** check "is the pack installed/enabled/compatible" — that needs the
environment's inventory and lives in `check_requirements`.

## Satisfaction check (pure, reusable)

```rust
/// A pack available in the caller's environment (installed & enabled).
pub struct AvailablePack { pub id: String, pub version: String, pub content_sha256: Option<String> }

/// Why a requirement is unmet.
pub enum Unmet {
    MissingPack { id: String },
    PackDisabled { id: String },                                  // caller distinguishes if it can
    VersionConflict { id: String, need: String, have: String },
    HashMismatch { id: String, need: String, have: Option<String> },
    MissingTool { name: String },                                 // optional, if caller supplies tool inventory
}

/// Evaluate a flow's requirements against what the environment offers. Pure, no IO.
pub fn check_requirements(req: &Requires, available: &[AvailablePack]) -> Vec<Unmet>;
```
- Semver match via `semver::VersionReq::matches(&Version)`.
- `optional` requirements produce `Unmet` entries too, but the caller treats them as
  warnings (the enum carries the id; the caller looks up `optional` from `req`). Simplest:
  return `Vec<Unmet>` and let the caller partition by `req.packs[].optional`. (Alternative:
  carry an `optional: bool` on each `Unmet`; decide at impl — leaning toward carrying it.)
- Tool-level checks are opt-in: a second helper
  `check_tools(req, available_tool_names: &[String]) -> Vec<Unmet>` for callers that have a
  flat tool inventory (agent does).

Callers wire this at: **import/1-click install** (offer to install+enable missing packs),
**enable-time** (refuse to mark `enabled` with a reason), **run preflight** (fail fast with
a structured message). That wiring is downstream work; the crate provides the pure core.

## Dependencies

Add `semver = "1"` as a normal (non-optional) dep — range parsing/matching is core to
both validation and `check_requirements`. It's small and `no_std`-friendly; fine for
flows-web's `default-features = false` usage.

## Tests

Match the existing inline `#[cfg(test)] mod tests` style in `validate.rs` (the `saved()`
helper builds a `SavedFlow` struct literal — **must add the new field there**, plus
`model.rs:325` and `store.rs:93`). Cases:
- legacy JSON without `requires` still parses (`requires == None`) and re-serializes
  without a `requires` key;
- a well-formed `requires` validates clean;
- bad semver range / bad id / 63-char-or-wrong-hash each yield `InvalidRequires`;
- `derive_requires` finds pack ids from `data.pack` + `vendor:` node types and tool names
  from `Tool` nodes, sorted+deduped;
- `check_requirements`: missing pack, version conflict, hash mismatch, satisfied, and
  `optional` handling.
Add a `tests/conformance.rs` fixture (`examples/requires_demo.json`) exercising a `tool`
node + a `requires` block.

## Public surface / versioning

Re-export from `lib.rs`: `Requires`, `PackRequirement`, `AvailablePack`, `Unmet`,
`derive_requires`, `check_requirements`. Every item needs a doc comment (`deny(missing_docs)`).
Bump `0.2.2 -> 0.3.0` (additive public API + field = minor). Update the three test-helper
struct literals so the crate compiles. Publish; then consumers bump at their own pace
(the field is optional, so old flows and old readers are unaffected).

## Downstream (enabled by this, not in this plan)

- agent + flows-web: call `derive_requires` at save/publish, enrich with pack
  versions/hashes from their inventory, stamp `SavedFlow.requires`; delete bespoke
  `scan_packs`/`derive_required_packs`.
- agent: call `check_requirements` at flow install / enable / run-preflight; surface the
  "install & enable pack X" UX (agent already has registry install via workshop API — see
  the packs-registry plan for the `content_sha256` + version params it will pass).
