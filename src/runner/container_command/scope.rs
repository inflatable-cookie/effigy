use std::path::Path;

use effigy_core::worktree_scope::{self, ScopeKind};
use serde_json::json;

use super::RunnerError;
use crate::runner::command_context::resolve_active_command_context;

pub(super) fn run_container_scope(
    repo_override: Option<std::path::PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    let checkout = context.resolved.resolved_root;
    let checkout = std::fs::canonicalize(&checkout).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to resolve checkout path {}: {error}",
            checkout.display()
        ))
    })?;
    let resolved = worktree_scope::load_or_create_scoped(&checkout)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    let (kind, token) = match resolved {
        Some((ScopeKind::LinkedWorktree, token)) => ("worktree", Some(token)),
        Some((ScopeKind::EphemeralClone, token)) => ("ephemeral-clone", Some(token)),
        None => ("none", None),
    };
    let report = scope_report(&checkout, kind, token.as_deref());

    if output_json {
        Ok(report.to_string())
    } else {
        Ok(format!(
            "checkout: {}\nscope.kind: {}\nscope.token: {}\n",
            checkout.display(),
            kind,
            token.as_deref().unwrap_or("none")
        ))
    }
}

fn scope_report(checkout: &Path, kind: &str, token: Option<&str>) -> serde_json::Value {
    json!({
        "schema": "effigy.container.scope.v1",
        "schema_version": 1,
        "ok": true,
        "checkout": checkout.display().to_string(),
        "scope": {
            "kind": kind,
            "token": token,
        },
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use super::run_container_scope;

    const MARKED_CONFIG: &str =
        "[core]\n\trepositoryformatversion = 0\n[effigy]\n\truntimeScope = ephemeral\n";

    fn json_report(checkout: &std::path::Path) -> Value {
        let output = run_container_scope(Some(checkout.to_path_buf()), true)
            .expect("scope query should succeed without a container manifest");
        serde_json::from_str(&output).expect("scope JSON")
    }

    fn clone_checkout(
        root: &std::path::Path,
        name: &str,
        config: Option<&str>,
    ) -> std::path::PathBuf {
        let checkout = root.join(name);
        fs::create_dir_all(checkout.join(".git")).expect("git dir");
        if let Some(config) = config {
            fs::write(checkout.join(".git/config"), config).expect("git config");
        }
        checkout
    }

    #[test]
    fn scope_query_reports_linked_worktree_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let checkout = temp.path().join("worker");
        let token_dir = temp.path().join("primary/.git/worktrees/worker");
        fs::create_dir_all(&checkout).expect("checkout");
        fs::create_dir_all(&token_dir).expect("worktree git dir");
        fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", token_dir.display()),
        )
        .expect("git pointer");
        fs::write(token_dir.join("commondir"), "../..\n").expect("commondir");

        let report = json_report(&checkout);
        assert_eq!(report["schema"], "effigy.container.scope.v1");
        assert_eq!(report["schema_version"], 1);
        assert_eq!(
            report["checkout"],
            checkout.canonicalize().unwrap().display().to_string()
        );
        assert_eq!(report["scope"]["kind"], "worktree");
        let token = report["scope"]["token"].as_str().expect("full token");
        assert_eq!(token.len(), 32);
        assert_eq!(
            fs::read_to_string(token_dir.join("effigy-runtime-scope")).unwrap(),
            token
        );
    }

    #[test]
    fn scope_query_reports_marked_clone_and_reuses_its_token_without_manifest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let checkout = clone_checkout(temp.path(), "marked", Some(MARKED_CONFIG));

        let first = json_report(&checkout);
        let second = json_report(&checkout);
        assert!(!checkout.join("effigy.toml").exists());
        assert_eq!(first["scope"]["kind"], "ephemeral-clone");
        assert_eq!(first["scope"]["token"], second["scope"]["token"]);
    }

    #[test]
    fn scope_query_reports_primary_checkout_and_unmarked_clone_as_unscoped() {
        let temp = tempfile::tempdir().expect("tempdir");
        let primary = clone_checkout(
            temp.path(),
            "primary",
            Some("[core]\n\trepositoryformatversion = 0\n"),
        );
        let unmarked = clone_checkout(temp.path(), "unmarked", None);

        for checkout in [primary, unmarked] {
            let report = json_report(&checkout);
            assert_eq!(report["scope"]["kind"], "none");
            assert!(report["scope"]["token"].is_null());
            assert!(!checkout.join(".git/effigy-runtime-scope").exists());
        }
    }

    #[test]
    fn scope_query_rejects_invalid_linked_worktree_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let checkout = temp.path().join("invalid");
        fs::create_dir_all(&checkout).expect("checkout");
        fs::write(checkout.join(".git"), "gitdir: /missing/worktree/gitdir\n")
            .expect("invalid git pointer");

        let error = run_container_scope(Some(checkout), true).expect_err("invalid metadata");
        assert!(error
            .to_string()
            .contains("linked worktree metadata is invalid"));
    }
}
