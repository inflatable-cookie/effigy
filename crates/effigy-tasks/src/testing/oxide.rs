//! Route a detected Rust suite through Oxide's leased build slot.
//!
//! Oxide has no per-workspace registration: any Cargo workspace leases a
//! slot, keyed by its checkout, so the route only needs a usable `oxide` on
//! PATH. `oxide pool describe --json` is the read-only proof: it creates no
//! pool and answers for any checkout. An `oxide` that is present but
//! unusable fails loudly and names the plain-Cargo escape; Effigy never
//! falls back on its own.

use std::sync::{Mutex, OnceLock};

use effigy_core::path_probe::command_available_in_path;

use super::{TestRunner, TestRunnerPlan};

/// The plain-Cargo escape Effigy's selectors share.
pub const ESCAPE_ENV: &str = "EFFIGY_OXIDE_ESCAPE";
/// The escape command every refusal names.
pub const ESCAPE_COMMAND: &str = "EFFIGY_OXIDE_ESCAPE=cargo";
/// The schema a usable Oxide answers with.
const POOL_SCHEMA: &str = "oxide.pool.describe/1";

/// Why a detected Rust suite could not be routed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxideRouteError {
    message: String,
}

impl OxideRouteError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for OxideRouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OxideRouteError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Route {
    Oxide { evidence: String },
    Cargo { evidence: String },
}

/// Route a detected Rust suite, leaving its arguments untouched.
///
/// `oxide nextest run` and `oxide test` lease a slot and map the profile to
/// `--cargo-profile`; nextest's inner `$CARGO` inherits the slot's build
/// directory, so the suite's intermediates never land unmanaged in `target`.
/// Vitest suites are never Oxide's business.
pub fn route_rust_plan(mut plan: TestRunnerPlan) -> Result<TestRunnerPlan, OxideRouteError> {
    if plan.runner == TestRunner::Vitest {
        return Ok(plan);
    }
    let evidence = match route()? {
        Route::Oxide { evidence } => {
            let Some(rest) = plan.command.strip_prefix("cargo ") else {
                return Err(OxideRouteError::new(format!(
                    "detected {} suite command `{}` is not a Cargo command to route through Oxide; {ESCAPE_COMMAND} runs plain Cargo",
                    plan.runner.label(),
                    plan.command
                )));
            };
            plan.command = format!("oxide {rest}");
            evidence
        }
        Route::Cargo { evidence } => evidence,
    };
    plan.evidence.insert(0, evidence);
    Ok(plan)
}

/// The route and why, cached for this invocation.
fn route() -> Result<Route, OxideRouteError> {
    type RouteMemo = Mutex<Option<(String, Result<Route, OxideRouteError>)>>;
    static MEMO: OnceLock<RouteMemo> = OnceLock::new();
    let key = route_key();
    let memo = MEMO.get_or_init(|| Mutex::new(None));
    let mut slot = memo.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((cached_key, cached)) = slot.as_ref() {
        if *cached_key == key {
            return cached.clone();
        }
    }
    let route = detect_route();
    *slot = Some((key, route.clone()));
    route
}

/// Everything the answer depends on: the escape knob and the PATH `oxide` is
/// resolved from.
fn route_key() -> String {
    let escape = std::env::var(ESCAPE_ENV).unwrap_or_default();
    let path = std::env::var("PATH").unwrap_or_default();
    format!("{escape}\u{0}{path}")
}

fn detect_route() -> Result<Route, OxideRouteError> {
    match std::env::var(ESCAPE_ENV).ok().as_deref() {
        Some("cargo") => {
            return Ok(Route::Cargo {
                evidence: format!("{ESCAPE_COMMAND} requested plain Cargo"),
            })
        }
        None | Some("") | Some("oxide") => {}
        Some(other) => {
            return Err(OxideRouteError::new(format!(
                "{ESCAPE_ENV} must be `oxide` or `cargo`, not `{other}`; {ESCAPE_COMMAND} runs plain Cargo"
            )))
        }
    }
    if !command_available_in_path("oxide") {
        return Ok(Route::Cargo {
            evidence: "no `oxide` on PATH; running plain Cargo".to_owned(),
        });
    }
    let output = std::process::Command::new("oxide")
        .args(["pool", "describe", "--json"])
        .output()
        .map_err(|error| {
            OxideRouteError::new(format!(
                "`oxide` is on PATH but `oxide pool describe --json` could not run ({error}); {ESCAPE_COMMAND} runs plain Cargo"
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let schema = output
        .status
        .success()
        .then(|| serde_json::from_str::<serde_json::Value>(&stdout).ok())
        .flatten()
        .and_then(|value| {
            value
                .get("schema")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    match schema.as_deref() {
        Some(POOL_SCHEMA) => Ok(Route::Oxide {
            evidence: format!(
                "`oxide pool describe --json` reports `{POOL_SCHEMA}`; routed through Oxide so the suite's build output stays in its slot"
            ),
        }),
        Some(unknown) => Err(OxideRouteError::new(format!(
            "`oxide pool describe --json` reported unknown schema `{unknown}`; {ESCAPE_COMMAND} runs plain Cargo"
        ))),
        None => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            let detail = if detail.is_empty() {
                "no diagnostic output".to_owned()
            } else {
                detail.to_owned()
            };
            Err(OxideRouteError::new(format!(
                "`oxide pool describe --json` failed ({}): {detail}; {ESCAPE_COMMAND} runs plain Cargo",
                output.status
            )))
        }
    }
}

#[cfg(test)]
#[path = "oxide/tests.rs"]
mod tests;
