//! Read-only report on the retired current-main runtime import.
//!
//! v0.15 ships only the v0.14.1 importers, which read legacy files on first
//! use. The importer for the central `~/.effigy/state/kernel.db` is gone
//! (contract 052 §11.1), so this command never stages, stops or imports
//! anything. A committed manifest is provenance; a store without one is
//! refused with the exact upgrade path.
use crate::runner::error::RunnerError;
use crate::runner::feedback::live;
use crate::runner::task_engine;
use effigy_kernel::ctx::Deadline;
use effigy_kernel::migration;
use effigy_kernel::store::StoreError;
use std::time::{Duration, Instant};

pub(super) fn run(json: bool) -> Result<String, RunnerError> {
    let home = live("locating the runtime state home", |step| {
        let home =
            task_engine::state_home_path().ok_or_else(|| error("HOME is unset or relative"))?;
        step.set_label(&format!("runtime state home: {}", home.display()));
        Ok(home)
    })?;
    let observe = live("opening the runtime state for reading", |_| {
        task_engine::reading_context(Deadline::At(Instant::now() + Duration::from_secs(30)))
            .observe()
            .map_err(|_| error("runtime observation context"))
    })?;
    let retired = live("checking for a retired current-main store", |step| {
        let failure = migration::refuse_unreleased_main(&observe, &home);
        step.set_label(match &failure {
            Ok(()) => "no retired current-main store is present",
            Err(StoreError::UnreleasedMainRuntime) => {
                "the retired current-main store needs Effigy main 7b82d26 to migrate"
            }
            Err(_) => "the retired current-main state could not be read",
        });
        Ok(failure)
    })?;
    if let Err(failure) = retired {
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
    let committed = live("reading the committed migration manifest", |step| {
        let committed = migration::committed(&observe, &home).map_err(error)?;
        let label = match &committed {
            Some(committed) => format!("the manifest was committed by {}", committed.writer_build),
            None => "no central kernel store exists, so there is nothing to migrate".to_owned(),
        };
        step.set_label(&label);
        Ok(committed)
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_only_report_shows_each_step_and_ends_with_a_line() {
        // The process-lifetime test home holds no central kernel store, so the
        // report is the read-only "nothing to migrate" path.
        let (outcome, lines) = effigy_ui::steps::capture_lines(|| run(false));
        let text = outcome.expect("a read-only migration report");
        assert!(text.contains("no central kernel store exists"), "{text}");
        assert_eq!(
            lines,
            vec![
                format!(
                    "✓ runtime state home: {}",
                    task_engine::state_home_path().expect("test home").display()
                ),
                "✓ opening the runtime state for reading".to_owned(),
                "✓ no retired current-main store is present".to_owned(),
                "✓ no central kernel store exists, so there is nothing to migrate".to_owned(),
            ]
        );
    }
}
