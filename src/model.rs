//! Core data model for the Flow specification.
//!
//! See [`SPEC.md`](https://github.com/rust4ai/metalcraft-flows/blob/main/SPEC.md)
//! for the formal wire format.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The current spec version this crate emits.
///
/// Documents without a `spec_version` field are parsed as version `"1"`.
pub const SPEC_VERSION: &str = "1";

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreNodeType {
    /// Marks the flow's start. At most one per [`FlowDefinition`].
    Entry,
    /// A natural-language instruction to be passed to an LLM agent.
    Prompt,
    /// Splits flow execution based on a condition.
    Branch,
    /// Branches based on the outcome of a tool call.
    BranchTool,
}

impl CoreNodeType {
    /// The wire-format string for this core node type.
    pub fn as_str(self) -> &'static str {
        match self {
            CoreNodeType::Entry => "entry",
            CoreNodeType::Prompt => "prompt",
            CoreNodeType::Branch => "branch",
            CoreNodeType::BranchTool => "branch_tool",
        }
    }

    /// Parse a wire-format string into a core node type, if it matches one.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "entry" => Some(CoreNodeType::Entry),
            "prompt" => Some(CoreNodeType::Prompt),
            "branch" => Some(CoreNodeType::Branch),
            "branch_tool" => Some(CoreNodeType::BranchTool),
            _ => None,
        }
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
    #[serde(default)]
    pub enabled: bool,
    /// The graph definition.
    pub flow: FlowDefinition,
}

fn default_spec_version() -> String {
    SPEC_VERSION.to_string()
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
