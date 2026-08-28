//! Typed views over each core node type's `data` payload.
//!
//! On the wire a [`crate::FlowNode`] carries `data` as a free-form
//! `serde_json::Value` (§3 of the spec). These structs are *parse-on-demand*
//! views: deserialize a node's `data` into the struct matching its
//! [`crate::CoreNodeType`] to read it ergonomically and to validate its shape.
//! They are never stored — the wire format remains the raw `Value`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One declared invocation parameter on an [`entry`](crate::CoreNodeType::Entry)
/// node. Seeds a flow variable at run start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputSpec {
    /// JSON type name (`"string"`, `"integer"`, `"boolean"`, …). Advisory.
    #[serde(rename = "type")]
    pub type_name: String,
    /// Whether the caller must supply this input.
    #[serde(default)]
    pub required: bool,
    /// Default used when the input is absent (and not `required`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

/// `data` for an [`entry`](crate::CoreNodeType::Entry) node.
///
/// Carried no scheduling since spec v3: when a flow runs is
/// [`ScheduledFlow`](crate::scheduled::ScheduledFlow)'s business. Pre-v3
/// documents may still have `schedule_type` / `interval` / `cron` here; they
/// parse (unknown fields are ignored) and mean nothing, which is why
/// [`crate::migrate`] strips them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryData {
    /// Optional typed invocation parameters, seeded into flow state at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<BTreeMap<String, InputSpec>>,
    /// The flow's default persona: what every `prompt`/`branch` node runs as
    /// unless it names its own.
    ///
    /// Survived the v3 split while the scheduling keys beside it did not, and
    /// deliberately: *when* a flow runs belongs to a schedule, but *what the
    /// flow is* includes the character doing the work — a flow built on a pack's
    /// tools is not the same flow run by an agent that has never heard of them.
    /// A [`ScheduledFlow`](crate::scheduled::ScheduledFlow) may still override it
    /// per schedule, and [`crate::migrate`] carries it into the schedule it
    /// extracts as well as leaving it here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

/// `data` for a [`prompt`](crate::CoreNodeType::Prompt) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptData {
    /// The instruction (supports `{{…}}` interpolation).
    pub prompt: String,
    /// Persona to run as; falls back to the flow/runtime default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Variable to store the final answer in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_var: Option<String>,
    /// Optional JSON Schema; when set, the answer is parsed as structured output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

/// One predicate on a [`conditional`](crate::CoreNodeType::Conditional) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    /// Output handle to take when this predicate matches.
    pub handle: String,
    /// State variable to read (dotted path, e.g. `_last`, `triage.severity`).
    pub variable: String,
    /// Operator wire name; see [`crate::eval::Operator`].
    pub operator: String,
    /// Right-hand comparison value (typed JSON; may be absent for `exists`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// `data` for a [`conditional`](crate::CoreNodeType::Conditional) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConditionalData {
    /// Ordered predicates; the first match wins.
    pub conditions: Vec<Condition>,
    /// Handle used when no predicate matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_handle: Option<String>,
}

/// One typed output handle on a [`branch`](crate::CoreNodeType::Branch) node —
/// a tool definition the classifier may select.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchOutput {
    /// Handle name (doubles as the tool name offered to the model).
    pub handle: String,
    /// Human/LLM-facing description of when to pick this handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for this handle's payload (scalar or object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    /// Optional variable to also persist the payload into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub var: Option<String>,
}

/// The reserved output handle a [`branch`](crate::CoreNodeType::Branch) node
/// takes on a **protocol failure** — an LLM/API error, a timeout, the agent
/// step budget being exhausted with no selection, or a chosen handle whose
/// payload does not satisfy its declared schema.
///
/// This mirrors the `error` handle emitted by `prompt`/`tool`/`http` nodes, so
/// every executable node shares one failure convention. The rail is always
/// available and **optional to wire**: a runtime routes to it on failure, and if
/// nothing is wired to it (and no `default_handle` is set) the run fails loudly
/// rather than reporting a false success. Because the rail carries a string
/// reason, a `branch` output that explicitly declares this handle must type its
/// `schema` as a string (or omit it) — see [`crate::validate`].
pub const BRANCH_ERROR_HANDLE: &str = "error";

/// `data` for a [`branch`](crate::CoreNodeType::Branch) node (the LLM classifier).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchData {
    /// The question/task the model answers by choosing an output.
    pub query: String,
    /// Typed output handles; the model must pick exactly one.
    pub outputs: Vec<BranchOutput>,
    /// Persona to run as (grants tools so the model can gather info first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Legacy fallback handle for a protocol failure. When unset, the runtime
    /// routes the reserved [`BRANCH_ERROR_HANDLE`] instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_handle: Option<String>,
    /// Seconds before treating the classification as failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// `data` for a [`set_variable`](crate::CoreNodeType::SetVariable) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetVariableData {
    /// Destination variable name.
    pub variable: String,
    /// Literal or `{{…}}`-templated value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// Dotted path into `_last` to copy from (alternative to `value`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// `data` for a [`tool`](crate::CoreNodeType::Tool) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolData {
    /// Registered tool to invoke directly.
    pub tool_name: String,
    /// Arguments object (values support `{{…}}` interpolation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    /// Variable to store the tool result in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_var: Option<String>,
}

/// `data` for an [`http`](crate::CoreNodeType::Http) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpData {
    /// HTTP method (`GET`, `POST`, …).
    pub method: String,
    /// Target URL (supports `{{…}}` interpolation).
    pub url: String,
    /// Optional request headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<serde_json::Value>,
    /// Optional request body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    /// Variable to store the response in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_var: Option<String>,
}

/// `data` for a [`sub_agent`](crate::CoreNodeType::SubAgent) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubAgentData {
    /// Task for the sub-agent.
    pub task: String,
    /// Run as a named persona (preferred).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Otherwise a tool-set preset (`read_only` | `full` | `all`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_set: Option<String>,
    /// Scope integration tools to a single pack (with `tool_set = "all"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    /// Variable to store the sub-agent's result in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_var: Option<String>,
}

/// `data` for an [`approval`](crate::CoreNodeType::Approval) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalData {
    /// Prompt shown to the human (supports `{{…}}` interpolation).
    pub message: String,
    /// Decision handles the human may choose; defaults to `["approve","reject"]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    /// Seconds before the approval times out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// `data` for a [`wait`](crate::CoreNodeType::Wait) node. Exactly one of
/// `duration` / `until` should be set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaitData {
    /// Relative delay, e.g. `"2h"`, `"30m"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    /// Absolute RFC-3339 timestamp to resume at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}

/// `data` for an [`end`](crate::CoreNodeType::End) node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EndData {
    /// Terminal status label (defaults to `"completed"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Values to publish as the flow's outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn entry_keeps_its_persona_and_drops_nothing_it_should_keep() {
        // The runtime resolves a flow's default persona from `entry.data.persona`
        // (and migration preserves it), so the typed view has to carry it too:
        // a host that parsed `EntryData` and wrote it back used to silently strip
        // the persona and hand the flow to whatever agent happened to run it.
        let data = json!({
            "persona": "calcom-agent",
            "inputs": { "timezone": { "type": "string", "required": false } },
        });
        let parsed: EntryData = serde_json::from_value(data.clone()).expect("parses");
        assert_eq!(parsed.persona.as_deref(), Some("calcom-agent"));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), data);
    }

    #[test]
    fn entry_scheduling_is_gone_but_still_parses() {
        // A pre-v3 document must keep loading; its scheduling keys are simply
        // not part of the type any more.
        let parsed: EntryData =
            serde_json::from_value(json!({ "schedule_type": "hours", "interval": 24 }))
                .expect("legacy entry data still parses");
        assert_eq!(parsed, EntryData { inputs: None, persona: None });
    }
}
