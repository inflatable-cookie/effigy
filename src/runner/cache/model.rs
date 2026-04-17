//! Runner-side cache model types.
//!
//! `TaskCacheEntry` was relocated into `effigy-builtin` under card 250
//! so the `BuiltinRuntimePorts` trait can carry it without pulling
//! runner-internal types; it is re-exported here for the runner's
//! cache store / ops modules. `TaskCacheCheck` remains runner-only
//! because no port surface references it.

#[derive(Debug)]
pub(in crate::runner) struct TaskCacheCheck {
    pub(in crate::runner) enabled: bool,
    pub(in crate::runner) hit: bool,
    pub(in crate::runner) reason: String,
    pub(in crate::runner) fingerprint: String,
}

pub(in crate::runner) use effigy_builtin::TaskCacheEntry;
