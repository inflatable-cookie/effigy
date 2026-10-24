//! Read-only report on the retired current-main runtime import.
//!
//! v0.15 ships only the v0.14.1 importers, which read legacy files on first
//! use. The importer for the central `~/.effigy/state/kernel.db` is gone
//! (contract 052 §11.2), so this command never stages, stops or imports
//! anything. A committed manifest is provenance; a store without one is
//! refused with the exact upgrade path.
use crate::runner::error::RunnerError;
use crate::runner::task_engine;
use effigy_kernel::ctx::Deadline;
use effigy_kernel::migration;
use effigy_kernel::store::StoreError;
use std::time::{Duration, Instant};

pub(super) fn run(json: bool) -> Result<String, RunnerError> {
    let home = task_engine::state_home_path().ok_or_else(|| error("HOME is unset or relative"))?;
    let observe =
        task_engine::reading_context(Deadline::At(Instant::now() + Duration::from_secs(30)))
            .observe()
            .map_err(|_| error("runtime observation context"))?;
    if let Err(failure) = migration::refuse_unreleased_main(&observe, &home) {
        if matches!(failure, StoreError::UnreleasedMainRuntime) {
            return refuse(
                render(
                    serde_json::json!({"schema":SCHEMA,"state":"unreleased-main-state","source":source(&home),"message":failure.to_string()}),
                    &format!("Runtime migration: unreleased-main state.\n{failure}\nThe retired store was not imported, changed or deleted."),
                    json,
                ),
                json,
            );
        }
        return Err(error(failure));
    }
    let committed = migration::committed(&observe, &home).map_err(error)?;
    let Some(committed) = committed else {
        return Ok(render(
            serde_json::json!({"schema":SCHEMA,"state":"no-legacy-state","source":source(&home)}),
            "Runtime migration: no central kernel store exists. The v0.14.1 importers read legacy files on first use.",
            json,
        ));
    };
    let mut text = format!(
        "Runtime migration: committed by {}.\n{} destination(s) were imported; the retired store is ignored and never deleted.\n",
        committed.writer_build,
        committed.destinations.len()
    );
    for destination in &committed.destinations {
        text.push_str(&format!("  {}\n", destination.checkout_root.display()));
    }
    Ok(render(
        serde_json::json!({"schema":SCHEMA,"state":"committed","source":source(&home),"committed":committed}),
        text.trim_end(),
        json,
    ))
}

const SCHEMA: &str = "effigy.runtime-migration.v1";

fn source(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".effigy/state/kernel.db")
}
fn error(reason: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(reason.to_string())
}
fn render(mut payload: serde_json::Value, text: &str, json: bool) -> String {
    payload["schema_version"] = serde_json::json!(1);
    if json {
        payload.to_string()
    } else {
        text.to_owned()
    }
}
fn refuse<T>(rendered: String, json: bool) -> Result<T, RunnerError> {
    Err(if json {
        RunnerError::CommandJsonFailure { rendered }
    } else {
        error(rendered)
    })
}
