#[path = "deferral/policy.rs"]
mod policy;
#[path = "deferral/run.rs"]
mod run;
#[path = "deferral/select.rs"]
mod select;
#[path = "deferral/trace.rs"]
mod trace;

pub(super) use policy::should_attempt_deferral;
pub(super) use run::run_deferred_request;
pub(super) use select::select_deferral;
#[cfg(test)]
pub(crate) use run::reset_composer_home_cache_for_tests;
