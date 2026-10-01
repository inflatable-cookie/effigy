//! Routing of heavy execution through the host-run scheduler.
//!
//! Contract 049 uses the trusted Client protocol v1 client in `effigy-host-run`
//! for heavy work by default. `EFFIGY_HOST_SCHEDULER=0` selects legacy lease
//! admission for rollback; Queue and Nucleus own scheduler admission, capacity
//! and process-group settlement. A present `HOST_RUN_TOKEN` always takes the
//! validation path first, so a scheduler-launched child can never acquire a
//! legacy lease or submit behind its own parent.

mod facts;
mod submit;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use effigy_host_run::{ClassSource, HostRunClient, HostRunRoot};

use super::error::RunnerError;

pub(super) use facts::{report_container_removed, report_container_started};
pub(super) use submit::{submit_and_settle, PreLaunch, Settled, SubmitContext};

pub(super) const SCHEDULER_ENV: &str = "EFFIGY_HOST_SCHEDULER";
pub(super) const ROOT_ENV: &str = "EFFIGY_HOST_RUN_ROOT";
pub(super) const OVERRIDE_ENV: &str = "EFFIGY_SCHEDULER_OVERRIDE";
pub(super) const TOKEN_ENV: &str = "HOST_RUN_TOKEN";
pub(super) const RUN_ID_ENV: &str = "HOST_RUN_ID";

const MAX_OVERRIDE_REASON_BYTES: usize = 500;

/// A validated scheduler run this process executes under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Nested {
    pub(super) run_id: String,
    pub(super) epoch: u64,
}

/// How a heavy invocation reaches execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Route {
    /// Explicit legacy rollback: lease admission, unchanged.
    Legacy,
    /// Executing inside a validated scheduler run: no second admission.
    Nested(Nested),
    /// Recorded operator override: execute directly with no admission.
    Override,
    /// Submit this invocation to the scheduler and relay its result.
    Submit,
}

static ACTIVE_NESTED: OnceLock<Nested> = OnceLock::new();
static REPORTED_NESTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Names the scheduler owns inside a launched run. They never cross a container
/// or `sudo` boundary.
pub(super) fn is_scheduler_owned_env(key: &str) -> bool {
    matches!(key, TOKEN_ENV | RUN_ID_ENV)
}

pub(super) fn refuse(code: i32, detail: impl Into<String>) -> RunnerError {
    RunnerError::HostScheduler {
        code,
        detail: detail.into(),
    }
}

fn unreachable_error(detail: impl std::fmt::Display) -> RunnerError {
    refuse(
        75,
        format!(
            "scheduler_unreachable: {detail}. Heavy work fails closed; start the host-run scheduler or set {OVERRIDE_ENV}=<reason> to record an override"
        ),
    )
}

/// The scheduler is the default; only `0` selects the legacy backend.
pub(super) fn scheduler_enabled() -> Result<bool, RunnerError> {
    parse_scheduler_setting(std::env::var_os(SCHEDULER_ENV))
}

fn parse_scheduler_setting(value: Option<OsString>) -> Result<bool, RunnerError> {
    match value {
        None => Ok(true),
        Some(value) => match value.to_str() {
            Some("1") => Ok(true),
            Some("0") => Ok(false),
            _ => Err(refuse(
                2,
                format!(
                    "{SCHEDULER_ENV} must be `1` (host scheduler) or `0` (legacy admission); unset selects the host scheduler"
                ),
            )),
        },
    }
}

/// Decide how one heavy invocation proceeds. Everything that can refuse does so
/// here, before any build, setup, container or group effect.
pub(super) fn route_heavy(selector: &str, cwd: &Path) -> Result<Route, RunnerError> {
    let scheduler_enabled = scheduler_enabled()?;
    if let Some(token) = std::env::var_os(TOKEN_ENV) {
        return nested_route(token, selector, cwd).map(Route::Nested);
    }
    if !scheduler_enabled {
        return Ok(Route::Legacy);
    }
    if let Some(reason) = override_reason()? {
        facts::record_override(&reason, selector)?;
        return Ok(Route::Override);
    }
    Ok(Route::Submit)
}

fn nested_route(token: OsString, selector: &str, cwd: &Path) -> Result<Nested, RunnerError> {
    let token = token.into_string().map_err(|_| {
        refuse(
            77,
            "invalid_parent_token: HOST_RUN_TOKEN is not valid UTF-8",
        )
    })?;
    let (mut client, _) = open_client()?;
    let parent = client
        .validate_parent_token(Some(&token), cwd)
        .map_err(|error| {
            let code = error.exit_code().map_or(77, i32::from);
            refuse(code, error.to_string())
        })?
        .ok_or_else(|| refuse(77, "invalid_parent_token: no token was validated"))?;
    if parent.class != "heavy" {
        return Err(refuse(
            77,
            format!(
                "invalid_parent_token: parent run {} is class `{}` and cannot cover heavy work",
                parent.run_id, parent.class
            ),
        ));
    }
    let nested = Nested {
        run_id: parent.run_id,
        epoch: parent.epoch,
    };
    let _ = ACTIVE_NESTED.set(nested.clone());
    facts::report_nested_once(&nested, selector);
    Ok(nested)
}

/// The scheduler run this process executes under, when one was validated.
pub(super) fn active_nested() -> Option<Nested> {
    ACTIVE_NESTED.get().cloned()
}

fn override_reason() -> Result<Option<String>, RunnerError> {
    let Some(raw) = std::env::var_os(OVERRIDE_ENV) else {
        return Ok(None);
    };
    parse_override_reason(&raw).map(Some)
}

fn parse_override_reason(raw: &OsString) -> Result<String, RunnerError> {
    let reason = raw.to_str().map(str::trim).unwrap_or_default();
    if reason.is_empty()
        || reason.len() > MAX_OVERRIDE_REASON_BYTES
        || reason.chars().any(char::is_control)
    {
        return Err(refuse(
            2,
            format!(
                "{OVERRIDE_ENV} needs a reason of 1-{MAX_OVERRIDE_REASON_BYTES} printable characters; an override is recorded, never silent"
            ),
        ));
    }
    Ok(reason.to_owned())
}

/// Scheduler state root: the explicit isolated-fixture setting, else the
/// canonical `~/.local/state/host-run`.
pub(super) fn root_path() -> Result<PathBuf, RunnerError> {
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(refuse(2, format!("{ROOT_ENV} must be an absolute path")));
        }
        return Ok(root);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| unreachable_error("HOME is not an absolute path"))?;
    Ok(home.join(".local/state/host-run"))
}

pub(super) fn open_client() -> Result<(HostRunClient, PathBuf), RunnerError> {
    let path = root_path()?;
    let (root, authority) = HostRunRoot::open(&path).map_err(unreachable_error)?;
    Ok((HostRunClient::open(root, authority), path))
}

/// Queue wait of the scheduler run this process was launched under, from its
/// own status record. `None` means unknown, never zero.
pub(super) fn nested_queue_wait_ms(nested: &Nested) -> Option<u64> {
    let (mut client, _) = open_client().ok()?;
    let status = client
        .status_query(effigy_host_run::StatusQuery::Run {
            run_id: nested.run_id.clone(),
        })
        .ok()?;
    queue_wait_from_status(&status)
}

pub(super) fn queue_wait_from_status(status: &serde_json::Value) -> Option<u64> {
    let parse = |key: &str| {
        status
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
    };
    let waited = parse("admittedAt")?.signed_duration_since(parse("submittedAt")?);
    u64::try_from(waited.num_milliseconds()).ok()
}

pub(super) fn class_source(selected_heavy: bool) -> ClassSource {
    if selected_heavy {
        ClassSource::Manifest
    } else {
        ClassSource::Default
    }
}
