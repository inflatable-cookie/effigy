use std::path::PathBuf;
use std::process::{Command, Output};

use effigy_cli::UninstallArgs;
use serde_json::json;

use super::RunnerError;

const MANAGED_COLIMA_PROFILE: &str = "effigy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UninstallMode {
    Plan,
    Apply,
}

impl UninstallMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UninstallTarget {
    kind: &'static str,
    path: Option<PathBuf>,
    label: String,
    exists: Option<bool>,
    owned: bool,
    action: &'static str,
    status: &'static str,
    removed: bool,
    error: Option<String>,
}

pub(in crate::runner) fn run_uninstall(args: UninstallArgs) -> Result<String, RunnerError> {
    if args.plan && args.yes {
        return Err(RunnerError::task_invocation(
            "`effigy uninstall` does not accept both `--plan` and `--yes`",
        ));
    }
    let mode = if args.yes {
        UninstallMode::Apply
    } else {
        UninstallMode::Plan
    };
    let targets = match mode {
        UninstallMode::Plan => plan_uninstall_targets()?,
        UninstallMode::Apply => apply_uninstall_targets()?,
    };
    Ok(render_uninstall_result(mode, &targets, args.output_json))
}

fn plan_uninstall_targets() -> Result<Vec<UninstallTarget>, RunnerError> {
    let mut targets = path_uninstall_targets()?;
    targets.push(plan_colima_profile_target());
    Ok(targets)
}

fn apply_uninstall_targets() -> Result<Vec<UninstallTarget>, RunnerError> {
    let mut targets = path_uninstall_targets()?
        .into_iter()
        .map(apply_path_target)
        .collect::<Vec<_>>();
    targets.push(apply_colima_profile_target());
    Ok(targets)
}

fn path_uninstall_targets() -> Result<Vec<UninstallTarget>, RunnerError> {
    let config_path = effigy_manifest::user_config_path().ok_or_else(|| {
        RunnerError::task_invocation("HOME is not set; cannot resolve Effigy user config path")
    })?;
    let effigy_home = config_path.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "cannot resolve Effigy home from `{}`",
            config_path.display()
        ))
    })?;
    let catalog_path = effigy_home.join("catalog");
    Ok(vec![
        plan_path_target("user_config", config_path),
        plan_path_target("user_catalog", catalog_path),
    ])
}

fn plan_path_target(kind: &'static str, path: PathBuf) -> UninstallTarget {
    let exists = path.exists();
    UninstallTarget {
        kind,
        label: path.display().to_string(),
        path: Some(path),
        exists: Some(exists),
        owned: true,
        action: "delete",
        status: if exists { "would_delete" } else { "absent" },
        removed: false,
        error: None,
    }
}

fn apply_path_target(mut target: UninstallTarget) -> UninstallTarget {
    let Some(path) = target.path.as_deref() else {
        target.status = "failed";
        target.error = Some("missing cleanup path".to_owned());
        return target;
    };
    if !path.exists() {
        target.status = "absent";
        target.exists = Some(false);
        return target;
    }

    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Ok(()) => {
            target.status = "removed";
            target.exists = Some(true);
            target.removed = true;
        }
        Err(error) => {
            target.status = "failed";
            target.exists = Some(true);
            target.error = Some(error.to_string());
        }
    }
    target
}

fn plan_colima_profile_target() -> UninstallTarget {
    match colima_profile_exists(MANAGED_COLIMA_PROFILE) {
        Ok(exists) => UninstallTarget {
            kind: "colima_profile",
            path: None,
            label: MANAGED_COLIMA_PROFILE.to_owned(),
            exists: Some(exists),
            owned: true,
            action: "delete",
            status: if exists { "would_delete" } else { "absent" },
            removed: false,
            error: None,
        },
        Err(error) => UninstallTarget {
            kind: "colima_profile",
            path: None,
            label: MANAGED_COLIMA_PROFILE.to_owned(),
            exists: None,
            owned: true,
            action: "delete",
            status: "unknown",
            removed: false,
            error: Some(error),
        },
    }
}

fn apply_colima_profile_target() -> UninstallTarget {
    let mut target = plan_colima_profile_target();
    if target.exists == Some(false) {
        target.status = "absent";
        return target;
    }

    if let Err(error) = run_colima_allow_failure(
        &["stop", "--profile", MANAGED_COLIMA_PROFILE],
        "colima stop",
    ) {
        target.status = "failed";
        target.error = Some(error);
        return target;
    }
    match run_colima(
        &[
            "delete",
            "--profile",
            MANAGED_COLIMA_PROFILE,
            "--force",
            "--data",
        ],
        "colima delete",
    ) {
        Ok(_) => {
            target.status = "removed";
            target.removed = true;
        }
        Err(error) => {
            target.status = "failed";
            target.error = Some(error);
        }
    }
    target
}

fn render_uninstall_result(
    mode: UninstallMode,
    targets: &[UninstallTarget],
    output_json: bool,
) -> String {
    let ok = !targets.iter().any(|target| target.status == "failed");
    let json = json!({
        "schema": "effigy.uninstall.v1",
        "schema_version": 1,
        "ok": ok,
        "mode": mode.as_str(),
        "targets": targets.iter().map(uninstall_target_json).collect::<Vec<_>>(),
    });
    if output_json {
        return json.to_string();
    }

    let mut lines = Vec::new();
    lines.push(match (mode, ok) {
        (UninstallMode::Plan, _) => "[info] uninstall plan".to_owned(),
        (UninstallMode::Apply, true) => "[ok] uninstall cleanup complete".to_owned(),
        (UninstallMode::Apply, false) => {
            "[warning] uninstall cleanup finished with failures".to_owned()
        }
    });
    for target in targets {
        lines.push(format!(
            "{}: {} ({}){}",
            target.kind,
            target.status,
            target.label,
            target
                .error
                .as_ref()
                .map(|error| format!(" - {error}"))
                .unwrap_or_default()
        ));
    }
    if mode == UninstallMode::Plan {
        lines.push("next: run `effigy uninstall --yes` to delete these targets".to_owned());
    }
    lines.join("\n")
}

fn uninstall_target_json(target: &UninstallTarget) -> serde_json::Value {
    json!({
        "kind": target.kind,
        "path": target.path.as_ref().map(|path| path.display().to_string()),
        "label": target.label,
        "exists": target.exists,
        "owned": target.owned,
        "action": target.action,
        "status": target.status,
        "removed": target.removed,
        "error": target.error,
    })
}

fn colima_profile_exists(profile: &str) -> Result<bool, String> {
    let output = run_colima(&["list", "--json"], "colima list")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let row = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
            format!("failed to parse `colima list --json` row during uninstall planning: {error}")
        })?;
        if row
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| name == profile)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_colima(args: &[&str], label: &str) -> Result<Output, String> {
    run_command("colima", args, label, false)
}

fn run_colima_allow_failure(args: &[&str], label: &str) -> Result<Output, String> {
    run_command("colima", args, label, true)
}

fn run_command(
    program: &str,
    args: &[&str],
    label: &str,
    allow_failure: bool,
) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("{label} failed to launch: {error}"))
        .and_then(|output| {
            if output.status.success() || allow_failure {
                Ok(output)
            } else {
                Err(format!(
                    "{label} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::with_test_user_config_home;
    use tempfile::tempdir;

    #[test]
    fn uninstall_plan_reports_user_config_and_catalog() {
        let tmp = tempdir().expect("tempdir");
        let home = tmp.path().join(".effigy-home");
        std::fs::create_dir_all(home.join("catalog")).expect("mkdir catalog");
        std::fs::write(home.join("config.toml"), "profile = \"effigy\"\n").expect("write config");

        let targets = with_test_user_config_home(&home, path_uninstall_targets).expect("targets");

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].kind, "user_config");
        assert_eq!(targets[0].exists, Some(true));
        assert_eq!(targets[1].kind, "user_catalog");
        assert_eq!(targets[1].exists, Some(true));
    }

    #[test]
    fn apply_path_target_removes_file_and_directory() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("config.toml");
        let dir = tmp.path().join("catalog");
        std::fs::write(&file, "backend = \"containerd\"\n").expect("write file");
        std::fs::create_dir_all(&dir).expect("mkdir dir");

        let file_result = apply_path_target(plan_path_target("user_config", file.clone()));
        let dir_result = apply_path_target(plan_path_target("user_catalog", dir.clone()));

        assert!(file_result.removed);
        assert!(dir_result.removed);
        assert!(!file.exists());
        assert!(!dir.exists());
    }

    #[test]
    fn uninstall_json_renders_targets() {
        let target = UninstallTarget {
            kind: "user_config",
            path: Some(PathBuf::from("/tmp/config.toml")),
            label: "/tmp/config.toml".to_owned(),
            exists: Some(true),
            owned: true,
            action: "delete",
            status: "would_delete",
            removed: false,
            error: None,
        };

        let rendered = render_uninstall_result(UninstallMode::Plan, &[target], true);
        let parsed = serde_json::from_str::<serde_json::Value>(&rendered).expect("json");

        assert_eq!(parsed["schema"], "effigy.uninstall.v1");
        assert_eq!(parsed["mode"], "plan");
        assert_eq!(parsed["targets"][0]["kind"], "user_config");
    }
}
