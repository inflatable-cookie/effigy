//! Lock-scope types.
//!
//! `LockScope` was relocated into `effigy-builtin` under card 250 so
//! the `BuiltinRuntimePorts` trait can carry it without pulling
//! runner-internal types. The runner-side modules continue to use it
//! via this re-export.

pub(in crate::runner) use effigy_builtin::LockScope;
