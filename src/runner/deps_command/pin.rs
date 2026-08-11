use std::path::{Path, PathBuf};

use effigy_deps::{
    apply_bun_pin_plan, plan_bun_pin, plan_bun_unpin, BunPinOperation, BunPinOperationReport,
    BunPinOutcome, BunPinPackageAction, BunPinVerificationStatus, BunPinWriteAction,
    ReadOnlyProcess,
};
use serde_json::json;

use super::{finish_deps_operation, resolve_link_library_path};
use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::RunnerError;

pub(super) fn run_bun_pin(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    dry_run: bool,
    output_json: bool,
    process: &impl ReadOnlyProcess,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let library_path = resolve_link_library_path(&repo_root, library_path);
    let plan = plan_bun_pin(&repo_root, &library_path, dry_run, process)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    finish_pin_operation(apply_bun_pin_plan(plan), output_json)
}

pub(super) fn run_bun_unpin(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    dry_run: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let library_path = resolve_link_library_path(&repo_root, library_path);
    let plan = plan_bun_unpin(&repo_root, &library_path, dry_run)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    finish_pin_operation(apply_bun_pin_plan(plan), output_json)
}

fn finish_pin_operation(
    report: BunPinOperationReport,
    output_json: bool,
) -> Result<String, RunnerError> {
    let command = match report.plan.operation {
        BunPinOperation::Pin => "deps pin bun",
        BunPinOperation::Unpin => "deps unpin bun",
    };
    let rendered = render_bun_pin(&report, output_json);
    finish_deps_operation(
        command,
        report.outcome.as_str(),
        report.outcome.is_success(),
        &report.errors,
        rendered,
    )
}

fn render_bun_pin(report: &BunPinOperationReport, output_json: bool) -> String {
    let operation = operation_name(report.plan.operation);
    let next_actions = next_actions(report);
    if output_json {
        return json!({
            "schema": "effigy.deps.pin.v1",
            "schema_version": 1,
            "command": format!("deps {operation} bun"),
            "operation": operation,
            "manager": "bun",
            "repo_root": report.plan.repo_root,
            "manifest_path": report.plan.manifest_path,
            "library_path": report.plan.library_path,
            "dry_run": report.plan.dry_run,
            "outcome": report.outcome,
            "packages": report.plan.packages,
            "writes": report.writes,
            "warnings": report.plan.warnings,
            "verification": report.verification,
            "errors": report.errors,
            "next_actions": next_actions,
        })
        .to_string();
    }

    let mut lines = vec![
        format!("[deps] {operation} bun"),
        format!("repo: {}", report.plan.repo_root.display()),
        format!("manifest: {}", report.plan.manifest_path.display()),
        format!("library: {}", report.plan.library_path.display()),
        format!("dry-run: {}", report.plan.dry_run),
        format!("outcome: {}", report.outcome.as_str()),
        String::new(),
        format!("Package plan ({})", report.plan.packages.len()),
    ];
    if report.plan.packages.is_empty() {
        lines.push("- none".to_owned());
    }
    for package in &report.plan.packages {
        lines.push(format!(
            "- {}: {}",
            package.name,
            package_action_name(package.action)
        ));
        lines.push(format!("  local: {}", package.local_path.display()));
        lines.push(format!(
            "  before: {}",
            package.before.as_deref().unwrap_or("<absent>")
        ));
        lines.push(format!(
            "  after: {}",
            package.after.as_deref().unwrap_or("<absent>")
        ));
    }

    lines.push(String::new());
    lines.push(format!("Writes ({})", report.writes.len()));
    if report.writes.is_empty() {
        lines.push("- none".to_owned());
    }
    for write in &report.writes {
        lines.push(format!(
            "- {}: {}",
            write_action_name(write.action),
            write.path.display()
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Verification: {}",
        verification_name(report.verification.status)
    ));
    lines.push(format!(
        "dependency install pending: {}",
        report.verification.install_pending
    ));
    for file in &report.verification.immutable_files {
        lines.push(format!(
            "- {}: {}",
            file.path.display(),
            if file.unchanged {
                "unchanged"
            } else {
                "changed"
            }
        ));
    }

    if !report.plan.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", report.plan.warnings.len()));
        lines.extend(
            report
                .plan
                .warnings
                .iter()
                .map(|warning| format!("- [{}] {}", warning.code, warning.message)),
        );
    }
    if !report.errors.is_empty() {
        lines.push(String::new());
        lines.push(format!("Errors ({})", report.errors.len()));
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }

    lines.push(String::new());
    lines.push("Next actions".to_owned());
    lines.extend(next_actions.into_iter().map(|action| format!("- {action}")));
    lines.join("\n")
}

fn next_actions(report: &BunPinOperationReport) -> Vec<String> {
    match report.outcome {
        BunPinOutcome::Applied => vec![format!(
            "run `bun install` in `{}` and review the resulting lockfile separately",
            report.plan.repo_root.display()
        )],
        BunPinOutcome::DryRun => vec![
            format!(
                "re-run `effigy deps {} bun {}` without `--dry-run`",
                operation_name(report.plan.operation),
                report.plan.library_path.display()
            ),
            "then run `bun install` and review the resulting lockfile separately".to_owned(),
        ],
        BunPinOutcome::AlreadyApplied => {
            vec!["run `bun install` if the committed override has not yet been resolved".to_owned()]
        }
        BunPinOutcome::NoMatch => vec![
            "confirm the consumer graph contains a named package from the library checkout"
                .to_owned(),
        ],
        BunPinOutcome::Conflict => report
            .errors
            .iter()
            .cloned()
            .chain(std::iter::once(
                "resolve the conflict, then re-run the same command".to_owned(),
            ))
            .collect(),
        BunPinOutcome::ApplyFailed => vec![
            "repair the reported manifest or lockfile problem, then re-run the same command"
                .to_owned(),
        ],
    }
}

fn operation_name(operation: BunPinOperation) -> &'static str {
    match operation {
        BunPinOperation::Pin => "pin",
        BunPinOperation::Unpin => "unpin",
    }
}

fn package_action_name(action: BunPinPackageAction) -> &'static str {
    match action {
        BunPinPackageAction::Add => "add",
        BunPinPackageAction::Remove => "remove",
        BunPinPackageAction::AlreadyApplied => "already-applied",
        BunPinPackageAction::Conflict => "conflict",
    }
}

fn verification_name(status: BunPinVerificationStatus) -> &'static str {
    match status {
        BunPinVerificationStatus::NotRun => "not-run",
        BunPinVerificationStatus::ManifestVerified => "manifest-verified",
        BunPinVerificationStatus::Failed => "failed",
    }
}

fn write_action_name(action: BunPinWriteAction) -> &'static str {
    match action {
        BunPinWriteAction::Update => "update",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use effigy_deps::{DepsError, ProcessOutput, ProcessRequest};
    use tempfile::TempDir;

    use super::*;

    struct FixtureProcess {
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl FixtureProcess {
        fn new() -> Self {
            Self {
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl ReadOnlyProcess for FixtureProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
            self.requests.borrow_mut().push(request.clone());
            Ok(ProcessOutput {
                status: Some(0),
                stdout: "consumer node_modules\n└── @acme/core@1.0.0\n".to_owned(),
                stderr: String::new(),
            })
        }
    }

    fn fixture() -> (TempDir, TempDir) {
        let consumer = TempDir::new().unwrap();
        let library = TempDir::new().unwrap();
        fs::write(
            consumer.path().join("package.json"),
            "{\n  \"name\": \"consumer\",\n  \"dependencies\": {\"@acme/core\": \"^1\"}\n}\n",
        )
        .unwrap();
        fs::write(
            library.path().join("package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"1.0.0\"}\n",
        )
        .unwrap();
        (consumer, library)
    }

    #[test]
    fn pin_text_and_json_share_committed_semantics_and_next_action() {
        let (consumer, library) = fixture();
        let process = FixtureProcess::new();
        let text = run_bun_pin(
            Some(consumer.path().to_path_buf()),
            library.path(),
            true,
            false,
            &process,
        )
        .unwrap();
        let rendered = run_bun_pin(
            Some(consumer.path().to_path_buf()),
            library.path(),
            true,
            true,
            &process,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] pin bun"));
        assert!(text.contains("outcome: dry-run"));
        assert!(text.contains("then run `bun install`"));
        assert_eq!(json["schema"], "effigy.deps.pin.v1");
        assert_eq!(json["operation"], "pin");
        assert_eq!(json["outcome"], "dry-run");
        assert_eq!(json["packages"][0]["action"], "add");
        assert_eq!(json["writes"], json!([]));
        assert!(json["next_actions"][1]
            .as_str()
            .unwrap()
            .contains("bun install"));
        assert_eq!(
            fs::read_to_string(consumer.path().join("package.json")).unwrap(),
            "{\n  \"name\": \"consumer\",\n  \"dependencies\": {\"@acme/core\": \"^1\"}\n}\n"
        );
    }

    #[test]
    fn relative_library_path_resolves_from_selected_repo_for_pin_and_unpin() {
        let root = TempDir::new().unwrap();
        let consumer = root.path().join("consumer");
        let library = root.path().join("library");
        fs::create_dir_all(&consumer).unwrap();
        fs::create_dir_all(&library).unwrap();
        fs::write(consumer.join("package.json"), "{}\n").unwrap();
        fs::write(
            library.join("package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"1.0.0\"}\n",
        )
        .unwrap();
        let process = FixtureProcess::new();

        let pin = run_bun_pin(
            Some(consumer.clone()),
            Path::new("../library"),
            true,
            true,
            &process,
        )
        .unwrap();
        let unpin =
            run_bun_unpin(Some(consumer.clone()), Path::new("../library"), true, true).unwrap();
        let expected = fs::canonicalize(library).unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&pin).unwrap()["library_path"],
            expected.display().to_string()
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&unpin).unwrap()["library_path"],
            expected.display().to_string()
        );
    }
}
