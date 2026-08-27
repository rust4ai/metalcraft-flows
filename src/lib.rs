//! # metalcraft-flows
//!
//! Reference types and helpers for the **Flow** specification — a serializable,
//! human-authored DAG format for AI agent workflows.
//!
//! See [`SPEC.md`](https://github.com/rust4ai/metalcraft-flows/blob/main/SPEC.md)
//! for the formal wire-format specification.
//!
//! ## Modules
//!
//! - [`model`] — core types: [`FlowDefinition`], [`FlowNode`], [`FlowEdge`],
//!   [`SavedFlow`], [`FlowNodeType`], [`CoreNodeType`].
//! - [`nodes`] — typed views over each node type's `data` payload.
//! - [`scheduled`] — [`ScheduledFlow`](scheduled::ScheduledFlow): *when* a flow
//!   runs, as a separate artifact from the flow itself.
//! - [`migrate`] — reading scheduling out of pre-v3 flow documents.
//! - [`requires`] — declared integration-pack / tool dependencies and their
//!   validation and satisfaction checks.
//! - [`walk`] — graph traversal: BFS reachability and handle-aware stepping.
//! - [`eval`] — deterministic predicate evaluation for `conditional` nodes.
//! - [`template`] — `{{path}}` interpolation for string fields.
//! - [`state`] — the running `variables` bag and dotted-path helpers.
//! - [`validate`](mod@validate) — spec conformance checks.
//! - [`store`] — directory-backed CRUD (enabled by the default `fs` feature).
//! - [`log`] — flow execution log entries (enabled by the `log` feature).

#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod eval;
pub mod migrate;
pub mod model;
pub mod nodes;
pub mod requires;
pub mod scheduled;
pub mod state;
pub mod template;
pub mod validate;
pub mod walk;

#[cfg(feature = "fs")]
#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
pub mod store;

#[cfg(feature = "log")]
#[cfg_attr(docsrs, doc(cfg(feature = "log")))]
pub mod log;

pub use eval::{evaluate, Operator};
pub use migrate::{extract, ExtractedSchedule, Extraction};
pub use model::{
    CoreNodeType, FlowDefinition, FlowEdge, FlowNode, FlowNodeType, FlowSummary, SavedFlow,
    SPEC_VERSION, SUPPORTED_SPEC_VERSIONS,
};
pub use scheduled::{
    validate_scheduled, ScheduleSpec, ScheduleTrigger, ScheduledFlow, Suggestion,
};
pub use nodes::BRANCH_ERROR_HANDLE;
pub use requires::{
    check_requirements, check_tools, derive_requires, AvailablePack, PackRequirement, Requires,
    Unmet,
};
pub use state::Variables;
pub use template::resolve as resolve_template;
pub use validate::{validate, ValidationError};
pub use walk::{next_by_handle, walk_bfs};

#[cfg(feature = "fs")]
pub use store::{
    delete_flow, delete_scheduled_flow, list_flows, list_scheduled_flows, load_flow,
    load_scheduled_flow, save_flow, save_scheduled_flow, scheduled_for_flow,
};

#[cfg(feature = "log")]
pub use log::{append_flow_log, load_flow_logs, FlowLogEntry};
