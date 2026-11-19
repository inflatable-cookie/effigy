//! The artifact substrate and the path helpers the deployment report shares.
//!
//! v0.16 retired the layered state workflow (`state plan|apply|capture|history`
//! and its manifest stacks); see `docs/knowledge/deployment-data-workflows.md`.
//! What remains is what other commands consume: local and OCI artifact staging
//! with immutable digests, and the two display helpers deployment report paths
//! use.

pub mod artifact;
mod atomic;
mod paths;

pub use paths::{path_display, safe_path_component};
