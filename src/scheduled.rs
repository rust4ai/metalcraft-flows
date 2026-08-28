//! **When** a flow runs — the artifact that stands beside a [`SavedFlow`].
//!
//! A [`SavedFlow`](crate::SavedFlow) says what work is. A [`ScheduledFlow`] says
//! when it happens, as whom, on which agent, with what inputs. They are separate
//! documents on purpose:
//!
//! * A flow that nothing points at cannot fire. Installing a pack, downloading a
//!   flow from a registry, or writing one by hand starts no background work —
//!   not because each install path remembers to force a flag off, but because
//!   scheduling is a second document nobody created yet.
//! * One flow can be scheduled many times over, each with its own trigger,
//!   inputs and persona. The 08:00-short and 18:00-long briefs are two artifacts
//!   pointing at one graph.
//! * Editing when something runs never rewrites what it does, so a schedule
//!   change can't corrupt a graph and a graph upgrade can't silently retime a
//!   person's morning.

use serde::{Deserialize, Serialize};

use crate::model::is_safe_scheduled_id;

/// A flow, scheduled: one trigger bound to one flow, and the agent it runs as.
///
/// This is the artifact a person creates when they say "yes, run this in the
/// background" — which is why it also carries the agent. Creating one *is*
/// arming; deleting one *is* disarming.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledFlow {
    /// Stable identifier, unique across the host. Must match
    /// `^[A-Za-z0-9_-]{1,64}$`.
    ///
    /// Opaque by convention: hosts generate `sf_<random>` rather than deriving
    /// something readable from the flow or the trigger, because a derived id goes
    /// stale the moment the thing it describes is edited — `morning-brief-0800`
    /// whose cron moved to 09:00 is a lie in every log line that mentions it. The
    /// human-readable handle is [`ScheduleSpec::name`].
    pub id: String,
    /// The [`SavedFlow`](crate::SavedFlow) this schedule runs, by its `id`.
    ///
    /// A dangling reference is possible (the flow may be deleted out from under
    /// it) and is not fatal to *parsing* — a host reports it rather than
    /// discarding the record, since silently dropping the schedule would be
    /// indistinguishable from it never having existed.
    pub flow_id: String,
    /// Whether this schedule fires. Defaults to `true`.
    ///
    /// The only switch there is. There is no flow-level master switch, because
    /// the flow no longer knows it is scheduled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// The trigger and its per-schedule overrides.
    pub schedule: ScheduleSpec,
    /// The agent instance this schedule runs as, so successive firings accumulate
    /// memory instead of waking up amnesiac. `None` on a host that does not model
    /// agents.
    ///
    /// An instance id is host-local and must never be published — a downloaded
    /// artifact carrying one would arrive pointing at somebody else's agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// The [`Suggestion::key`] this artifact was created from, when it came from a
    /// pack or registry suggestion rather than a person.
    ///
    /// **Provenance, not identity.** Nothing keys off it and it need not be
    /// unique; its one job is letting an upgrade ask "the author moved their
    /// `morning` suggestion to 07:30 — is this the artifact they mean?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_suggestion: Option<String>,
    /// ISO-8601 / RFC-3339 creation timestamp.
    pub created_at: String,
    /// ISO-8601 / RFC-3339 last-modified timestamp.
    pub updated_at: String,
}

/// A trigger plus the overrides applied to runs it starts.
///
/// Carries no identifier of its own: the enclosing [`ScheduledFlow::id`] is the
/// only handle, and [`name`](Self::name) is the only label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleSpec {
    /// The trigger, tagged by `type`: `manual` | `minutes` | `hours` | `cron`.
    #[serde(flatten)]
    pub trigger: ScheduleTrigger,
    /// Human-readable label (`"Morning brief"`). This is what a UI shows and what
    /// a host names a minted agent after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// IANA timezone the `cron` trigger is evaluated in (e.g.
    /// `"America/Detroit"`). `None` means the host's local/server time. Ignored
    /// by non-cron triggers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Inputs handed to the flow when this schedule fires, so one flow can run
    /// with different parameters on different schedules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    /// Persona override for runs this schedule starts. `None` falls back to the
    /// flow/host default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
}

/// A schedule's trigger: how its firing times are computed.
///
/// Serialized with an internal `type` tag, so a cron schedule is
/// `{ "type": "cron", "cron": "0 0 8 * * *" }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScheduleTrigger {
    /// No timed firing. The flow runs only when something runs it explicitly.
    ///
    /// Worth an artifact anyway: a manual [`ScheduledFlow`] names the agent (and
    /// inputs, and persona) a hand-run resolves to, which is the only way to say
    /// "when I run this myself, remember it".
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
        /// The cron expression, e.g. `"0 0 8 * * *"` — daily at 08:00.
        ///
        /// Not parsed here (§1.3): the dialect belongs to whichever runtime
        /// computes firing times, and this crate takes no cron dependency in
        /// order to say so. Worth knowing which dialect you are writing for,
        /// though — the reference runtime wants **six** fields, seconds first,
        /// and rejects the five-field POSIX form outright rather than assuming
        /// a zero second.
        cron: String,
    },
}

/// A schedule an author *suggests* for a flow they publish — in a pack sidecar or
/// a registry listing.
///
/// Inert: installing a flow never turns a suggestion into a [`ScheduledFlow`].
/// The install report offers them and a person accepts, at which point the
/// created artifact records [`key`](Self::key) as its
/// [`from_suggestion`](ScheduledFlow::from_suggestion).
///
/// A suggestion keeps an id and a live schedule does not, because the key lives
/// in the *author's* namespace: it is how the author refers to this suggestion
/// across versions of their pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    /// The author's stable key for this suggestion (`"morning"`), unique within
    /// the flow it accompanies.
    pub key: String,
    /// The schedule being suggested.
    pub schedule: ScheduleSpec,
}

fn default_true() -> bool {
    true
}

impl ScheduleTrigger {
    /// A one-line human description: `"Every 15 minute(s)"`, ``"Cron `0 0 8 * * *`
    /// (America/Detroit)"``.
    ///
    /// Lives here so a pod, a desktop client and a web listing describe the same
    /// trigger the same way rather than each inventing a phrasing.
    pub fn describe(&self, timezone: Option<&str>) -> String {
        match self {
            ScheduleTrigger::Manual => "Manual (runs only when triggered)".to_string(),
            ScheduleTrigger::Minutes { interval } => format!("Every {interval} minute(s)"),
            ScheduleTrigger::Hours { interval } => format!("Every {interval} hour(s)"),
            ScheduleTrigger::Cron { cron } => match timezone {
                Some(tz) => format!("Cron `{cron}` ({tz})"),
                None => format!("Cron `{cron}`"),
            },
        }
    }

    /// Whether this trigger ever fires on its own.
    pub fn is_timed(&self) -> bool {
        !matches!(self, ScheduleTrigger::Manual)
    }
}

impl ScheduleSpec {
    /// [`ScheduleTrigger::describe`], with this spec's timezone applied.
    pub fn describe(&self) -> String {
        self.trigger.describe(self.timezone.as_deref())
    }

    /// The label to show, falling back to the trigger description when unnamed.
    pub fn display_name(&self) -> String {
        match self.name.as_deref().filter(|n| !n.trim().is_empty()) {
            Some(n) => n.to_string(),
            None => self.describe(),
        }
    }
}

impl ScheduledFlow {
    /// Whether this schedule should be considered by a scheduler right now:
    /// enabled, and on a trigger that fires by itself.
    pub fn is_armed_timer(&self) -> bool {
        self.enabled && self.schedule.trigger.is_timed()
    }
}

/// Validate a [`ScheduledFlow`] against the spec.
///
/// Returns all detected errors; an empty `Vec` means conformant. Cron *syntax* is
/// deliberately not checked — this crate does not depend on a cron parser, and
/// the host that computes firing times is the one that must agree with itself
/// about what parses.
pub fn validate_scheduled(sf: &ScheduledFlow) -> Vec<crate::validate::ValidationError> {
    use crate::validate::ValidationError;
    let mut errors = Vec::new();

    if !is_safe_scheduled_id(&sf.id) {
        errors.push(ValidationError::InvalidSchedule {
            message: format!(
                "invalid scheduled flow id {:?}: must match [A-Za-z0-9_-]{{1,64}}",
                sf.id
            ),
        });
    }
    if !crate::model::is_safe_id(&sf.flow_id) {
        errors.push(ValidationError::InvalidSchedule {
            message: format!("invalid flow_id {:?}", sf.flow_id),
        });
    }
    match &sf.schedule.trigger {
        ScheduleTrigger::Minutes { interval } | ScheduleTrigger::Hours { interval } => {
            if *interval == 0 {
                errors.push(ValidationError::InvalidSchedule {
                    message: format!("schedule {:?} interval must be positive", sf.id),
                });
            }
        }
        ScheduleTrigger::Cron { cron } => {
            if cron.trim().is_empty() {
                errors.push(ValidationError::InvalidSchedule {
                    message: format!("schedule {:?} has an empty cron expression", sf.id),
                });
            }
        }
        ScheduleTrigger::Manual => {}
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> ScheduledFlow {
        ScheduledFlow {
            id: "sf_9c31a4".into(),
            flow_id: "morning-brief".into(),
            enabled: true,
            schedule: ScheduleSpec {
                trigger: ScheduleTrigger::Cron {
                    cron: "0 0 8 * * *".into(),
                },
                name: Some("Morning brief".into()),
                timezone: Some("America/Detroit".into()),
                inputs: Some(json!({ "depth": "short" })),
                persona: Some("morning-briefer".into()),
            },
            instance_id: Some("inst_4f2".into()),
            from_suggestion: Some("morning".into()),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn round_trips_with_a_flat_trigger() {
        let sf = sample();
        let v = serde_json::to_value(&sf).unwrap();
        // The trigger stays flat inside `schedule`, as authored.
        assert_eq!(v["schedule"]["type"], "cron");
        assert_eq!(v["schedule"]["cron"], "0 0 8 * * *");
        assert_eq!(v["from_suggestion"], "morning");
        let back: ScheduledFlow = serde_json::from_value(v).unwrap();
        assert_eq!(sf, back);
    }

    #[test]
    fn enabled_defaults_to_true_and_optionals_stay_absent() {
        let sf: ScheduledFlow = serde_json::from_value(json!({
            "id": "sf_1", "flow_id": "f",
            "schedule": { "type": "manual" },
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert!(sf.enabled);
        assert!(sf.instance_id.is_none());
        assert!(sf.from_suggestion.is_none());
        let v = serde_json::to_value(&sf).unwrap();
        assert!(v.get("instance_id").is_none());
        assert!(v.get("from_suggestion").is_none());
    }

    #[test]
    fn manual_is_never_a_timer_even_when_enabled() {
        let mut sf = sample();
        sf.schedule.trigger = ScheduleTrigger::Manual;
        assert!(sf.enabled);
        assert!(!sf.is_armed_timer());
        // …and a disabled cron is not one either.
        let mut sf = sample();
        sf.enabled = false;
        assert!(!sf.is_armed_timer());
    }

    #[test]
    fn describes_a_trigger_the_same_way_everywhere() {
        assert_eq!(
            sample().schedule.describe(),
            "Cron `0 0 8 * * *` (America/Detroit)"
        );
        assert_eq!(
            ScheduleTrigger::Minutes { interval: 15 }.describe(None),
            "Every 15 minute(s)"
        );
        // An unnamed schedule falls back to its description rather than showing
        // an opaque id in a list of automations.
        let mut sf = sample();
        sf.schedule.name = None;
        assert_eq!(sf.schedule.display_name(), "Cron `0 0 8 * * *` (America/Detroit)");
    }

    #[test]
    fn validation_catches_the_malformed_cases() {
        assert!(validate_scheduled(&sample()).is_empty());

        let mut bad = sample();
        bad.id = "../escape".into();
        assert_eq!(validate_scheduled(&bad).len(), 1);

        let mut bad = sample();
        bad.schedule.trigger = ScheduleTrigger::Minutes { interval: 0 };
        assert_eq!(validate_scheduled(&bad).len(), 1);

        let mut bad = sample();
        bad.schedule.trigger = ScheduleTrigger::Cron { cron: "  ".into() };
        assert_eq!(validate_scheduled(&bad).len(), 1);

        let mut bad = sample();
        bad.flow_id = "not a flow id".into();
        assert_eq!(validate_scheduled(&bad).len(), 1);
    }
}
