use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cargo_apply::{
    apply_exact_change, not_run_verification, rollback_physical_changes, validate_all_preconditions,
};
use crate::cargo_plan::{cargo_config_patches_library, plan_adopted_cargo_unlink, plan_cargo_link};
use crate::{
    inventory_cargo_consumer_roots, inventory_cargo_consumers, inventory_cargo_library,
    plan_cargo_unlink, CargoDependencyPlan, CargoExpectedResolution, CargoLibraryInventory,
    CargoLinkRollback, CargoLockfileEvidence, CargoLockfileState, CargoPackageInventory,
    CargoUnlinkOperationReport, CargoUnlinkOutcome, DependencyVerification, DepsError,
    GitCargoPlanObserver, PlannedChange, ProcessRequest, ReadOnlyProcess, RepoLinkStateStore,
    VerificationEvidence, VerificationStatus,
};

pub fn execute_cargo_unlink(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<CargoUnlinkOperationReport, DepsError> {
    let observer = GitCargoPlanObserver::new(process);
    let mut plan = plan_cargo_unlink(&repo_root, &library_path, dry_run, &observer)?;
    if plan.operation.changes.is_empty()
        && cargo_config_patches_library(
            &plan.operation.key.consumer_repo,
            &plan.operation.key.library_path,
        )?
    {
        let library = inventory_cargo_library(&plan.operation.key.library_path, process)?;
        let workspaces =
            inventory_cargo_consumers(&plan.operation.key.consumer_repo, &library, process)?;
        let adoption = plan_cargo_link(
            &plan.operation.key.consumer_repo,
            &library,
            &workspaces,
            dry_run,
            &observer,
        )?;
        plan = plan_adopted_cargo_unlink(adoption)?;
    }
    apply_cargo_unlink_plan(plan, process)
}

pub fn apply_cargo_unlink_plan(
    plan: CargoDependencyPlan,
    process: &impl ReadOnlyProcess,
) -> Result<CargoUnlinkOperationReport, DepsError> {
    if plan.operation.dry_run {
        return Ok(report(
            plan,
            CargoUnlinkOutcome::DryRun,
            Vec::new(),
            Vec::new(),
            not_run_verification(),
            Vec::new(),
            CargoLinkRollback::not_required(),
            Vec::new(),
        ));
    }
    if plan.operation.changes.is_empty() {
        return Ok(report(
            plan,
            CargoUnlinkOutcome::NoOp,
            Vec::new(),
            Vec::new(),
            not_run_verification(),
            Vec::new(),
            CargoLinkRollback::not_required(),
            Vec::new(),
        ));
    }

    validate_all_preconditions(&plan.operation.changes)?;
    let before_locks = inspect_lockfiles(
        &plan,
        &plan.lockfile_guard_packages,
        &plan.lockfile_guard_packages,
        process,
    )?;
    let unexpected_before = lock_errors(&before_locks, true);
    if !unexpected_before.is_empty() {
        return Ok(report(
            plan,
            CargoUnlinkOutcome::VerificationFailed,
            Vec::new(),
            Vec::new(),
            not_run_verification(),
            before_locks,
            CargoLinkRollback::not_required(),
            unexpected_before,
        ));
    }

    let ledger_path = RepoLinkStateStore::for_repo(&plan.operation.key.consumer_repo)
        .path()
        .to_path_buf();
    let (ledger_changes, physical_changes): (Vec<_>, Vec<_>) = plan
        .operation
        .changes
        .iter()
        .cloned()
        .partition(|change| change.target == ledger_path);
    if ledger_changes.len() > 1 {
        return Err(DepsError::invalid(
            &ledger_path,
            "Cargo unlink plan contains duplicate desired-state changes",
        ));
    }

    let mut applied = Vec::new();
    for change in &physical_changes {
        if let Err(error) = apply_exact_change(change) {
            let rollback = rollback_physical_changes(&plan, &applied);
            return Ok(report(
                plan,
                CargoUnlinkOutcome::ApplyFailed,
                applied_paths(&applied),
                Vec::new(),
                not_run_verification(),
                before_locks,
                rollback,
                vec![error.to_string()],
            ));
        }
        applied.push(change.clone());
    }

    let verification = verify_cargo_unlink(&plan, process);
    let mut after_locks = inspect_lockfiles(
        &plan,
        &plan.lockfile_guard_packages,
        &plan.remaining_linked_packages,
        process,
    )?;
    for lock in &mut after_locks {
        if let Some(before) = before_locks.iter().find(|before| before.path == lock.path) {
            lock.before_state = before.before_state;
        }
    }
    let mut errors = verification
        .evidence
        .iter()
        .filter_map(|evidence| evidence.message.clone())
        .collect::<Vec<_>>();
    errors.extend(lock_errors(&after_locks, false));
    if verification.status != VerificationStatus::Passed || !errors.is_empty() {
        return Ok(report(
            plan,
            CargoUnlinkOutcome::VerificationFailed,
            applied_paths(&applied),
            Vec::new(),
            verification,
            after_locks,
            CargoLinkRollback::not_required(),
            errors,
        ));
    }

    if let Some(ledger_change) = ledger_changes.into_iter().next() {
        if let Err(error) = apply_exact_change(&ledger_change) {
            let rollback = rollback_physical_changes(&plan, &applied);
            return Ok(report(
                plan,
                CargoUnlinkOutcome::ApplyFailed,
                applied_paths(&applied),
                Vec::new(),
                verification,
                after_locks,
                rollback,
                vec![error.to_string()],
            ));
        }
        applied.push(ledger_change);
    }

    let (removed_directories, cleanup_errors) = remove_owned_empty_directories(&plan);
    let outcome = if cleanup_errors.is_empty() {
        CargoUnlinkOutcome::Unlinked
    } else {
        CargoUnlinkOutcome::ApplyFailed
    };
    Ok(report(
        plan,
        outcome,
        applied_paths(&applied),
        removed_directories,
        verification,
        after_locks,
        CargoLinkRollback::not_required(),
        cleanup_errors,
    ))
}

#[allow(clippy::too_many_arguments)]
fn report(
    plan: CargoDependencyPlan,
    outcome: CargoUnlinkOutcome,
    applied_files: Vec<PathBuf>,
    removed_directories: Vec<PathBuf>,
    verification: DependencyVerification,
    lockfiles: Vec<CargoLockfileEvidence>,
    rollback: CargoLinkRollback,
    errors: Vec<String>,
) -> CargoUnlinkOperationReport {
    CargoUnlinkOperationReport {
        plan,
        outcome,
        applied_files,
        removed_directories,
        verification,
        lockfiles,
        rollback,
        errors,
    }
}

fn verify_cargo_unlink(
    plan: &CargoDependencyPlan,
    process: &impl ReadOnlyProcess,
) -> DependencyVerification {
    if plan.expected_resolutions.is_empty() {
        return DependencyVerification {
            status: VerificationStatus::Failed,
            evidence: vec![VerificationEvidence {
                package: "cargo-closure".to_owned(),
                consumer_root: None,
                committed_sources: Vec::new(),
                expected_source: "committed Git sources".to_owned(),
                observed_source: None,
                methods: vec!["desired-state-ledger".to_owned()],
                message: Some(
                    "desired Cargo link has no persisted workspace/source closure; unlink cannot verify exact recovery"
                        .to_owned(),
                ),
            }],
        };
    }

    let library = planned_library(&plan.expected_resolutions, &plan.operation.key.library_path);
    let consumer_roots = plan
        .expected_resolutions
        .iter()
        .map(|resolution| resolution.consumer_root.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let workspaces = match inventory_cargo_consumer_roots(
        &plan.operation.key.consumer_repo,
        &consumer_roots,
        &library,
        false,
        process,
    ) {
        Ok(workspaces) => workspaces,
        Err(error) => return failed_metadata_verification(plan, error),
    };

    let mut evidence = Vec::new();
    for expected in &plan.expected_resolutions {
        let Some(workspace) = workspaces
            .iter()
            .find(|workspace| workspace.root == expected.consumer_root)
        else {
            evidence.push(remote_evidence(
                expected,
                None,
                vec!["cargo-metadata".to_owned()],
                Some("Cargo metadata omitted the expected consumer workspace".to_owned()),
            ));
            continue;
        };
        let candidates = workspace
            .resolved_packages
            .iter()
            .filter(|candidate| candidate.name == expected.package)
            .collect::<Vec<_>>();
        let exact = candidates
            .iter()
            .copied()
            .find(|candidate| candidate.source.as_ref() == Some(&expected.committed_source));
        let observed = candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .source
                    .as_ref()
                    .map(|source| source.identity.clone())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let Some(exact) = exact else {
            evidence.push(remote_evidence(
                expected,
                (!observed.is_empty()).then_some(observed),
                vec!["cargo-metadata".to_owned()],
                Some("Cargo metadata did not resolve the exact committed Git source".to_owned()),
            ));
            continue;
        };
        if candidates.len() != 1 {
            evidence.push(remote_evidence(
                expected,
                (!observed.is_empty()).then_some(observed),
                vec!["cargo-metadata".to_owned()],
                Some("Cargo metadata resolved more than one copy of the unlinked crate".to_owned()),
            ));
            continue;
        }

        let tree = process.run(&ProcessRequest {
            program: "cargo".to_owned(),
            args: vec![
                "tree".to_owned(),
                "--manifest-path".to_owned(),
                expected
                    .consumer_root
                    .join("Cargo.toml")
                    .display()
                    .to_string(),
                "-p".to_owned(),
                exact.id.clone(),
                "--prefix".to_owned(),
                "none".to_owned(),
                "--format".to_owned(),
                "{p}".to_owned(),
            ],
            cwd: plan.operation.key.consumer_repo.clone(),
        });
        match tree {
            Ok(_) => evidence.push(remote_evidence(
                expected,
                Some(expected.committed_source.identity.clone()),
                vec!["cargo-metadata".to_owned(), "cargo-tree".to_owned()],
                None,
            )),
            Err(error) => evidence.push(remote_evidence(
                expected,
                Some(expected.committed_source.identity.clone()),
                vec!["cargo-metadata".to_owned(), "cargo-tree".to_owned()],
                Some(format!("Cargo tree verification failed: {error}")),
            )),
        }
    }
    let passed = evidence.iter().all(|item| item.message.is_none());
    DependencyVerification {
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        evidence,
    }
}

fn planned_library(
    expected: &[CargoExpectedResolution],
    library_path: &Path,
) -> CargoLibraryInventory {
    let packages = expected
        .iter()
        .map(|resolution| CargoPackageInventory {
            id: format!("planned:{}", resolution.package),
            name: resolution.package.clone(),
            manifest_path: resolution.local_path.join("Cargo.toml"),
            source: None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    CargoLibraryInventory {
        root: library_path.to_path_buf(),
        packages,
    }
}

fn failed_metadata_verification(
    plan: &CargoDependencyPlan,
    error: DepsError,
) -> DependencyVerification {
    DependencyVerification {
        status: VerificationStatus::Failed,
        evidence: plan
            .expected_resolutions
            .iter()
            .map(|expected| {
                remote_evidence(
                    expected,
                    None,
                    vec!["cargo-metadata".to_owned()],
                    Some(format!("Cargo metadata verification failed: {error}")),
                )
            })
            .collect(),
    }
}

fn remote_evidence(
    expected: &CargoExpectedResolution,
    observed_source: Option<String>,
    methods: Vec<String>,
    message: Option<String>,
) -> VerificationEvidence {
    VerificationEvidence {
        package: expected.package.clone(),
        consumer_root: Some(expected.consumer_root.clone()),
        committed_sources: vec![expected.committed_source.clone()],
        expected_source: expected.committed_source.identity.clone(),
        observed_source,
        methods,
        message,
    }
}

fn inspect_lockfiles(
    plan: &CargoDependencyPlan,
    before_allowed: &[String],
    after_allowed: &[String],
    process: &impl ReadOnlyProcess,
) -> Result<Vec<CargoLockfileEvidence>, DepsError> {
    plan.affected_lockfiles
        .iter()
        .map(|path| {
            let baseline = git_head_file(&plan.operation.key.consumer_repo, path, process)?;
            let current = fs::read_to_string(path)
                .map_err(|error| DepsError::io("read Cargo lockfile", path, error))?;
            let before_state = classify_lockfile(path, &baseline, &current, before_allowed)?;
            let after_state = classify_lockfile(path, &baseline, &current, after_allowed)?;
            let message = (after_state == CargoLockfileState::UnexpectedDrift).then(|| {
                format!(
                    "tracked lockfile `{}` differs from HEAD outside packages owned by active dependency links",
                    path.display()
                )
            });
            Ok(CargoLockfileEvidence {
                path: path.clone(),
                before_state,
                after_state,
                remaining_linked_packages: after_allowed.to_vec(),
                message,
            })
        })
        .collect()
}

pub(crate) fn validate_owned_lockfile_drift(
    plan: &CargoDependencyPlan,
    process: &impl ReadOnlyProcess,
) -> Result<(), DepsError> {
    let evidence = inspect_lockfiles(
        plan,
        &plan.lockfile_guard_packages,
        &plan.lockfile_guard_packages,
        process,
    )?;
    let errors = lock_errors(&evidence, true);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(DepsError::invalid(
            &plan.operation.key.consumer_repo,
            errors.join("; "),
        ))
    }
}

pub(crate) fn git_head_file(
    repo_root: &Path,
    path: &Path,
    process: &impl ReadOnlyProcess,
) -> Result<String, DepsError> {
    let relative = path.strip_prefix(repo_root).map_err(|_| {
        DepsError::invalid(
            path,
            "tracked Cargo.lock is outside the consumer repository",
        )
    })?;
    process
        .run(&ProcessRequest {
            program: "git".to_owned(),
            args: vec!["show".to_owned(), format!("HEAD:{}", relative.display())],
            cwd: repo_root.to_path_buf(),
        })
        .map(|output| output.stdout)
}

pub(crate) fn classify_lockfile(
    path: &Path,
    baseline: &str,
    current: &str,
    allowed_packages: &[String],
) -> Result<CargoLockfileState, DepsError> {
    if baseline == current {
        return Ok(CargoLockfileState::Clean);
    }
    let allowed = allowed_packages
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let baseline = lock_without_packages(path, baseline, &allowed)?;
    let current = lock_without_packages(path, current, &allowed)?;
    Ok(if baseline == current {
        CargoLockfileState::ActiveLinks
    } else {
        CargoLockfileState::UnexpectedDrift
    })
}

fn lock_without_packages(
    path: &Path,
    raw: &str,
    allowed: &BTreeSet<&str>,
) -> Result<toml::Value, DepsError> {
    let mut value: toml::Value = toml::from_str(raw).map_err(|error| {
        DepsError::invalid(path, format!("failed to parse Cargo.lock: {error}"))
    })?;
    if let Some(packages) = value
        .as_table_mut()
        .and_then(|table| table.get_mut("package"))
        .and_then(toml::Value::as_array_mut)
    {
        packages.retain(|package| {
            package
                .get("name")
                .and_then(toml::Value::as_str)
                .is_none_or(|name| !allowed.contains(name))
        });
    }
    if let Some(unused) = value
        .as_table_mut()
        .and_then(|table| table.get_mut("patch"))
        .and_then(toml::Value::as_table_mut)
        .and_then(|patch| patch.get_mut("unused"))
        .and_then(toml::Value::as_array_mut)
    {
        unused.retain(|package| {
            package
                .get("name")
                .and_then(toml::Value::as_str)
                .is_none_or(|name| !allowed.contains(name))
        });
    }
    let remove_patch = value
        .as_table_mut()
        .and_then(|table| table.get_mut("patch"))
        .and_then(toml::Value::as_table_mut)
        .is_some_and(|patch| {
            if patch
                .get("unused")
                .and_then(toml::Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                patch.remove("unused");
            }
            patch.is_empty()
        });
    if remove_patch {
        value
            .as_table_mut()
            .expect("Cargo.lock is a table")
            .remove("patch");
    }
    Ok(value)
}

fn lock_errors(lockfiles: &[CargoLockfileEvidence], before: bool) -> Vec<String> {
    lockfiles
        .iter()
        .filter(|lock| {
            if before {
                lock.before_state == CargoLockfileState::UnexpectedDrift
            } else {
                lock.after_state == CargoLockfileState::UnexpectedDrift
            }
        })
        .map(|lock| {
            lock.message.clone().unwrap_or_else(|| {
                format!(
                    "tracked lockfile `{}` has unexpected drift",
                    lock.path.display()
                )
            })
        })
        .collect()
}

fn applied_paths(changes: &[PlannedChange]) -> Vec<PathBuf> {
    changes.iter().map(|change| change.target.clone()).collect()
}

fn remove_owned_empty_directories(plan: &CargoDependencyPlan) -> (Vec<PathBuf>, Vec<String>) {
    let mut removed = Vec::new();
    let mut errors = Vec::new();
    for path in &plan.remove_empty_directories {
        match fs::remove_dir(path) {
            Ok(()) => removed.push(path.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => errors.push(format!(
                "owned directory `{}` became non-empty during unlink; preserved it",
                path.display()
            )),
            Err(error) => {
                errors.push(DepsError::io("remove owned empty directory", path, error).to_string())
            }
        }
    }
    (removed, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_unused_patch_entries_are_active_link_state() {
        let baseline = r#"
version = 4

[[package]]
name = "consumer"
version = "0.1.0"

[[package]]
name = "signal-core"
version = "0.1.0"
source = "git+https://example.test/signal.git#012345"
"#;
        let current = r#"
version = 4

[[package]]
name = "consumer"
version = "0.1.0"

[[package]]
name = "signal-core"
version = "0.1.0"

[[patch.unused]]
name = "signal-extra"
version = "0.1.0"
"#;
        let path = Path::new("Cargo.lock");

        assert_eq!(
            classify_lockfile(
                path,
                baseline,
                current,
                &["signal-core".to_owned(), "signal-extra".to_owned()]
            )
            .unwrap(),
            CargoLockfileState::ActiveLinks
        );
        assert_eq!(
            classify_lockfile(path, baseline, current, &["signal-core".to_owned()]).unwrap(),
            CargoLockfileState::UnexpectedDrift
        );
    }
}
