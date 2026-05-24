//! Directory-backed CRUD for [`SavedFlow`] documents.
//!
//! Each flow lives in its own `{id}.json` file inside the given directory.
//! Enabled by the default `fs` feature.

use crate::model::{is_safe_id, FlowSummary, SavedFlow};
use std::io;
use std::path::Path;

/// Persist a [`SavedFlow`] to `{dir}/{flow.id}.json`.
///
/// Creates `dir` (and any missing parents) if needed.
pub fn save_flow(dir: &Path, flow: &SavedFlow) -> io::Result<()> {
    if !is_safe_id(&flow.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid flow id: {:?}", flow.id),
        ));
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", flow.id));
    let json = serde_json::to_string_pretty(flow)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    std::fs::write(&path, json)
}

/// Load `{dir}/{id}.json` into a [`SavedFlow`].
///
/// Returns `None` if the id is malformed, the file is missing, or the contents
/// are not parseable. Use [`load_flow_strict`] if you need to distinguish those
/// cases.
pub fn load_flow(dir: &Path, id: &str) -> Option<SavedFlow> {
    load_flow_strict(dir, id).ok()
}

/// Like [`load_flow`] but returns the underlying error.
pub fn load_flow_strict(dir: &Path, id: &str) -> io::Result<SavedFlow> {
    if !is_safe_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid flow id: {id:?}"),
        ));
    }
    let path = dir.join(format!("{id}.json"));
    let data = std::fs::read_to_string(path)?;
    serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// List every flow in `dir`, returning lightweight summaries.
///
/// Files that are not parseable as a [`SavedFlow`] are silently skipped.
/// Result is sorted by `updated_at` descending.
pub fn list_flows(dir: &Path) -> Vec<FlowSummary> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut summaries: Vec<FlowSummary> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|e| {
            let data = std::fs::read_to_string(e.path()).ok()?;
            let flow: SavedFlow = serde_json::from_str(&data).ok()?;
            Some(FlowSummary {
                id: flow.id,
                name: flow.name,
                node_count: flow.flow.nodes.len(),
                created_at: flow.created_at,
                updated_at: flow.updated_at,
                enabled: flow.enabled,
            })
        })
        .collect();
    summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    summaries
}

/// Delete `{dir}/{id}.json`. Returns `true` if a file was removed.
pub fn delete_flow(dir: &Path, id: &str) -> bool {
    if !is_safe_id(id) {
        return false;
    }
    let path = dir.join(format!("{id}.json"));
    std::fs::remove_file(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CoreNodeType, FlowDefinition, FlowEdge, FlowNode, FlowNodeType};
    use serde_json::json;
    use tempfile::tempdir;

    fn sample(id: &str, updated_at: &str) -> SavedFlow {
        SavedFlow {
            spec_version: "1".into(),
            id: id.into(),
            name: format!("Flow {id}"),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: updated_at.into(),
            enabled: false,
            flow: FlowDefinition {
                nodes: vec![FlowNode {
                    id: "e".into(),
                    node_type: FlowNodeType::Core(CoreNodeType::Entry),
                    data: json!({}),
                    position: [0.0, 0.0],
                }],
                edges: vec![],
            },
        }
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let flow = sample("abc-123", "2026-01-02T00:00:00Z");
        save_flow(dir.path(), &flow).unwrap();
        let loaded = load_flow(dir.path(), "abc-123").unwrap();
        assert_eq!(loaded, flow);
    }

    #[test]
    fn save_rejects_unsafe_id() {
        let dir = tempdir().unwrap();
        let mut flow = sample("../escape", "2026-01-02T00:00:00Z");
        flow.id = "../escape".into();
        assert!(save_flow(dir.path(), &flow).is_err());
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempdir().unwrap();
        assert!(load_flow(dir.path(), "missing-id").is_none());
    }

    #[test]
    fn list_sorts_by_updated_at_desc() {
        let dir = tempdir().unwrap();
        save_flow(dir.path(), &sample("a", "2026-01-01T00:00:00Z")).unwrap();
        save_flow(dir.path(), &sample("b", "2026-01-03T00:00:00Z")).unwrap();
        save_flow(dir.path(), &sample("c", "2026-01-02T00:00:00Z")).unwrap();
        let list = list_flows(dir.path());
        assert_eq!(list.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["b", "c", "a"]);
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempdir().unwrap();
        save_flow(dir.path(), &sample("zap", "2026-01-01T00:00:00Z")).unwrap();
        assert!(delete_flow(dir.path(), "zap"));
        assert!(load_flow(dir.path(), "zap").is_none());
        assert!(!delete_flow(dir.path(), "zap"));
    }

    #[test]
    fn list_skips_unrelated_files() {
        let dir = tempdir().unwrap();
        save_flow(dir.path(), &sample("good", "2026-01-01T00:00:00Z")).unwrap();
        std::fs::write(dir.path().join("README.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("broken.json"), "{ not valid json").unwrap();
        let list = list_flows(dir.path());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "good");
    }

    #[test]
    fn supports_optional_edge_fields() {
        let dir = tempdir().unwrap();
        let mut flow = sample("withedge", "2026-01-01T00:00:00Z");
        flow.flow.nodes.push(FlowNode {
            id: "p".into(),
            node_type: FlowNodeType::Core(CoreNodeType::Prompt),
            data: json!({"prompt": "hi"}),
            position: [10.0, 10.0],
        });
        flow.flow.edges.push(FlowEdge {
            id: "x".into(),
            source: "e".into(),
            target: "p".into(),
            source_handle: Some("out".into()),
            target_handle: Some("in".into()),
        });
        save_flow(dir.path(), &flow).unwrap();
        let loaded = load_flow(dir.path(), "withedge").unwrap();
        assert_eq!(loaded, flow);
    }
}
