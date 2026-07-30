//! Spec-conformance validation for a [`SavedFlow`] or [`FlowDefinition`].

use crate::eval::Operator;
use crate::model::{
    is_safe_id, is_valid_vendor, CoreNodeType, FlowDefinition, FlowNode, FlowNodeType, SavedFlow,
    SUPPORTED_SPEC_VERSIONS,
};
use crate::nodes::{BranchData, ConditionalData};
use std::collections::HashSet;
use std::fmt;

/// A single spec-conformance failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// `id` does not match `^[A-Za-z0-9-]{1,64}$`.
    InvalidFlowId(String),
    /// `spec_version` is a value this parser doesn't understand.
    UnsupportedSpecVersion(String),
    /// More than one node in the graph has `node_type = "entry"`.
    MultipleEntryNodes(usize),
    /// Two nodes share the same `id`.
    DuplicateNodeId(String),
    /// Two edges share the same `id`.
    DuplicateEdgeId(String),
    /// An edge's `source` does not match any node `id`.
    DanglingEdgeSource {
        /// Id of the offending edge.
        edge: String,
        /// The unknown source node id it referenced.
        source: String,
    },
    /// An edge's `target` does not match any node `id`.
    DanglingEdgeTarget {
        /// Id of the offending edge.
        edge: String,
        /// The unknown target node id it referenced.
        target: String,
    },
    /// A custom `node_type` has a malformed vendor namespace.
    InvalidVendorNamespace {
        /// Id of the offending node.
        node: String,
        /// The raw `node_type` string that failed the vendor-namespace check.
        node_type: String,
    },
    /// A v2-only core node type is used in a document declaring `spec_version = "1"`.
    V2NodeInV1Document {
        /// Id of the offending node.
        node: String,
        /// The node's wire-format type string.
        node_type: String,
    },
    /// A node's `data` payload does not match the schema for its `node_type`.
    InvalidNodeData {
        /// Id of the offending node.
        node: String,
        /// What was wrong.
        message: String,
    },
    /// A `conditional` condition names an operator the runtime doesn't know.
    UnknownOperator {
        /// Id of the offending node.
        node: String,
        /// The unrecognized operator string.
        operator: String,
    },
    /// A node declares an output `handle` that has no matching outgoing edge.
    MissingHandleEdge {
        /// Id of the offending node.
        node: String,
        /// The handle that has no `source_handle` edge leaving the node.
        handle: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidFlowId(id) => {
                write!(f, "invalid flow id {id:?}: must match [A-Za-z0-9-]{{1,64}}")
            }
            ValidationError::UnsupportedSpecVersion(v) => {
                write!(f, "unsupported spec_version {v:?}; this parser supports {SUPPORTED_SPEC_VERSIONS:?}")
            }
            ValidationError::MultipleEntryNodes(n) => {
                write!(f, "flow has {n} entry nodes; at most one is allowed")
            }
            ValidationError::DuplicateNodeId(id) => {
                write!(f, "duplicate node id {id:?}")
            }
            ValidationError::DuplicateEdgeId(id) => {
                write!(f, "duplicate edge id {id:?}")
            }
            ValidationError::DanglingEdgeSource { edge, source } => {
                write!(f, "edge {edge:?} references unknown source node {source:?}")
            }
            ValidationError::DanglingEdgeTarget { edge, target } => {
                write!(f, "edge {edge:?} references unknown target node {target:?}")
            }
            ValidationError::InvalidVendorNamespace { node, node_type } => {
                write!(
                    f,
                    "node {node:?} has malformed custom node_type {node_type:?}: \
                     vendor prefix must match [a-z][a-z0-9_-]{{0,31}}"
                )
            }
            ValidationError::V2NodeInV1Document { node, node_type } => {
                write!(
                    f,
                    "node {node:?} uses v2 node type {node_type:?} but the document \
                     declares spec_version \"1\"; set spec_version to \"2\""
                )
            }
            ValidationError::InvalidNodeData { node, message } => {
                write!(f, "node {node:?} has invalid data: {message}")
            }
            ValidationError::UnknownOperator { node, operator } => {
                write!(f, "node {node:?} uses unknown operator {operator:?}")
            }
            ValidationError::MissingHandleEdge { node, handle } => {
                write!(f, "node {node:?} declares handle {handle:?} but no edge leaves it via that handle")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate a [`SavedFlow`] against the spec.
///
/// Returns all detected errors. An empty `Vec` means the document is conformant.
pub fn validate(flow: &SavedFlow) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if !is_safe_id(&flow.id) {
        errors.push(ValidationError::InvalidFlowId(flow.id.clone()));
    }

    if !SUPPORTED_SPEC_VERSIONS.contains(&flow.spec_version.as_str()) {
        errors.push(ValidationError::UnsupportedSpecVersion(
            flow.spec_version.clone(),
        ));
    }

    validate_definition(&flow.flow, &flow.spec_version, &mut errors);
    errors
}

/// Validate a bare [`FlowDefinition`] (the graph only, no envelope fields).
///
/// Node types are checked against the current [`crate::SPEC_VERSION`] (v2).
pub fn validate_definition_only(def: &FlowDefinition) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_definition(def, crate::SPEC_VERSION, &mut errors);
    errors
}

fn validate_definition(def: &FlowDefinition, spec_version: &str, errors: &mut Vec<ValidationError>) {
    // Entry-node count.
    let entry_count = def
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, FlowNodeType::Core(CoreNodeType::Entry)))
        .count();
    if entry_count > 1 {
        errors.push(ValidationError::MultipleEntryNodes(entry_count));
    }

    // Duplicate node ids.
    let mut seen_nodes = HashSet::new();
    for n in &def.nodes {
        if !seen_nodes.insert(n.id.as_str()) {
            errors.push(ValidationError::DuplicateNodeId(n.id.clone()));
        }
        match &n.node_type {
            // Vendor prefix check for custom node types.
            FlowNodeType::Custom(s) => {
                if let Some((prefix, _)) = s.split_once(':') {
                    if !is_valid_vendor(prefix) {
                        errors.push(ValidationError::InvalidVendorNamespace {
                            node: n.id.clone(),
                            node_type: s.clone(),
                        });
                    }
                } else {
                    // Custom strings without a colon are treated as future core
                    // types — not vendor-namespaced. They are accepted but flagged
                    // here as an invalid vendor namespace so authors notice.
                    errors.push(ValidationError::InvalidVendorNamespace {
                        node: n.id.clone(),
                        node_type: s.clone(),
                    });
                }
            }
            FlowNodeType::Core(core) => {
                // v2 node types require spec_version "2".
                if spec_version == "1" && core.is_v2() {
                    errors.push(ValidationError::V2NodeInV1Document {
                        node: n.id.clone(),
                        node_type: core.as_str().to_string(),
                    });
                }
                validate_core_node_data(n, *core, def, errors);
            }
        }
    }

    // Duplicate edge ids + dangling refs.
    let node_ids: HashSet<&str> = def.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut seen_edges = HashSet::new();
    for e in &def.edges {
        if !seen_edges.insert(e.id.as_str()) {
            errors.push(ValidationError::DuplicateEdgeId(e.id.clone()));
        }
        if !node_ids.contains(e.source.as_str()) {
            errors.push(ValidationError::DanglingEdgeSource {
                edge: e.id.clone(),
                source: e.source.clone(),
            });
        }
        if !node_ids.contains(e.target.as_str()) {
            errors.push(ValidationError::DanglingEdgeTarget {
                edge: e.id.clone(),
                target: e.target.clone(),
            });
        }
    }
}

/// Whether an edge leaves `node_id` via the named `source_handle`.
fn has_handle_edge(def: &FlowDefinition, node_id: &str, handle: &str) -> bool {
    def.edges
        .iter()
        .any(|e| e.source == node_id && e.source_handle.as_deref() == Some(handle))
}

/// Per-node-type `data` validation for the structured core nodes. Best-effort:
/// only the nodes whose routing/behavior depends on `data` shape are checked
/// here (`conditional`, `branch`). Effector nodes are validated by the runtime.
fn validate_core_node_data(
    node: &FlowNode,
    core: CoreNodeType,
    def: &FlowDefinition,
    errors: &mut Vec<ValidationError>,
) {
    match core {
        CoreNodeType::Conditional => {
            match serde_json::from_value::<ConditionalData>(node.data.clone()) {
                Ok(data) => {
                    for cond in &data.conditions {
                        if Operator::from_wire(&cond.operator).is_none() {
                            errors.push(ValidationError::UnknownOperator {
                                node: node.id.clone(),
                                operator: cond.operator.clone(),
                            });
                        }
                        if !has_handle_edge(def, &node.id, &cond.handle) {
                            errors.push(ValidationError::MissingHandleEdge {
                                node: node.id.clone(),
                                handle: cond.handle.clone(),
                            });
                        }
                    }
                    if let Some(dh) = &data.default_handle
                        && !has_handle_edge(def, &node.id, dh)
                    {
                        errors.push(ValidationError::MissingHandleEdge {
                            node: node.id.clone(),
                            handle: dh.clone(),
                        });
                    }
                }
                Err(e) => errors.push(ValidationError::InvalidNodeData {
                    node: node.id.clone(),
                    message: format!("expected conditional data: {e}"),
                }),
            }
        }
        CoreNodeType::Branch => {
            match serde_json::from_value::<BranchData>(node.data.clone()) {
                Ok(data) => {
                    if data.outputs.is_empty() {
                        errors.push(ValidationError::InvalidNodeData {
                            node: node.id.clone(),
                            message: "branch must declare at least one output".into(),
                        });
                    }
                    for out in &data.outputs {
                        if !has_handle_edge(def, &node.id, &out.handle) {
                            errors.push(ValidationError::MissingHandleEdge {
                                node: node.id.clone(),
                                handle: out.handle.clone(),
                            });
                        }
                        // The reserved `error` rail carries a string reason (see
                        // `BRANCH_ERROR_HANDLE`). A branch may still declare it
                        // explicitly so the model can select it, but its payload
                        // type must stay coherent with the runtime-injected
                        // reason: string, or schema omitted.
                        if out.handle == crate::BRANCH_ERROR_HANDLE
                            && out.schema.as_ref().is_some_and(|s| {
                                s.get("type").and_then(|t| t.as_str()) != Some("string")
                            })
                        {
                            errors.push(ValidationError::InvalidNodeData {
                                node: node.id.clone(),
                                message: "the reserved `error` handle carries a string reason; \
                                          its schema must be omitted or {\"type\":\"string\"}"
                                    .into(),
                            });
                        }
                    }
                    if let Some(dh) = &data.default_handle
                        && !has_handle_edge(def, &node.id, dh)
                    {
                        errors.push(ValidationError::MissingHandleEdge {
                            node: node.id.clone(),
                            handle: dh.clone(),
                        });
                    }
                }
                Err(e) => errors.push(ValidationError::InvalidNodeData {
                    node: node.id.clone(),
                    message: format!("expected branch data: {e}"),
                }),
            }
        }
        // Other core nodes: no data-shape validation at the spec layer.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowEdge, FlowNode};
    use serde_json::json;

    fn entry(id: &str) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: FlowNodeType::Core(CoreNodeType::Entry),
            data: json!({}),
            position: [0.0, 0.0],
        }
    }
    fn prompt(id: &str) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: FlowNodeType::Core(CoreNodeType::Prompt),
            data: json!({}),
            position: [0.0, 0.0],
        }
    }
    fn edge(id: &str, src: &str, tgt: &str) -> FlowEdge {
        FlowEdge {
            id: id.into(),
            source: src.into(),
            target: tgt.into(),
            source_handle: None,
            target_handle: None,
        }
    }
    fn saved(def: FlowDefinition) -> SavedFlow {
        SavedFlow {
            spec_version: "1".into(),
            id: "ok-id".into(),
            name: "X".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            enabled: false,
            flow: def,
        }
    }

    #[test]
    fn valid_minimal_flow_has_no_errors() {
        let def = FlowDefinition {
            nodes: vec![entry("e")],
            edges: vec![],
        };
        assert!(validate(&saved(def)).is_empty());
    }

    #[test]
    fn invalid_flow_id_caught() {
        let mut sf = saved(FlowDefinition::default());
        sf.id = "bad id with spaces".into();
        let errs = validate(&sf);
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidFlowId(_))));
    }

    #[test]
    fn multiple_entries_caught() {
        let def = FlowDefinition {
            nodes: vec![entry("a"), entry("b")],
            edges: vec![],
        };
        let errs = validate(&saved(def));
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::MultipleEntryNodes(2))));
    }

    #[test]
    fn dangling_edge_caught() {
        let def = FlowDefinition {
            nodes: vec![entry("e")],
            edges: vec![edge("x", "e", "missing")],
        };
        let errs = validate(&saved(def));
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::DanglingEdgeTarget { .. })));
    }

    #[test]
    fn duplicate_node_id_caught() {
        let def = FlowDefinition {
            nodes: vec![entry("e"), prompt("e")],
            edges: vec![],
        };
        let errs = validate(&saved(def));
        assert!(errs.iter().any(|e| matches!(e, ValidationError::DuplicateNodeId(_))));
    }

    #[test]
    fn both_spec_versions_accepted() {
        let mut sf = saved(FlowDefinition {
            nodes: vec![entry("e")],
            edges: vec![],
        });
        sf.spec_version = "1".into();
        assert!(validate(&sf).is_empty());
        sf.spec_version = "2".into();
        assert!(validate(&sf).is_empty());
    }

    #[test]
    fn unsupported_spec_version_caught() {
        let mut sf = saved(FlowDefinition::default());
        sf.spec_version = "3".into();
        let errs = validate(&sf);
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::UnsupportedSpecVersion(_))));
    }

    fn core(id: &str, ty: CoreNodeType, data: serde_json::Value) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: FlowNodeType::Core(ty),
            data,
            position: [0.0, 0.0],
        }
    }
    fn eh(id: &str, src: &str, tgt: &str, handle: &str) -> FlowEdge {
        FlowEdge {
            id: id.into(),
            source: src.into(),
            target: tgt.into(),
            source_handle: Some(handle.into()),
            target_handle: None,
        }
    }

    #[test]
    fn v2_node_in_v1_document_caught() {
        let def = FlowDefinition {
            nodes: vec![entry("e"), core("c", CoreNodeType::Conditional, json!({ "conditions": [] }))],
            edges: vec![edge("x", "e", "c")],
        };
        let mut sf = saved(def);
        sf.spec_version = "1".into();
        let errs = validate(&sf);
        assert!(errs.iter().any(|e| matches!(e, ValidationError::V2NodeInV1Document { .. })));
    }

    #[test]
    fn valid_conditional_v2_passes() {
        let def = FlowDefinition {
            nodes: vec![
                entry("e"),
                core("c", CoreNodeType::Conditional, json!({
                    "conditions": [ { "handle": "hot", "variable": "_last", "operator": "gt", "value": 50 } ],
                    "default_handle": "cold"
                })),
                prompt("hot_node"),
                prompt("cold_node"),
            ],
            edges: vec![
                edge("e0", "e", "c"),
                eh("e1", "c", "hot_node", "hot"),
                eh("e2", "c", "cold_node", "cold"),
            ],
        };
        let mut sf = saved(def);
        sf.spec_version = "2".into();
        assert!(validate(&sf).is_empty(), "{:?}", validate(&sf));
    }

    #[test]
    fn conditional_unknown_operator_and_missing_edge_caught() {
        let def = FlowDefinition {
            nodes: vec![
                entry("e"),
                core("c", CoreNodeType::Conditional, json!({
                    "conditions": [ { "handle": "hot", "variable": "_last", "operator": "bogus", "value": 1 } ]
                })),
            ],
            edges: vec![edge("e0", "e", "c")], // no "hot" handle edge
        };
        let mut sf = saved(def);
        sf.spec_version = "2".into();
        let errs = validate(&sf);
        assert!(errs.iter().any(|e| matches!(e, ValidationError::UnknownOperator { .. })));
        assert!(errs.iter().any(|e| matches!(e, ValidationError::MissingHandleEdge { .. })));
    }

    #[test]
    fn branch_bad_data_caught() {
        let def = FlowDefinition {
            nodes: vec![
                entry("e"),
                // missing required `query` and `outputs`
                core("b", CoreNodeType::Branch, json!({ "persona": "weather-agent" })),
            ],
            edges: vec![edge("e0", "e", "b")],
        };
        let mut sf = saved(def);
        sf.spec_version = "2".into();
        let errs = validate(&sf);
        assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidNodeData { .. })));
    }

    #[test]
    fn branch_error_handle_with_nonstring_schema_caught() {
        // A branch that declares the reserved `error` handle with an object
        // schema conflicts with the string reason the runtime injects.
        let def = FlowDefinition {
            nodes: vec![
                entry("e"),
                core("b", CoreNodeType::Branch, json!({
                    "query": "classify",
                    "outputs": [
                        { "handle": "ok", "schema": { "type": "string" } },
                        { "handle": "error", "schema": { "type": "object" } }
                    ]
                })),
                prompt("ok_t"),
                prompt("err_t"),
            ],
            edges: vec![
                edge("e0", "e", "b"),
                eh("e1", "b", "ok_t", "ok"),
                eh("e2", "b", "err_t", "error"),
            ],
        };
        let mut sf = saved(def);
        sf.spec_version = "2".into();
        let errs = validate(&sf);
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::InvalidNodeData { node, message }
                    if node == "b" && message.contains("`error` handle")
            )),
            "expected reserved-error-handle error, got {errs:?}"
        );
    }

    #[test]
    fn branch_error_handle_with_string_schema_passes() {
        // The same branch with a string-typed (or omitted) `error` schema is fine.
        let def = FlowDefinition {
            nodes: vec![
                entry("e"),
                core("b", CoreNodeType::Branch, json!({
                    "query": "classify",
                    "outputs": [
                        { "handle": "ok", "schema": { "type": "string" } },
                        { "handle": "error", "schema": { "type": "string" } }
                    ]
                })),
                prompt("ok_t"),
                prompt("err_t"),
            ],
            edges: vec![
                edge("e0", "e", "b"),
                eh("e1", "b", "ok_t", "ok"),
                eh("e2", "b", "err_t", "error"),
            ],
        };
        let mut sf = saved(def);
        sf.spec_version = "2".into();
        assert!(validate(&sf).is_empty(), "{:?}", validate(&sf));
    }

    #[test]
    fn well_formed_custom_type_passes() {
        let mut p = prompt("p");
        p.node_type = FlowNodeType::Custom("slack:send_message".into());
        let def = FlowDefinition {
            nodes: vec![entry("e"), p],
            edges: vec![edge("x", "e", "p")],
        };
        assert!(validate(&saved(def)).is_empty());
    }

    #[test]
    fn malformed_custom_type_caught() {
        let mut p = prompt("p");
        p.node_type = FlowNodeType::Custom("BadVendor:thing".into());
        let def = FlowDefinition {
            nodes: vec![entry("e"), p],
            edges: vec![],
        };
        let errs = validate(&saved(def));
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidVendorNamespace { .. })));
    }

    #[test]
    fn custom_type_without_colon_caught() {
        let mut p = prompt("p");
        p.node_type = FlowNodeType::Custom("no_namespace".into());
        let def = FlowDefinition {
            nodes: vec![entry("e"), p],
            edges: vec![],
        };
        let errs = validate(&saved(def));
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidVendorNamespace { .. })));
    }
}
