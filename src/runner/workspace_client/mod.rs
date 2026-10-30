//! Container-mode workspace client.
//!
//! `context` owns the materialised guest context, its version lock and the
//! guest-side placement detection. The shared integration file
//! (`src/runner/workspace_client.rs`, contract 052 §8.11) joins this lane with
//! the host channel and the workspace session; until it lands this directory
//! is the module root.

pub(crate) mod context;

#[cfg(test)]
pub(crate) mod guest_fake;
