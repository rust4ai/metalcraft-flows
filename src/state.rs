//! The running `variables` state threaded through a flow, plus dotted-path
//! helpers shared with [`crate::template`].
//!
//! State is a single JSON object. Nodes read and write named variables; the
//! reserved keys are:
//!
//! - `_last` — the payload of the edge just traversed into the current node
//!   (its typed input).
//! - `_inputs` — an immutable copy of the entry inputs the run was seeded with.
//! - `_run` — run metadata (reserved).

use crate::nodes::InputSpec;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Look up a dotted path (`"a.b.c"`) within a JSON value.
///
/// An empty path (or `"."`) returns the root. Returns `None` if any segment is
/// missing or traverses a non-object.
pub fn lookup_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Some(root);
    }
    let mut cur = root;
    for seg in path.split('.') {
        if seg.is_empty() {
            continue;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// The mutable variable bag for one flow run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Variables {
    root: Value,
}

impl Variables {
    /// An empty state (`{}`).
    pub fn new() -> Self {
        Self { root: Value::Object(Map::new()) }
    }

    /// Wrap an existing JSON object as state. A non-object is replaced by `{}`.
    pub fn from_value(value: Value) -> Self {
        if value.is_object() {
            Self { root: value }
        } else {
            Self::new()
        }
    }

    /// Seed state from an entry node's declared `inputs` and the caller-supplied
    /// argument object. Required inputs missing from `args` and lacking a default
    /// are reported by name (so the caller can reject the invocation); present
    /// values and defaults are written, and `_inputs` is set to the seeded map.
    pub fn seed_from_inputs(
        inputs: &BTreeMap<String, InputSpec>,
        args: &Value,
    ) -> (Self, Vec<String>) {
        let mut state = Self::new();
        let mut missing = Vec::new();
        let mut seeded = Map::new();
        for (name, spec) in inputs {
            let provided = args.get(name).cloned();
            let value = match provided {
                Some(v) => Some(v),
                None => match &spec.default {
                    Some(d) => Some(d.clone()),
                    None => {
                        if spec.required {
                            missing.push(name.clone());
                        }
                        None
                    }
                },
            };
            if let Some(v) = value {
                state.set(name, v.clone());
                seeded.insert(name.clone(), v);
            }
        }
        state.set("_inputs", Value::Object(seeded));
        (state, missing)
    }

    /// Borrow the underlying JSON object.
    pub fn as_value(&self) -> &Value {
        &self.root
    }

    /// Consume and return the underlying JSON object.
    pub fn into_value(self) -> Value {
        self.root
    }

    /// Look up a dotted path within the state.
    pub fn get(&self, path: &str) -> Option<&Value> {
        lookup_path(&self.root, path)
    }

    /// Set a **top-level** variable. (Nested assignment uses [`Self::set_path`].)
    pub fn set(&mut self, name: &str, value: Value) {
        if let Value::Object(map) = &mut self.root {
            map.insert(name.to_string(), value);
        }
    }

    /// Set the reserved `_last` edge payload (the next node's input).
    pub fn set_last(&mut self, value: Value) {
        self.set("_last", value);
    }

    /// Set a value at a dotted path, creating intermediate objects as needed.
    /// A leading/empty segment is ignored; an empty path is a no-op.
    pub fn set_path(&mut self, path: &str, value: Value) {
        let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return;
        }
        let mut cur = &mut self.root;
        for seg in &segments[..segments.len() - 1] {
            if !cur.is_object() {
                *cur = Value::Object(Map::new());
            }
            let map = cur.as_object_mut().expect("just ensured object");
            cur = map.entry(seg.to_string()).or_insert_with(|| Value::Object(Map::new()));
        }
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        cur.as_object_mut()
            .expect("just ensured object")
            .insert(segments[segments.len() - 1].to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lookup_nested_and_root() {
        let v = json!({ "a": { "b": 2 } });
        assert_eq!(lookup_path(&v, "a.b"), Some(&json!(2)));
        assert_eq!(lookup_path(&v, "."), Some(&v));
        assert_eq!(lookup_path(&v, "a.missing"), None);
    }

    #[test]
    fn set_and_get() {
        let mut s = Variables::new();
        s.set("temp", json!(18));
        s.set_last(json!({ "celsius": 5 }));
        assert_eq!(s.get("temp"), Some(&json!(18)));
        assert_eq!(s.get("_last.celsius"), Some(&json!(5)));
    }

    #[test]
    fn set_path_creates_intermediates() {
        let mut s = Variables::new();
        s.set_path("triage.severity", json!("P0"));
        assert_eq!(s.get("triage.severity"), Some(&json!("P0")));
    }

    #[test]
    fn seed_reports_missing_required() {
        let mut inputs = BTreeMap::new();
        inputs.insert("repo".to_string(), InputSpec { type_name: "string".into(), required: true, default: None });
        inputs.insert("since".to_string(), InputSpec { type_name: "string".into(), required: false, default: Some(json!("24h")) });

        let (state, missing) = Variables::seed_from_inputs(&inputs, &json!({ "repo": "acme/app" }));
        assert!(missing.is_empty());
        assert_eq!(state.get("repo"), Some(&json!("acme/app")));
        assert_eq!(state.get("since"), Some(&json!("24h")));

        let (_state2, missing2) = Variables::seed_from_inputs(&inputs, &json!({}));
        assert_eq!(missing2, vec!["repo".to_string()]);
    }
}
