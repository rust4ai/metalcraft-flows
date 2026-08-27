//! Reading scheduling out of a pre-v3 flow document.
//!
//! Before spec v3 a flow carried its own scheduling three different ways: a
//! top-level `schedules` array, else a trigger on the entry node's `data`, else
//! nothing (meaning manual). [`extract`] collapses all three into the same
//! shape and hands back a clean v3 [`SavedFlow`] alongside the schedules that
//! were living inside it.
//!
//! Deliberately pure and id-free. Minting ids, resolving which agent a schedule
//! was armed with, and deciding what to do with a manual trigger are all host
//! concerns — this only answers "what scheduling was in this document?", which
//! is the part that must be identical everywhere.

use serde_json::Value;

use crate::model::SavedFlow;
use crate::scheduled::{ScheduleSpec, ScheduleTrigger};

/// One schedule lifted out of a legacy document.
///
/// Not a [`ScheduledFlow`](crate::scheduled::ScheduledFlow) yet: it has no id,
/// no timestamps and no agent, because none of those are in the source document.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedSchedule {
    /// The legacy `schedules[].id` (`"morning"`), or `"default"` for a trigger
    /// synthesized from the entry node.
    ///
    /// Two jobs, both of which end here: it is what the host looks up in its
    /// arming records to find the agent this schedule ran as, and it becomes the
    /// new artifact's
    /// [`from_suggestion`](crate::scheduled::ScheduledFlow::from_suggestion).
    pub key: String,
    /// Whether this schedule was actually firing — the **conjunction** of the
    /// flow's master switch and the schedule's own toggle.
    ///
    /// Both had to be on for anything to happen, so both have to be on for the
    /// migrated artifact to fire. Migration must never start something that was
    /// not already running.
    pub enabled: bool,
    /// The trigger and its overrides.
    pub schedule: ScheduleSpec,
}

/// A legacy document, split.
#[derive(Debug, Clone, PartialEq)]
pub struct Extraction {
    /// The document as a v3 flow: scheduling removed, `spec_version` bumped.
    pub flow: SavedFlow,
    /// The schedules that were inside it, in document order. Empty when the flow
    /// was never scheduled — including the common "no `schedules`, no entry
    /// trigger" case, which meant manual and needs no artifact.
    pub schedules: Vec<ExtractedSchedule>,
    /// Whether anything actually changed. `false` for a document already at v3,
    /// which lets a migrator skip it without comparing bytes.
    pub changed: bool,
}

/// Split a flow document into a v3 flow plus the schedules it was carrying.
///
/// Accepts any spec version. Errors only if the document is not a flow at all.
///
/// Manual triggers are returned like any other. Whether a manual schedule
/// deserves an artifact is a host question — it does if the host had armed it (it
/// names the agent a hand-run resolves to) and does not otherwise — and the host
/// is the one holding the arming records.
pub fn extract(doc: &Value) -> Result<Extraction, String> {
    let mut flow: SavedFlow =
        serde_json::from_value(doc.clone()).map_err(|e| format!("not a flow document: {e}"))?;

    let had_array = doc
        .get("schedules")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    let flow_enabled = doc
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let schedules = if had_array {
        from_array(doc, flow_enabled)
    } else {
        from_entry_node(doc, flow_enabled)
            .map(|s| vec![s])
            .unwrap_or_default()
    };

    let already_v3 = flow.spec_version == "3"
        && doc.get("schedules").is_none()
        && doc.get("enabled").is_none();
    flow.spec_version = "3".to_string();
    let stripped = strip_entry_scheduling(&mut flow);

    Ok(Extraction {
        flow,
        schedules,
        changed: !already_v3 || stripped,
    })
}

/// Lift the top-level `schedules` array.
fn from_array(doc: &Value, flow_enabled: bool) -> Vec<ExtractedSchedule> {
    let Some(array) = doc.get("schedules").and_then(Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            // An empty or missing id was never legal, but a document on disk is
            // not obliged to be legal — and refusing to migrate it would strand
            // a schedule that had been happily firing.
            let key = raw
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("schedule-{i}"));
            let enabled = flow_enabled
                && raw
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
            ExtractedSchedule {
                enabled,
                schedule: ScheduleSpec {
                    trigger: trigger_from_tagged(raw),
                    // The old id was often the only label a schedule had, so it
                    // becomes the name rather than being dropped — otherwise
                    // every unnamed schedule migrates into a blank row.
                    name: raw
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_string)
                        .or_else(|| Some(key.clone())),
                    timezone: string_field(raw, "timezone"),
                    inputs: raw.get("inputs").filter(|v| !v.is_null()).cloned(),
                    persona: string_field(raw, "persona"),
                },
                key,
            }
        })
        .collect()
}

/// Synthesize the single schedule a legacy entry node described, if it described
/// one. The entry node's `persona` rides along, as it did when the daemon read
/// this shape directly.
fn from_entry_node(doc: &Value, flow_enabled: bool) -> Option<ExtractedSchedule> {
    let data = entry_data(doc)?;
    let schedule_type = data.get("schedule_type").and_then(Value::as_str)?;
    let interval = data.get("interval").and_then(Value::as_u64).unwrap_or(0);
    let trigger = match schedule_type {
        "minutes" => ScheduleTrigger::Minutes { interval },
        "hours" => ScheduleTrigger::Hours { interval },
        "cron" => ScheduleTrigger::Cron {
            cron: data
                .get("cron")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        // "manual", and any unknown legacy value, degraded to manual then too.
        _ => ScheduleTrigger::Manual,
    };
    Some(ExtractedSchedule {
        key: "default".to_string(),
        enabled: flow_enabled,
        schedule: ScheduleSpec {
            trigger,
            name: None,
            timezone: None,
            inputs: None,
            persona: string_field(data, "persona"),
        },
    })
}

/// Remove the entry node's scheduling keys, leaving everything else (`inputs`,
/// `persona`, `max_steps`) alone. Returns whether anything was removed.
///
/// Without this a migrated flow keeps a `schedule_type: "cron"` that no longer
/// does anything — the kind of leftover that gets read as truth by the next
/// person to open the file.
fn strip_entry_scheduling(flow: &mut SavedFlow) -> bool {
    use crate::model::{CoreNodeType, FlowNodeType};
    let mut stripped = false;
    for node in &mut flow.flow.nodes {
        if !matches!(node.node_type, FlowNodeType::Core(CoreNodeType::Entry)) {
            continue;
        }
        if let Some(obj) = node.data.as_object_mut() {
            for key in ["schedule_type", "cron", "interval"] {
                stripped |= obj.remove(key).is_some();
            }
        }
    }
    stripped
}

/// Read a `{ "type": "cron", "cron": "…" }`-style trigger out of a raw schedule
/// object, degrading to manual on anything unrecognized.
fn trigger_from_tagged(raw: &Value) -> ScheduleTrigger {
    match raw.get("type").and_then(Value::as_str).unwrap_or("manual") {
        "minutes" => ScheduleTrigger::Minutes {
            interval: raw.get("interval").and_then(Value::as_u64).unwrap_or(0),
        },
        "hours" => ScheduleTrigger::Hours {
            interval: raw.get("interval").and_then(Value::as_u64).unwrap_or(0),
        },
        "cron" => ScheduleTrigger::Cron {
            cron: raw
                .get("cron")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => ScheduleTrigger::Manual,
    }
}

fn entry_data(doc: &Value) -> Option<&Value> {
    doc.get("flow")?
        .get("nodes")?
        .as_array()?
        .iter()
        .find(|n| n.get("node_type").and_then(Value::as_str) == Some("entry"))?
        .get("data")
}

fn string_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc_with(extra: Value) -> Value {
        let mut base = json!({
            "spec_version": "2",
            "id": "morning-brief",
            "name": "Morning brief",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry", "data": {}, "position": [0, 0] }
            ], "edges": [] }
        });
        let (Value::Object(base_obj), Value::Object(extra_obj)) = (&mut base, extra) else {
            panic!("objects");
        };
        for (k, v) in extra_obj {
            base_obj.insert(k, v);
        }
        base
    }

    #[test]
    fn lifts_the_top_level_array_and_bumps_the_document() {
        let doc = doc_with(json!({
            "enabled": true,
            "schedules": [
                { "id": "morning", "type": "cron", "cron": "0 0 8 * * *",
                  "timezone": "America/Detroit", "persona": "briefer",
                  "inputs": { "depth": "short" } },
                { "id": "evening", "type": "cron", "cron": "0 0 18 * * *", "enabled": false }
            ]
        }));
        let out = extract(&doc).unwrap();

        assert_eq!(out.flow.spec_version, "3");
        assert!(out.changed);
        assert_eq!(out.schedules.len(), 2);

        let morning = &out.schedules[0];
        assert_eq!(morning.key, "morning");
        assert!(morning.enabled);
        assert_eq!(morning.schedule.timezone.as_deref(), Some("America/Detroit"));
        assert_eq!(morning.schedule.persona.as_deref(), Some("briefer"));
        assert_eq!(morning.schedule.inputs, Some(json!({ "depth": "short" })));
        assert_eq!(
            morning.schedule.trigger,
            ScheduleTrigger::Cron { cron: "0 0 8 * * *".into() }
        );

        // A schedule that was off stays off.
        assert!(!out.schedules[1].enabled);
    }

    #[test]
    fn a_disabled_flow_migrates_to_nothing_that_fires() {
        // The safety property: `enabled` is the conjunction. A flow whose master
        // switch was off had two live-looking crons that fired nothing, and must
        // not start firing because the switch no longer exists to hold them back.
        let doc = doc_with(json!({
            "enabled": false,
            "schedules": [
                { "id": "a", "type": "cron", "cron": "0 0 8 * * *" },
                { "id": "b", "type": "minutes", "interval": 5, "enabled": true }
            ]
        }));
        let out = extract(&doc).unwrap();
        assert_eq!(out.schedules.len(), 2);
        assert!(out.schedules.iter().all(|s| !s.enabled));
    }

    #[test]
    fn synthesizes_from_a_legacy_entry_node() {
        let doc = json!({
            "spec_version": "1",
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "enabled": true,
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry",
                  "data": { "schedule_type": "cron", "cron": "0 0 9 * * *",
                            "persona": "briefer", "max_steps": 40 },
                  "position": [0, 0] }
            ], "edges": [] }
        });
        let out = extract(&doc).unwrap();

        assert_eq!(out.schedules.len(), 1);
        let s = &out.schedules[0];
        assert_eq!(s.key, "default");
        assert!(s.enabled);
        assert_eq!(s.schedule.trigger, ScheduleTrigger::Cron { cron: "0 0 9 * * *".into() });
        assert_eq!(s.schedule.persona.as_deref(), Some("briefer"));

        // The dead scheduling keys are gone; everything else on the node stays.
        let data = &out.flow.flow.nodes[0].data;
        assert!(data.get("schedule_type").is_none());
        assert!(data.get("cron").is_none());
        assert_eq!(data["persona"], "briefer");
        assert_eq!(data["max_steps"], 40);
    }

    #[test]
    fn the_array_wins_over_the_entry_node() {
        let doc = json!({
            "spec_version": "2",
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "enabled": true,
            "schedules": [ { "id": "morning", "type": "hours", "interval": 6 } ],
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry",
                  "data": { "schedule_type": "cron", "cron": "0 0 0 * * *" }, "position": [0, 0] }
            ], "edges": [] }
        });
        let out = extract(&doc).unwrap();
        assert_eq!(out.schedules.len(), 1);
        assert_eq!(out.schedules[0].schedule.trigger, ScheduleTrigger::Hours { interval: 6 });
    }

    #[test]
    fn an_unscheduled_flow_yields_no_artifacts() {
        let out = extract(&doc_with(json!({}))).unwrap();
        assert!(out.schedules.is_empty());
        // Still "changed": the document was v2 and is now v3.
        assert!(out.changed);
    }

    #[test]
    fn a_v3_document_is_left_alone() {
        let doc = json!({
            "spec_version": "3",
            "id": "f", "name": "F",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "flow": { "nodes": [
                { "id": "entry", "node_type": "entry", "data": { "persona": "p" }, "position": [0, 0] }
            ], "edges": [] }
        });
        let out = extract(&doc).unwrap();
        assert!(!out.changed);
        assert!(out.schedules.is_empty());

        // Extraction is a fixed point: running it on its own output changes nothing.
        let again = extract(&serde_json::to_value(&out.flow).unwrap()).unwrap();
        assert!(!again.changed);
        assert_eq!(again.flow, out.flow);
    }

    #[test]
    fn an_unnamed_schedule_keeps_its_old_id_as_a_label() {
        let doc = doc_with(json!({
            "enabled": true,
            "schedules": [ { "id": "morning", "type": "manual" } ]
        }));
        let out = extract(&doc).unwrap();
        assert_eq!(out.schedules[0].schedule.name.as_deref(), Some("morning"));

        // An explicit name wins.
        let doc = doc_with(json!({
            "enabled": true,
            "schedules": [ { "id": "morning", "name": "Wake up", "type": "manual" } ]
        }));
        let out = extract(&doc).unwrap();
        assert_eq!(out.schedules[0].schedule.name.as_deref(), Some("Wake up"));
    }

    #[test]
    fn a_malformed_schedule_still_migrates() {
        // No id, unknown trigger type. Illegal, but it is on somebody's disk, and
        // stranding it would be worse than degrading it.
        let doc = doc_with(json!({
            "enabled": true,
            "schedules": [ { "type": "sunspots" }, { "id": "  ", "type": "cron", "cron": "0 0 1 * * *" } ]
        }));
        let out = extract(&doc).unwrap();
        assert_eq!(out.schedules[0].key, "schedule-0");
        assert_eq!(out.schedules[0].schedule.trigger, ScheduleTrigger::Manual);
        assert_eq!(out.schedules[1].key, "schedule-1");
    }

    #[test]
    fn a_non_flow_document_is_an_error() {
        assert!(extract(&json!({ "hello": "world" })).is_err());
    }
}
