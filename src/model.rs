//! Core data model for the Flow specification.
//!
//! See [`SPEC.md`](https://github.com/rust4ai/metalcraft-flows/blob/main/SPEC.md)
//! for the formal wire format.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The current spec version this crate emits.
///
/// Documents without a `spec_version` field are parsed as version `"1"`.
/// New v2 node types (`conditional`, `branch` classifier, effectors, pause
/// nodes) require `spec_version = "2"`; see [`SUPPORTED_SPEC_VERSIONS`].
pub const SPEC_VERSION: &str = "2";

/// Spec versions this crate can parse and validate. v2 is a superset of v1, so
/// both are accepted; documents declaring any other version are rejected by
/// [`crate::validate()`].
pub const SUPPORTED_SPEC_VERSIONS: &[&str] = &["1", "2"];

/// A single vertex in a [`FlowDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowNode {
    /// Unique identifier within the enclosing [`FlowDefinition`].
    pub id: String,
    /// The node kind. See [`FlowNodeType`].
    pub node_type: FlowNodeType,
    /// Free-form per-node configuration. Schema depends on `node_type`.
    pub data: serde_json::Value,
    /// `[x, y]` coordinates for visual editors. Defaults to `[0.0, 0.0]`.
    #[serde(default)]
    pub position: [f64; 2],
}

/// A node's kind.
///
/// Core types are spec-defined and understood by all conformant runtimes.
/// Custom types are vendor-namespaced (`vendor:name`) and opaque to the spec —
/// runtimes preserve them but may refuse to execute unknown ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowNodeType {
    /// A spec-defined core node type.
    Core(CoreNodeType),
    /// A vendor-namespaced custom node type, e.g. `"slack:send_message"`.
    ///
    /// The string is preserved verbatim, including the vendor prefix.
    Custom(String),
}

/// The closed set of core node types defined by the spec.
///
/// See [`SPEC.md` §5.1](https://github.com/rust4ai/metalcraft-flows/blob/main/SPEC.md).
///
/// Variants marked *(v2)* require `spec_version = "2"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreNodeType {
    /// Marks the flow's start. At most one per [`FlowDefinition`].
    Entry,
    /// A natural-language instruction run by an LLM agent.
    Prompt,
    /// *(v2)* Deterministic routing: evaluate structured predicates against flow
    /// state and follow the first matching handle. (In v1 the deterministic node
    /// was `branch`; in v2 it is renamed to `conditional` and `branch` is
    /// reassigned to the LLM classifier below.)
    Conditional,
    /// LLM classifier: the model picks exactly one typed output handle and fills
    /// its arguments, which become that edge's payload.
    ///
    /// In v1 this wire name meant an opaque, non-executable condition stub; in v2
    /// it is the classifier. The two `data` shapes are disjoint, so validation
    /// distinguishes them by `spec_version`.
    Branch,
    /// *(v2)* Assign a value into flow state (literal, template, or a path into
    /// the incoming edge payload).
    SetVariable,
    /// *(v2)* Call a single registered tool directly (no agent loop).
    Tool,
    /// *(v2)* Make a direct HTTP request.
    Http,
    /// *(v2)* Delegate a subtask to a scoped sub-agent.
    SubAgent,
    /// *(v2)* Pause for human input (human-in-the-loop) and resume on a decision.
    Approval,
    /// *(v2)* Pause for a durable delay and resume when it elapses.
    Wait,
    /// *(v2)* Fan out over a list, running a sub-body per item.
    Foreach,
    /// *(v2)* Explicit terminal node; may publish flow outputs.
    End,
    /// **Deprecated (v1).** Branch on the outcome of a tool call. Retained so v1
    /// documents round-trip; superseded by [`CoreNodeType::Conditional`] +
    /// [`CoreNodeType::Branch`].
    BranchTool,
}

impl CoreNodeType {
    /// The wire-format string for this core node type.
    pub fn as_str(self) -> &'static str {
        match self {
            CoreNodeType::Entry => "entry",
            CoreNodeType::Prompt => "prompt",
            CoreNodeType::Conditional => "conditional",
            CoreNodeType::Branch => "branch",
            CoreNodeType::SetVariable => "set_variable",
            CoreNodeType::Tool => "tool",
            CoreNodeType::Http => "http",
            CoreNodeType::SubAgent => "sub_agent",
            CoreNodeType::Approval => "approval",
            CoreNodeType::Wait => "wait",
            CoreNodeType::Foreach => "foreach",
            CoreNodeType::End => "end",
            CoreNodeType::BranchTool => "branch_tool",
        }
    }

    /// Parse a wire-format string into a core node type, if it matches one.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "entry" => Some(CoreNodeType::Entry),
            "prompt" => Some(CoreNodeType::Prompt),
            "conditional" => Some(CoreNodeType::Conditional),
            "branch" => Some(CoreNodeType::Branch),
            "set_variable" => Some(CoreNodeType::SetVariable),
            "tool" => Some(CoreNodeType::Tool),
            "http" => Some(CoreNodeType::Http),
            "sub_agent" => Some(CoreNodeType::SubAgent),
            "approval" => Some(CoreNodeType::Approval),
            "wait" => Some(CoreNodeType::Wait),
            "foreach" => Some(CoreNodeType::Foreach),
            "end" => Some(CoreNodeType::End),
            "branch_tool" => Some(CoreNodeType::BranchTool),
            _ => None,
        }
    }

    /// Whether this node type was introduced in spec v2 (and therefore requires
    /// `spec_version = "2"`).
    pub fn is_v2(self) -> bool {
        !matches!(
            self,
            CoreNodeType::Entry
                | CoreNodeType::Prompt
                | CoreNodeType::BranchTool
        )
    }
}

impl FlowNodeType {
    /// The wire-format string for this node type.
    pub fn as_wire(&self) -> &str {
        match self {
            FlowNodeType::Core(c) => c.as_str(),
            FlowNodeType::Custom(s) => s.as_str(),
        }
    }
}

impl Serialize for FlowNodeType {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for FlowNodeType {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        if let Some(core) = CoreNodeType::from_wire(&s) {
            Ok(FlowNodeType::Core(core))
        } else {
            Ok(FlowNodeType::Custom(s))
        }
    }
}

/// A directed arc connecting two nodes in a [`FlowDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowEdge {
    /// Unique identifier within the enclosing [`FlowDefinition`].
    pub id: String,
    /// The id of the source [`FlowNode`].
    pub source: String,
    /// The id of the target [`FlowNode`].
    pub target: String,
    /// Optional named output port on the source node (multi-output nodes
    /// like [`CoreNodeType::Branch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_handle: Option<String>,
    /// Optional named input port on the target node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_handle: Option<String>,
}

/// A graph: nodes and the directed edges between them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FlowDefinition {
    /// All vertices in the graph.
    pub nodes: Vec<FlowNode>,
    /// All directed arcs in the graph.
    pub edges: Vec<FlowEdge>,
}

/// A persisted flow document — what a `.json` file on disk contains.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedFlow {
    /// Spec version this document conforms to. Defaults to `"1"` when absent.
    #[serde(default = "default_spec_version")]
    pub spec_version: String,
    /// Stable identifier. Must match `^[A-Za-z0-9-]{1,64}$`.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// ISO-8601 / RFC-3339 creation timestamp.
    pub created_at: String,
    /// ISO-8601 / RFC-3339 last-modified timestamp.
    pub updated_at: String,
    /// Whether the flow should be executed by a scheduler. Defaults to `false`.
    ///
    /// This is the **master switch**: a scheduler must ignore a flow entirely
    /// when this is `false`, regardless of its [`schedules`](Self::schedules).
    #[serde(default)]
    pub enabled: bool,
    /// Flow-level schedules — **when** the flow runs. Absent/empty on legacy
    /// documents, whose trigger lives on the entry node's `data.schedule_type`
    /// instead; see [`Self::effective_schedules`] for the precedence rule.
    ///
    /// A flow may declare **any number** of schedules (e.g. one at 08:00 and one
    /// at 18:00). Published flows may ship default schedules here that seed onto a
    /// host when the flow is installed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<FlowScheduleSpec>,
    /// Declared integration-pack / tool dependencies. Absent on legacy documents;
    /// see [`crate::requires`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<crate::requires::Requires>,
    /// The graph definition.
    pub flow: FlowDefinition,
}

/// A single flow-level schedule: one trigger, plus the toggle and overrides that
/// apply when it fires.
///
/// See [`SavedFlow::schedules`]. A flow may carry many of these; each fires
/// independently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowScheduleSpec {
    /// Stable identifier within the enclosing flow (e.g. `"morning"`). Must be
    /// unique among the flow's schedules. Author-assigned for published defaults
    /// so an upgrade can diff schedules by id.
    pub id: String,
    /// Whether this individual trigger is active. Defaults to `true`. Distinct
    /// from [`SavedFlow::enabled`], the flow-wide master switch.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// The trigger, tagged by `type`: `manual` | `minutes` | `hours` | `cron`.
    #[serde(flatten)]
    pub trigger: ScheduleTrigger,
    /// Human-readable label for editors (`"Morning brief"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// IANA timezone the `cron` trigger is evaluated in (e.g.
    /// `"America/Detroit"`). `None` means the host's local/server time. Ignored
    /// by non-cron triggers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Inputs handed to the flow when this schedule fires, so the same flow can
    /// run with different parameters on different schedules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    /// Persona override for runs triggered by this schedule. `None` falls back to
    /// the flow/host default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

/// A schedule's trigger: how its firing times are computed.
///
/// Serialized with an internal `type` tag, so a cron schedule is
/// `{ "type": "cron", "cron": "0 8 * * *" }`. This mirrors the legacy entry-node
/// `schedule_type` vocabulary so back-compat conversion is mechanical.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScheduleTrigger {
    /// No scheduled firing; the flow runs only via an explicit run/agent action.
    Manual,
    /// Fire every `interval` minutes.
    Minutes {
        /// Interval in minutes. Must be positive.
        interval: u64,
    },
    /// Fire every `interval` hours.
    Hours {
        /// Interval in hours. Must be positive.
        interval: u64,
    },
    /// Fire on a cron expression. The string is a standard cron expression; this
    /// crate does not parse it (that is the host runtime's concern).
    Cron {
        /// The cron expression, e.g. `"0 8 * * *"`.
        cron: String,
    },
}

fn default_true() -> bool {
    true
}

impl SavedFlow {
    /// The normalized schedule list a runtime should honor.
    ///
    /// Precedence:
    /// 1. If [`schedules`](Self::schedules) is non-empty, it wins verbatim and
    ///    any entry-node `schedule_type` is ignored.
    /// 2. Otherwise, if the entry node declares a `schedule_type`, synthesize a
    ///    single spec from it (the legacy v1 behavior), preserving the entry
    ///    node's optional `persona`.
    /// 3. Otherwise, a single [`ScheduleTrigger::Manual`] spec.
    ///
    /// This lets existing flows (schedule on the entry node) keep running with no
    /// migration.
    pub fn effective_schedules(&self) -> Vec<FlowScheduleSpec> {
        if !self.schedules.is_empty() {
            return self.schedules.clone();
        }
        if let Some(spec) = self.entry_schedule_from_node() {
            return vec![spec];
        }
        vec![FlowScheduleSpec {
            id: "default".to_string(),
            enabled: true,
            trigger: ScheduleTrigger::Manual,
            name: None,
            timezone: None,
            inputs: None,
            persona: None,
        }]
    }

    /// Synthesize a schedule spec from the legacy entry-node `data` fields, if an
    /// entry node with a `schedule_type` is present. Returns `None` when there is
    /// no entry node or it declares no `schedule_type`.
    fn entry_schedule_from_node(&self) -> Option<FlowScheduleSpec> {
        let entry = self
            .flow
            .nodes
            .iter()
            .find(|n| matches!(n.node_type, FlowNodeType::Core(CoreNodeType::Entry)))?;
        let schedule_type = entry.data.get("schedule_type").and_then(|v| v.as_str())?;
        let interval = entry
            .data
            .get("interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let trigger = match schedule_type {
            "minutes" => ScheduleTrigger::Minutes { interval },
            "hours" => ScheduleTrigger::Hours { interval },
            "cron" => ScheduleTrigger::Cron {
                cron: entry
                    .data
                    .get("cron")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
            // "manual" and any unknown legacy value degrade to manual.
            _ => ScheduleTrigger::Manual,
        };
        let persona = entry
            .data
            .get("persona")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Some(FlowScheduleSpec {
            id: "default".to_string(),
            enabled: true,
            trigger,
            name: None,
            timezone: None,
            inputs: None,
            persona,
        })
    }
}

/// The version assumed for a document that omits `spec_version`.
///
/// Per SPEC §6 a missing field means version `"1"` — this is a back-compat rule
/// and is intentionally distinct from [`SPEC_VERSION`] (the version this crate
/// *emits*). Using v2 node types therefore requires setting `spec_version` to
/// `"2"` explicitly.
pub const DEFAULT_SPEC_VERSION: &str = "1";

fn default_spec_version() -> String {
    DEFAULT_SPEC_VERSION.to_string()
}

/// Lightweight metadata describing a saved flow, without the graph payload.
///
/// Returned by directory listings — see [`crate::store::list_flows`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowSummary {
    /// The flow's stable identifier.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Number of nodes in the graph.
    pub node_count: usize,
    /// ISO-8601 / RFC-3339 creation timestamp.
    pub created_at: String,
    /// ISO-8601 / RFC-3339 last-modified timestamp.
    pub updated_at: String,
    /// Whether the flow is enabled for scheduling.
    #[serde(default)]
    pub enabled: bool,
    /// Number of effective schedules (from `schedules`, else the legacy
    /// entry-node trigger). See [`SavedFlow::effective_schedules`].
    #[serde(default)]
    pub schedule_count: usize,
}

/// Whether an id is safe to use as a filename per [`SPEC.md` §1.1].
pub(crate) fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Whether a vendor namespace conforms to the rules in [`SPEC.md` §5.2].
pub(crate) fn is_valid_vendor(prefix: &str) -> bool {
    let mut chars = prefix.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    if prefix.len() > 32 {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn core_node_type_round_trips() {
        for ct in [
            CoreNodeType::Entry,
            CoreNodeType::Prompt,
            CoreNodeType::Branch,
            CoreNodeType::BranchTool,
        ] {
            let nt = FlowNodeType::Core(ct);
            let j = serde_json::to_string(&nt).unwrap();
            let back: FlowNodeType = serde_json::from_str(&j).unwrap();
            assert_eq!(nt, back);
        }
    }

    #[test]
    fn custom_node_type_round_trips() {
        let nt = FlowNodeType::Custom("slack:send_message".to_string());
        let j = serde_json::to_string(&nt).unwrap();
        assert_eq!(j, "\"slack:send_message\"");
        let back: FlowNodeType = serde_json::from_str(&j).unwrap();
        assert_eq!(nt, back);
    }

    #[test]
    fn unknown_bare_node_type_becomes_custom() {
        let back: FlowNodeType = serde_json::from_str("\"future_core_type\"").unwrap();
        assert_eq!(back, FlowNodeType::Custom("future_core_type".into()));
    }

    #[test]
    fn missing_spec_version_defaults_to_v1() {
        let doc = json!({
            "id": "x",
            "name": "X",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "flow": { "nodes": [], "edges": [] }
        });
        let parsed: SavedFlow = serde_json::from_value(doc).unwrap();
        assert_eq!(parsed.spec_version, "1");
        assert!(!parsed.enabled);
    }

    #[test]
    fn saved_flow_round_trips() {
        let sf = SavedFlow {
            spec_version: "1".into(),
            id: "f1".into(),
            name: "F1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            enabled: true,
            schedules: vec![],
            requires: None,
            flow: FlowDefinition {
                nodes: vec![FlowNode {
                    id: "n1".into(),
                    node_type: FlowNodeType::Core(CoreNodeType::Entry),
                    data: json!({"schedule_type": "manual"}),
                    position: [10.0, 20.0],
                }],
                edges: vec![],
            },
        };
        let j = serde_json::to_string(&sf).unwrap();
        let back: SavedFlow = serde_json::from_str(&j).unwrap();
        assert_eq!(sf, back);
    }

    #[test]
    fn effective_schedules_prefers_top_level_array() {
        let mut sf: SavedFlow = serde_json::from_value(json!({
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "schedules": [
                { "id": "morning", "type": "cron", "cron": "0 8 * * *" },
                { "id": "evening", "type": "cron", "cron": "0 18 * * *", "enabled": false }
            ],
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry", "data": { "schedule_type": "cron", "cron": "0 0 * * *" }, "position": [0,0] }
            ], "edges": [] }
        }))
        .unwrap();
        let eff = sf.effective_schedules();
        assert_eq!(eff.len(), 2, "top-level array wins over the entry node");
        assert_eq!(eff[0].id, "morning");
        assert!(eff[0].enabled);
        assert!(!eff[1].enabled);
        assert_eq!(eff[0].trigger, ScheduleTrigger::Cron { cron: "0 8 * * *".into() });

        // Clearing the array falls back to the legacy entry-node trigger.
        sf.schedules.clear();
        let eff = sf.effective_schedules();
        assert_eq!(eff.len(), 1);
        assert_eq!(eff[0].trigger, ScheduleTrigger::Cron { cron: "0 0 * * *".into() });
    }

    #[test]
    fn effective_schedules_legacy_entry_and_manual_fallback() {
        // No schedules, no entry schedule_type → a single manual spec.
        let sf: SavedFlow = serde_json::from_value(json!({
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry", "data": {}, "position": [0,0] }
            ], "edges": [] }
        }))
        .unwrap();
        let eff = sf.effective_schedules();
        assert_eq!(eff.len(), 1);
        assert_eq!(eff[0].trigger, ScheduleTrigger::Manual);

        // Legacy minutes trigger + entry persona carries through.
        let sf: SavedFlow = serde_json::from_value(json!({
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry", "data": { "schedule_type": "minutes", "interval": 15, "persona": "briefer" }, "position": [0,0] }
            ], "edges": [] }
        }))
        .unwrap();
        let eff = sf.effective_schedules();
        assert_eq!(eff[0].trigger, ScheduleTrigger::Minutes { interval: 15 });
        assert_eq!(eff[0].persona.as_deref(), Some("briefer"));
    }

    #[test]
    fn schedule_trigger_serializes_with_type_tag() {
        let spec = FlowScheduleSpec {
            id: "s".into(),
            enabled: true,
            trigger: ScheduleTrigger::Cron { cron: "0 8 * * *".into() },
            name: Some("Morning".into()),
            timezone: Some("America/Detroit".into()),
            inputs: None,
            persona: None,
        };
        let v = serde_json::to_value(&spec).unwrap();
        assert_eq!(v["type"], "cron");
        assert_eq!(v["cron"], "0 8 * * *");
        assert_eq!(v["timezone"], "America/Detroit");
        // enabled defaults to true when omitted on the wire.
        let back: FlowScheduleSpec =
            serde_json::from_value(json!({ "id": "s", "type": "manual" })).unwrap();
        assert!(back.enabled);
    }

    #[test]
    fn id_validation() {
        assert!(is_safe_id("ok-id"));
        assert!(is_safe_id("a"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id("has space"));
        assert!(!is_safe_id("../escape"));
        assert!(!is_safe_id(&"x".repeat(65)));
    }

    #[test]
    fn vendor_validation() {
        assert!(is_valid_vendor("slack"));
        assert!(is_valid_vendor("my-co"));
        assert!(is_valid_vendor("my_co"));
        assert!(is_valid_vendor("co0"));
        assert!(!is_valid_vendor(""));
        assert!(!is_valid_vendor("0starts-with-digit"));
        assert!(!is_valid_vendor("Capital"));
        assert!(!is_valid_vendor(&"a".repeat(33)));
    }
}
