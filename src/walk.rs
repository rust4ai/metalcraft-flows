//! Generic graph traversal over a [`FlowDefinition`].

use crate::model::{CoreNodeType, FlowDefinition, FlowNode, FlowNodeType};
use std::collections::{HashMap, HashSet, VecDeque};

/// Resolve the next node id to visit from `source`, following an optional named
/// output `handle`.
///
/// Routing rules (a handle-aware step, unlike [`walk_bfs`]):
///
/// 1. If `handle` is `Some`, prefer the edge whose `source_handle` equals it.
/// 2. Otherwise (or if no handle matched), fall back to the node's *default*
///    output edge — one whose `source_handle` is absent **or** the literal
///    `"default"`. (Visual editors serialize an unlabeled edge with the handle
///    id `"default"`, so both must count as the default output.)
///
/// Returns the target node id, or `None` if no edge applies. Runtimes use this to
/// route `conditional` / `branch` / `ok`/`error` outcomes.
pub fn next_by_handle(def: &FlowDefinition, source: &str, handle: Option<&str>) -> Option<String> {
    if let Some(h) = handle
        && let Some(edge) = def
            .edges
            .iter()
            .find(|e| e.source == source && e.source_handle.as_deref() == Some(h))
    {
        return Some(edge.target.clone());
    }
    def.edges
        .iter()
        .find(|e| e.source == source && matches!(e.source_handle.as_deref(), None | Some("default")))
        .map(|e| e.target.clone())
}

/// Walk a flow graph in breadth-first order starting from its `entry` node,
/// invoking `visit` once for each reachable node.
///
/// - Nodes are visited at most once even if the graph contains cycles.
/// - If the flow has no `entry` node, the walk is a no-op.
/// - Disconnected nodes (not reachable from `entry`) are not visited.
/// - Edge `source_handle` / `target_handle` are not interpreted here — every
///   outgoing edge is followed. Runtimes that care about handles should
///   implement their own traversal.
pub fn walk_bfs<F: FnMut(&FlowNode)>(flow: &FlowDefinition, mut visit: F) {
    let entry = flow
        .nodes
        .iter()
        .find(|n| matches!(n.node_type, FlowNodeType::Core(CoreNodeType::Entry)));
    let Some(entry) = entry else { return };

    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &flow.edges {
        adj.entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
    }

    let node_map: HashMap<&str, &FlowNode> =
        flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    queue.push_back(entry.id.as_str());
    visited.insert(entry.id.as_str());

    while let Some(current_id) = queue.pop_front() {
        if let Some(node) = node_map.get(current_id) {
            visit(node);
        }
        if let Some(targets) = adj.get(current_id) {
            for &target in targets {
                if visited.insert(target) {
                    queue.push_back(target);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowEdge, FlowNode};
    use serde_json::json;

    fn n(id: &str, t: CoreNodeType) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: FlowNodeType::Core(t),
            data: json!({}),
            position: [0.0, 0.0],
        }
    }
    fn cn(id: &str, kind: &str) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: FlowNodeType::Custom(kind.into()),
            data: json!({}),
            position: [0.0, 0.0],
        }
    }
    fn e(id: &str, src: &str, tgt: &str) -> FlowEdge {
        FlowEdge {
            id: id.into(),
            source: src.into(),
            target: tgt.into(),
            source_handle: None,
            target_handle: None,
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
    fn next_by_handle_prefers_matching_handle() {
        let def = FlowDefinition {
            nodes: vec![],
            edges: vec![eh("1", "c", "hot_node", "hot"), eh("2", "c", "cold_node", "cold")],
        };
        assert_eq!(next_by_handle(&def, "c", Some("hot")).as_deref(), Some("hot_node"));
        assert_eq!(next_by_handle(&def, "c", Some("cold")).as_deref(), Some("cold_node"));
    }

    #[test]
    fn next_by_handle_treats_default_label_as_unlabeled() {
        // A visual editor serializes an unlabeled edge with source_handle
        // "default"; routing with no handle must still follow it.
        let def = FlowDefinition {
            nodes: vec![],
            edges: vec![eh("1", "entry", "check", "default")],
        };
        assert_eq!(next_by_handle(&def, "entry", None).as_deref(), Some("check"));
        // And a named route still wins over the default edge.
        let def2 = FlowDefinition {
            nodes: vec![],
            edges: vec![eh("1", "c", "d_node", "default"), eh("2", "c", "ok_node", "ok")],
        };
        assert_eq!(next_by_handle(&def2, "c", Some("ok")).as_deref(), Some("ok_node"));
    }

    #[test]
    fn next_by_handle_falls_back_to_unlabeled() {
        let def = FlowDefinition {
            nodes: vec![],
            edges: vec![e("1", "a", "b")],
        };
        // No handle, or a handle with no matching edge → the unlabeled edge.
        assert_eq!(next_by_handle(&def, "a", None).as_deref(), Some("b"));
        assert_eq!(next_by_handle(&def, "a", Some("missing")).as_deref(), Some("b"));
    }

    #[test]
    fn next_by_handle_none_when_no_edge() {
        let def = FlowDefinition {
            nodes: vec![],
            edges: vec![eh("1", "c", "x", "only")],
        };
        // A handle miss with no unlabeled fallback → None.
        assert_eq!(next_by_handle(&def, "c", Some("other")), None);
        assert_eq!(next_by_handle(&def, "nope", None), None);
    }

    #[test]
    fn empty_flow_visits_nothing() {
        let mut count = 0;
        walk_bfs(&FlowDefinition::default(), |_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn missing_entry_visits_nothing() {
        let flow = FlowDefinition {
            nodes: vec![n("p", CoreNodeType::Prompt)],
            edges: vec![],
        };
        let mut count = 0;
        walk_bfs(&flow, |_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn linear_chain() {
        let flow = FlowDefinition {
            nodes: vec![
                n("e", CoreNodeType::Entry),
                n("a", CoreNodeType::Prompt),
                n("b", CoreNodeType::Prompt),
            ],
            edges: vec![e("e1", "e", "a"), e("e2", "a", "b")],
        };
        let mut order = vec![];
        walk_bfs(&flow, |n| order.push(n.id.clone()));
        assert_eq!(order, vec!["e", "a", "b"]);
    }

    #[test]
    fn cycle_terminates() {
        let flow = FlowDefinition {
            nodes: vec![
                n("e", CoreNodeType::Entry),
                n("a", CoreNodeType::Prompt),
                n("b", CoreNodeType::Prompt),
            ],
            edges: vec![
                e("1", "e", "a"),
                e("2", "a", "b"),
                e("3", "b", "a"),
            ],
        };
        let mut order = vec![];
        walk_bfs(&flow, |n| order.push(n.id.clone()));
        assert_eq!(order, vec!["e", "a", "b"]);
    }

    #[test]
    fn disconnected_node_skipped() {
        let flow = FlowDefinition {
            nodes: vec![
                n("e", CoreNodeType::Entry),
                n("a", CoreNodeType::Prompt),
                n("orphan", CoreNodeType::Prompt),
            ],
            edges: vec![e("1", "e", "a")],
        };
        let mut order = vec![];
        walk_bfs(&flow, |n| order.push(n.id.clone()));
        assert_eq!(order, vec!["e", "a"]);
    }

    #[test]
    fn custom_node_types_traversed() {
        let flow = FlowDefinition {
            nodes: vec![n("e", CoreNodeType::Entry), cn("slack", "slack:send_message")],
            edges: vec![e("1", "e", "slack")],
        };
        let mut order = vec![];
        walk_bfs(&flow, |n| order.push(n.id.clone()));
        assert_eq!(order, vec!["e", "slack"]);
    }
}
