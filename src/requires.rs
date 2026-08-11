//! Declared dependencies of a flow: the integration packs and tool surface it
//! needs in order to run.
//!
//! A [`SavedFlow`](crate::SavedFlow) may carry an optional
//! [`Requires`](crate::Requires) block naming the integration packs it depends
//! on (by id, with an optional semver range and/or exact content hash) and the
//! flat tool names it invokes. This lets a host validate dependencies at import,
//! enable, or run-preflight time — turning what is otherwise a silent
//! mid-execution failure (a `tool` node whose providing pack is disabled) into an
//! actionable "install & enable pack X" prompt.
//!
//! This crate deliberately knows nothing about any registry or filesystem:
//!
//! - [`derive_requires`] extracts the *shape* of the dependency (pack ids and
//!   tool names) from a flow's graph alone. Version ranges and hashes are left
//!   unset — the host, which knows its installed packs, stamps those.
//! - [`check_requirements`] is a pure function: the caller passes in the packs it
//!   has available (id + version + optional hash) and gets back the list of
//!   unmet requirements. No I/O, so both the agent and the web backend reuse it.

use crate::model::SavedFlow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The dependencies a flow declares.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Requires {
    /// Integration packs this flow needs, each with an optional version/hash
    /// contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<PackRequirement>,
    /// Flat tool names the flow invokes (the real API surface — `tool` nodes bind
    /// by bare name). Auto-derived by [`derive_requires`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

impl Requires {
    /// Whether this block declares no dependencies at all.
    pub fn is_empty(&self) -> bool {
        self.packs.is_empty() && self.tools.is_empty()
    }
}

/// A single integration-pack dependency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackRequirement {
    /// Pack id / slug — the stable identity. Must match
    /// `^[a-z0-9][a-z0-9_-]{0,63}$`.
    pub id: String,
    /// Semver range the pack must satisfy, e.g. `">=1.2.0, <2.0.0"`. `None` means
    /// any installed version is acceptable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional integrity lock: the exact canonical content hash (64 lowercase
    /// hex chars) the author resolved against. When set, a host may refuse to
    /// install a pack whose content hash differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    /// Human-readable reason, surfaced in an install/enable prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When `true`, an unmet requirement is a warning rather than a hard failure —
    /// the flow is expected to degrade gracefully.
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
    /// The concrete version the authoring environment resolved to, recorded as a
    /// reproducibility hint (a lock). Advisory; hosts may re-resolve within
    /// [`version`](Self::version).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl PackRequirement {
    /// Construct a bare requirement on `id` with no version/hash constraint.
    pub fn new(id: impl Into<String>) -> Self {
        PackRequirement {
            id: id.into(),
            version: None,
            content_sha256: None,
            reason: None,
            optional: false,
            resolved_version: None,
        }
    }
}

/// A pack the host has available (installed and enabled), against which a
/// [`Requires`] block is evaluated by [`check_requirements`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailablePack {
    /// Pack id / slug.
    pub id: String,
    /// The installed version string (parsed as semver when a range is checked).
    pub version: String,
    /// The pack's canonical content hash, if the host knows it.
    pub content_sha256: Option<String>,
}

/// One reason a flow's requirement is not met by the host environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unmet {
    /// No available pack has this id.
    MissingPack {
        /// The required pack id.
        id: String,
        /// Whether the requirement was declared `optional`.
        optional: bool,
    },
    /// A pack with this id exists but its version is outside the required range.
    VersionConflict {
        /// The required pack id.
        id: String,
        /// The required semver range.
        need: String,
        /// The version actually available.
        have: String,
        /// Whether the requirement was declared `optional`.
        optional: bool,
    },
    /// A pack with this id exists but its content hash does not match the pin
    /// (or the host could not report a hash to compare).
    HashMismatch {
        /// The required pack id.
        id: String,
        /// The required content hash.
        need: String,
        /// The available pack's content hash, if any.
        have: Option<String>,
        /// Whether the requirement was declared `optional`.
        optional: bool,
    },
    /// A required tool name is not provided by any available pack.
    MissingTool {
        /// The required tool name.
        name: String,
    },
}

impl Unmet {
    /// Whether this shortfall came from an `optional` requirement (and so should
    /// be treated as a warning, not a hard failure). [`Unmet::MissingTool`] is
    /// always considered required.
    pub fn is_optional(&self) -> bool {
        match self {
            Unmet::MissingPack { optional, .. }
            | Unmet::VersionConflict { optional, .. }
            | Unmet::HashMismatch { optional, .. } => *optional,
            Unmet::MissingTool { .. } => false,
        }
    }
}

/// Derive the *shape* of a flow's dependencies from its graph alone.
///
/// Fills [`Requires::packs`] (ids only — no version or hash, which this crate
/// cannot know) and [`Requires::tools`]. A host enriches the pack entries with
/// versions/hashes from its own inventory before persisting.
///
/// Two graph signals name a pack:
/// - a node whose `data` carries a `"pack"` string (sub-agent scoping), and
/// - a vendor-namespaced custom `node_type` (`"vendor:action"` → `vendor`).
///
/// Tool names come from every [`tool`](crate::CoreNodeType::Tool) node's
/// `tool_name`. All output is sorted and de-duplicated.
pub fn derive_requires(flow: &SavedFlow) -> Requires {
    use crate::model::{CoreNodeType, FlowNodeType};

    let mut pack_ids: BTreeSet<String> = BTreeSet::new();
    let mut tools: BTreeSet<String> = BTreeSet::new();

    for node in &flow.flow.nodes {
        // (1) explicit `data.pack` scoping (sub_agent and any future node).
        if let Some(pack) = node.data.get("pack").and_then(|v| v.as_str())
            && !pack.is_empty()
        {
            pack_ids.insert(pack.to_string());
        }

        match &node.node_type {
            // (2) vendor-namespaced custom node types → the vendor prefix.
            FlowNodeType::Custom(s) => {
                if let Some((vendor, _)) = s.split_once(':')
                    && !vendor.is_empty()
                {
                    pack_ids.insert(vendor.to_string());
                }
            }
            // (3) tool nodes → the invoked tool name (the real binding).
            FlowNodeType::Core(CoreNodeType::Tool) => {
                if let Some(name) = node.data.get("tool_name").and_then(|v| v.as_str())
                    && !name.is_empty()
                {
                    tools.insert(name.to_string());
                }
            }
            FlowNodeType::Core(_) => {}
        }
    }

    Requires {
        packs: pack_ids.into_iter().map(PackRequirement::new).collect(),
        tools: tools.into_iter().collect(),
    }
}

/// Evaluate a flow's pack requirements against what the host has available.
///
/// Pure and I/O-free: the caller supplies the packs it has installed and enabled.
/// Returns every unmet requirement (see [`Unmet`]); an empty `Vec` means all pack
/// requirements are satisfied. Tool-surface requirements are checked separately
/// by [`check_tools`], since not every host has a flat tool inventory to compare
/// against.
///
/// Version matching uses semver: the requirement's [`version`](PackRequirement::version)
/// is parsed as a range and the available version as a concrete version. A range
/// that fails to parse is treated as unsatisfiable here (validate the flow first
/// with [`crate::validate`] to surface malformed ranges as errors instead).
pub fn check_requirements(req: &Requires, available: &[AvailablePack]) -> Vec<Unmet> {
    let mut unmet = Vec::new();

    for pr in &req.packs {
        let Some(found) = available.iter().find(|a| a.id == pr.id) else {
            unmet.push(Unmet::MissingPack {
                id: pr.id.clone(),
                optional: pr.optional,
            });
            continue;
        };

        if let Some(range) = &pr.version {
            let satisfied = match (
                semver::VersionReq::parse(range),
                semver::Version::parse(&found.version),
            ) {
                (Ok(vr), Ok(v)) => vr.matches(&v),
                // Unparseable range or version: cannot prove satisfaction.
                _ => false,
            };
            if !satisfied {
                unmet.push(Unmet::VersionConflict {
                    id: pr.id.clone(),
                    need: range.clone(),
                    have: found.version.clone(),
                    optional: pr.optional,
                });
                // Don't also emit a hash mismatch for the same pack; the version
                // problem is the actionable one.
                continue;
            }
        }

        if let Some(need) = &pr.content_sha256
            && found.content_sha256.as_deref() != Some(need.as_str())
        {
            unmet.push(Unmet::HashMismatch {
                id: pr.id.clone(),
                need: need.clone(),
                have: found.content_sha256.clone(),
                optional: pr.optional,
            });
        }
    }

    unmet
}

/// Check a flow's required tool names against a host's flat tool inventory.
///
/// Returns an [`Unmet::MissingTool`] for each name in [`Requires::tools`] that is
/// not present in `available_tools`.
pub fn check_tools(req: &Requires, available_tools: &[String]) -> Vec<Unmet> {
    req.tools
        .iter()
        .filter(|name| !available_tools.iter().any(|a| a == *name))
        .map(|name| Unmet::MissingTool { name: name.clone() })
        .collect()
}

/// Whether a pack id is well-formed: `^[a-z0-9][a-z0-9_-]{0,63}$`.
pub(crate) fn is_valid_pack_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 {
        return false;
    }
    let mut chars = id.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Whether a string is a 64-character lowercase hex digest.
pub(crate) fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CoreNodeType, FlowDefinition, FlowNode, FlowNodeType, SavedFlow};
    use serde_json::json;

    fn saved_with(nodes: Vec<FlowNode>) -> SavedFlow {
        SavedFlow {
            spec_version: "2".into(),
            id: "f".into(),
            name: "F".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            enabled: false,
            schedules: vec![],
            requires: None,
            flow: FlowDefinition {
                nodes,
                edges: vec![],
            },
        }
    }

    fn node(id: &str, ty: FlowNodeType, data: serde_json::Value) -> FlowNode {
        FlowNode {
            id: id.into(),
            node_type: ty,
            data,
            position: [0.0, 0.0],
        }
    }

    #[test]
    fn derive_finds_packs_and_tools_sorted_deduped() {
        let sf = saved_with(vec![
            node(
                "t1",
                FlowNodeType::Core(CoreNodeType::Tool),
                json!({ "tool_name": "cloudflare_purge_cache" }),
            ),
            node(
                "t2",
                FlowNodeType::Core(CoreNodeType::Tool),
                json!({ "tool_name": "cloudflare_list_zones" }),
            ),
            node(
                "sa",
                FlowNodeType::Core(CoreNodeType::SubAgent),
                json!({ "task": "do it", "pack": "cloudflare", "tool_set": "all" }),
            ),
            node(
                "cust",
                FlowNodeType::Custom("linear:create_issue".into()),
                json!({}),
            ),
        ]);
        let req = derive_requires(&sf);
        let ids: Vec<&str> = req.packs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["cloudflare", "linear"]);
        assert_eq!(
            req.tools,
            vec![
                "cloudflare_list_zones".to_string(),
                "cloudflare_purge_cache".to_string()
            ]
        );
    }

    #[test]
    fn check_missing_and_satisfied() {
        let req = Requires {
            packs: vec![PackRequirement {
                id: "cloudflare".into(),
                version: Some(">=1.2.0, <2.0.0".into()),
                ..PackRequirement::new("cloudflare")
            }],
            tools: vec![],
        };
        // Missing entirely.
        let unmet = check_requirements(&req, &[]);
        assert!(matches!(unmet.as_slice(), [Unmet::MissingPack { id, .. }] if id == "cloudflare"));
        // Present and in range.
        let ok = check_requirements(
            &req,
            &[AvailablePack {
                id: "cloudflare".into(),
                version: "1.3.1".into(),
                content_sha256: None,
            }],
        );
        assert!(ok.is_empty(), "{ok:?}");
        // Present but out of range.
        let conflict = check_requirements(
            &req,
            &[AvailablePack {
                id: "cloudflare".into(),
                version: "2.0.0".into(),
                content_sha256: None,
            }],
        );
        assert!(matches!(conflict.as_slice(), [Unmet::VersionConflict { .. }]));
    }

    #[test]
    fn check_hash_pin() {
        let hash = "a".repeat(64);
        let req = Requires {
            packs: vec![PackRequirement {
                id: "cloudflare".into(),
                content_sha256: Some(hash.clone()),
                ..PackRequirement::new("cloudflare")
            }],
            tools: vec![],
        };
        let mismatch = check_requirements(
            &req,
            &[AvailablePack {
                id: "cloudflare".into(),
                version: "1.0.0".into(),
                content_sha256: Some("b".repeat(64)),
            }],
        );
        assert!(matches!(mismatch.as_slice(), [Unmet::HashMismatch { .. }]));
        let ok = check_requirements(
            &req,
            &[AvailablePack {
                id: "cloudflare".into(),
                version: "1.0.0".into(),
                content_sha256: Some(hash),
            }],
        );
        assert!(ok.is_empty());
    }

    #[test]
    fn check_tools_reports_missing() {
        let req = Requires {
            packs: vec![],
            tools: vec!["a_tool".into(), "b_tool".into()],
        };
        let unmet = check_tools(&req, &["a_tool".to_string()]);
        assert!(matches!(unmet.as_slice(), [Unmet::MissingTool { name }] if name == "b_tool"));
    }

    #[test]
    fn optional_flag_surfaces_on_unmet() {
        let req = Requires {
            packs: vec![PackRequirement {
                optional: true,
                ..PackRequirement::new("maybe")
            }],
            tools: vec![],
        };
        let unmet = check_requirements(&req, &[]);
        assert_eq!(unmet.len(), 1);
        assert!(unmet[0].is_optional());
    }

    #[test]
    fn id_and_hash_validation() {
        assert!(is_valid_pack_id("cloudflare"));
        assert!(is_valid_pack_id("metalcraft-calendar"));
        assert!(is_valid_pack_id("digitalocean_spaces"));
        assert!(!is_valid_pack_id(""));
        assert!(!is_valid_pack_id("-leading-dash"));
        assert!(!is_valid_pack_id("Capital"));
        assert!(!is_valid_pack_id(&"x".repeat(65)));

        assert!(is_valid_sha256(&"a1b2c3d4".repeat(8)));
        assert!(!is_valid_sha256("short"));
        assert!(!is_valid_sha256(&"A".repeat(64))); // uppercase rejected
        assert!(!is_valid_sha256(&"g".repeat(64))); // non-hex
    }
}
