use std::path::{Path, PathBuf};

use effigy_cli::{DepsArgs, DepsManager, DepsSubcommand};
use effigy_deps::{
    execute_bun_link, execute_bun_unlink, execute_cargo_link, execute_cargo_unlink,
    inspect_dependency_status, BunLinkOperationReport, BunLinkOutcome, BunRegistrationIndexStore,
    BunUnlinkOperationReport, CargoLinkOperationReport, CargoUnlinkOperationReport,
    CommittedSource, DependencyHealthSeverity, DependencyLinkReport, DependencyStatusReport,
    ObservedState, PackageManager, ReadOnlyProcess, RepoLinkStateStore, StdReadOnlyProcess,
};
use serde_json::json;

use super::command_context::resolve_active_repo_root;
use super::RunnerError;

mod pin;

pub(in crate::runner) fn run_deps(args: DepsArgs) -> Result<String, RunnerError> {
    let needs_home = matches!(
        &args.subcommand,
        DepsSubcommand::Status { .. }
            | DepsSubcommand::Link {
                manager: DepsManager::Bun,
                ..
            }
            | DepsSubcommand::Unlink {
                manager: DepsManager::Bun,
                ..
            }
    );
    if !needs_home {
        return run_deps_with_home(args, Path::new(""));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| RunnerError::task_invocation("HOME is not set; cannot inspect Bun links"))?;
    run_deps_with_home(args, &home)
}

fn run_deps_with_home(args: DepsArgs, home: &Path) -> Result<String, RunnerError> {
    run_deps_with_home_and_process(args, home, &StdReadOnlyProcess)
}

fn run_deps_with_home_and_process(
    args: DepsArgs,
    home: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<String, RunnerError> {
    match args.subcommand {
        DepsSubcommand::Status { manager } => {
            run_deps_status(args.repo_override, manager, args.output_json, home)
        }
        DepsSubcommand::Link {
            manager,
            library_path,
            dry_run,
        } => match manager {
            DepsManager::Cargo => {
                run_cargo_link(args.repo_override, &library_path, dry_run, args.output_json)
            }
            DepsManager::Bun => run_bun_link(
                args.repo_override,
                &library_path,
                home,
                dry_run,
                args.output_json,
                process,
            ),
        },
        DepsSubcommand::Unlink {
            manager,
            library_path,
            dry_run,
        } => match manager {
            DepsManager::Cargo => {
                run_cargo_unlink(args.repo_override, &library_path, dry_run, args.output_json)
            }
            DepsManager::Bun => run_bun_unlink(
                args.repo_override,
                &library_path,
                home,
                dry_run,
                args.output_json,
                process,
            ),
        },
        DepsSubcommand::Pin {
            manager,
            library_path,
            dry_run,
        } => match manager {
            DepsManager::Cargo => Err(RunnerError::task_invocation(
                "`effigy deps pin cargo` is unsupported; committed pinning is available only for Bun overrides",
            )),
            DepsManager::Bun => pin::run_bun_pin(
                args.repo_override,
                &library_path,
                dry_run,
                args.output_json,
                process,
            ),
        },
        DepsSubcommand::Unpin {
            manager,
            library_path,
            dry_run,
        } => match manager {
            DepsManager::Cargo => Err(RunnerError::task_invocation(
                "`effigy deps unpin cargo` is unsupported; committed pinning is available only for Bun overrides",
            )),
            DepsManager::Bun => pin::run_bun_unpin(
                args.repo_override,
                &library_path,
                dry_run,
                args.output_json,
            ),
        },
    }
}

fn run_bun_link(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    home: &Path,
    dry_run: bool,
    output_json: bool,
    process: &impl ReadOnlyProcess,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let library_path = resolve_link_library_path(&repo_root, library_path);
    let report = execute_bun_link(&repo_root, &library_path, home, dry_run, process)
        .map_err(map_deps_error)?;
    let rendered = render_bun_link(&repo_root, &report, output_json);
    finish_deps_operation(
        "deps link bun",
        report.outcome.as_str(),
        report.outcome.is_success(),
        &report.errors,
        rendered,
    )
}

fn render_bun_link(repo_root: &Path, report: &BunLinkOperationReport, output_json: bool) -> String {
    if output_json {
        return json!({
            "schema": "effigy.deps.link.v1",
            "schema_version": 1,
            "command": "deps link bun",
            "repo_root": repo_root,
            "manager": "bun",
            "library_path": report.plan.operation.key.library_path,
            "dry_run": report.plan.operation.dry_run,
            "report": report,
        })
        .to_string();
    }

    let mut lines = vec![
        "[deps] link bun".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "library: {}",
            report.plan.operation.key.library_path.display()
        ),
        format!("outcome: {}", report.outcome.as_str()),
        format!("verification: {:?}", report.verification.status).to_lowercase(),
        String::new(),
        format!("Process intents ({})", report.plan.process_intents.len()),
    ];
    if report.plan.process_intents.is_empty() {
        lines.push(if report.outcome == BunLinkOutcome::CommittedPinActive {
            "- none; a committed override blocks ephemeral linking".to_owned()
        } else {
            "- none; Bun links already match".to_owned()
        });
    }
    for intent in &report.plan.process_intents {
        lines.push(format!(
            "- {} {} @ {}: {} {}",
            format!("{:?}", intent.action).to_lowercase(),
            intent.packages.join(","),
            intent.cwd.display(),
            intent.program,
            intent.args.join(" ")
        ));
    }

    lines.push(String::new());
    lines.push(format!("Package closure ({})", report.plan.packages.len()));
    for package in &report.plan.packages {
        let observed = report
            .verification
            .evidence
            .iter()
            .find(|evidence| evidence.package == package.name);
        lines.push(format!(
            "- {}: {}, {}",
            package.name,
            format!("{:?}", package.registration).to_lowercase(),
            format!("{:?}", package.consumer_link).to_lowercase()
        ));
        lines.push(format!("  planned: {}", package.local_path.display()));
        lines.push(format!(
            "  observed: {}",
            observed
                .and_then(|evidence| evidence.observed_source.as_deref())
                .unwrap_or(if report.plan.operation.dry_run {
                    "not run (dry-run)"
                } else {
                    "not resolved"
                })
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Immutable files ({})",
        report.immutable_files.len()
    ));
    for file in &report.immutable_files {
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
    if !report.peer_diagnostics.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Peer diagnostics ({})",
            report.peer_diagnostics.len()
        ));
        for peer in &report.peer_diagnostics {
            lines.push(format!(
                "- {} -> {} {}: {}",
                peer.package,
                peer.peer,
                peer.requirement,
                peer.status.as_str()
            ));
            if let Some(message) = &peer.message {
                lines.push(format!("  {message}"));
            }
        }
    }
    if !report.plan.operation.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Warnings ({})",
            report.plan.operation.warnings.len()
        ));
        lines.extend(
            report
                .plan
                .operation
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }
    if report.rollback.attempted {
        lines.push(String::new());
        lines.push(format!(
            "Rollback: consumer {}, registrations {}, files {}, failures {}",
            report.rollback.restored_consumer_links.len(),
            report.rollback.removed_registrations.len(),
            report.rollback.restored_files.len(),
            report.rollback.failures.len()
        ));
    }
    if !report.errors.is_empty() {
        lines.push(String::new());
        lines.push(format!("Errors ({})", report.errors.len()));
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }
    lines.join("\n")
}

fn run_bun_unlink(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    home: &Path,
    dry_run: bool,
    output_json: bool,
    process: &impl ReadOnlyProcess,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let report = execute_bun_unlink(&repo_root, library_path, home, dry_run, process)
        .map_err(map_deps_error)?;
    let rendered = render_bun_unlink(&repo_root, &report, output_json);
    finish_deps_operation(
        "deps unlink bun",
        report.outcome.as_str(),
        report.outcome.is_success(),
        &report.errors,
        rendered,
    )
}

fn render_bun_unlink(
    repo_root: &Path,
    report: &BunUnlinkOperationReport,
    output_json: bool,
) -> String {
    if output_json {
        return json!({
            "schema": "effigy.deps.unlink.v1",
            "schema_version": 1,
            "command": "deps unlink bun",
            "repo_root": repo_root,
            "manager": "bun",
            "library_path": report.plan.operation.key.library_path,
            "dry_run": report.plan.operation.dry_run,
            "report": report,
        })
        .to_string();
    }

    let mut lines = vec![
        "[deps] unlink bun".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "library: {}",
            report.plan.operation.key.library_path.display()
        ),
        format!("outcome: {}", report.outcome.as_str()),
        format!("verification: {:?}", report.verification.status).to_lowercase(),
        String::new(),
        format!(
            "Consumer link removals ({})",
            report.plan.symlink_intents.len()
        ),
    ];
    if report.plan.symlink_intents.is_empty() {
        lines.push("- none; matching local symlinks are already absent".to_owned());
    }
    for intent in &report.plan.symlink_intents {
        lines.push(format!(
            "- {}: {} -> {}",
            intent.package,
            intent.path.display(),
            intent.expected_target.display()
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Registration releases ({})",
        report.plan.packages.len()
    ));
    for package in &report.plan.packages {
        lines.push(format!(
            "- {}: {}",
            package.name,
            package
                .reference_release
                .map(|release| format!("{release:?}").to_lowercase())
                .unwrap_or_else(|| "none".to_owned())
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Unregister processes ({})",
        report.plan.process_intents.len()
    ));
    if report.plan.process_intents.is_empty() {
        lines.push("- none; shared/foreign/unverifiable registrations retained".to_owned());
    }
    for intent in &report.plan.process_intents {
        lines.push(format!(
            "- {} @ {}: {} {}",
            intent.packages.join(","),
            intent.cwd.display(),
            intent.program,
            intent.args.join(" ")
        ));
    }

    lines.push(String::new());
    lines.push(format!(
        "Immutable files ({})",
        report.immutable_files.len()
    ));
    for file in &report.immutable_files {
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
    if !report.plan.operation.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Notes ({})", report.plan.operation.warnings.len()));
        lines.extend(
            report
                .plan
                .operation
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }
    if report.rollback.attempted {
        lines.push(String::new());
        lines.push(format!(
            "Rollback: consumer {}, registrations {}, failures {}",
            report.rollback.relinked_consumer_packages.len(),
            report.rollback.restored_registrations.len(),
            report.rollback.failures.len()
        ));
    }
    if !report.errors.is_empty() {
        lines.push(String::new());
        lines.push(format!("Errors ({})", report.errors.len()));
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }
    lines.join("\n")
}

fn run_cargo_unlink(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    dry_run: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let report = execute_cargo_unlink(&repo_root, library_path, dry_run, &StdReadOnlyProcess)
        .map_err(map_deps_error)?;
    let rendered = render_cargo_unlink(&repo_root, &report, output_json);
    finish_deps_operation(
        "deps unlink cargo",
        report.outcome.as_str(),
        report.outcome.is_success(),
        &report.errors,
        rendered,
    )
}

fn render_cargo_unlink(
    repo_root: &Path,
    report: &CargoUnlinkOperationReport,
    output_json: bool,
) -> String {
    if output_json {
        return json!({
            "schema": "effigy.deps.unlink.v1",
            "schema_version": 1,
            "command": "deps unlink cargo",
            "repo_root": repo_root,
            "manager": "cargo",
            "library_path": report.plan.operation.key.library_path,
            "dry_run": report.plan.operation.dry_run,
            "report": report,
        })
        .to_string();
    }

    let mut lines = vec![
        "[deps] unlink cargo".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "library: {}",
            report.plan.operation.key.library_path.display()
        ),
        format!("outcome: {}", report.outcome.as_str()),
        format!("verification: {:?}", report.verification.status).to_lowercase(),
        String::new(),
        format!("Planned changes ({})", report.plan.operation.changes.len()),
    ];
    if report.plan.operation.changes.is_empty() {
        lines.push("- none; dependency link is already absent".to_owned());
    }
    for change in &report.plan.operation.changes {
        lines.push(format!(
            "- {} {}: {}",
            format!("{:?}", change.action).to_lowercase(),
            change.target.display(),
            change.description
        ));
        render_snapshot(&mut lines, "before", change.before.as_deref());
        render_snapshot(&mut lines, "after", change.after.as_deref());
    }

    lines.push(String::new());
    lines.push(format!(
        "Committed resolutions ({})",
        report.plan.expected_resolutions.len()
    ));
    if report.plan.expected_resolutions.is_empty() {
        lines.push("- none".to_owned());
    }
    for expected in &report.plan.expected_resolutions {
        let observed = report.verification.evidence.iter().find(|evidence| {
            evidence.package == expected.package
                && evidence.consumer_root.as_ref() == Some(&expected.consumer_root)
                && evidence
                    .committed_sources
                    .contains(&expected.committed_source)
        });
        lines.push(format!(
            "- {} @ {}",
            expected.package,
            expected.consumer_root.display()
        ));
        lines.push(format!(
            "  expected: {}",
            render_committed_source(&expected.committed_source)
        ));
        lines.push(format!(
            "  observed: {}",
            observed
                .and_then(|evidence| evidence.observed_source.as_deref())
                .unwrap_or(if report.plan.operation.dry_run {
                    "not run (dry-run)"
                } else {
                    "not resolved"
                })
        ));
        if let Some(message) = observed.and_then(|evidence| evidence.message.as_deref()) {
            lines.push(format!("  error: {message}"));
        }
    }

    lines.push(String::new());
    lines.push(format!("Lock recovery ({})", report.lockfiles.len()));
    if report.lockfiles.is_empty() {
        lines.push("- not run".to_owned());
    }
    for lock in &report.lockfiles {
        lines.push(
            format!(
                "- {}: {:?} -> {:?}",
                lock.path.display(),
                lock.before_state,
                lock.after_state
            )
            .to_lowercase(),
        );
    }
    if !report.removed_directories.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Removed owned directories ({})",
            report.removed_directories.len()
        ));
        lines.extend(
            report
                .removed_directories
                .iter()
                .map(|path| format!("- {}", path.display())),
        );
    }
    if !report.plan.operation.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Notes ({})", report.plan.operation.warnings.len()));
        lines.extend(
            report
                .plan
                .operation
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }
    if !report.errors.is_empty() {
        lines.push(String::new());
        lines.push(format!("Errors ({})", report.errors.len()));
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }
    lines.join("\n")
}

fn run_cargo_link(
    repo_override: Option<PathBuf>,
    library_path: &Path,
    dry_run: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let library_path = resolve_link_library_path(&repo_root, library_path);
    let report = execute_cargo_link(&repo_root, &library_path, dry_run, &StdReadOnlyProcess)
        .map_err(map_deps_error)?;
    let rendered = render_cargo_link(&repo_root, &report, output_json);
    finish_deps_operation(
        "deps link cargo",
        report.outcome.as_str(),
        report.outcome.is_success(),
        &report.errors,
        rendered,
    )
}

fn render_cargo_link(
    repo_root: &Path,
    report: &CargoLinkOperationReport,
    output_json: bool,
) -> String {
    if output_json {
        return json!({
            "schema": "effigy.deps.link.v1",
            "schema_version": 1,
            "command": "deps link cargo",
            "repo_root": repo_root,
            "manager": "cargo",
            "library_path": report.plan.operation.key.library_path,
            "dry_run": report.plan.operation.dry_run,
            "report": report,
        })
        .to_string();
    }

    let mut lines = vec![
        "[deps] link cargo".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "library: {}",
            report.plan.operation.key.library_path.display()
        ),
        format!("outcome: {}", report.outcome.as_str()),
        format!("verification: {:?}", report.verification.status).to_lowercase(),
    ];
    lines.push(String::new());
    lines.push(format!(
        "Planned changes ({})",
        report.plan.operation.changes.len()
    ));
    if report.plan.operation.changes.is_empty() {
        lines.push("- none; physical state already matches the plan".to_owned());
    }
    for change in &report.plan.operation.changes {
        lines.push(format!(
            "- {} {}: {}",
            format!("{:?}", change.action).to_lowercase(),
            change.target.display(),
            change.description
        ));
        render_snapshot(&mut lines, "before", change.before.as_deref());
        render_snapshot(&mut lines, "after", change.after.as_deref());
    }

    lines.push(String::new());
    lines.push(format!(
        "Package resolutions ({})",
        report.plan.expected_resolutions.len()
    ));
    for expected in &report.plan.expected_resolutions {
        let observed = report.verification.evidence.iter().find(|evidence| {
            evidence.package == expected.package
                && evidence.consumer_root.as_ref() == Some(&expected.consumer_root)
                && evidence
                    .committed_sources
                    .contains(&expected.committed_source)
        });
        lines.push(format!(
            "- {} @ {}",
            expected.package,
            expected.consumer_root.display()
        ));
        lines.push(format!(
            "  committed: {}",
            render_committed_source(&expected.committed_source)
        ));
        lines.push(format!("  planned: {}", expected.local_path.display()));
        lines.push(format!(
            "  observed: {}",
            observed
                .and_then(|evidence| evidence.observed_source.as_deref())
                .unwrap_or(if report.plan.operation.dry_run {
                    "not run (dry-run)"
                } else {
                    "not resolved"
                })
        ));
        if let Some(message) = observed.and_then(|evidence| evidence.message.as_deref()) {
            lines.push(format!("  error: {message}"));
        }
    }

    lines.push(String::new());
    lines.push(format!(
        "Affected lockfiles ({})",
        report.plan.affected_lockfiles.len()
    ));
    if report.plan.affected_lockfiles.is_empty() {
        lines.push("- none tracked".to_owned());
    } else {
        lines.extend(
            report
                .plan
                .affected_lockfiles
                .iter()
                .map(|path| format!("- {} (do not commit while linked)", path.display())),
        );
    }
    if !report.plan.operation.warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Warnings ({})",
            report.plan.operation.warnings.len()
        ));
        lines.extend(
            report
                .plan
                .operation
                .warnings
                .iter()
                .map(|warning| format!("- {warning}")),
        );
    }
    if report.rollback.attempted {
        lines.push(String::new());
        lines.push(format!(
            "Rollback: restored {}, failures {}",
            report.rollback.restored.len(),
            report.rollback.failures.len()
        ));
    }
    if !report.errors.is_empty() {
        lines.push(String::new());
        lines.push(format!("Errors ({})", report.errors.len()));
        lines.extend(report.errors.iter().map(|error| format!("- {error}")));
    }
    lines.join("\n")
}

fn finish_deps_operation(
    command: &'static str,
    outcome: &'static str,
    outcome_succeeded: bool,
    errors: &[String],
    rendered: String,
) -> Result<String, RunnerError> {
    if outcome_succeeded && errors.is_empty() {
        return Ok(rendered);
    }
    Err(RunnerError::DepsOperationNonZero {
        command,
        outcome,
        error_count: errors.len(),
        rendered,
    })
}

fn render_snapshot(lines: &mut Vec<String>, label: &str, snapshot: Option<&str>) {
    lines.push(format!("  {label}:"));
    match snapshot {
        None => lines.push("    <absent>".to_owned()),
        Some("") => lines.push("    <empty>".to_owned()),
        Some(snapshot) => lines.extend(snapshot.lines().map(|line| format!("    {line}"))),
    }
}

fn render_committed_source(source: &CommittedSource) -> String {
    format!(
        "{} {}",
        format!("{:?}", source.kind).to_lowercase(),
        source.identity
    )
}

fn run_deps_status(
    repo_override: Option<PathBuf>,
    manager: Option<DepsManager>,
    output_json: bool,
    home: &Path,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let state = RepoLinkStateStore::for_repo(&repo_root)
        .read()
        .map_err(map_deps_error)?;
    let bun_index = BunRegistrationIndexStore::for_home(home)
        .read()
        .map_err(map_deps_error)?;
    let mut report =
        inspect_dependency_status(&repo_root, home, &state, &bun_index, &StdReadOnlyProcess)
            .map_err(map_deps_error)?;
    if let Some(manager) = manager {
        let manager = domain_manager(manager);
        report.links.retain(|link| link.manager == manager);
    }
    Ok(render_deps_status(
        &repo_root,
        manager,
        &report,
        output_json,
    ))
}

fn render_deps_status(
    repo_root: &Path,
    manager: Option<DepsManager>,
    report: &DependencyStatusReport,
    output_json: bool,
) -> String {
    if output_json {
        return json!({
            "schema": "effigy.deps.status.v1",
            "schema_version": 1,
            "command": "deps status",
            "repo_root": repo_root,
            "manager": manager.map(DepsManager::as_str),
            "summary": status_summary(report),
            "links": report.links,
        })
        .to_string();
    }

    let mut lines = vec![
        "[deps] status".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "manager: {}",
            manager.map(DepsManager::as_str).unwrap_or("all")
        ),
        format!("links: {}", report.links.len()),
    ];
    if report.links.is_empty() {
        lines.push("no machine-local dependency links configured".to_owned());
        return lines.join("\n");
    }
    for link in &report.links {
        render_link_text(&mut lines, link);
    }
    lines.join("\n")
}

fn render_link_text(lines: &mut Vec<String>, link: &DependencyLinkReport) {
    let library = link
        .desired
        .as_ref()
        .map(|desired| desired.key.library_path.display().to_string())
        .unwrap_or_else(|| "unowned physical state".to_owned());
    lines.push(String::new());
    lines.push(format!(
        "[{}] {}: {library}",
        link.manager.as_str(),
        link.observed.state.as_str()
    ));
    if let Some(desired) = &link.desired {
        lines.push("desired: linked".to_owned());
        lines.push(format!("mechanism: {}", desired.mechanism.as_str()));
        lines.push(format!(
            "consumers: {}",
            desired
                .consumer_roots
                .iter()
                .map(|root| root.canonical_path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        lines.push(format!(
            "packages: {}",
            desired
                .packages
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines.push(format!("verification: {:?}", link.verification.status).to_lowercase());
    for reason in &link.observed.drift {
        lines.push(format!(
            "- [{}] {}: {}",
            reason.severity.as_str(),
            reason.code,
            reason.message
        ));
        if let Some(package) = &reason.package {
            lines.push(format!("  package: {package}"));
        }
        for evidence in &reason.evidence {
            lines.push(format!("  evidence: {evidence}"));
        }
        if let Some(remediation) = &reason.remediation {
            lines.push(format!("  remediation: {remediation}"));
        }
    }
    if !link.peer_diagnostics.is_empty() {
        lines.push(format!("peer diagnostics: {}", link.peer_diagnostics.len()));
        for peer in &link.peer_diagnostics {
            lines.push(format!(
                "- {} -> {} {}: {}",
                peer.package,
                peer.peer,
                peer.requirement,
                peer.status.as_str()
            ));
            if let Some(path) = &peer.consumer_resolution {
                lines.push(format!("  consumer: {}", path.display()));
            }
            if let Some(path) = &peer.local_resolution {
                lines.push(format!("  local: {}", path.display()));
            }
        }
    }
}

fn status_summary(report: &DependencyStatusReport) -> serde_json::Value {
    let count = |state| {
        report
            .links
            .iter()
            .filter(|link| link.observed.state == state)
            .count()
    };
    json!({
        "total": report.links.len(),
        "missing": count(ObservedState::Missing),
        "healthy": count(ObservedState::Healthy),
        "drifted": count(ObservedState::Drifted),
        "conflict": count(ObservedState::Conflict),
        "warnings": report.links.iter().flat_map(|link| &link.observed.drift)
            .filter(|finding| finding.severity == DependencyHealthSeverity::Warning).count(),
        "errors": report.links.iter().flat_map(|link| &link.observed.drift)
            .filter(|finding| finding.severity == DependencyHealthSeverity::Error).count(),
    })
}

fn domain_manager(manager: DepsManager) -> PackageManager {
    match manager {
        DepsManager::Cargo => PackageManager::Cargo,
        DepsManager::Bun => PackageManager::Bun,
    }
}

fn map_deps_error(error: effigy_deps::DepsError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

fn resolve_link_library_path(repo_root: &Path, library_path: &Path) -> PathBuf {
    if library_path.is_absolute() {
        library_path.to_path_buf()
    } else {
        repo_root.join(library_path)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::process::Command;

    use effigy_deps::{
        BunLinkOutcome, BunPeerDiagnostic, BunPeerResolutionStatus, BunUnlinkOutcome,
        CargoLinkOutcome, CargoLinkOwnership, CargoUnlinkOutcome, ConsumerRoot, DependencyLinkKey,
        DependencyPackage, DependencyVerification, DepsError, DesiredDependencyLink, DriftReason,
        LinkMechanism, ObservedDependencyLink, ProcessOutput, ProcessRequest, RepoLinkState,
        VerificationStatus,
    };
    use tempfile::TempDir;

    use super::*;

    fn repo() -> TempDir {
        let repo = TempDir::new().unwrap();
        fs::write(repo.path().join("package.json"), "{}\n").unwrap();
        repo
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn run(cwd: &Path, program: &str, args: &[&str]) {
        let output = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn cargo_link_fixture() -> (TempDir, TempDir) {
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("Cargo.toml"),
            "[workspace]\nmembers=['crates/fixture']\nresolver='2'\n",
        );
        write(
            &library.path().join("crates/fixture/Cargo.toml"),
            "[package]\nname='effigy-cli-link-fixture'\nversion='0.1.0'\nedition='2021'\n",
        );
        write(
            &library.path().join("crates/fixture/src/lib.rs"),
            "pub fn value() {}\n",
        );
        run(library.path(), "git", &["init", "-q"]);
        run(
            library.path(),
            "git",
            &["config", "user.email", "effigy-fixture@example.test"],
        );
        run(
            library.path(),
            "git",
            &["config", "user.name", "Effigy Fixture"],
        );
        run(library.path(), "git", &["add", "."]);
        run(library.path(), "git", &["commit", "-qm", "fixture"]);
        run(library.path(), "git", &["tag", "v0.1.0"]);

        let consumer = TempDir::new().unwrap();
        let library_path = fs::canonicalize(library.path()).unwrap();
        write(
            &consumer.path().join("Cargo.toml"),
            &format!(
                "[package]\nname='effigy-cli-link-consumer'\nversion='0.1.0'\nedition='2021'\n[dependencies]\neffigy-cli-link-fixture={{git='file://{}',tag='v0.1.0'}}\n",
                library_path.display()
            ),
        );
        write(
            &consumer.path().join("src/lib.rs"),
            "pub fn consumer() { effigy_cli_link_fixture::value(); }\n",
        );
        run(consumer.path(), "cargo", &["generate-lockfile"]);
        run(consumer.path(), "git", &["init", "-q"]);
        run(
            consumer.path(),
            "git",
            &["config", "user.email", "effigy-fixture@example.test"],
        );
        run(
            consumer.path(),
            "git",
            &["config", "user.name", "Effigy Fixture"],
        );
        run(consumer.path(), "git", &["add", "."]);
        run(consumer.path(), "git", &["commit", "-qm", "consumer"]);
        (consumer, library)
    }

    fn status_args(manager: Option<DepsManager>, repo: &Path, output_json: bool) -> DepsArgs {
        DepsArgs {
            subcommand: DepsSubcommand::Status { manager },
            repo_override: Some(repo.to_path_buf()),
            output_json,
        }
    }

    #[test]
    fn relative_link_library_paths_resolve_from_the_selected_repo() {
        let consumer = repo();
        let relative = Path::new("missing-library");
        let expected = consumer.path().join(relative);

        assert_eq!(
            resolve_link_library_path(consumer.path(), consumer.path()),
            consumer.path()
        );

        let error =
            run_cargo_link(Some(consumer.path().to_path_buf()), relative, true, false).unwrap_err();

        assert!(
            error.to_string().contains(&expected.display().to_string()),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn cargo_pin_and_unpin_fail_as_explicitly_unsupported() {
        let repo = repo();
        let home = TempDir::new().unwrap();
        for subcommand in [
            DepsSubcommand::Pin {
                manager: DepsManager::Cargo,
                library_path: PathBuf::from("../library"),
                dry_run: true,
            },
            DepsSubcommand::Unpin {
                manager: DepsManager::Cargo,
                library_path: PathBuf::from("../library"),
                dry_run: false,
            },
        ] {
            let error = run_deps_with_home(
                DepsArgs {
                    subcommand,
                    repo_override: Some(repo.path().to_path_buf()),
                    output_json: false,
                },
                home.path(),
            )
            .unwrap_err();

            assert!(error.to_string().contains("unsupported"));
            assert!(error.to_string().contains("only for Bun overrides"));
        }
    }

    #[test]
    fn bare_status_renders_empty_text_and_json_from_the_same_report() {
        let repo = repo();
        let home = TempDir::new().unwrap();

        let text = run_deps_with_home(status_args(None, repo.path(), false), home.path()).unwrap();
        let rendered =
            run_deps_with_home(status_args(None, repo.path(), true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] status"));
        assert!(text.contains("no machine-local dependency links configured"));
        assert_eq!(json["schema"], "effigy.deps.status.v1");
        assert_eq!(json["manager"], serde_json::Value::Null);
        assert_eq!(json["summary"]["total"], 0);
        assert_eq!(json["links"], json!([]));
    }

    #[test]
    fn status_text_and_json_share_health_evidence_and_peer_diagnostics() {
        let repo = repo();
        let repo_root = fs::canonicalize(repo.path()).unwrap();
        let library = repo_root.join("library");
        let package_path = library.join("core");
        let consumer_peer = repo_root.join("node_modules/svelte");
        let local_peer = package_path.join("node_modules/svelte");
        let desired = DesiredDependencyLink {
            key: DependencyLinkKey {
                manager: PackageManager::Bun,
                consumer_repo: repo_root.clone(),
                library_path: library,
            },
            mechanism: LinkMechanism::BunLink,
            consumer_roots: vec![ConsumerRoot {
                canonical_path: repo_root.clone(),
            }],
            packages: vec![DependencyPackage {
                name: "@scope/core".to_owned(),
                local_path: package_path,
                committed_sources: Vec::new(),
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: None,
        };
        let report = DependencyStatusReport {
            links: vec![DependencyLinkReport {
                manager: PackageManager::Bun,
                desired: Some(desired),
                observed: ObservedDependencyLink {
                    state: ObservedState::Conflict,
                    packages: Vec::new(),
                    drift: vec![DriftReason {
                        code: "bun-peer-duplicate-resolution".to_owned(),
                        severity: DependencyHealthSeverity::Error,
                        message: "Svelte resolves from two paths".to_owned(),
                        evidence: vec![
                            consumer_peer.display().to_string(),
                            local_peer.display().to_string(),
                        ],
                        remediation: Some("hoist/dedupe Svelte".to_owned()),
                        package: Some("@scope/core".to_owned()),
                    }],
                },
                plan: None,
                verification: DependencyVerification {
                    status: VerificationStatus::Failed,
                    evidence: Vec::new(),
                },
                peer_diagnostics: vec![BunPeerDiagnostic {
                    package: "@scope/core".to_owned(),
                    peer: "svelte".to_owned(),
                    requirement: "^5".to_owned(),
                    status: BunPeerResolutionStatus::Duplicate,
                    consumer_resolution: Some(consumer_peer.clone()),
                    local_resolution: Some(local_peer.clone()),
                    message: Some("hoist/dedupe Svelte".to_owned()),
                }],
            }],
        };

        let text = render_deps_status(&repo_root, None, &report, false);
        let rendered = render_deps_status(&repo_root, None, &report, true);
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("mechanism: bun-link"));
        assert!(text.contains(&format!("consumers: {}", repo_root.display())));
        assert!(text.contains("[error] bun-peer-duplicate-resolution"));
        assert!(text.contains(&format!("evidence: {}", consumer_peer.display())));
        assert!(text.contains("remediation: hoist/dedupe Svelte"));
        assert!(text.contains("peer diagnostics: 1"));
        assert_eq!(json["summary"]["errors"], 1);
        assert_eq!(json["summary"]["warnings"], 0);
        assert_eq!(json["links"][0]["desired"]["mechanism"], "bun-link");
        assert_eq!(
            json["links"][0]["observed"]["drift"][0]["evidence"][0],
            consumer_peer.display().to_string()
        );
        assert_eq!(
            json["links"][0]["peer_diagnostics"][0]["local_resolution"],
            local_peer.display().to_string()
        );
    }

    #[test]
    fn manager_filter_keeps_only_matching_reports() {
        let repo = repo();
        let repo_root = fs::canonicalize(repo.path()).unwrap();
        let home = TempDir::new().unwrap();
        let links = [
            (PackageManager::Cargo, LinkMechanism::CargoPatch),
            (PackageManager::Bun, LinkMechanism::BunLink),
        ]
        .into_iter()
        .map(|(manager, mechanism)| DesiredDependencyLink {
            key: DependencyLinkKey {
                manager,
                consumer_repo: repo_root.clone(),
                library_path: repo_root.join(format!("missing-{}", manager.as_str())),
            },
            mechanism,
            consumer_roots: vec![ConsumerRoot {
                canonical_path: repo_root.clone(),
            }],
            packages: vec![DependencyPackage {
                name: "example".to_owned(),
                local_path: repo_root.join(format!("missing-{}/example", manager.as_str())),
                committed_sources: Vec::new(),
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: (manager == PackageManager::Cargo).then_some(CargoLinkOwnership {
                config_created_by_effigy: true,
                cargo_dir_created_by_effigy: true,
            }),
        })
        .collect();
        RepoLinkStateStore::for_repo(&repo_root)
            .write(&RepoLinkState {
                schema: effigy_deps::REPO_LINK_STATE_SCHEMA.to_owned(),
                schema_version: effigy_deps::REPO_LINK_STATE_SCHEMA_VERSION,
                links,
            })
            .unwrap();

        let rendered = run_deps_with_home(
            status_args(Some(DepsManager::Cargo), &repo_root, true),
            home.path(),
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(json["manager"], "cargo");
        assert_eq!(json["summary"]["total"], 1);
        assert_eq!(json["links"][0]["manager"], "cargo");
    }

    #[test]
    fn bun_unlink_noop_text_and_json_are_successful_and_non_mutating() {
        let repo = repo();
        let home = TempDir::new().unwrap();
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Unlink {
                manager: DepsManager::Bun,
                library_path: repo.path().join("library"),
                dry_run: false,
            },
            repo_override: Some(repo.path().to_path_buf()),
            output_json,
        };
        let text = run_deps_with_home(args(false), home.path()).unwrap();
        let rendered = run_deps_with_home(args(true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] unlink bun"));
        assert!(text.contains("outcome: no-op"));
        assert_eq!(json["schema"], "effigy.deps.unlink.v1");
        assert_eq!(json["report"]["outcome"], "no-op");
        assert!(!RepoLinkStateStore::for_repo(repo.path()).path().exists());
        assert!(!BunRegistrationIndexStore::for_home(home.path())
            .path()
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn bun_unlink_dry_run_text_and_json_expose_exact_non_mutating_removals() {
        use std::os::unix::fs::symlink;

        let consumer = repo();
        let repo_root = fs::canonicalize(consumer.path()).unwrap();
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("package.json"),
            "{\"name\":\"underlay\",\"version\":\"0.1.0\"}\n",
        );
        let library_root = fs::canonicalize(library.path()).unwrap();
        let home = TempDir::new().unwrap();
        let desired = DesiredDependencyLink {
            key: DependencyLinkKey {
                manager: PackageManager::Bun,
                consumer_repo: repo_root.clone(),
                library_path: library_root.clone(),
            },
            mechanism: LinkMechanism::BunLink,
            consumer_roots: vec![ConsumerRoot {
                canonical_path: repo_root.clone(),
            }],
            packages: vec![DependencyPackage {
                name: "underlay".to_owned(),
                local_path: library_root.clone(),
                committed_sources: vec![CommittedSource {
                    kind: effigy_deps::CommittedSourceKind::Registry,
                    identity: "1.2.3".to_owned(),
                }],
            }],
            cargo_resolutions: Vec::new(),
            cargo_ownership: None,
        };
        RepoLinkStateStore::for_repo(&repo_root)
            .write(&RepoLinkState {
                schema: effigy_deps::REPO_LINK_STATE_SCHEMA.to_owned(),
                schema_version: effigy_deps::REPO_LINK_STATE_SCHEMA_VERSION,
                links: vec![desired],
            })
            .unwrap();
        BunRegistrationIndexStore::for_home(home.path())
            .update(|index| {
                index.add_reference(
                    "underlay",
                    library_root.clone(),
                    true,
                    effigy_deps::BunConsumerReference {
                        consumer_repo: repo_root.clone(),
                        library_path: library_root.clone(),
                    },
                )
            })
            .unwrap();
        let registration = effigy_deps::bun_registration_path(home.path(), "underlay");
        fs::create_dir_all(registration.parent().unwrap()).unwrap();
        symlink(&library_root, &registration).unwrap();
        let consumer_link = repo_root.join("node_modules/underlay");
        fs::create_dir_all(consumer_link.parent().unwrap()).unwrap();
        symlink(&library_root, &consumer_link).unwrap();
        let state_before = fs::read(RepoLinkStateStore::for_repo(&repo_root).path()).unwrap();
        let index_before =
            fs::read(BunRegistrationIndexStore::for_home(home.path()).path()).unwrap();
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Unlink {
                manager: DepsManager::Bun,
                library_path: library_root.clone(),
                dry_run: true,
            },
            repo_override: Some(repo_root.clone()),
            output_json,
        };

        let text = run_deps_with_home(args(false), home.path()).unwrap();
        let rendered = run_deps_with_home(args(true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] unlink bun"));
        assert!(text.contains("outcome: dry-run"));
        assert!(text.contains("node_modules/underlay"));
        assert!(text.contains("bun unlink --no-save"));
        assert_eq!(json["schema"], "effigy.deps.unlink.v1");
        assert_eq!(json["report"]["outcome"], "dry-run");
        assert_eq!(
            json["report"]["plan"]["symlink_intents"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            fs::read(RepoLinkStateStore::for_repo(&repo_root).path()).unwrap(),
            state_before
        );
        assert_eq!(
            fs::read(BunRegistrationIndexStore::for_home(home.path()).path()).unwrap(),
            index_before
        );
        assert_eq!(fs::canonicalize(consumer_link).unwrap(), library_root);
    }

    struct BunInventoryProcess {
        requests: RefCell<Vec<ProcessRequest>>,
    }

    impl ReadOnlyProcess for BunInventoryProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
            self.requests.borrow_mut().push(request.clone());
            Ok(ProcessOutput {
                status: Some(0),
                stdout:
                    "consumer node_modules (2)\n├── @acme/core@1.2.3\n└── @acme/protocol@1.2.3\n"
                        .to_owned(),
                stderr: String::new(),
            })
        }
    }

    struct FailingBunLinkProcess;

    impl ReadOnlyProcess for FailingBunLinkProcess {
        fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
            if request.args == ["pm", "ls", "--all"] {
                return Ok(ProcessOutput {
                    status: Some(0),
                    stdout: "consumer node_modules (1)\n└── @acme/core@1.2.3\n".to_owned(),
                    stderr: String::new(),
                });
            }
            Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: Some(23),
                stderr: "fixture link failed".to_owned(),
            })
        }
    }

    #[test]
    fn bun_link_reports_matching_committed_override_without_mutation() {
        let consumer = repo();
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"0.1.0\"}\n",
        );
        let relative = format!(
            "../{}",
            library.path().file_name().unwrap().to_string_lossy()
        );
        let manifest = format!(
            "{{\"name\":\"consumer\",\"dependencies\":{{\"@acme/core\":\"1.2.3\"}},\"overrides\":{{\"@acme/core\":\"file:{relative}\"}}}}\n"
        );
        write(&consumer.path().join("package.json"), &manifest);
        let home = TempDir::new().unwrap();
        let process = BunInventoryProcess {
            requests: RefCell::new(Vec::new()),
        };
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Link {
                manager: DepsManager::Bun,
                library_path: library.path().to_path_buf(),
                dry_run: false,
            },
            repo_override: Some(consumer.path().to_path_buf()),
            output_json,
        };

        let text_error =
            run_deps_with_home_and_process(args(false), home.path(), &process).unwrap_err();
        let text = text_error.rendered_output().unwrap();
        assert!(text.contains("outcome: committed-pin-active"));
        assert!(text.contains("effigy deps unpin bun"));

        let json_error =
            run_deps_with_home_and_process(args(true), home.path(), &process).unwrap_err();
        let json: serde_json::Value =
            serde_json::from_str(json_error.rendered_output().unwrap()).unwrap();
        assert_eq!(json["schema"], "effigy.deps.link.v1");
        assert_eq!(json["report"]["outcome"], "committed-pin-active");
        assert!(json["report"]["plan"]["operation"]["changes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            fs::read_to_string(consumer.path().join("package.json")).unwrap(),
            manifest
        );
        assert!(!RepoLinkStateStore::for_repo(consumer.path())
            .path()
            .exists());
        assert!(process
            .requests
            .borrow()
            .iter()
            .all(|request| request.args == ["pm", "ls", "--all"]));
    }

    #[test]
    fn bun_link_reported_errors_are_non_zero_with_human_and_json_details() {
        let consumer = repo();
        write(
            &consumer.path().join("package.json"),
            "{\"name\":\"consumer\",\"dependencies\":{\"@acme/core\":\"1.2.3\"}}\n",
        );
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"0.1.0\"}\n",
        );
        let home = TempDir::new().unwrap();
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Link {
                manager: DepsManager::Bun,
                library_path: library.path().to_path_buf(),
                dry_run: false,
            },
            repo_override: Some(consumer.path().to_path_buf()),
            output_json,
        };

        let text_error =
            run_deps_with_home_and_process(args(false), home.path(), &FailingBunLinkProcess)
                .unwrap_err();
        assert!(text_error
            .to_string()
            .contains("failed (outcome: apply-failed; 1 reported error)"));
        let text = text_error.rendered_output().unwrap();
        assert!(text.contains("outcome: apply-failed"));
        assert!(text.contains("Errors (1)"));
        assert!(text.contains("fixture link failed"));

        let json_error =
            run_deps_with_home_and_process(args(true), home.path(), &FailingBunLinkProcess)
                .unwrap_err();
        let json: serde_json::Value =
            serde_json::from_str(json_error.rendered_output().unwrap()).unwrap();
        assert_eq!(json["report"]["outcome"], "apply-failed");
        assert_eq!(json["report"]["errors"].as_array().unwrap().len(), 1);
        assert!(json["report"]["errors"][0]
            .as_str()
            .unwrap()
            .contains("fixture link failed"));
    }

    #[test]
    fn dependency_mutation_outcomes_have_explicit_shell_success_contracts() {
        assert!(BunLinkOutcome::DryRun.is_success());
        assert!(BunLinkOutcome::Applied.is_success());
        assert!(!BunLinkOutcome::CommittedPinActive.is_success());
        assert!(!BunLinkOutcome::ApplyFailed.is_success());
        assert!(!BunLinkOutcome::InvariantFailed.is_success());
        assert!(!BunLinkOutcome::VerificationFailed.is_success());

        assert!(BunUnlinkOutcome::DryRun.is_success());
        assert!(BunUnlinkOutcome::Unlinked.is_success());
        assert!(BunUnlinkOutcome::NoOp.is_success());
        assert!(!BunUnlinkOutcome::ApplyFailed.is_success());
        assert!(!BunUnlinkOutcome::InvariantFailed.is_success());
        assert!(!BunUnlinkOutcome::VerificationFailed.is_success());

        assert!(CargoLinkOutcome::DryRun.is_success());
        assert!(CargoLinkOutcome::Applied.is_success());
        assert!(!CargoLinkOutcome::ApplyFailed.is_success());
        assert!(!CargoLinkOutcome::VerificationFailed.is_success());

        assert!(CargoUnlinkOutcome::DryRun.is_success());
        assert!(CargoUnlinkOutcome::Unlinked.is_success());
        assert!(CargoUnlinkOutcome::NoOp.is_success());
        assert!(!CargoUnlinkOutcome::ApplyFailed.is_success());
        assert!(!CargoUnlinkOutcome::VerificationFailed.is_success());
    }

    #[test]
    fn every_dependency_mutation_surface_promotes_failure_reports() {
        for (command, outcome) in [
            ("deps link bun", BunLinkOutcome::ApplyFailed.as_str()),
            ("deps unlink bun", BunUnlinkOutcome::ApplyFailed.as_str()),
            ("deps link cargo", CargoLinkOutcome::ApplyFailed.as_str()),
            (
                "deps unlink cargo",
                CargoUnlinkOutcome::ApplyFailed.as_str(),
            ),
        ] {
            let error = finish_deps_operation(
                command,
                outcome,
                false,
                &["fixture failure".to_owned()],
                "full report".to_owned(),
            )
            .unwrap_err();
            assert_eq!(error.rendered_output(), Some("full report"));
            assert!(error.to_string().contains(command));
        }

        let error = finish_deps_operation(
            "deps link cargo",
            CargoLinkOutcome::Applied.as_str(),
            true,
            &["cleanup failed".to_owned()],
            "full report".to_owned(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("1 reported error"));
    }

    #[test]
    fn cargo_link_human_failure_report_includes_errors() {
        let _process_lock = crate::contract_test_support::lock_test();
        let (consumer, library) = cargo_link_fixture();
        let mut report =
            execute_cargo_link(consumer.path(), library.path(), true, &StdReadOnlyProcess).unwrap();
        report.outcome = CargoLinkOutcome::VerificationFailed;
        report.errors = vec!["fixture verification failed".to_owned()];

        let rendered = render_cargo_link(consumer.path(), &report, false);
        assert!(rendered.contains("Errors (1)"));
        assert!(rendered.contains("fixture verification failed"));
    }

    #[test]
    fn bun_link_dry_run_text_and_json_expose_exact_non_mutating_intents() {
        let consumer = repo();
        write(
            &consumer.path().join("package.json"),
            "{\"name\":\"consumer\",\"dependencies\":{\"@acme/core\":\"1.2.3\"}}\n",
        );
        let library = TempDir::new().unwrap();
        write(
            &library.path().join("package.json"),
            "{\"workspaces\":[\"packages/*\"]}\n",
        );
        write(
            &library.path().join("packages/core/package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"0.1.0\"}\n",
        );
        write(
            &library.path().join("packages/protocol/package.json"),
            "{\"name\":\"@acme/protocol\",\"version\":\"0.1.0\"}\n",
        );
        let home = TempDir::new().unwrap();
        let process = BunInventoryProcess {
            requests: RefCell::new(Vec::new()),
        };
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Link {
                manager: DepsManager::Bun,
                library_path: library.path().to_path_buf(),
                dry_run: true,
            },
            repo_override: Some(consumer.path().to_path_buf()),
            output_json,
        };

        let text = run_deps_with_home_and_process(args(false), home.path(), &process).unwrap();
        let rendered = run_deps_with_home_and_process(args(true), home.path(), &process).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] link bun"));
        assert!(text.contains("outcome: dry-run"));
        assert!(text.contains("bun link --no-save"));
        assert!(text.contains("bun link @acme/core @acme/protocol --no-save"));
        assert!(text.contains("@acme/protocol: absent, missing"));
        assert_eq!(json["schema"], "effigy.deps.link.v1");
        assert_eq!(json["manager"], "bun");
        assert_eq!(json["report"]["outcome"], "dry-run");
        assert_eq!(
            json["report"]["plan"]["process_intents"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(process
            .requests
            .borrow()
            .iter()
            .all(|request| request.args == ["pm", "ls", "--all"]));
        assert!(!RepoLinkStateStore::for_repo(consumer.path())
            .path()
            .exists());
        assert!(!BunRegistrationIndexStore::for_home(home.path())
            .path()
            .exists());
    }

    #[test]
    fn cargo_link_dry_run_text_and_json_share_the_exact_non_mutating_report() {
        let _process_lock = crate::contract_test_support::lock_test();
        let (consumer, library) = cargo_link_fixture();
        let home = TempDir::new().unwrap();
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Link {
                manager: DepsManager::Cargo,
                library_path: library.path().to_path_buf(),
                dry_run: true,
            },
            repo_override: Some(consumer.path().to_path_buf()),
            output_json,
        };

        let text = run_deps_with_home(args(false), home.path()).unwrap();
        let rendered = run_deps_with_home(args(true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] link cargo"));
        assert!(text.contains("outcome: dry-run"));
        assert!(text.contains("before:\n    <absent>"));
        assert!(text.contains("do not commit while linked"));
        assert_eq!(json["schema"], "effigy.deps.link.v1");
        assert_eq!(json["report"]["outcome"], "dry-run");
        assert_eq!(json["report"]["verification"]["status"], "not-run");
        assert_eq!(
            json["report"]["plan"]["operation"]["changes"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(!consumer.path().join(".cargo/config.toml").exists());
        assert!(!consumer.path().join(".gitignore").exists());
        assert!(!RepoLinkStateStore::for_repo(consumer.path())
            .path()
            .exists());
    }

    #[test]
    fn cargo_unlink_dry_run_text_and_json_keep_the_exact_linked_state() {
        let _process_lock = crate::contract_test_support::lock_test();
        let (consumer, library) = cargo_link_fixture();
        let home = TempDir::new().unwrap();
        let linked = run_deps_with_home(
            DepsArgs {
                subcommand: DepsSubcommand::Link {
                    manager: DepsManager::Cargo,
                    library_path: library.path().to_path_buf(),
                    dry_run: false,
                },
                repo_override: Some(consumer.path().to_path_buf()),
                output_json: false,
            },
            home.path(),
        )
        .unwrap();
        assert!(linked.contains("outcome: applied"), "{linked}");
        let config_path = consumer.path().join(".cargo/config.toml");
        let state_path = RepoLinkStateStore::for_repo(consumer.path())
            .path()
            .to_path_buf();
        let config_before = fs::read_to_string(&config_path).unwrap();
        let state_before = fs::read_to_string(&state_path).unwrap();
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Unlink {
                manager: DepsManager::Cargo,
                library_path: library.path().to_path_buf(),
                dry_run: true,
            },
            repo_override: Some(consumer.path().to_path_buf()),
            output_json,
        };

        let text = run_deps_with_home(args(false), home.path()).unwrap();
        let rendered = run_deps_with_home(args(true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] unlink cargo"));
        assert!(text.contains("outcome: dry-run"));
        assert!(text.contains("Committed resolutions (1)"));
        assert_eq!(json["schema"], "effigy.deps.unlink.v1");
        assert_eq!(json["report"]["outcome"], "dry-run");
        assert_eq!(
            json["report"]["plan"]["operation"]["changes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(fs::read_to_string(config_path).unwrap(), config_before);
        assert_eq!(fs::read_to_string(state_path).unwrap(), state_before);
    }

    #[test]
    fn cargo_unlink_noop_text_and_json_share_the_successful_non_mutating_report() {
        let repo = repo();
        let home = TempDir::new().unwrap();
        run(repo.path(), "git", &["init", "-q"]);
        let args = |output_json| DepsArgs {
            subcommand: DepsSubcommand::Unlink {
                manager: DepsManager::Cargo,
                library_path: repo.path().join("missing-library"),
                dry_run: false,
            },
            repo_override: Some(repo.path().to_path_buf()),
            output_json,
        };

        let text = run_deps_with_home(args(false), home.path()).unwrap();
        let rendered = run_deps_with_home(args(true), home.path()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert!(text.contains("[deps] unlink cargo"));
        assert!(text.contains("outcome: no-op"));
        assert_eq!(json["schema"], "effigy.deps.unlink.v1");
        assert_eq!(json["report"]["outcome"], "no-op");
        assert_eq!(json["report"]["verification"]["status"], "not-run");
        assert!(!RepoLinkStateStore::for_repo(repo.path()).path().exists());
    }
}
