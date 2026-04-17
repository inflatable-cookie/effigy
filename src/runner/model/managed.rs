//! Thin re-export shim — plan-shape types live in `effigy-managed`.
//!
//! `ManagedProcessSpec` and `ManagedTaskPlan` moved out of the runner
//! binary in batch 240. The runner keeps this shim so non-managed
//! call sites (e.g. demo bridge code, plan-surface rendering)
//! compile unchanged against the existing paths.

pub(in crate::runner) use effigy_managed::{ManagedProcessSpec, ManagedTaskPlan};
