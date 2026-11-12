use std::path::Path;
use std::process::Command as ProcessCommand;

use super::report::DeployCodeRef;
use crate::runner::error::RunnerError;

/// The forms `code_ref` accepts, as the refusal names them.
const CODE_REF_FORMS: &str =
    "`release-tag`, `branch:<branch or rev>`, `tag:<tag>` or `sha:<commit sha>`";

/// Resolve the environment's `code_ref` to the revision a deployment names.
///
/// `release-tag` reports the newest tag and what it points at; an unresolvable
/// branch or tag stays `None` so a plan can report that it could not pin the
/// code. An unrecognised `<kind>:<value>` form is refused: it names nothing a
/// plan can resolve, and silently reporting no commit would read as a resolved
/// deployment.
pub(super) fn resolve_code_ref(
    repo_root: &Path,
    requested: &str,
) -> Result<DeployCodeRef, RunnerError> {
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
        "branch" | "tag" => git_output(repo_root, &["rev-parse", value]).ok(),
        "sha" => Some(value.to_owned()),
        other => {
            return Err(RunnerError::task_invocation(format!(
                "deploy environment `code_ref = \"{requested}\"` names unsupported kind `{other}`; use one of {CODE_REF_FORMS}"
            )))
        }
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

#[cfg(test)]
mod tests {
    use super::resolve_code_ref;
    use std::path::Path;

    #[test]
    fn a_sha_ref_resolves_without_touching_git() {
        let resolved = resolve_code_ref(Path::new("."), "sha:0123456789abcdef0123456789abcdef01234567")
            .expect("a sha names its own commit");
        assert_eq!(resolved.kind, "sha");
        assert_eq!(
            resolved.resolved_commit.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
        assert_eq!(
            resolved.resolved_ref.as_deref(),
            Some("0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn a_bare_ref_is_a_branch() {
        let resolved = resolve_code_ref(Path::new("."), "main").expect("bare ref is a branch");
        assert_eq!(resolved.kind, "branch");
        assert_eq!(resolved.requested_ref, "main");
        assert_eq!(resolved.resolved_ref.as_deref(), Some("main"));
    }

    #[test]
    fn an_unrecognised_ref_kind_is_refused_instead_of_reporting_no_commit() {
        // Before, an unknown `<kind>:<value>` fell through to `resolved_commit:
        // None` and a plan reported a deployment it could not pin. The form
        // accepted is the documented closed set.
        let error = resolve_code_ref(Path::new("."), "release:main")
            .expect_err("`release` is not a code_ref kind");
        let message = error.to_string();
        assert!(
            message.contains("names unsupported kind `release`"),
            "got: {message}"
        );
        assert!(
            message.contains("`release-tag`, `branch:<branch or rev>`, `tag:<tag>` or `sha:<commit sha>`"),
            "the refusal names the accepted forms: {message}"
        );
    }
}
