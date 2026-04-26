#[path = "deferral/builtins.rs"]
mod builtins;
#[path = "deferral/policy.rs"]
mod policy;
#[path = "deferral/run.rs"]
mod run;
#[path = "deferral/select.rs"]
mod select;
#[path = "deferral/trace.rs"]
mod trace;

pub(crate) use builtins::{deferred_builtins_for_root, deferred_builtins_from_catalogs};
pub(super) use policy::should_attempt_deferral;
pub(super) use run::run_deferred_request;
pub(super) use select::select_deferral;
