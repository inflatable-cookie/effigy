use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cargo_apply::{apply_exact_change, not_run_verification, read_optional_string};
use crate::{
    bun_registration_path, inspect_bun_peer_resolutions, inventory_bun_consumer,
    inventory_bun_library, plan_bun_link, BunConsumerLinkDisposition, BunDependencyPlan,
    BunImmutableFileEvidence, BunLinkOperationReport, BunLinkOutcome, BunLinkRollback,
    BunPackagePlan, BunPathObservation, BunPeerDiagnostic, BunPeerResolutionStatus,
    BunPlanObserver, BunProcessAction, BunProcessIntent, BunRegistrationIndex,
    BunRegistrationIndexStore, DependencyVerification, DepsError, FsBunPlanObserver, PlannedChange,
    ProcessRequest, ReadOnlyProcess, RepoLinkStateStore, VerificationEvidence, VerificationStatus,
};

pub fn execute_bun_link(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<BunLinkOperationReport, DepsError> {
    let library_path = library_path.as_ref();
    let packages = inventory_bun_library(library_path)?;
    let consumer = inventory_bun_consumer(&repo_root, &packages, process)?;
    let observer = FsBunPlanObserver;
    let plan = plan_bun_link(
        repo_root,
        library_path,
        &packages,
        &consumer,
        &home,
        dry_run,
        &observer,
    )?;
    apply_bun_link_plan(plan, home, process, &observer)
}

pub fn apply_bun_link_plan(
    plan: BunDependencyPlan,
    home: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> Result<BunLinkOperationReport, DepsError> {
    if plan.operation.dry_run {
        let peer_diagnostics = peer_diagnostics(&plan)?;
        return Ok(BunLinkOperationReport {
            immutable_files: immutable_evidence(&plan),
            plan,
            outcome: BunLinkOutcome::DryRun,
            applied_processes: Vec::new(),
            verification: not_run_verification(),
            peer_diagnostics,
            rollback: BunLinkRollback::not_required(),
            errors: Vec::new(),
        });
    }
    if plan.desired.is_none() {
        return Err(DepsError::invalid(
            &plan.operation.key.consumer_repo,
            "Bun link apply requires desired link state",
        ));
    }
    validate_state_preconditions(&plan)?;
    validate_immutable_preconditions(&plan)?;
    validate_process_intents(&plan)?;

    let home = home.as_ref();
    let index_store = BunRegistrationIndexStore::for_home(home);
    let ledger_path = RepoLinkStateStore::for_repo(&plan.operation.key.consumer_repo)
        .path()
        .to_path_buf();
    let index_path = index_store.path().to_path_buf();
    let index_change = unique_change(&plan, &index_path)?;
    let ledger_change = unique_change(&plan, &ledger_path)?;
    let physical_changes = plan
        .operation
        .changes
        .iter()
        .filter(|change| change.target != index_path && change.target != ledger_path)
        .cloned()
        .collect::<Vec<_>>();
    let index_before = state_precondition(&plan, &index_path)?;
    let planned_index = planned_index(index_change, &index_store)?;

    let mut applied_processes = Vec::new();
    let mut attempted_processes = Vec::new();
    let mut applied_files = Vec::new();
    let mut backups = Vec::new();
    let mut verification = not_run_verification();
    let mut peer_diagnostics = Vec::new();
    let mut evidence = immutable_evidence(&plan);
    let mut rollback = BunLinkRollback::not_required();
    let mut errors = Vec::new();
    let mut outcome = BunLinkOutcome::Applied;

    index_store.update_exact(index_before, |_current| {
        validate_physical_preconditions(&plan, observer)?;
        for change in &physical_changes {
            if let Err(error) = apply_exact_change(change) {
                outcome = BunLinkOutcome::ApplyFailed;
                errors.push(error.to_string());
                rollback = rollback_bun_link(
                    &plan,
                    home,
                    &attempted_processes,
                    &backups,
                    &applied_files,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
            applied_files.push(change.clone());
        }

        for intent in &plan.process_intents {
            if intent.action == BunProcessAction::LinkConsumer {
                for package_name in &intent.packages {
                    match backup_consumer_package(&plan, package_name, backups.len()) {
                        Ok(backup) => backups.push(backup),
                        Err(error) => {
                            outcome = BunLinkOutcome::ApplyFailed;
                            errors.push(error.to_string());
                            rollback = rollback_bun_link(
                                &plan,
                                home,
                                &attempted_processes,
                                &backups,
                                &applied_files,
                                process,
                                observer,
                            );
                            return Ok((None, ()));
                        }
                    }
                }
            }
            attempted_processes.push(intent.clone());
            let request = ProcessRequest {
                program: intent.program.clone(),
                args: intent.args.clone(),
                cwd: intent.cwd.clone(),
            };
            if let Err(error) = process.run(&request) {
                outcome = BunLinkOutcome::ApplyFailed;
                errors.push(error.to_string());
                rollback = rollback_bun_link(
                    &plan,
                    home,
                    &attempted_processes,
                    &backups,
                    &applied_files,
                    process,
                    observer,
                );
                evidence = immutable_evidence(&plan);
                return Ok((None, ()));
            }
            applied_processes.push(intent.clone());
            evidence = immutable_evidence(&plan);
            if evidence.iter().any(|item| !item.unchanged) {
                outcome = BunLinkOutcome::InvariantFailed;
                errors.extend(evidence.iter().filter_map(|item| item.message.clone()));
                rollback = rollback_bun_link(
                    &plan,
                    home,
                    &attempted_processes,
                    &backups,
                    &applied_files,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
        }

        verification = verify_bun_link(&plan, observer);
        if verification.status != VerificationStatus::Passed {
            outcome = BunLinkOutcome::VerificationFailed;
            errors.extend(
                verification
                    .evidence
                    .iter()
                    .filter_map(|item| item.message.clone()),
            );
            rollback = rollback_bun_link(
                &plan,
                home,
                &attempted_processes,
                &backups,
                &applied_files,
                process,
                observer,
            );
            return Ok((None, ()));
        }

        match peer_diagnostics_for_verified_link(&plan) {
            Ok(diagnostics) => {
                peer_diagnostics = diagnostics;
                let duplicates = peer_diagnostics
                    .iter()
                    .filter(|item| item.status == BunPeerResolutionStatus::Duplicate)
                    .cloned()
                    .collect::<Vec<_>>();
                if !duplicates.is_empty() {
                    outcome = BunLinkOutcome::VerificationFailed;
                    verification.status = VerificationStatus::Failed;
                    for duplicate in &duplicates {
                        if let Some(message) = &duplicate.message {
                            errors.push(message.clone());
                        }
                        verification.evidence.push(peer_verification(duplicate));
                    }
                    rollback = rollback_bun_link(
                        &plan,
                        home,
                        &attempted_processes,
                        &backups,
                        &applied_files,
                        process,
                        observer,
                    );
                    return Ok((None, ()));
                }
            }
            Err(error) => {
                outcome = BunLinkOutcome::VerificationFailed;
                verification.status = VerificationStatus::Failed;
                errors.push(error.to_string());
                rollback = rollback_bun_link(
                    &plan,
                    home,
                    &attempted_processes,
                    &backups,
                    &applied_files,
                    process,
                    observer,
                );
                return Ok((None, ()));
            }
        }

        let next = index_change.map(|_| planned_index.clone());
        Ok((next, ()))
    })?;

    if outcome != BunLinkOutcome::Applied {
        return Ok(BunLinkOperationReport {
            plan,
            outcome,
            applied_processes,
            immutable_files: evidence,
            verification,
            peer_diagnostics,
            rollback,
            errors,
        });
    }

    if let Some(change) = ledger_change {
        if let Err(error) = apply_exact_change(change) {
            errors.push(error.to_string());
            outcome = BunLinkOutcome::ApplyFailed;
            if let Some(index_change) = index_change {
                if let Err(error) = index_store.replace_exact(
                    index_change.after.as_deref(),
                    index_change.before.as_deref(),
                ) {
                    errors.push(error.to_string());
                }
            }
            rollback = rollback_bun_link(
                &plan,
                home,
                &attempted_processes,
                &backups,
                &applied_files,
                process,
                observer,
            );
        }
    }

    if outcome == BunLinkOutcome::Applied {
        cleanup_backups(&backups, &mut errors);
    }
    Ok(BunLinkOperationReport {
        plan,
        outcome,
        applied_processes,
        immutable_files: evidence,
        verification,
        peer_diagnostics,
        rollback,
        errors,
    })
}

#[derive(Debug, Clone)]
struct ConsumerBackup {
    original: PathBuf,
    backup: Option<PathBuf>,
    expected_target: PathBuf,
}

fn backup_consumer_package(
    plan: &BunDependencyPlan,
    package_name: &str,
    index: usize,
) -> Result<ConsumerBackup, DepsError> {
    let package = planned_package(plan, package_name)?;
    let original = plan
        .operation
        .key
        .consumer_repo
        .join("node_modules")
        .join(package_name);
    let backup = if package.consumer_link == BunConsumerLinkDisposition::Registry {
        let backup = plan
            .operation
            .key
            .consumer_repo
            .join(".effigy/local/bun-link-backups")
            .join(std::process::id().to_string())
            .join(index.to_string());
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                DepsError::io("create Bun link backup directory", parent, error)
            })?;
        }
        fs::rename(&original, &backup)
            .map_err(|error| DepsError::io("backup Bun consumer package", &original, error))?;
        Some(backup)
    } else {
        None
    };
    Ok(ConsumerBackup {
        original,
        backup,
        expected_target: package.local_path.clone(),
    })
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
    let evidence = immutable_evidence(plan);
    if let Some(item) = evidence.iter().find(|item| !item.unchanged) {
        return Err(DepsError::invalid(
            &item.path,
            "planned immutable package/lock snapshot is stale; no manager process was run",
        ));
    }
    Ok(())
}

fn validate_process_intents(plan: &BunDependencyPlan) -> Result<(), DepsError> {
    for intent in &plan.process_intents {
        let valid_action = matches!(
            intent.action,
            BunProcessAction::Register | BunProcessAction::LinkConsumer
        );
        if intent.program != "bun"
            || !valid_action
            || !intent.args.iter().any(|arg| arg == "--no-save")
            || intent.args.iter().any(|arg| arg == "--save")
        {
            return Err(DepsError::invalid(
                &intent.cwd,
                format!(
                    "unsafe Bun process intent for `{}`; link apply requires explicit --no-save",
                    intent.packages.join(", ")
                ),
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
                "planned Bun physical before-state is stale; no manager process was run",
            ));
        }
    }
    Ok(())
}

fn peer_diagnostics(plan: &BunDependencyPlan) -> Result<Vec<BunPeerDiagnostic>, DepsError> {
    let Some(desired) = &plan.desired else {
        return Ok(Vec::new());
    };
    inspect_bun_peer_resolutions(&plan.operation.key.consumer_repo, &desired.packages)
}

fn peer_diagnostics_for_verified_link(
    plan: &BunDependencyPlan,
) -> Result<Vec<BunPeerDiagnostic>, DepsError> {
    peer_diagnostics(plan)
}

fn peer_verification(diagnostic: &BunPeerDiagnostic) -> VerificationEvidence {
    VerificationEvidence {
        package: diagnostic.package.clone(),
        consumer_root: None,
        committed_sources: Vec::new(),
        expected_source: "one shared peer path".to_owned(),
        observed_source: Some(
            [
                diagnostic.consumer_resolution.as_ref(),
                diagnostic.local_resolution.as_ref(),
            ]
            .into_iter()
            .flatten()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" | "),
        ),
        methods: vec!["bun-peer-resolution".to_owned()],
        message: diagnostic.message.clone(),
    }
}

fn verify_bun_link(
    plan: &BunDependencyPlan,
    observer: &impl BunPlanObserver,
) -> DependencyVerification {
    let evidence = plan
        .packages
        .iter()
        .map(|package| {
            let link_path = plan
                .operation
                .key
                .consumer_repo
                .join("node_modules")
                .join(&package.name);
            match observer.observe_path(&link_path) {
                Ok(BunPathObservation::Symlink { target })
                    if same_path(&target, &package.local_path) =>
                {
                    VerificationEvidence {
                        package: package.name.clone(),
                        consumer_root: Some(plan.operation.key.consumer_repo.clone()),
                        committed_sources: plan
                            .desired
                            .as_ref()
                            .and_then(|desired| {
                                desired
                                    .packages
                                    .iter()
                                    .find(|candidate| candidate.name == package.name)
                            })
                            .map(|package| package.committed_sources.clone())
                            .unwrap_or_default(),
                        expected_source: package.local_path.display().to_string(),
                        observed_source: Some(target.display().to_string()),
                        methods: vec!["bun-symlink".to_owned()],
                        message: None,
                    }
                }
                Ok(observed) => VerificationEvidence {
                    package: package.name.clone(),
                    consumer_root: Some(plan.operation.key.consumer_repo.clone()),
                    committed_sources: Vec::new(),
                    expected_source: package.local_path.display().to_string(),
                    observed_source: observed_target(&observed),
                    methods: vec!["bun-symlink".to_owned()],
                    message: Some(
                        "Bun consumer link does not resolve to the planned canonical local path"
                            .to_owned(),
                    ),
                },
                Err(error) => VerificationEvidence {
                    package: package.name.clone(),
                    consumer_root: Some(plan.operation.key.consumer_repo.clone()),
                    committed_sources: Vec::new(),
                    expected_source: package.local_path.display().to_string(),
                    observed_source: None,
                    methods: vec!["bun-symlink".to_owned()],
                    message: Some(format!("Bun symlink verification failed: {error}")),
                },
            }
        })
        .collect::<Vec<_>>();
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

fn rollback_bun_link(
    plan: &BunDependencyPlan,
    home: &Path,
    attempted_processes: &[BunProcessIntent],
    backups: &[ConsumerBackup],
    applied_files: &[PlannedChange],
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> BunLinkRollback {
    let mut report = BunLinkRollback {
        attempted: !attempted_processes.is_empty()
            || !backups.is_empty()
            || !applied_files.is_empty(),
        restored_consumer_links: Vec::new(),
        removed_registrations: Vec::new(),
        restored_files: Vec::new(),
        failures: Vec::new(),
    };
    for backup in backups.iter().rev() {
        match observer.observe_path(&backup.original) {
            Ok(BunPathObservation::Symlink { target })
                if same_path(&target, &backup.expected_target) =>
            {
                if let Err(error) = fs::remove_file(&backup.original) {
                    report.failures.push(
                        DepsError::io(
                            "remove rolled-back Bun consumer link",
                            &backup.original,
                            error,
                        )
                        .to_string(),
                    );
                    continue;
                }
            }
            Ok(BunPathObservation::Missing) => {}
            Ok(_) => {
                report.failures.push(format!(
                    "rollback left `{}` untouched because it no longer matches the planned Bun link",
                    backup.original.display()
                ));
                continue;
            }
            Err(error) => {
                report.failures.push(error.to_string());
                continue;
            }
        }
        if let Some(saved) = &backup.backup {
            if let Err(error) = fs::rename(saved, &backup.original) {
                report.failures.push(
                    DepsError::io("restore Bun consumer package", &backup.original, error)
                        .to_string(),
                );
                continue;
            }
        }
        report.restored_consumer_links.push(backup.original.clone());
    }
    for intent in attempted_processes.iter().rev() {
        if intent.action != BunProcessAction::Register {
            continue;
        }
        for package_name in &intent.packages {
            let registration_path = bun_registration_path(home, package_name);
            let package = match planned_package(plan, package_name) {
                Ok(package) => package,
                Err(error) => {
                    report.failures.push(error.to_string());
                    continue;
                }
            };
            match observer.observe_path(&registration_path) {
                Ok(BunPathObservation::Symlink { target })
                    if same_path(&target, &package.local_path) =>
                {
                    let request = ProcessRequest {
                        program: "bun".to_owned(),
                        args: vec!["unlink".to_owned(), "--no-save".to_owned()],
                        cwd: package.local_path.clone(),
                    };
                    match process.run(&request) {
                        Ok(_) => report.removed_registrations.push(package_name.clone()),
                        Err(error) => report.failures.push(error.to_string()),
                    }
                }
                Ok(BunPathObservation::Missing) => {}
                Ok(_) => report.failures.push(format!(
                    "rollback left Bun registration `{package_name}` untouched because its target no longer matches"
                )),
                Err(error) => report.failures.push(error.to_string()),
            }
        }
    }
    for change in applied_files.iter().rev() {
        let reverse = PlannedChange {
            target: change.target.clone(),
            action: change.action,
            description: format!("rollback: {}", change.description),
            before: change.after.clone(),
            after: change.before.clone(),
        };
        match apply_exact_change(&reverse) {
            Ok(()) => report.restored_files.push(change.target.clone()),
            Err(error) => report.failures.push(error.to_string()),
        }
    }
    cleanup_backups(backups, &mut report.failures);
    report
}

fn immutable_evidence(plan: &BunDependencyPlan) -> Vec<BunImmutableFileEvidence> {
    plan.immutable_files
        .iter()
        .map(|snapshot| {
            let current = read_optional_bytes(&snapshot.path);
            match current {
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
            }
        })
        .collect()
}

fn cleanup_backups(backups: &[ConsumerBackup], errors: &mut Vec<String>) {
    for backup in backups {
        let Some(saved) = &backup.backup else {
            continue;
        };
        if !saved.exists() {
            continue;
        }
        let result = if saved.is_dir() {
            fs::remove_dir_all(saved)
        } else {
            fs::remove_file(saved)
        };
        if let Err(error) = result {
            errors.push(DepsError::io("remove Bun link backup", saved, error).to_string());
        }
    }
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
            "Bun link plan contains duplicate state changes",
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
                "Bun link plan is missing an exact state precondition",
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
            "Bun link cannot delete the registration index",
        )
    })?;
    serde_json::from_str(after)
        .map_err(|error| DepsError::json("parse planned", &change.target, error))
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
                format!("Bun process intent names unplanned package `{package_name}`"),
            )
        })
}

fn observed_target(observed: &BunPathObservation) -> Option<String> {
    match observed {
        BunPathObservation::Missing => None,
        BunPathObservation::NonSymlink => Some("registry/non-symlink".to_owned()),
        BunPathObservation::Symlink { target } => Some(target.display().to_string()),
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

#[cfg(test)]
mod tests;
