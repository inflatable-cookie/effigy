//! Retire stacks whose checkout is gone.
//!
//! The runtime's labels alone name the checkout and scope of every resource, so
//! a deleted worktree's stack is found, retired and reported without any local
//! record. Dead means the checkout path is missing; an unreadable path is
//! reported and never guessed about.

use std::path::{Component, Path};

use super::*;

const DEAD_RETIRE_SCHEMA: &str = "effigy.container.retire-dead.v1";

/// Whether a labelled checkout path is still on disk.
enum PathState {
    Present,
    Missing,
    Unknown(String),
}

fn path_state(label: &str) -> PathState {
    let path = Path::new(label);
    if !path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return PathState::Unknown("the checkout label is not an absolute path".to_owned());
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => PathState::Present,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => PathState::Missing,
        Err(error) => PathState::Unknown(error.to_string()),
    }
}

pub(super) fn run(
    repo_override: Option<PathBuf>,
    keep_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let caller = retire_checkout(repo_override)?;
    let inventory = Inventory::read(&caller)?;
    let mut dead = Vec::new();
    let mut unknown = Vec::new();
    for group in inventory.groups() {
        match path_state(&group.checkout) {
            PathState::Missing => dead.push(group),
            PathState::Present => {}
            PathState::Unknown(reason) => unknown.push(format!("{}: {reason}", group.checkout)),
        }
    }
    if !dead.is_empty() {
        maybe_confirm_destructive_container_action(
            "effigy container retire --dead",
            &format!(
                "Retire {} stack(s) whose checkout is gone and release their gateway state",
                dead.len()
            ),
            output_json,
            yes,
        )?;
    }
    let mut outcomes = Vec::new();
    let mut failed = false;
    for group in dead {
        let mut outcome = retire_owned(&caller, &group.resources, keep_data)?;
        outcome
            .kept
            .extend(inventory.unlabelled_kept(&group.projects));
        let release = release_gateway(&group.checkout);
        match release {
            Ok(gateway) => outcome.gateway = gateway,
            Err(error) => {
                failed = true;
                outcomes.push(serde_json::json!({
                    "checkout": group.checkout,
                    "ok": false,
                    "error": error.to_string(),
                    "report": retire_report(Some(&group.checkout), &outcome).json,
                }));
                continue;
            }
        }
        let ok = outcome.ok();
        failed |= !ok;
        outcomes.push(serde_json::json!({
            "checkout": group.checkout,
            "ok": ok,
            "report": retire_report(Some(&group.checkout), &outcome).json,
        }));
    }
    let value = serde_json::json!({
        "schema": DEAD_RETIRE_SCHEMA,
        "schema_version": 2,
        "scopes": outcomes,
        "unknown": unknown,
    });
    let rendered = if output_json {
        value.to_string()
    } else {
        serde_json::to_string_pretty(&value).map_err(|error| {
            RunnerError::task_invocation(format!("render dead retirement: {error}"))
        })?
    };
    if failed {
        if output_json {
            Err(RunnerError::CommandJsonFailure { rendered })
        } else {
            Err(RunnerError::task_invocation(rendered))
        }
    } else {
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_checkout_is_dead_and_a_present_one_is_not() {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let live = temp.path().join("live");
        std::fs::create_dir(&live).expect("live checkout");
        assert!(matches!(
            path_state(&live.display().to_string()),
            PathState::Present
        ));
        assert!(matches!(
            path_state(&temp.path().join("gone").display().to_string()),
            PathState::Missing
        ));
    }

    #[test]
    fn a_relative_or_unnormalised_label_is_unknown() {
        assert!(matches!(path_state("relative/path"), PathState::Unknown(_)));
        assert!(matches!(path_state("/a/../b"), PathState::Unknown(_)));
        assert!(matches!(path_state(""), PathState::Unknown(_)));
    }
}
