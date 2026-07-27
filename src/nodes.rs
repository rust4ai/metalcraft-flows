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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryData {
    /// `"manual" | "minutes" | "hours" | "cron"`.
    #[serde(default = "default_schedule_type")]
    pub schedule_type: String,
    /// Interval for `"minutes"` / `"hours"` schedules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,
    /// Cron expression for the `"cron"` schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    /// Optional typed invocation parameters, seeded into flow state at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<BTreeMap<String, InputSpec>>,
}

fn default_schedule_type() -> String {
    "manual".to_string()
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
    /// Fallback handle for timeout / no valid choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_handle: Option<String>,
    /// Seconds before falling back to `default_handle`.
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
