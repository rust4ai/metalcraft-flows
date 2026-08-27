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
///
/// v3 removes scheduling from the flow document entirely: *when* a flow runs is
/// a separate artifact ([`crate::scheduled::ScheduledFlow`]), so a `SavedFlow`
/// answers only *what work is this*.
pub const SPEC_VERSION: &str = "3";

/// Spec versions this crate can parse and validate. Each is a superset of the
/// last for node vocabulary, so all are accepted; documents declaring any other
/// version are rejected by [`crate::validate()`].
///
/// v1 and v2 documents may still carry the removed `enabled` / `schedules`
/// fields. Parsing ignores them — [`crate::migrate`] is what reads them, from
/// the raw JSON, on the way to v3.
pub const SUPPORTED_SPEC_VERSIONS: &[&str] = &["1", "2", "3"];

/// A single vertex in a [`FlowDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
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

/// Written by hand rather than derived: on the wire this is a *string*, not the
/// tagged union the Rust type is, and a derive would describe the shape of the
/// enum instead of the shape of the JSON.
///
/// Deliberately **not** an `enum` of the core names either. Any `vendor:name` is
/// valid (SPEC §5.2) and a generated client that rejected one would refuse to
/// load a flow the pod is perfectly happy to run — so this is an open string
/// that documents the core names rather than closing over them.
#[cfg(feature = "schema")]
impl utoipa::PartialSchema for FlowNodeType {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Type};
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some(
                "A core node type (entry, prompt, conditional, branch, set_variable, \
                 tool, http, sub_agent, approval, wait, foreach, end, branch_tool) or a \
                 vendor-namespaced custom type such as `slack:send_message`. Custom types \
                 are opaque and must round-trip unchanged.",
            ))
            .examples([serde_json::json!("prompt")])
            .into()
    }
}

#[cfg(feature = "schema")]
impl utoipa::ToSchema for FlowNodeType {}

/// A directed arc connecting two nodes in a [`FlowDefinition`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct FlowDefinition {
    /// All vertices in the graph.
    pub nodes: Vec<FlowNode>,
    /// All directed arcs in the graph.
    pub edges: Vec<FlowEdge>,
}

/// A persisted flow document — what a `.json` file on disk contains.
///
/// A flow describes **what work is this**, and nothing else. When it runs, as
/// whom, and on which agent live in [`ScheduledFlow`](crate::scheduled::ScheduledFlow)
/// documents that point at it by id. A flow with no `ScheduledFlow` pointing at
/// it cannot fire — which is what makes "installing something never starts
/// background work" a property of the format rather than a rule every install
/// path has to remember.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
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
    /// Declared integration-pack / tool dependencies. Absent on legacy documents;
    /// see [`crate::requires`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<crate::requires::Requires>,
    /// The graph definition.
    pub flow: FlowDefinition,
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
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
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
}

/// Whether an id is safe to use as a filename per [`SPEC.md` §1.1].
pub(crate) fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Whether a [`ScheduledFlow`](crate::scheduled::ScheduledFlow) id is safe to use
/// as a filename.
///
/// Looser than [`is_safe_id`] by one character: `_`, because generated ids are
/// prefixed (`sf_9c31a4`) to be recognisable in a log line without being parsed.
pub(crate) fn is_safe_scheduled_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
    }

    #[test]
    fn legacy_scheduling_fields_are_ignored_not_rejected() {
        // A v2 document straight off an un-migrated pod. It must still parse —
        // `crate::migrate` reads the scheduling out of the raw JSON, and a
        // document that refused to load could not be migrated at all.
        let doc = json!({
            "spec_version": "2",
            "id": "x", "name": "X",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "enabled": true,
            "schedules": [ { "id": "morning", "type": "cron", "cron": "0 0 8 * * *" } ],
            "flow": { "nodes": [], "edges": [] }
        });
        let parsed: SavedFlow = serde_json::from_value(doc).unwrap();
        assert_eq!(parsed.id, "x");
        // …and re-serializing drops them, which is why migration must run before
        // anything else writes a flow back out.
        let out = serde_json::to_value(&parsed).unwrap();
        assert!(out.get("schedules").is_none());
        assert!(out.get("enabled").is_none());
    }

    #[test]
    fn saved_flow_round_trips() {
        let sf = SavedFlow {
            spec_version: "3".into(),
            id: "f1".into(),
            name: "F1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            requires: None,
            flow: FlowDefinition {
                nodes: vec![FlowNode {
                    id: "n1".into(),
                    node_type: FlowNodeType::Core(CoreNodeType::Entry),
                    data: json!({}),
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
    fn id_validation() {
        assert!(is_safe_id("ok-id"));
        assert!(is_safe_id("a"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id("has space"));
        assert!(!is_safe_id("../escape"));
        assert!(!is_safe_id(&"x".repeat(65)));
        // Scheduled-flow ids additionally allow `_` for the `sf_` prefix.
        assert!(is_safe_scheduled_id("sf_9c31a4"));
        assert!(!is_safe_id("sf_9c31a4"));
        assert!(!is_safe_scheduled_id("../escape"));
        assert!(!is_safe_scheduled_id(""));
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
