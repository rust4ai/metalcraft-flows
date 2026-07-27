//! Flow execution log entries.
//!
//! Enabled by the `log` feature.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// The maximum number of entries retained on disk before old entries are
/// rotated out by [`append_flow_log`].
pub const DEFAULT_LOG_RETENTION: usize = 2000;

/// A single line in a flow's execution log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowLogEntry {
    /// ISO-8601 / RFC-3339 timestamp of when the entry was recorded.
    pub timestamp: String,
    /// Id of the flow this entry belongs to.
    pub flow_id: String,
    /// Human-readable name of the flow at the time the entry was recorded.
    pub flow_name: String,
    /// Short verb describing what happened (e.g. `"started"`, `"node_ran"`,
    /// `"failed"`). The spec does not constrain the value set.
    pub action: String,
    /// Longer free-form detail about the action.
    pub detail: String,
    /// Optional run identifier to correlate entries from a single execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// Append `entry` to `log_path` (a JSON-array file), trimming to the most
/// recent [`DEFAULT_LOG_RETENTION`] entries.
///
/// Creates the file if it doesn't exist.
pub fn append_flow_log(log_path: &Path, entry: &FlowLogEntry) -> io::Result<()> {
    let mut entries = load_flow_logs(log_path);
    entries.push(entry.clone());
    if entries.len() > DEFAULT_LOG_RETENTION {
        let drop = entries.len() - DEFAULT_LOG_RETENTION;
        entries.drain(0..drop);
    }
    let json = serde_json::to_string_pretty(&entries).map_err(io::Error::other)?;
    std::fs::write(log_path, json)
}

/// Read all log entries from `log_path`.
///
/// Returns an empty vec if the file is missing or unparseable.
pub fn load_flow_logs(log_path: &Path) -> Vec<FlowLogEntry> {
    if !log_path.exists() {
        return vec![];
    }
    std::fs::read_to_string(log_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(action: &str) -> FlowLogEntry {
        FlowLogEntry {
            timestamp: "2026-01-01T00:00:00Z".into(),
            flow_id: "f1".into(),
            flow_name: "F1".into(),
            action: action.into(),
            detail: "stuff".into(),
            run_id: Some("r1".into()),
        }
    }

    #[test]
    fn append_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.json");
        append_flow_log(&path, &entry("started")).unwrap();
        append_flow_log(&path, &entry("finished")).unwrap();
        let loaded = load_flow_logs(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].action, "started");
        assert_eq!(loaded[1].action, "finished");
    }

    #[test]
    fn missing_log_returns_empty() {
        let dir = tempdir().unwrap();
        assert!(load_flow_logs(&dir.path().join("nope.json")).is_empty());
    }

    #[test]
    fn retention_drops_oldest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log.json");
        for i in 0..(DEFAULT_LOG_RETENTION + 50) {
            let mut e = entry("tick");
            e.detail = format!("{i}");
            append_flow_log(&path, &e).unwrap();
        }
        let loaded = load_flow_logs(&path);
        assert_eq!(loaded.len(), DEFAULT_LOG_RETENTION);
        // First retained entry should be the 50th appended (0-indexed).
        assert_eq!(loaded.first().unwrap().detail, "50");
        assert_eq!(
            loaded.last().unwrap().detail,
            format!("{}", DEFAULT_LOG_RETENTION + 49)
        );
    }
}
