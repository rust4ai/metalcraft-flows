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
//! - [`walk`] — generic BFS traversal over a [`FlowDefinition`].
//! - [`validate`] — spec conformance checks.
//! - [`store`] — directory-backed CRUD (enabled by the default `fs` feature).
//! - [`log`] — flow execution log entries (enabled by the `log` feature).

#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod model;
pub mod walk;
pub mod validate;

#[cfg(feature = "fs")]
#[cfg_attr(docsrs, doc(cfg(feature = "fs")))]
pub mod store;

#[cfg(feature = "log")]
#[cfg_attr(docsrs, doc(cfg(feature = "log")))]
pub mod log;

pub use model::{
    CoreNodeType, FlowDefinition, FlowEdge, FlowNode, FlowNodeType, FlowSummary, SavedFlow,
    SPEC_VERSION,
};
pub use validate::{validate, ValidationError};
pub use walk::walk_bfs;

#[cfg(feature = "fs")]
pub use store::{delete_flow, list_flows, load_flow, save_flow};

#[cfg(feature = "log")]
pub use log::{append_flow_log, load_flow_logs, FlowLogEntry};
