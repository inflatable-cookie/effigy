//! Read-only report on the retired central-store import.
//!
//! The importer for the central `~/.effigy/state/kernel.db` is retired
//! (contract 052 §11.1). This command stages, imports, stops and deletes
//! nothing; it only says whether a cutover manifest exists. Any legacy store
//! or old migration record is ignored, never changed and never a refusal.
use crate::runner::error::RunnerError;
use crate::runner::feedback::live;
use crate::runner::task_engine;
use std::path::PathBuf;

pub(super) fn run(json: bool) -> Result<String, RunnerError> {
    let home = live("locating the runtime state home", |step| {
        let home =
            task_engine::state_home_path().ok_or_else(|| error("HOME is unset or relative"))?;
        step.set_label(&format!("runtime state home: {}", home.display()));
        Ok(home)
    })?;
    let committed = live("checking for a committed cutover manifest", |step| {
        let committed = source(&home).join("migration/cutover.json").is_file();
        step.set_label(if committed {
            "a central-store cutover manifest exists"
        } else {
            "no central-store cutover manifest exists"
        });
        Ok(committed)
    })?;
    let (state, text) = if committed {
        (
            "committed",
            "Runtime migration: a central-store cutover was committed. The retired store is \
             ignored, never changed and never deleted.",
        )
    } else {
        (
            "no-legacy-state",
            "Runtime migration: nothing to migrate. The central-store importer is retired; any \
             legacy kernel.db is ignored, never changed and never deleted.",
        )
    };
    Ok(render(
        serde_json::json!({"schema":SCHEMA,"state":state,"source":source(&home)}),
        text,
        json,
    ))
}

const SCHEMA: &str = "effigy.runtime-migration.v1";

fn source(home: &std::path::Path) -> PathBuf {
    home.join(".effigy/state")
}
fn error(reason: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(reason.to_string())
}
fn render(payload: serde_json::Value, text: &str, json: bool) -> String {
    let mut payload = payload;
    payload["schema_version"] = serde_json::json!(1);
    if json {
        payload.to_string()
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_only_report_shows_each_step_and_ends_with_a_line() {
        // The process-lifetime test home holds no cutover manifest, so the
        // report is the read-only "nothing to migrate" path.
        let (outcome, lines) = effigy_ui::steps::capture_lines(|| run(false));
        let text = outcome.expect("a read-only migration report");
        assert!(text.contains("nothing to migrate"), "{text}");
        assert_eq!(
            lines,
            vec![
                format!(
                    "✓ runtime state home: {}",
                    task_engine::state_home_path().expect("test home").display()
                ),
                "✓ no central-store cutover manifest exists".to_owned(),
            ]
        );
    }
}
