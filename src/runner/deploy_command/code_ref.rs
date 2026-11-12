use std::path::Path;
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use super::report::DeployCodeRef;
use crate::runner::error::RunnerError;

pub(super) fn resolve_code_ref(repo_root: &Path, requested: &str) -> Result<DeployCodeRef, RunnerError> {
    if requested == "release-tag" {
        let tag = git_output(repo_root, &["describe", "--tags", "--abbrev=0"]).ok();
        let commit = git_output(repo_root, &["rev-parse", tag.as_deref().unwrap_or("HEAD")]).ok();
        return Ok(DeployCodeRef {
            requested_ref: requested.to_owned(),
            kind: "tag".to_owned(),
            resolved_ref: tag,
            resolved_commit: commit,
        });
    }
    let (kind, value) = requested.split_once(':').unwrap_or(("branch", requested));
    let commit = match kind {
        "branch" => git_output(repo_root, &["rev-parse", value]).ok(),
        "tag" => git_output(repo_root, &["rev-parse", value]).ok(),
        "sha" => Some(value.to_owned()),
        _ => None,
    };
    Ok(DeployCodeRef {
        requested_ref: requested.to_owned(),
        kind: kind.to_owned(),
        resolved_ref: Some(value.to_owned()),
        resolved_commit: commit,
    })
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| RunnerError::task_invocation(format!("failed to run git: {error}")))?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub(super) fn utc_basic_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}
