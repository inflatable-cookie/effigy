use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cargo_unlink::validate_owned_lockfile_drift;
use crate::state::write_atomic;
use crate::{
    inventory_cargo_consumer_roots, inventory_cargo_consumers, inventory_cargo_library,
    plan_cargo_link, CargoDependencyPlan, CargoLibraryInventory, CargoLinkOperationReport,
    CargoLinkOutcome, CargoLinkRollback, CargoPackageInventory, CommittedSourceKind,
    DependencyVerification, DepsError, GitCargoPlanObserver, PlannedChange, ProcessRequest,
    ReadOnlyProcess, RepoLinkStateStore, VerificationEvidence, VerificationStatus,
};

pub fn execute_cargo_link(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<CargoLinkOperationReport, DepsError> {
    let library = inventory_cargo_library(library_path, process)?;
    let workspaces = inventory_cargo_consumers(&repo_root, &library, process)?;
    let observer = GitCargoPlanObserver::new(process);
    let plan = plan_cargo_link(repo_root, &library, &workspaces, dry_run, &observer)?;
    apply_cargo_link_plan(plan, process)
}

pub fn apply_cargo_link_plan(
    plan: CargoDependencyPlan,
    process: &impl ReadOnlyProcess,
) -> Result<CargoLinkOperationReport, DepsError> {
    if plan.operation.dry_run {
        return Ok(CargoLinkOperationReport {
            plan,
            outcome: CargoLinkOutcome::DryRun,
            applied_files: Vec::new(),
            verification: not_run_verification(),
            rollback: CargoLinkRollback::not_required(),
            errors: Vec::new(),
        });
    }
    if plan.desired.is_none() {
        return Err(DepsError::invalid(
            &plan.operation.key.consumer_repo,
            "Cargo link apply requires desired link state",
        ));
    }

    validate_all_preconditions(&plan.operation.changes)?;
    if !plan.lockfile_guard_packages.is_empty() && !plan.affected_lockfiles.is_empty() {
        validate_owned_lockfile_drift(&plan, process)?;
    }
    let ledger_path = RepoLinkStateStore::for_checkout(&plan.operation.key.consumer_repo)
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
            "Cargo link plan contains duplicate desired-state changes",
        ));
    }

    let mut applied = Vec::new();
    for change in &physical_changes {
        if let Err(error) = apply_exact_change(change) {
            let rollback = rollback_physical_changes(&plan, &applied);
            return Ok(CargoLinkOperationReport {
                plan,
                outcome: CargoLinkOutcome::ApplyFailed,
                applied_files: applied
                    .iter()
                    .map(|change: &PlannedChange| change.target.clone())
                    .collect(),
                verification: not_run_verification(),
                rollback,
                errors: vec![error.to_string()],
            });
        }
        applied.push(change.clone());
    }

    let verification = verify_cargo_link(&plan, process);
    if verification.status != VerificationStatus::Passed {
        let errors = verification
            .evidence
            .iter()
            .filter_map(|evidence| evidence.message.clone())
            .collect();
        let rollback = rollback_physical_changes(&plan, &applied);
        return Ok(CargoLinkOperationReport {
            plan,
            outcome: CargoLinkOutcome::VerificationFailed,
            applied_files: applied.iter().map(|change| change.target.clone()).collect(),
            verification,
            rollback,
            errors,
        });
    }

    for change in ledger_changes {
        if let Err(error) = apply_exact_change(&change) {
            let rollback = rollback_physical_changes(&plan, &applied);
            return Ok(CargoLinkOperationReport {
                plan,
                outcome: CargoLinkOutcome::ApplyFailed,
                applied_files: applied.iter().map(|change| change.target.clone()).collect(),
                verification,
                rollback,
                errors: vec![error.to_string()],
            });
        }
        applied.push(change);
    }

    Ok(CargoLinkOperationReport {
        plan,
        outcome: CargoLinkOutcome::Applied,
        applied_files: applied.iter().map(|change| change.target.clone()).collect(),
        verification,
        rollback: CargoLinkRollback::not_required(),
        errors: Vec::new(),
    })
}

fn verify_cargo_link(
    plan: &CargoDependencyPlan,
    process: &impl ReadOnlyProcess,
) -> DependencyVerification {
    let library = planned_library(plan);
    let consumer_roots = plan
        .desired
        .as_ref()
        .expect("validated desired Cargo link")
        .consumer_roots
        .iter()
        .map(|root| root.canonical_path.clone())
        .collect::<Vec<_>>();
    let workspaces = match inventory_cargo_consumer_roots(
        &plan.operation.key.consumer_repo,
        &consumer_roots,
        &library,
        false,
        process,
    ) {
        Ok(workspaces) => workspaces,
        Err(error) => {
            return DependencyVerification {
                status: VerificationStatus::Failed,
                evidence: plan
                    .expected_resolutions
                    .iter()
                    .map(|expected| VerificationEvidence {
                        package: expected.package.clone(),
                        consumer_root: Some(expected.consumer_root.clone()),
                        committed_sources: vec![expected.committed_source.clone()],
                        expected_source: expected.local_path.display().to_string(),
                        observed_source: None,
                        methods: vec!["cargo-metadata".to_owned()],
                        message: Some(format!("Cargo metadata verification failed: {error}")),
                    })
                    .collect(),
            };
        }
    };

    let mut evidence = Vec::new();
    for expected in &plan.expected_resolutions {
        let Some(workspace) = workspaces
            .iter()
            .find(|workspace| workspace.root == expected.consumer_root)
        else {
            evidence.push(VerificationEvidence {
                package: expected.package.clone(),
                consumer_root: Some(expected.consumer_root.clone()),
                committed_sources: vec![expected.committed_source.clone()],
                expected_source: expected.local_path.display().to_string(),
                observed_source: None,
                methods: vec!["cargo-metadata".to_owned()],
                message: Some("Cargo metadata omitted the expected consumer workspace".to_owned()),
            });
            continue;
        };

        let candidates = workspace
            .resolved_packages
            .iter()
            .filter(|candidate| candidate.name == expected.package)
            .collect::<Vec<_>>();
        let local = candidates.iter().copied().find(|candidate| {
            candidate
                .manifest_path
                .parent()
                .is_some_and(|path| canonical_or_original(path) == expected.local_path)
        });
        let observed = observed_sources(&candidates);
        let Some(local) = local else {
            evidence.push(VerificationEvidence {
                package: expected.package.clone(),
                consumer_root: Some(expected.consumer_root.clone()),
                committed_sources: vec![expected.committed_source.clone()],
                expected_source: expected.local_path.display().to_string(),
                observed_source: observed,
                methods: vec!["cargo-metadata".to_owned()],
                message: Some(
                    "Cargo metadata did not resolve the planned canonical local path".to_owned(),
                ),
            });
            continue;
        };
        if candidates.len() != 1 {
            evidence.push(VerificationEvidence {
                package: expected.package.clone(),
                consumer_root: Some(expected.consumer_root.clone()),
                committed_sources: vec![expected.committed_source.clone()],
                expected_source: expected.local_path.display().to_string(),
                observed_source: observed,
                methods: vec!["cargo-metadata".to_owned()],
                message: Some(
                    "Cargo metadata resolved more than one copy of the linked crate".to_owned(),
                ),
            });
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
                local.id.clone(),
                "--prefix".to_owned(),
                "none".to_owned(),
                "--format".to_owned(),
                "{p}".to_owned(),
            ],
            cwd: plan.operation.key.consumer_repo.clone(),
        });
        match tree {
            Ok(output)
                if output
                    .stdout
                    .contains(&expected.local_path.display().to_string()) =>
            {
                evidence.push(VerificationEvidence {
                    package: expected.package.clone(),
                    consumer_root: Some(expected.consumer_root.clone()),
                    committed_sources: vec![expected.committed_source.clone()],
                    expected_source: expected.local_path.display().to_string(),
                    observed_source: Some(expected.local_path.display().to_string()),
                    methods: vec!["cargo-metadata".to_owned(), "cargo-tree".to_owned()],
                    message: None,
                });
            }
            Ok(output) => evidence.push(VerificationEvidence {
                package: expected.package.clone(),
                consumer_root: Some(expected.consumer_root.clone()),
                committed_sources: vec![expected.committed_source.clone()],
                expected_source: expected.local_path.display().to_string(),
                observed_source: observed,
                methods: vec!["cargo-metadata".to_owned(), "cargo-tree".to_owned()],
                message: Some(format!(
                    "Cargo tree did not show the planned local path: {}",
                    output.stdout.trim()
                )),
            }),
            Err(error) => evidence.push(VerificationEvidence {
                package: expected.package.clone(),
                consumer_root: Some(expected.consumer_root.clone()),
                committed_sources: vec![expected.committed_source.clone()],
                expected_source: expected.local_path.display().to_string(),
                observed_source: observed,
                methods: vec!["cargo-metadata".to_owned(), "cargo-tree".to_owned()],
                message: Some(format!("Cargo tree verification failed: {error}")),
            }),
        }
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

fn planned_library(plan: &CargoDependencyPlan) -> CargoLibraryInventory {
    let desired = plan.desired.as_ref().expect("validated desired Cargo link");
    CargoLibraryInventory {
        root: desired.key.library_path.clone(),
        packages: desired
            .packages
            .iter()
            .map(|package| CargoPackageInventory {
                id: format!("planned:{}", package.name),
                name: package.name.clone(),
                manifest_path: package.local_path.join("Cargo.toml"),
                source: None,
            })
            .collect(),
    }
}

fn observed_sources(candidates: &[&CargoPackageInventory]) -> Option<String> {
    let values = candidates
        .iter()
        .map(|candidate| match &candidate.source {
            Some(source) if source.kind != CommittedSourceKind::Path => source.identity.clone(),
            _ => candidate
                .manifest_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .display()
                .to_string(),
        })
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join(", "))
}

pub(crate) fn validate_all_preconditions(changes: &[PlannedChange]) -> Result<(), DepsError> {
    for change in changes {
        let current = read_optional_string(&change.target)?;
        if current != change.before {
            return Err(stale_change(change));
        }
    }
    Ok(())
}

pub(crate) fn apply_exact_change(change: &PlannedChange) -> Result<(), DepsError> {
    let current = read_optional_string(&change.target)?;
    if current != change.before {
        return Err(stale_change(change));
    }
    write_optional(&change.target, change.after.as_deref())
}

pub(crate) fn rollback_physical_changes(
    plan: &CargoDependencyPlan,
    applied: &[PlannedChange],
) -> CargoLinkRollback {
    if applied.is_empty() {
        return CargoLinkRollback::not_required();
    }
    let mut report = CargoLinkRollback {
        attempted: true,
        restored: Vec::new(),
        failures: Vec::new(),
    };
    for change in applied.iter().rev() {
        let result = (|| {
            let current = read_optional_string(&change.target)?;
            if current != change.after {
                return Err(DepsError::invalid(
                    &change.target,
                    "rollback refused because the file changed after Effigy applied it",
                ));
            }
            write_optional(&change.target, change.before.as_deref())
        })();
        match result {
            Ok(()) => report.restored.push(change.target.clone()),
            Err(error) => report.failures.push(error.to_string()),
        }
    }
    remove_rolled_back_cargo_dir(plan, &mut report);
    report
}

fn remove_rolled_back_cargo_dir(plan: &CargoDependencyPlan, report: &mut CargoLinkRollback) {
    let Some(ownership) = plan
        .desired
        .as_ref()
        .and_then(|desired| desired.cargo_ownership)
    else {
        return;
    };
    if !ownership.cargo_dir_created_by_effigy {
        return;
    }
    let cargo_dir = plan.operation.key.consumer_repo.join(".cargo");
    match fs::remove_dir(&cargo_dir) {
        Ok(()) => report.restored.push(cargo_dir),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
        Err(error) => report
            .failures
            .push(DepsError::io("remove rolled-back directory", cargo_dir, error).to_string()),
    }
}

fn write_optional(path: &Path, contents: Option<&str>) -> Result<(), DepsError> {
    match contents {
        Some(contents) => write_atomic(path, contents.as_bytes(), false),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(DepsError::io("remove", path, error)),
        },
    }
}

pub(crate) fn read_optional_string(path: &Path) -> Result<Option<String>, DepsError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(DepsError::io("read", path, error)),
    }
}

fn stale_change(change: &PlannedChange) -> DepsError {
    DepsError::invalid(
        &change.target,
        "planned before-state is stale; no dependency files were changed",
    )
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn not_run_verification() -> DependencyVerification {
    DependencyVerification {
        status: VerificationStatus::NotRun,
        evidence: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
