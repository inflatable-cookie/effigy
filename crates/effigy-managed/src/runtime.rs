//! Managed run policy shared by the run supervisor (contract 052 §10.2).
//! The supervisor itself lives in the runner: it owns processes, the run
//! journal and settle through the kernel.

#[path = "runtime/policy.rs"]
mod policy;

pub use policy::enforce_non_zero_exit_policy;
