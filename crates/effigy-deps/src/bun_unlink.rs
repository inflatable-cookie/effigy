use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cargo_apply::{apply_exact_change, not_run_verification, read_optional_string};
use crate::{
    bun_registration_path, plan_bun_unlink, BunDependencyPlan, BunImmutableFileEvidence,
    BunPackagePlan, BunPathObservation, BunPlanObserver, BunProcessAction, BunProcessIntent,
    BunReferenceRelease, BunRegistrationIndex, BunRegistrationIndexStore, BunSymlinkAction,
    BunUnlinkOperationReport, BunUnlinkOutcome, BunUnlinkRollback, DependencyVerification,
    DepsError, FsBunPlanObserver, PlanAction, PlannedChange, ProcessRequest, ReadOnlyProcess,
    RepoLinkStateStore, VerificationEvidence, VerificationStatus,
};

pub fn execute_bun_unlink(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<BunUnlinkOperationReport, DepsError> {
    let observer = FsBunPlanObserver;
    let plan = plan_bun_unlink(repo_root, library_path, &home, dry_run, &observer)?;
    apply_bun_unlink_plan(plan, home, process, &observer)
}

pub fn apply_bun_unlink_plan(
    plan: BunDependencyPlan,
    home: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> Result<BunUnlinkOperationReport, DepsError> {
    if plan.operation.action != PlanAction::Unlink {
        return Err(DepsError::invalid(
            &plan.operation.key.consumer_repo,
            "Bun unlink apply requires an unlink plan",
        ));
    }
    let immutable_files = immutable_evidence(&plan);
    if plan.operation.dry_run {
        return Ok(report(
            plan,
            BunUnlinkOutcome::DryRun,
            BunUnlinkReportDetails {
                removed_consumer_links: Vec::new(),
                applied_processes: Vec::new(),
                immutable_files,
                verification: not_run_verification(),
                rollback: BunUnlinkRollback::not_required(),
                errors: Vec::new(),
            },
        ));
    }
    if plan.packages.is_empty() && plan.operation.changes.is_empty() {
        return Ok(report(
            plan,
            BunUnlinkOutcome::NoOp,
            BunUnlinkReportDetails {
                removed_consumer_links: Vec::new(),
                applied_processes: Vec::new(),
                immutable_files,
                verification: not_run_verification(),
                rollback: BunUnlinkRollback::not_required(),
                errors: Vec::new(),
            },
        ));
    }

    validate_state_preconditions(&plan)?;
    validate_immutable_preconditions(&plan)?;
    validate_process_intents(&plan)?;
    validate_symlink_intents(&plan)?;

    let home = home.as_ref();
    let index_store = BunRegistrationIndexStore::for_home(home);
    let ledger_path = RepoLinkStateStore::for_repo(&plan.operation.key.consumer_repo)
        .path()
        .to_path_buf();
    let index_path = index_store.path().to_path_buf();
    let index_change = unique_change(&plan, &index_path)?;
    let ledger_change = unique_change(&plan, &ledger_path)?;
    let index_before = state_precondition(&plan, &index_path)?;
    let planned_index = planned_index(index_change, &index_store)?;

    let mut removed_links = Vec::new();
    let mut applied_processes = Vec::new();
    let mut attempted_processes = Vec::new();
    let mut evidence = immutable_evidence(&plan);
    let mut verification = not_run_verification();
    let mut rollback = BunUnlinkRollback::not_required();
    let mut errors = Vec::new();
    let mut outcome = BunUnlinkOutcome::Unlinked;

    index_store.update_exact(index_before, |_current| {
        validate_state_preconditions(&plan)?;
        validate_physical_preconditions(&plan, observer)?;

        for intent in &plan.symlink_intents {
            if let Err(error) = remove_exact_consumer_link(intent, observer) {
                outcome = BunUnlinkOutcome::ApplyFailed;
                errors.push(error.to_string());
                rollback = rollback_bun_unlink(
                    &plan,
                    home,
                    &removed_links,
                    &attempted_processes,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
            removed_links.push(intent.path.clone());
            evidence = immutable_evidence(&plan);
            if evidence.iter().any(|item| !item.unchanged) {
                outcome = BunUnlinkOutcome::InvariantFailed;
                errors.extend(evidence.iter().filter_map(|item| item.message.clone()));
                rollback = rollback_bun_unlink(
                    &plan,
                    home,
                    &removed_links,
                    &attempted_processes,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
        }

        for intent in &plan.process_intents {
            attempted_processes.push(intent.clone());
            let request = ProcessRequest {
                program: intent.program.clone(),
                args: intent.args.clone(),
                cwd: intent.cwd.clone(),
            };
            if let Err(error) = process.run(&request) {
                outcome = BunUnlinkOutcome::ApplyFailed;
                errors.push(error.to_string());
                rollback = rollback_bun_unlink(
                    &plan,
                    home,
                    &removed_links,
                    &attempted_processes,
                    process,
                    observer,
                );
                evidence = immutable_evidence(&plan);
                return Ok((None, ()));
            }
            applied_processes.push(intent.clone());
            evidence = immutable_evidence(&plan);
            if evidence.iter().any(|item| !item.unchanged) {
                outcome = BunUnlinkOutcome::InvariantFailed;
                errors.extend(evidence.iter().filter_map(|item| item.message.clone()));
                rollback = rollback_bun_unlink(
                    &plan,
                    home,
                    &removed_links,
                    &attempted_processes,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
        }

        verification = verify_bun_unlink(&plan, home, observer);
        if verification.status != VerificationStatus::Passed {
            outcome = BunUnlinkOutcome::VerificationFailed;
            errors.extend(
                verification
                    .evidence
                    .iter()
                    .filter_map(|item| item.message.clone()),
            );
            rollback = rollback_bun_unlink(
                &plan,
                home,
                &removed_links,
                &attempted_processes,
                process,
                observer,
            );
            return Ok((None, ()));
        }

        Ok((index_change.map(|_| planned_index.clone()), ()))
    })?;

    if outcome != BunUnlinkOutcome::Unlinked {
        return Ok(report(
            plan,
            outcome,
            BunUnlinkReportDetails {
                removed_consumer_links: removed_links,
                applied_processes,
                immutable_files: evidence,
                verification,
                rollback,
                errors,
            },
        ));
    }

    if let Some(change) = ledger_change {
        if let Err(error) = apply_exact_change(change) {
            outcome = BunUnlinkOutcome::ApplyFailed;
            errors.push(error.to_string());
            if let Some(index_change) = index_change {
                if let Err(error) = index_store.replace_exact(
                    index_change.after.as_deref(),
                    index_change.before.as_deref(),
                ) {
                    errors.push(error.to_string());
                }
            }
            rollback = rollback_bun_unlink(
                &plan,
                home,
                &removed_links,
                &attempted_processes,
                process,
                observer,
            );
        }
    }

    Ok(report(
        plan,
        outcome,
        BunUnlinkReportDetails {
            removed_consumer_links: removed_links,
            applied_processes,
            immutable_files: evidence,
            verification,
            rollback,
            errors,
        },
    ))
}

fn remove_exact_consumer_link(
    intent: &crate::BunSymlinkIntent,
    observer: &impl BunPlanObserver,
) -> Result<(), DepsError> {
    if intent.action != BunSymlinkAction::RemoveConsumerLink {
        return Err(DepsError::invalid(
            &intent.path,
            "unsupported Bun unlink symlink intent",
        ));
    }
    match observer.observe_path(&intent.path)? {
        BunPathObservation::Symlink { target } if same_path(&target, &intent.expected_target) => {
            fs::remove_file(&intent.path)
                .map_err(|error| DepsError::io("remove Bun consumer link", &intent.path, error))
        }
        _ => Err(DepsError::invalid(
            &intent.path,
            format!(
                "Bun consumer link for `{}` changed after planning; no foreign path was removed",
                intent.package
            ),
        )),
    }
}

fn validate_state_preconditions(plan: &BunDependencyPlan) -> Result<(), DepsError> {
    for snapshot in &plan.state_preconditions {
        if read_optional_string(&snapshot.path)? != snapshot.contents {
            return Err(DepsError::invalid(
                &snapshot.path,
                "planned Bun state before-state is stale; no manager process was run",
            ));
        }
    }
    Ok(())
}

fn validate_immutable_preconditions(plan: &BunDependencyPlan) -> Result<(), DepsError> {
    if let Some(item) = immutable_evidence(plan).iter().find(|item| !item.unchanged) {
        return Err(DepsError::invalid(
            &item.path,
            "planned immutable package/lock snapshot is stale; no unlink mutation was run",
        ));
    }
    Ok(())
}

fn validate_process_intents(plan: &BunDependencyPlan) -> Result<(), DepsError> {
    for intent in &plan.process_intents {
        if intent.program != "bun"
            || intent.action != BunProcessAction::Unregister
            || intent.args != ["unlink", "--no-save"]
            || intent.packages.len() != 1
        {
            return Err(DepsError::invalid(
                &intent.cwd,
                format!(
                    "unsafe Bun unregister intent for `{}`; unlink requires exact bun unlink --no-save",
                    intent.packages.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn validate_symlink_intents(plan: &BunDependencyPlan) -> Result<(), DepsError> {
    for intent in &plan.symlink_intents {
        let package = planned_package(plan, &intent.package)?;
        let expected_path = plan
            .operation
            .key
            .consumer_repo
            .join("node_modules")
            .join(&intent.package);
        if intent.action != BunSymlinkAction::RemoveConsumerLink
            || intent.path != expected_path
            || !same_path(&intent.expected_target, &package.local_path)
        {
            return Err(DepsError::invalid(
                &intent.path,
                "unsafe Bun consumer unlink intent",
            ));
        }
    }
    Ok(())
}

fn validate_physical_preconditions(
    plan: &BunDependencyPlan,
    observer: &impl BunPlanObserver,
) -> Result<(), DepsError> {
    for precondition in &plan.physical_preconditions {
        if observer.observe_path(&precondition.path)? != precondition.observation {
            return Err(DepsError::invalid(
                &precondition.path,
                "planned Bun physical before-state is stale; no unlink mutation was run",
            ));
        }
    }
    Ok(())
}

fn verify_bun_unlink(
    plan: &BunDependencyPlan,
    home: &Path,
    observer: &impl BunPlanObserver,
) -> DependencyVerification {
    let mut evidence = Vec::new();
    for package in &plan.packages {
        let consumer_path = plan
            .operation
            .key
            .consumer_repo
            .join("node_modules")
            .join(&package.name);
        let consumer_expected = if plan
            .symlink_intents
            .iter()
            .any(|intent| intent.package == package.name)
        {
            BunPathObservation::Missing
        } else {
            planned_observation(plan, &consumer_path).unwrap_or(BunPathObservation::Missing)
        };
        evidence.push(observation_evidence(
            package,
            &consumer_path,
            "bun-consumer-unlink",
            &consumer_expected,
            observer,
        ));

        let registration_path = bun_registration_path(home, &package.name);
        let registration_expected =
            if package.reference_release == Some(BunReferenceRelease::RemoveOwned) {
                BunPathObservation::Missing
            } else {
                planned_observation(plan, &registration_path).unwrap_or(BunPathObservation::Missing)
            };
        evidence.push(observation_evidence(
            package,
            &registration_path,
            "bun-registration-release",
            &registration_expected,
            observer,
        ));
    }
    let passed = !evidence.is_empty() && evidence.iter().all(|item| item.message.is_none());
    DependencyVerification {
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        evidence,
    }
}

fn observation_evidence(
    package: &BunPackagePlan,
    path: &Path,
    method: &str,
    expected: &BunPathObservation,
    observer: &impl BunPlanObserver,
) -> VerificationEvidence {
    match observer.observe_path(path) {
        Ok(observed) => VerificationEvidence {
            package: package.name.clone(),
            consumer_root: Some(path.to_path_buf()),
            committed_sources: Vec::new(),
            expected_source: render_observation(expected),
            observed_source: Some(render_observation(&observed)),
            methods: vec![method.to_owned()],
            message: (observed != *expected).then(|| {
                format!(
                    "Bun unlink verification for `{}` at `{}` did not match the exact planned state",
                    package.name,
                    path.display()
                )
            }),
        },
        Err(error) => VerificationEvidence {
            package: package.name.clone(),
            consumer_root: Some(path.to_path_buf()),
            committed_sources: Vec::new(),
            expected_source: render_observation(expected),
            observed_source: None,
            methods: vec![method.to_owned()],
            message: Some(error.to_string()),
        },
    }
}

fn rollback_bun_unlink(
    plan: &BunDependencyPlan,
    home: &Path,
    removed_links: &[PathBuf],
    attempted_processes: &[BunProcessIntent],
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> BunUnlinkRollback {
    let mut rollback = BunUnlinkRollback {
        attempted: !removed_links.is_empty() || !attempted_processes.is_empty(),
        relinked_consumer_packages: Vec::new(),
        restored_registrations: Vec::new(),
        restored_files: Vec::new(),
        failures: Vec::new(),
    };

    for intent in attempted_processes.iter().rev() {
        let Some(package_name) = intent.packages.first() else {
            continue;
        };
        let package = match planned_package(plan, package_name) {
            Ok(package) => package,
            Err(error) => {
                rollback.failures.push(error.to_string());
                continue;
            }
        };
        let registration_path = bun_registration_path(home, package_name);
        match observer.observe_path(&registration_path) {
            Ok(BunPathObservation::Missing) => {
                let request = ProcessRequest {
                    program: "bun".to_owned(),
                    args: vec!["link".to_owned(), "--no-save".to_owned()],
                    cwd: package.local_path.clone(),
                };
                match process.run(&request) {
                    Ok(_) => rollback.restored_registrations.push(package_name.clone()),
                    Err(error) => rollback.failures.push(error.to_string()),
                }
            }
            Ok(BunPathObservation::Symlink { target })
                if same_path(&target, &package.local_path) => {}
            Ok(_) => rollback.failures.push(format!(
                "rollback left Bun registration `{package_name}` untouched because its target no longer matches"
            )),
            Err(error) => rollback.failures.push(error.to_string()),
        }
    }

    let removed_packages = plan
        .symlink_intents
        .iter()
        .filter(|intent| removed_links.contains(&intent.path))
        .map(|intent| intent.package.clone())
        .collect::<Vec<_>>();
    if !removed_packages.is_empty() {
        let mut args = vec!["link".to_owned()];
        args.extend(removed_packages.iter().cloned());
        args.push("--no-save".to_owned());
        let request = ProcessRequest {
            program: "bun".to_owned(),
            args,
            cwd: plan.operation.key.consumer_repo.clone(),
        };
        match process.run(&request) {
            Ok(_) => {
                for package_name in &removed_packages {
                    let path = plan
                        .operation
                        .key
                        .consumer_repo
                        .join("node_modules")
                        .join(package_name);
                    let package = match planned_package(plan, package_name) {
                        Ok(package) => package,
                        Err(error) => {
                            rollback.failures.push(error.to_string());
                            continue;
                        }
                    };
                    match observer.observe_path(&path) {
                        Ok(BunPathObservation::Symlink { target })
                            if same_path(&target, &package.local_path) =>
                        {
                            rollback
                                .relinked_consumer_packages
                                .push(package_name.clone());
                        }
                        Ok(_) => rollback.failures.push(format!(
                            "rollback did not restore Bun consumer link `{package_name}`"
                        )),
                        Err(error) => rollback.failures.push(error.to_string()),
                    }
                }
            }
            Err(error) => rollback.failures.push(error.to_string()),
        }
    }
    rollback
}

fn immutable_evidence(plan: &BunDependencyPlan) -> Vec<BunImmutableFileEvidence> {
    plan.immutable_files
        .iter()
        .map(|snapshot| match read_optional_bytes(&snapshot.path) {
            Ok(current) if current == snapshot.contents => BunImmutableFileEvidence {
                path: snapshot.path.clone(),
                unchanged: true,
                message: None,
            },
            Ok(_) => BunImmutableFileEvidence {
                path: snapshot.path.clone(),
                unchanged: false,
                message: Some(format!(
                    "Bun mutated immutable package/lock file `{}`",
                    snapshot.path.display()
                )),
            },
            Err(error) => BunImmutableFileEvidence {
                path: snapshot.path.clone(),
                unchanged: false,
                message: Some(error.to_string()),
            },
        })
        .collect()
}

fn planned_observation(plan: &BunDependencyPlan, path: &Path) -> Option<BunPathObservation> {
    plan.physical_preconditions
        .iter()
        .find(|item| item.path == path)
        .map(|item| item.observation.clone())
}

fn planned_package<'a>(
    plan: &'a BunDependencyPlan,
    package_name: &str,
) -> Result<&'a BunPackagePlan, DepsError> {
    plan.packages
        .iter()
        .find(|package| package.name == package_name)
        .ok_or_else(|| {
            DepsError::invalid(
                &plan.operation.key.consumer_repo,
                format!("Bun unlink intent names unplanned package `{package_name}`"),
            )
        })
}

fn unique_change<'a>(
    plan: &'a BunDependencyPlan,
    target: &Path,
) -> Result<Option<&'a PlannedChange>, DepsError> {
    let changes = plan
        .operation
        .changes
        .iter()
        .filter(|change| change.target == target)
        .collect::<Vec<_>>();
    if changes.len() > 1 {
        return Err(DepsError::invalid(
            target,
            "Bun unlink plan contains duplicate state changes",
        ));
    }
    Ok(changes.into_iter().next())
}

fn state_precondition<'a>(
    plan: &'a BunDependencyPlan,
    target: &Path,
) -> Result<Option<&'a str>, DepsError> {
    plan.state_preconditions
        .iter()
        .find(|snapshot| snapshot.path == target)
        .map(|snapshot| snapshot.contents.as_deref())
        .ok_or_else(|| {
            DepsError::invalid(
                target,
                "Bun unlink plan is missing an exact state precondition",
            )
        })
}

fn planned_index(
    change: Option<&PlannedChange>,
    store: &BunRegistrationIndexStore,
) -> Result<BunRegistrationIndex, DepsError> {
    let Some(change) = change else {
        return store.read();
    };
    let after = change.after.as_deref().ok_or_else(|| {
        DepsError::invalid(
            &change.target,
            "Bun unlink cannot delete the registration index",
        )
    })?;
    serde_json::from_str(after)
        .map_err(|error| DepsError::json("parse planned", &change.target, error))
}

fn render_observation(observation: &BunPathObservation) -> String {
    match observation {
        BunPathObservation::Missing => "missing".to_owned(),
        BunPathObservation::NonSymlink => "registry/non-symlink".to_owned(),
        BunPathObservation::Symlink { target } => target.display().to_string(),
    }
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, DepsError> {
    match fs::read(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    canonical_or_original(left) == canonical_or_original(right)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

struct BunUnlinkReportDetails {
    removed_consumer_links: Vec<PathBuf>,
    applied_processes: Vec<BunProcessIntent>,
    immutable_files: Vec<BunImmutableFileEvidence>,
    verification: DependencyVerification,
    rollback: BunUnlinkRollback,
    errors: Vec<String>,
}

fn report(
    plan: BunDependencyPlan,
    outcome: BunUnlinkOutcome,
    details: BunUnlinkReportDetails,
) -> BunUnlinkOperationReport {
    let BunUnlinkReportDetails {
        removed_consumer_links,
        applied_processes,
        immutable_files,
        verification,
        rollback,
        errors,
    } = details;
    BunUnlinkOperationReport {
        plan,
        outcome,
        removed_consumer_links,
        applied_processes,
        immutable_files,
        verification,
        rollback,
        errors,
    }
}

#[cfg(test)]
mod tests;
