//! Runtime mechanisms shared by Effigy engines. No engine policy lives here.
//!
//! This foundation does not open a state root, install signal handlers, mint
//! checkout tokens, spawn processes or perform network probes.
#![deny(missing_debug_implementations, unsafe_op_in_unsafe_fn)]

pub mod ctx;
pub mod id;
pub mod ident;
pub mod observe;
pub mod outcome;
pub mod probe;

#[cfg(all(test, feature = "fault-injection"))]
mod fault;
