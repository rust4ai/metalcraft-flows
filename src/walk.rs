//! Generic graph traversal over a [`FlowDefinition`].

use crate::model::{CoreNodeType, FlowDefinition, FlowNode, FlowNodeType};
use std::collections::{HashMap, HashSet, VecDeque};

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
