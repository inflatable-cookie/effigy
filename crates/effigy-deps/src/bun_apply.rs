use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cargo_apply::{apply_exact_change, not_run_verification, read_optional_string};
use crate::{
    bun_registration_path, inspect_bun_peer_resolutions, inventory_bun_consumer,
    inventory_bun_library, plan_bun_link, BunConsumerLinkDisposition, BunConsumerReference,
    BunDependencyPlan, BunImmutableFileEvidence, BunLinkOperationReport, BunLinkOutcome,
    BunLinkRollback, BunPackagePlan, BunPathObservation, BunPeerDiagnostic,
    BunPeerResolutionStatus, BunPlanObserver, BunProcessAction, BunProcessIntent,
    BunRegistrationDisposition, BunRegistrationIndex, BunRegistrationIndexStore,
    BunRegistrationRetirement, DependencyVerification, DepsError, FsBunPlanObserver, PlannedChange,
    ProcessRequest, ReadOnlyProcess, RepoLinkStateStore, VerificationEvidence, VerificationStatus,
};

pub fn execute_bun_link(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<BunLinkOperationReport, DepsError> {
    execute_bun_link_inner(repo_root, library_path, home, dry_run, process, false)
}

/// Run a link as the owner whose completion or interrupt retires it.
///
/// This is the CLI surface: the process that creates the registration retires
/// it when the command finishes, and a SIGINT/SIGTERM retires it before the
/// signal takes its default course.
pub fn execute_owned_bun_link(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
) -> Result<BunLinkOperationReport, DepsError> {
    execute_bun_link_inner(repo_root, library_path, home, dry_run, process, true)
}

fn execute_bun_link_inner(
    repo_root: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    home: impl AsRef<Path>,
    dry_run: bool,
    process: &impl ReadOnlyProcess,
    owned: bool,
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
    let (consumer_root, library_identity) = plan
        .desired
        .as_ref()
        .map(|desired| {
            (
                desired.key.consumer_repo.clone(),
                desired.key.library_path.clone(),
            )
        })
        .unwrap_or_else(|| (plan.repo_root.clone(), library_path.to_path_buf()));
    let owner = (owned && !dry_run).then(|| {
        crate::bun_owner::BunLinkOwner::arm(
            home.as_ref(),
            &consumer_root,
            &library_identity,
            crate::bun_owner::link_registrations(&plan),
        )
    });
    // The apply retires the registration it created inside its own index
    // transaction, so there is no post-apply window in which an interrupt could
    // leave a registration behind. The owner guard covers that transaction.
    let report = apply_bun_link_plan_owned(
        plan,
        &home,
        process,
        &observer,
        owner.as_ref().map(crate::bun_owner::BunLinkOwner::started),
    )?;
    // Leaving here must not outrun an interrupt cleanup: the watcher retires
    // the registration this process created, and it can only do that while the
    // process is still alive.
    drop(owner);
    Ok(report)
}

/// Retire registrations this owner created when it is interrupted.
///
/// An owner interrupted mid-link still holds the machine index lock for its
/// whole apply, so it cannot wait for that lock: it would deadlock against its
/// own blocked thread. It reads the index instead — a read is an atomic
/// snapshot — so it still sees another consumer's claim. A registration another
/// consumer claims is retained, never unregistered by an owner that is dying.
/// The claim this owner recorded is left for `deps status` to report and
/// `deps unlink bun` or the next link to retire.
pub fn retire_owned_bun_registrations(
    home: impl AsRef<Path>,
    consumer_repo: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    registrations: &[(String, PathBuf)],
    started: bool,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> BunRegistrationRetirement {
    let home = home.as_ref();
    let consumer_repo = consumer_repo.as_ref();
    let library_path = library_path.as_ref();
    let mut retirement = BunRegistrationRetirement::default();
    if !started {
        // This owner never began mutating: a link that only wanted the index
        // lock planned its registrations before this one created anything, and
        // they are not in the index yet. Retire nothing.
        retirement
            .retained
            .extend(registrations.iter().map(|(name, _)| name.clone()));
        return retirement;
    }
    let index_store = BunRegistrationIndexStore::for_home(home);
    // Take the index lock rather than assuming this process still holds it:
    // stopping the in-flight `bun link` lets the apply fail and release its own
    // transaction before the retiral runs. Waiting for the lock also proves no
    // other link is mid-apply while this retiral decides.
    let _lock = match crate::state::StateLock::acquire_waiting(
        index_store.lock_path(),
        std::time::Duration::from_secs(10),
    ) {
        Ok(lock) => lock,
        Err(error) => {
            retirement.failures.push(error.to_string());
            retirement
                .retained
                .extend(registrations.iter().map(|(name, _)| name.clone()));
            return retirement;
        }
    };
    let index = match index_store.read() {
        Ok(index) => index,
        Err(error) => {
            // Without the index no registration can be proved to be this
            // owner's alone: another consumer may hold it. Retire nothing and
            // say why, so the next link or `deps unlink bun` decides.
            retirement.failures.push(error.to_string());
            retirement
                .retained
                .extend(registrations.iter().map(|(name, _)| name.clone()));
            return retirement;
        }
    };
    for (name, package_path) in registrations {
        let claimed_elsewhere = index
            .registrations
            .iter()
            .find(|registration| {
                registration.package_name == *name
                    && same_path(&registration.package_path, package_path)
            })
            .is_some_and(|registration| {
                registration
                    .consumers
                    .iter()
                    .any(|consumer| !claim_owned_by(consumer, consumer_repo, library_path))
            });
        if claimed_elsewhere {
            retirement.retained.push(name.clone());
            continue;
        }
        let owned = OwnedRegistration {
            name: name.clone(),
            package_path: package_path.clone(),
        };
        match unregister_owned_registration(home, &owned, process, observer) {
            ClaimRetirement::Unregistered => retirement.unregistered.push(name.clone()),
            ClaimRetirement::Retained => retirement.retained.push(name.clone()),
            ClaimRetirement::Failed(failure) => retirement.failures.push(failure),
        }
    }
    retirement
}

/// Retire the machine-wide registrations this consumer's link created.
///
/// `bun link` registers each package in the machine-wide Bun install so the
/// consumer's save-less `bun link` can resolve it. Nothing needs that
/// registration once the consumer's `node_modules` symlink exists, so the owner
/// that created it retires its own claim when it completes. The claim is
/// released while another consumer still holds one, and stays recorded when the
/// registration is foreign or unverifiable, so `deps unlink bun` can finish the
/// job instead of guessing.
///
/// Retirement is idempotent: an owner that already retired its claims has
/// nothing left to do, which lets an interrupted run call it again.
pub fn retire_bun_registration_claims(
    home: impl AsRef<Path>,
    consumer_repo: impl AsRef<Path>,
    library_path: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> BunRegistrationRetirement {
    let home = home.as_ref();
    let consumer_repo = consumer_repo.as_ref();
    let library_path = library_path.as_ref();
    let index_store = BunRegistrationIndexStore::for_home(home);
    let mut retirement = BunRegistrationRetirement::default();
    if !index_store.path().exists() {
        return retirement;
    }
    let outcome = index_store.update(|index| {
        retirement =
            retire_claims_in_index(index, home, consumer_repo, library_path, process, observer);
        Ok(())
    });
    if let Err(error) = outcome {
        retirement.failures.push(error.to_string());
    }
    retirement
}

/// Retire this link's claims inside an index the caller already holds.
///
/// The completion path runs this inside the apply's own index transaction, so
/// the registration is gone before that lock is released and no interrupt can
/// land in a window where the owner still has a registration to retire.
fn retire_claims_in_index(
    index: &mut BunRegistrationIndex,
    home: &Path,
    consumer_repo: &Path,
    library_path: &Path,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> BunRegistrationRetirement {
    let mut retirement = BunRegistrationRetirement::default();
    let claimed = index
        .registrations
        .iter()
        .flat_map(|registration| {
            registration
                .consumers
                .iter()
                .filter(|reference| claim_owned_by(reference, consumer_repo, library_path))
                .map(move |reference| (registration.clone(), reference.clone()))
        })
        .collect::<Vec<_>>();
    for (registration, reference) in &claimed {
        let Some(stored) = index
            .registrations
            .iter()
            .find(|candidate| {
                candidate.package_name == registration.package_name
                    && same_path(&candidate.package_path, &registration.package_path)
            })
            .and_then(|candidate| {
                candidate
                    .consumers
                    .iter()
                    .find(|candidate| candidate.same_claim(reference))
                    .cloned()
            })
        else {
            continue;
        };
        let shared = index
            .registrations
            .iter()
            .find(|candidate| candidate.package_name == registration.package_name)
            .is_some_and(|candidate| candidate.consumers.len() > 1);
        if shared {
            // Another consumer still holds the registration.
            index.release_reference(&registration.package_name, &stored);
            retirement
                .released_shared
                .push(registration.package_name.clone());
            continue;
        }
        if !registration.effigy_created {
            // A registration Effigy did not create is never removed, but this
            // consumer's claim on it is not its own either.
            index.release_reference(&registration.package_name, &stored);
            retirement.retained.push(registration.package_name.clone());
            continue;
        }
        let owned = OwnedRegistration {
            name: registration.package_name.clone(),
            package_path: registration.package_path.clone(),
        };
        match unregister_owned_registration(home, &owned, process, observer) {
            ClaimRetirement::Unregistered => {
                index.release_reference(&registration.package_name, &stored);
                retirement
                    .unregistered
                    .push(registration.package_name.clone());
            }
            ClaimRetirement::Retained => {
                retirement.retained.push(registration.package_name.clone());
            }
            ClaimRetirement::Failed(failure) => retirement.failures.push(failure),
        }
    }
    retirement
}

struct OwnedRegistration {
    name: String,
    package_path: PathBuf,
}

enum ClaimRetirement {
    Unregistered,
    Retained,
    Failed(String),
}

fn unregister_owned_registration(
    home: &Path,
    owned: &OwnedRegistration,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> ClaimRetirement {
    let registration_path = bun_registration_path(home, &owned.name);
    match observer.observe_path(&registration_path) {
        Ok(BunPathObservation::Missing) => ClaimRetirement::Unregistered,
        Ok(BunPathObservation::Symlink { target }) if same_path(&target, &owned.package_path) => {
            let request = ProcessRequest {
                program: "bun".to_owned(),
                args: vec!["unlink".to_owned(), "--no-save".to_owned()],
                cwd: owned.package_path.clone(),
            };
            match process.run(&request) {
                Ok(_) => ClaimRetirement::Unregistered,
                Err(error) => ClaimRetirement::Failed(format!(
                    "retire Bun registration `{}`: {error}",
                    owned.name
                )),
            }
        }
        Ok(_) => ClaimRetirement::Retained,
        Err(error) => ClaimRetirement::Failed(format!(
            "inspect Bun registration `{}`: {error}",
            owned.name
        )),
    }
}

/// Put back the registrations a transaction retired but could not commit.
///
/// Only a registration that already existed before this attempt has to come
/// back: one this attempt created is removed by the rollback, and a registration
/// another consumer shares was never unregistered.
fn restore_retired_registrations(
    plan: &BunDependencyPlan,
    home: &Path,
    retirement: &BunRegistrationRetirement,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
    errors: &mut Vec<String>,
) {
    for name in &retirement.unregistered {
        let Some(package) = plan.packages.iter().find(|package| &package.name == name) else {
            continue;
        };
        if package.registration != BunRegistrationDisposition::MatchingOwned {
            continue;
        }
        let request = ProcessRequest {
            program: "bun".to_owned(),
            args: vec!["link".to_owned(), "--no-save".to_owned()],
            cwd: package.local_path.clone(),
        };
        if let Err(error) = process.run(&request) {
            errors.push(format!("restore Bun registration `{name}`: {error}"));
        }
        if let Ok(BunPathObservation::Missing) =
            observer.observe_path(&bun_registration_path(home, name))
        {
            errors.push(format!(
                "restore Bun registration `{name}`: the registration is still missing"
            ));
        }
    }
}

/// Whether a recorded claim is the one this owner's link wrote.
///
/// A checkout can hold several Bun package roots (`studio/`, `harness/`), and
/// each is a separate consumer with its own claim. Retirement therefore matches
/// the exact root and library this operation recorded; sharing a checkout is
/// not evidence that a claim belongs to this operation.
fn claim_owned_by(
    reference: &BunConsumerReference,
    consumer_repo: &Path,
    library_path: &Path,
) -> bool {
    same_path(&reference.consumer_repo, consumer_repo)
        && same_path(&reference.library_path, library_path)
}

/// Whether applying a link also retires the registrations it created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetireRegistrations {
    /// Apply the plan and leave its registrations and claims for the caller.
    /// Fixtures use this to construct the claimed state a recovery reads.
    Never,
    /// Retire this link's own registrations inside the same index transaction.
    /// The command that owns the link does this.
    LinkOwned,
}

pub fn apply_bun_link_plan(
    plan: BunDependencyPlan,
    home: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
) -> Result<BunLinkOperationReport, DepsError> {
    apply_bun_link_plan_inner(
        plan,
        home,
        process,
        observer,
        RetireRegistrations::Never,
        None,
    )
}

fn apply_bun_link_plan_owned(
    plan: BunDependencyPlan,
    home: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
    started: Option<&std::sync::atomic::AtomicBool>,
) -> Result<BunLinkOperationReport, DepsError> {
    apply_bun_link_plan_inner(
        plan,
        home,
        process,
        observer,
        RetireRegistrations::LinkOwned,
        started,
    )
}

fn apply_bun_link_plan_inner(
    plan: BunDependencyPlan,
    home: impl AsRef<Path>,
    process: &impl ReadOnlyProcess,
    observer: &impl BunPlanObserver,
    retire: RetireRegistrations,
    started: Option<&std::sync::atomic::AtomicBool>,
) -> Result<BunLinkOperationReport, DepsError> {
    if plan.desired.is_none() {
        let errors = plan.operation.warnings.clone();
        return Ok(BunLinkOperationReport {
            immutable_files: immutable_evidence(&plan),
            plan,
            outcome: BunLinkOutcome::CommittedPinActive,
            applied_processes: Vec::new(),
            verification: not_run_verification(),
            peer_diagnostics: Vec::new(),
            rollback: BunLinkRollback::not_required(),
            registration_retirement: BunRegistrationRetirement::default(),
            errors,
        });
    }
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
            registration_retirement: BunRegistrationRetirement::default(),
            errors: Vec::new(),
        });
    }
    validate_state_preconditions(&plan)?;
    validate_immutable_preconditions(&plan)?;
    validate_process_intents(&plan)?;

    let home = home.as_ref();
    let index_store = BunRegistrationIndexStore::for_home(home);
    let ledger_path = RepoLinkStateStore::for_repo(&plan.repo_root)
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
    let mut planned_index = planned_index(index_change, &index_store)?;
    let mut registration_retirement = BunRegistrationRetirement::default();

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

    index_store.update_exact(index_before, |current| {
        validate_physical_preconditions(&plan, observer)?;
        if let Some(started) = started {
            // From here the transaction can create a registration, so an
            // interrupt from here on owns one it has to retire.
            started.store(true, std::sync::atomic::Ordering::SeqCst);
        }
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

        if let Some(change) = &ledger_change {
            // The ledger commits inside the same transaction. A failed write
            // then leaves the index untouched instead of needing a restore
            // after the lock is released.
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
            applied_files.push(PlannedChange::clone(change));
        }

        if retire == RetireRegistrations::LinkOwned {
            // Retire inside this transaction, while the index lock is held:
            // the registration only exists so the consumer's save-less `bun
            // link` resolves, and an interrupt must never land between the
            // apply and the retiral with a registration still owned.
            registration_retirement = retire_claims_in_index(
                &mut planned_index,
                home,
                &plan.operation.key.consumer_repo,
                &plan.operation.key.library_path,
                process,
                observer,
            );
        }
        // Commit inside the transaction, while the lock is held. `update_exact`
        // then has nothing left to write, so a failed commit is handled here
        // with the same rollback as any other failed step instead of returning
        // after the ledger, the consumer link and the retiral had landed.
        if planned_index != *current {
            if let Err(error) = index_store.commit_planned(&planned_index) {
                outcome = BunLinkOutcome::ApplyFailed;
                errors.push(error.to_string());
                restore_retired_registrations(
                    &plan,
                    home,
                    &registration_retirement,
                    process,
                    observer,
                    &mut errors,
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
        }
        Ok((None, ()))
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
            registration_retirement: BunRegistrationRetirement::default(),
            errors,
        });
    }

    cleanup_backups(&backups, &mut errors);
    Ok(BunLinkOperationReport {
        plan,
        outcome,
        applied_processes,
        immutable_files: evidence,
        verification,
        peer_diagnostics,
        rollback,
        registration_retirement,
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
            .repo_root
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
