//! Barge container-lease coordination, ended/orphan sweeps, and `effigy container prune`.
//!
//! Coordinates per-profile worker and interactive container leases against the
//! host Barge account, tears down ended or orphaned `barge.lease` stacks via
//! `teardown_project_on_profile` before returning leases, and falls back
//! gracefully to unmanaged execution when Barge is absent or a profile is not
//! declared in `container-profiles.json`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use barge::container::{
    ContainerClass, ContainerEndReason, ContainerEnding, ContainerLease, ContainerLeaseOutcome,
    ContainerLeaseRequest, ContainerLeaseStatus, ContainerOwner,
};
use barge::{Barge, ErrorKind};
use effigy_containers::RetireOutcome;
use effigy_ui::HumanError;
use serde::Serialize;
use serde_json::json;

use super::lifecycle::teardown_project_on_profile;
use super::profile_inventory::{list_containers, ContainerRow};
use super::RunnerError;

const CONTAINER_PRUNE_SCHEMA: &str = "effigy.container.prune.v1";
const DEFAULT_LEASE_WAIT_SECS: u64 = 300;
pub(super) const DEFAULT_LEASE_WAIT_SLICE: Duration = Duration::from_secs(2);
const LEASE_WAIT_ENV: &str = "EFFIGY_CONTAINER_LEASE_WAIT_SECS";

/// Seam for Colima profile discovery, container inventory, and project teardown.
pub(in crate::runner) trait LeaseRuntime {
    fn running_profiles(&self) -> Result<Vec<String>, RunnerError>;
    fn list_containers(&self, profile: &str) -> Result<Vec<ContainerRow>, RunnerError>;
    fn teardown_project(
        &self,
        profile: &str,
        project: &str,
        remove_volumes: bool,
    ) -> Result<RetireOutcome, RunnerError>;
}

/// Real host Colima/Docker runtime implementation.
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::runner) struct RealLeaseRuntime;

impl LeaseRuntime for RealLeaseRuntime {
    fn running_profiles(&self) -> Result<Vec<String>, RunnerError> {
        let cwd = std::env::temp_dir();
        effigy_containers::exec::running_colima_profiles(&cwd).map_err(RunnerError::from)
    }

    fn list_containers(&self, profile: &str) -> Result<Vec<ContainerRow>, RunnerError> {
        let cwd = std::env::temp_dir();
        list_containers(&cwd, profile)
    }

    fn teardown_project(
        &self,
        profile: &str,
        project: &str,
        remove_volumes: bool,
    ) -> Result<RetireOutcome, RunnerError> {
        teardown_project_on_profile(profile, project, remove_volumes)
    }
}

/// Result of attempting to acquire a Barge container lease.
#[derive(Debug)]
pub(in crate::runner) enum LeaseAcquireOutcome {
    Leased(ContainerLease),
    NotManaged,
}

/// Inputs used to classify a container session as interactive or worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct ContainerClassContext<'a> {
    pub profile: &'a str,
    pub is_worktree: bool,
    pub is_ephemeral: bool,
    pub is_ci: bool,
    pub is_detached_worker: bool,
    pub is_release_builder: bool,
}

/// Resolve whether a container session uses an interactive or worker Barge lease.
pub(in crate::runner) fn resolve_container_class(ctx: ContainerClassContext<'_>) -> ContainerClass {
    if ctx.is_worktree
        || ctx.is_ephemeral
        || ctx.is_ci
        || ctx.is_detached_worker
        || ctx.is_release_builder
        || ctx.profile == "effigy-release"
    {
        ContainerClass::Worker
    } else {
        ContainerClass::Interactive
    }
}

/// Whether the current environment indicates a CI runner.
pub(in crate::runner) fn is_ci_environment() -> bool {
    is_ci_env_value(std::env::var("CI").ok().as_deref())
}

fn is_ci_env_value(value: Option<&str>) -> bool {
    value.is_some_and(|raw| {
        let trimmed = raw.trim();
        !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
    })
}

/// One ended Barge lease torn down and returned during a sweep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct SweptLease {
    pub id: String,
    pub profile: String,
    pub project: String,
    pub reason: String,
    pub remove_volumes: bool,
    pub removed_count: usize,
}

/// One orphaned `barge.lease`-labelled stack torn down during a sweep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct SweptOrphan {
    pub lease_id: String,
    pub profile: String,
    pub project: String,
    pub containers: Vec<String>,
    pub remove_volumes: bool,
    pub removed_count: usize,
}

/// One orphaned `barge.lease`-labelled stack skipped because a lease holds `(profile, project)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct SkippedOrphan {
    pub lease_id: String,
    pub held_by_lease_id: String,
    pub profile: String,
    pub project: String,
    pub containers: Vec<String>,
    pub reason: String,
}

/// One lease or orphan whose teardown failed or whose profile was stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct SweepFailure {
    pub lease_id: String,
    pub profile: String,
    pub project: String,
    pub kind: String,
    pub error: String,
}

/// Aggregate report returned by ended-lease and orphan sweeps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct SweepReport {
    pub barge_managed: bool,
    pub profiles_swept: Vec<String>,
    pub stopped_profiles: Vec<String>,
    pub returned_leases: Vec<SweptLease>,
    pub orphans_torn_down: Vec<SweptOrphan>,
    pub skipped_orphans: Vec<SkippedOrphan>,
    pub retained_failures: Vec<SweepFailure>,
}

impl SweepReport {
    pub(in crate::runner) fn ok(&self) -> bool {
        self.retained_failures.is_empty()
    }
}

/// Open the host Barge account, returning `Ok(None)` when Barge is unconfigured
/// or unavailable on this machine (`NotFound` or `Unsupported`). Unreadable or
/// corrupt state (`UnknownState`, `Io`) surfaces as an error so an orphan sweep
/// never runs against an empty fallback lease list.
pub(in crate::runner) fn open_host_barge() -> Result<Option<Barge>, RunnerError> {
    classify_open_barge_result(Barge::open())
}

pub(super) fn classify_open_barge_result(
    result: std::result::Result<Barge, barge::Error>,
) -> Result<Option<Barge>, RunnerError> {
    match result {
        Ok(barge) => Ok(Some(barge)),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::Unsupported) => {
            Ok(None)
        }
        Err(error) => Err(barge_error(error)),
    }
}

/// Resolve the Barge owner for `checkout`: `ContainerOwner::Worktree(id)` if it
/// matches an active Barge worktree lease root, otherwise `ContainerOwner::Checkout`.
pub(in crate::runner) fn resolve_container_owner(
    barge: &Barge,
    checkout: &Path,
) -> Result<ContainerOwner, RunnerError> {
    let canonical = canonical_or_absolute(checkout)?;
    let pools = barge.worktree_status(None).map_err(barge_error)?;
    for pool in pools {
        for root in pool.roots {
            let Some(lease_id) = root.lease_id.as_deref() else {
                continue;
            };
            if root_matches_checkout(barge, lease_id, &root.path, &canonical) {
                return Ok(ContainerOwner::Worktree(lease_id.to_owned()));
            }
        }
    }
    Ok(ContainerOwner::Checkout(canonical))
}

fn canonical_or_absolute(path: &Path) -> Result<PathBuf, RunnerError> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot resolve current directory for checkout `{}`: {error}",
            path.display()
        ))
    })?;
    Ok(cwd.join(path))
}

fn root_matches_checkout(
    barge: &Barge,
    lease_id: &str,
    root_path: &Path,
    canonical_checkout: &Path,
) -> bool {
    let mut candidates = vec![root_path.to_path_buf(), root_path.join("checkout")];
    if let Ok(lease) = barge.worktree_lease_by_id(lease_id) {
        candidates.push(PathBuf::from(lease.path));
    }
    candidates.into_iter().any(|candidate| {
        std::fs::canonicalize(&candidate)
            .map(|resolved| resolved == canonical_checkout)
            .unwrap_or_else(|_| candidate == canonical_checkout)
    })
}

/// Sweep ended leases and orphaned `barge.lease` containers on one profile.
pub(in crate::runner) fn sweep_profile_leases_with(
    barge: Option<&Barge>,
    runtime: &dyn LeaseRuntime,
    profile: &str,
) -> Result<SweepReport, RunnerError> {
    let Some(barge) = barge else {
        return Ok(SweepReport::default());
    };
    let statuses = barge.container_status().map_err(barge_error)?;
    let running: BTreeSet<String> = runtime.running_profiles()?.into_iter().collect();
    let mut report = SweepReport {
        barge_managed: true,
        ..SweepReport::default()
    };

    if !running.contains(profile) {
        report.stopped_profiles.push(profile.to_owned());
        record_stopped_profile_leases(&statuses, profile, &mut report);
        return Ok(report);
    }

    let mut known_ids: HashSet<String> = statuses.iter().map(|lease| lease.id.clone()).collect();
    report.profiles_swept.push(profile.to_owned());
    sweep_running_profile(
        barge,
        runtime,
        profile,
        &statuses,
        &mut known_ids,
        &mut report,
    )?;
    Ok(report)
}

/// Sweep ended leases and orphaned `barge.lease` containers across all running
/// Colima profiles.
pub(in crate::runner) fn sweep_all_running_profiles_with(
    barge: Option<&Barge>,
    runtime: &dyn LeaseRuntime,
) -> Result<SweepReport, RunnerError> {
    let Some(barge) = barge else {
        return Ok(SweepReport::default());
    };
    let statuses = barge.container_status().map_err(barge_error)?;
    let running: BTreeSet<String> = runtime.running_profiles()?.into_iter().collect();
    let mut report = SweepReport {
        barge_managed: true,
        ..SweepReport::default()
    };
    let mut known_ids: HashSet<String> = statuses.iter().map(|lease| lease.id.clone()).collect();

    for profile in &running {
        report.profiles_swept.push(profile.clone());
        sweep_running_profile(
            barge,
            runtime,
            profile,
            &statuses,
            &mut known_ids,
            &mut report,
        )?;
    }

    let mut stopped = BTreeSet::new();
    for lease in &statuses {
        if lease.ended.is_some() && !running.contains(&lease.profile) {
            stopped.insert(lease.profile.clone());
            record_stopped_lease(lease, &mut report);
        }
    }
    report.stopped_profiles = stopped.into_iter().collect();

    Ok(report)
}

fn record_stopped_profile_leases(
    statuses: &[ContainerLeaseStatus],
    profile: &str,
    report: &mut SweepReport,
) {
    for lease in statuses {
        if lease.profile == profile && lease.ended.is_some() {
            record_stopped_lease(lease, report);
        }
    }
}

fn record_stopped_lease(lease: &ContainerLeaseStatus, report: &mut SweepReport) {
    report.retained_failures.push(SweepFailure {
        lease_id: lease.id.clone(),
        profile: lease.profile.clone(),
        project: lease.project.clone(),
        kind: "ended_lease".to_owned(),
        error: format!(
            "Colima profile `{}` is stopped; lease retained for retry",
            lease.profile
        ),
    });
}

fn sweep_running_profile(
    barge: &Barge,
    runtime: &dyn LeaseRuntime,
    profile: &str,
    statuses: &[ContainerLeaseStatus],
    known_ids: &mut HashSet<String>,
    report: &mut SweepReport,
) -> Result<(), RunnerError> {
    for lease in statuses {
        if lease.profile != profile {
            continue;
        }
        let Some(ending) = lease.ended else {
            continue;
        };
        match runtime.teardown_project(&lease.profile, &lease.project, ending.remove_volumes) {
            Ok(outcome) if outcome.ok() => {
                if let Err(error) = barge.container_return(&lease.id) {
                    report.retained_failures.push(SweepFailure {
                        lease_id: lease.id.clone(),
                        profile: lease.profile.clone(),
                        project: lease.project.clone(),
                        kind: "ended_lease".to_owned(),
                        error: format!(
                            "stack was torn down, but returning lease to Barge failed: {error}"
                        ),
                    });
                } else {
                    report.returned_leases.push(SweptLease {
                        id: lease.id.clone(),
                        profile: lease.profile.clone(),
                        project: lease.project.clone(),
                        reason: end_reason_label(ending).to_owned(),
                        remove_volumes: ending.remove_volumes,
                        removed_count: outcome.removed.len(),
                    });
                }
            }
            Ok(outcome) => {
                report.retained_failures.push(SweepFailure {
                    lease_id: lease.id.clone(),
                    profile: lease.profile.clone(),
                    project: lease.project.clone(),
                    kind: "ended_lease".to_owned(),
                    error: summarize_retire_failures(&outcome),
                });
            }
            Err(error) => {
                report.retained_failures.push(SweepFailure {
                    lease_id: lease.id.clone(),
                    profile: lease.profile.clone(),
                    project: lease.project.clone(),
                    kind: "ended_lease".to_owned(),
                    error: error.to_string(),
                });
            }
        }
    }

    let containers = match runtime.list_containers(profile) {
        Ok(rows) => rows,
        Err(error) => {
            report.retained_failures.push(SweepFailure {
                lease_id: String::new(),
                profile: profile.to_owned(),
                project: String::new(),
                kind: "profile_inventory".to_owned(),
                error: error.to_string(),
            });
            return Ok(());
        }
    };

    let mut candidate_orphans: Vec<(String, ContainerRow)> = Vec::new();
    for row in containers {
        let Some(lease_id) = row.barge_lease.clone() else {
            continue;
        };
        if lease_id.trim().is_empty() || known_ids.contains(&lease_id) {
            continue;
        }
        candidate_orphans.push((lease_id, row));
    }

    if candidate_orphans.is_empty() {
        return Ok(());
    }

    // Re-read Barge status once before tearing down candidates so a lease
    // minted concurrently between step 1 and step 3 is never treated as orphan,
    // and collect every (profile, project) held by a live or retained lease.
    let mut held_projects: BTreeMap<String, String> = BTreeMap::new();
    for fresh in barge.container_status().map_err(barge_error)? {
        known_ids.insert(fresh.id.clone());
        if fresh.profile == profile {
            held_projects.insert(fresh.project, fresh.id);
        }
    }
    candidate_orphans.retain(|(lease_id, _)| !known_ids.contains(lease_id));

    let mut by_project: BTreeMap<String, Vec<(String, ContainerRow)>> = BTreeMap::new();
    for (lease_id, row) in candidate_orphans {
        let Some(project) = row.project.clone().or_else(|| row.effigy_project.clone()) else {
            report.retained_failures.push(SweepFailure {
                lease_id,
                profile: profile.to_owned(),
                project: row.name,
                kind: "orphan_container".to_owned(),
                error: "container carries `barge.lease` but has no Compose or Effigy project label"
                    .to_owned(),
            });
            continue;
        };
        by_project.entry(project).or_default().push((lease_id, row));
    }

    for (project, entries) in by_project {
        let lease_id = entries
            .first()
            .map(|(id, _)| id.clone())
            .unwrap_or_default();
        let container_names: Vec<String> =
            entries.iter().map(|(_, row)| row.name.clone()).collect();
        if let Some(holding_lease_id) = held_projects.get(&project) {
            report.skipped_orphans.push(SkippedOrphan {
                lease_id,
                held_by_lease_id: holding_lease_id.clone(),
                profile: profile.to_owned(),
                project: project.clone(),
                containers: container_names,
                reason: format!(
                    "project `{project}` on profile `{profile}` is currently held by Barge lease `{holding_lease_id}`; skipped orphan teardown"
                ),
            });
            continue;
        }
        let remove_volumes = group_checkout_confirmed_gone(&entries);
        match runtime.teardown_project(profile, &project, remove_volumes) {
            Ok(outcome) if outcome.ok() => {
                report.orphans_torn_down.push(SweptOrphan {
                    lease_id,
                    profile: profile.to_owned(),
                    project,
                    containers: container_names,
                    remove_volumes,
                    removed_count: outcome.removed.len(),
                });
            }
            Ok(outcome) => {
                report.retained_failures.push(SweepFailure {
                    lease_id,
                    profile: profile.to_owned(),
                    project,
                    kind: "orphan_container".to_owned(),
                    error: summarize_retire_failures(&outcome),
                });
            }
            Err(error) => {
                report.retained_failures.push(SweepFailure {
                    lease_id,
                    profile: profile.to_owned(),
                    project,
                    kind: "orphan_container".to_owned(),
                    error: error.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn group_checkout_confirmed_gone(entries: &[(String, ContainerRow)]) -> bool {
    let mut saw_owner_path = false;
    for (_, row) in entries {
        let Some(owner) = row.owner_path() else {
            return false;
        };
        saw_owner_path = true;
        if !is_checkout_confirmed_gone(Some(&owner)) {
            return false;
        }
    }
    saw_owner_path
}

fn is_checkout_confirmed_gone(owner_path: Option<&str>) -> bool {
    let Some(raw) = owner_path.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let path = Path::new(raw);
    if !path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return false;
    }
    let Some(parent) = path.parent() else {
        return false;
    };
    // Never treat an unmounted volume root (e.g. `/Volumes/Stash` or `/mnt/drive`)
    // as a deleted checkout merely because `/Volumes` or `/mnt` exists.
    if parent == Path::new("/")
        || parent == Path::new("/Volumes")
        || parent == Path::new("/mnt")
        || parent == Path::new("/media")
    {
        return false;
    }
    // Only confirm absence when the owning parent directory is present; if a
    // removable drive is unplugged, the parent path itself is absent.
    let parent_present = std::fs::metadata(parent).is_ok_and(|meta| meta.is_dir());
    if !parent_present {
        return false;
    }
    matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn end_reason_label(ending: ContainerEnding) -> &'static str {
    match ending.reason {
        ContainerEndReason::OwnerEnded => "owner_ended",
        ContainerEndReason::HoldReleased => "hold_released",
    }
}

fn summarize_retire_failures(outcome: &RetireOutcome) -> String {
    if outcome.failed.is_empty() {
        return "container teardown did not complete cleanly".to_owned();
    }
    outcome
        .failed
        .iter()
        .map(|failure| failure.error.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Resolve the wait budget for [`acquire_container_lease_with`]: `EFFIGY_CONTAINER_LEASE_WAIT_SECS`
/// when set, else `request_wait`, else 300 seconds.
pub(in crate::runner) fn lease_wait_budget_for(
    request_wait: Option<Duration>,
) -> Result<Duration, RunnerError> {
    if let Ok(raw) = std::env::var(LEASE_WAIT_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let secs = trimmed.parse::<u64>().map_err(|_| {
                RunnerError::task_invocation(format!(
                    "invalid `{LEASE_WAIT_ENV}` value `{trimmed}`; expected a non-negative integer number of seconds"
                ))
            })?;
            return Ok(Duration::from_secs(secs));
        }
    }
    Ok(request_wait.unwrap_or(Duration::from_secs(DEFAULT_LEASE_WAIT_SECS)))
}

/// Acquire a Barge container lease, sweeping ended and orphaned leases before
/// and between short wait slices when the profile is at capacity.
pub(in crate::runner) fn acquire_container_lease_with(
    barge: Option<&Barge>,
    runtime: &dyn LeaseRuntime,
    request: &ContainerLeaseRequest,
    wait_budget: Duration,
    slice: Duration,
) -> Result<LeaseAcquireOutcome, RunnerError> {
    let Some(barge) = barge else {
        return Ok(LeaseAcquireOutcome::NotManaged);
    };
    if !barge.state_dir().join("container-profiles.json").exists() {
        return Ok(LeaseAcquireOutcome::NotManaged);
    }

    if let Some(renewed) = renew_released_lease(barge, request)? {
        return Ok(renewed);
    }

    let mut announced_wait = false;
    let mut last_sweep_notes = Vec::new();
    collect_sweep_notes(
        sweep_profile_leases_with(Some(barge), runtime, &request.profile),
        &mut last_sweep_notes,
    );

    let started = Instant::now();
    loop {
        let remaining = wait_budget.saturating_sub(started.elapsed());
        let mut attempt = request.clone();
        attempt.wait = (!remaining.is_zero()).then(|| slice.min(remaining));

        match barge.container_lease(&attempt).map_err(barge_error)? {
            ContainerLeaseOutcome::Leased(lease) => {
                return Ok(LeaseAcquireOutcome::Leased(lease));
            }
            ContainerLeaseOutcome::NotManaged => {
                return Ok(LeaseAcquireOutcome::NotManaged);
            }
            ContainerLeaseOutcome::Full {
                profile,
                max,
                active,
            } => {
                if started.elapsed() >= wait_budget {
                    return Err(lease_pool_full_error(
                        &profile,
                        active,
                        max,
                        wait_budget,
                        &last_sweep_notes,
                    ));
                }
                if !announced_wait {
                    announced_wait = true;
                    eprintln!(
                        "[info] waiting up to {}s for a container slot on profile `{profile}` ({active}/{max} in use)",
                        wait_budget.as_secs()
                    );
                }
                collect_sweep_notes(
                    sweep_profile_leases_with(Some(barge), runtime, &request.profile),
                    &mut last_sweep_notes,
                );
            }
        }
    }
}

/// Return and retake `(profile, project)` when its own owner comes back after
/// the hold ended, so the stack it left is reused instead of swept and rebuilt.
/// `None` when there is no such lease, or the slot was taken in between.
fn renew_released_lease(
    barge: &Barge,
    request: &ContainerLeaseRequest,
) -> Result<Option<LeaseAcquireOutcome>, RunnerError> {
    let released = find_lease(barge, &request.profile, &request.project)?.filter(|lease| {
        lease.owner == request.owner
            && lease
                .ended
                .is_some_and(|ending| ending.reason == ContainerEndReason::HoldReleased)
    });
    let Some(released) = released else {
        return Ok(None);
    };
    return_lease_by_id_with(Some(barge), &released.id)?;
    let mut attempt = request.clone();
    attempt.wait = None;
    Ok(
        match barge.container_lease(&attempt).map_err(barge_error)? {
            ContainerLeaseOutcome::Leased(lease) => Some(LeaseAcquireOutcome::Leased(lease)),
            ContainerLeaseOutcome::NotManaged => Some(LeaseAcquireOutcome::NotManaged),
            ContainerLeaseOutcome::Full { .. } => None,
        },
    )
}

/// The lease recorded for `(profile, project)`, ended or not.
pub(in crate::runner) fn find_lease(
    barge: &Barge,
    profile: &str,
    project: &str,
) -> Result<Option<ContainerLeaseStatus>, RunnerError> {
    Ok(barge
        .container_status()
        .map_err(barge_error)?
        .into_iter()
        .find(|lease| lease.profile == profile && lease.project == project))
}

/// Hold the lease `owner`'s stack already has. It never reserves a slot for a
/// stack that has none, and leaves another owner's lease alone.
pub(in crate::runner) fn hold_existing_lease_with(
    barge: &Barge,
    profile: &str,
    project: &str,
    owner: &ContainerOwner,
) -> Result<Option<ContainerLease>, RunnerError> {
    let Some(found) = find_lease(barge, profile, project)? else {
        return Ok(None);
    };
    if &found.owner != owner
        || found
            .ended
            .is_some_and(|ending| ending.reason == ContainerEndReason::OwnerEnded)
    {
        return Ok(None);
    }
    let request = ContainerLeaseRequest {
        profile: profile.to_owned(),
        project: project.to_owned(),
        owner: owner.clone(),
        class: found.class,
        hold: true,
        wait: None,
    };
    let outcome = match renew_released_lease(barge, &request)? {
        Some(renewed) => renewed,
        None if found.ended.is_none() => {
            match barge.container_lease(&request).map_err(barge_error)? {
                ContainerLeaseOutcome::Leased(lease) => LeaseAcquireOutcome::Leased(lease),
                _ => LeaseAcquireOutcome::NotManaged,
            }
        }
        None => LeaseAcquireOutcome::NotManaged,
    };
    Ok(match outcome {
        LeaseAcquireOutcome::Leased(lease) => Some(lease),
        LeaseAcquireOutcome::NotManaged => None,
    })
}

fn collect_sweep_notes(result: Result<SweepReport, RunnerError>, last_notes: &mut Vec<String>) {
    last_notes.clear();
    match result {
        Ok(report) => {
            for failure in report.retained_failures {
                let target = if failure.project.is_empty() {
                    failure.profile
                } else {
                    format!("{}/{}", failure.profile, failure.project)
                };
                last_notes.push(format!("{target}: {}", failure.error));
            }
        }
        Err(error) => last_notes.push(error.to_string()),
    }
}

fn lease_pool_full_error(
    profile: &str,
    active: u32,
    max: u32,
    wait_budget: Duration,
    sweep_notes: &[String],
) -> RunnerError {
    let mut message = format!(
        "container lease pool for profile `{profile}` is full ({active}/{max} active worker slots) after waiting {}s",
        wait_budget.as_secs(),
    );
    if !sweep_notes.is_empty() {
        message.push_str(&format!(
            "\nsweep failures during wait: {}",
            sweep_notes.join("; ")
        ));
    }
    message.push_str(&format!(
        "\nnext: wait for an active worker stack to finish, or run `effigy container prune --profile {profile}` to reclaim ended stacks"
    ));
    RunnerError::task_invocation(message)
}

/// Immediately return a known Barge container lease ID (idempotent no-op when
/// already returned or when Barge is unmanaged).
pub(in crate::runner) fn return_lease_by_id_with(
    barge: Option<&Barge>,
    lease_id: &str,
) -> Result<(), RunnerError> {
    let Some(barge) = barge else {
        return Ok(());
    };
    match barge.container_return(lease_id) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(barge_error(error)),
    }
}

/// Look up `(profile, project)` in Barge and return its lease if present
/// (idempotent no-op when already returned or when Barge is unmanaged).
pub(in crate::runner) fn return_lease_for_project_with(
    barge: Option<&Barge>,
    profile: &str,
    project: &str,
) -> Result<bool, RunnerError> {
    let Some(barge) = barge else {
        return Ok(false);
    };
    let statuses = barge.container_status().map_err(barge_error)?;
    let Some(found) = statuses
        .into_iter()
        .find(|lease| lease.profile == profile && lease.project == project)
    else {
        return Ok(false);
    };
    barge.container_return(&found.id).map_err(barge_error)?;
    Ok(true)
}

/// Execute `effigy container prune [--profile <PROFILE>] [--json]`.
pub(super) fn run_container_prune_command(
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let barge = open_host_barge()?;
    run_container_prune_command_with(barge.as_ref(), &RealLeaseRuntime, profile, output_json)
}

/// Injectable variant of [`run_container_prune_command`].
pub(in crate::runner) fn run_container_prune_command_with(
    barge: Option<&Barge>,
    runtime: &dyn LeaseRuntime,
    profile: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = match profile {
        Some(target) => sweep_profile_leases_with(barge, runtime, target)?,
        None => sweep_all_running_profiles_with(barge, runtime)?,
    };

    let ok = report.ok();
    let payload = json!({
        "schema": CONTAINER_PRUNE_SCHEMA,
        "schema_version": 1,
        "ok": ok,
        "profile": profile,
        "barge_managed": report.barge_managed,
        "profiles_swept": report.profiles_swept,
        "stopped_profiles": report.stopped_profiles,
        "returned_leases": report.returned_leases,
        "orphans_torn_down": report.orphans_torn_down,
        "skipped_orphans": report.skipped_orphans,
        "retained_failures": report.retained_failures,
    });

    if ok {
        return Ok(if output_json {
            payload.to_string()
        } else {
            render_prune_human(&report, profile)
        });
    }

    let human = report.retained_failures.iter().fold(
        HumanError::new("container lease prune did not complete cleanly"),
        |human, failure| human.evidence(&failure.error),
    );
    if output_json {
        Err(RunnerError::structured_json(payload, false, human, None))
    } else {
        Err(RunnerError::task_invocation(render_prune_human(
            &report, profile,
        )))
    }
}

fn render_prune_human(report: &SweepReport, profile: Option<&str>) -> String {
    let scope = match profile {
        Some(name) => format!("profile `{name}`"),
        None if report.profiles_swept.is_empty() => {
            "all running Colima profiles (none running)".to_owned()
        }
        None => format!("running profiles ({})", report.profiles_swept.join(", ")),
    };
    let mut lines = Vec::new();
    if report.ok() {
        lines.push(format!(
            "[ok] pruned {scope}: returned {} ended lease(s), tore down {} orphan stack(s)",
            report.returned_leases.len(),
            report.orphans_torn_down.len(),
        ));
    } else {
        lines.push(format!(
            "[error] prune on {scope}: returned {} ended lease(s), tore down {} orphan stack(s), retained {} failure(s)",
            report.returned_leases.len(),
            report.orphans_torn_down.len(),
            report.retained_failures.len(),
        ));
    }
    for lease in &report.returned_leases {
        lines.push(format!(
            "  returned lease {} ({} / {}, {}, remove_volumes={})",
            lease.id, lease.profile, lease.project, lease.reason, lease.remove_volumes
        ));
    }
    for orphan in &report.orphans_torn_down {
        lines.push(format!(
            "  tore down orphan {} ({} / {}, containers={}, remove_volumes={})",
            orphan.lease_id,
            orphan.profile,
            orphan.project,
            orphan.containers.join(","),
            orphan.remove_volumes
        ));
    }
    for skipped in &report.skipped_orphans {
        lines.push(format!(
            "  skipped orphan {} ({} / {}, containers={}): {}",
            skipped.lease_id,
            skipped.profile,
            skipped.project,
            skipped.containers.join(","),
            skipped.reason
        ));
    }
    for failure in &report.retained_failures {
        let target = if failure.project.is_empty() {
            failure.profile.clone()
        } else {
            format!("{} / {}", failure.profile, failure.project)
        };
        let lease_label = if failure.lease_id.is_empty() {
            String::new()
        } else {
            format!(" [lease {}]", failure.lease_id)
        };
        lines.push(format!(
            "  retained ({}) {}{}: {}",
            failure.kind, target, lease_label, failure.error
        ));
    }
    if !report.stopped_profiles.is_empty() && report.retained_failures.is_empty() {
        lines.push(format!(
            "  stopped profile(s) skipped: {}",
            report.stopped_profiles.join(", ")
        ));
    }
    lines.join("\n")
}

fn barge_error(error: barge::Error) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use barge::{FakeOs, ManualClock};
    use effigy_containers::{FailedResource, OwnedResource, ResourceKind};
    use std::sync::{Arc, Mutex};

    struct BargeFixture {
        _temp: tempfile::TempDir,
        state_dir: PathBuf,
        barge: Barge,
    }

    impl BargeFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("tempdir");
            let state_dir = temp.path().join("barge-state");
            let os = Arc::new(FakeOs::new());
            let clock = Arc::new(ManualClock::new(1_700_000_000_000));
            let barge =
                Barge::open_with(&state_dir, os, clock).expect("open injected barge instance");
            Self {
                _temp: temp,
                state_dir,
                barge,
            }
        }

        fn declare_profiles(&self, profiles: &[(&str, u32)]) {
            let entries: Vec<_> = profiles
                .iter()
                .map(|(name, max)| json!({ "name": name, "max": max }))
                .collect();
            std::fs::write(
                self.state_dir.join("container-profiles.json"),
                json!({ "profiles": entries }).to_string(),
            )
            .expect("write container-profiles.json");
        }
    }

    #[derive(Clone)]
    struct TeardownCall {
        profile: String,
        project: String,
        remove_volumes: bool,
    }

    struct FakeLeaseRuntime {
        running: Mutex<Vec<String>>,
        containers: Mutex<BTreeMap<String, Vec<ContainerRow>>>,
        teardown_calls: Mutex<Vec<TeardownCall>>,
        teardown_results: Mutex<BTreeMap<(String, String), Result<RetireOutcome, String>>>,
    }

    impl FakeLeaseRuntime {
        fn new(running: &[&str]) -> Self {
            Self {
                running: Mutex::new(running.iter().map(|s| (*s).to_owned()).collect()),
                containers: Mutex::new(BTreeMap::new()),
                teardown_calls: Mutex::new(Vec::new()),
                teardown_results: Mutex::new(BTreeMap::new()),
            }
        }

        fn set_containers(&self, profile: &str, rows: Vec<ContainerRow>) {
            self.containers
                .lock()
                .expect("containers lock")
                .insert(profile.to_owned(), rows);
        }

        fn set_teardown_result(
            &self,
            profile: &str,
            project: &str,
            result: Result<RetireOutcome, String>,
        ) {
            self.teardown_results
                .lock()
                .expect("teardown results lock")
                .insert((profile.to_owned(), project.to_owned()), result);
        }

        fn teardown_calls(&self) -> Vec<TeardownCall> {
            self.teardown_calls
                .lock()
                .expect("teardown calls lock")
                .clone()
        }
    }

    impl LeaseRuntime for FakeLeaseRuntime {
        fn running_profiles(&self) -> Result<Vec<String>, RunnerError> {
            Ok(self.running.lock().expect("running lock").clone())
        }

        fn list_containers(&self, profile: &str) -> Result<Vec<ContainerRow>, RunnerError> {
            Ok(self
                .containers
                .lock()
                .expect("containers lock")
                .get(profile)
                .map(|rows| {
                    rows.iter()
                        .map(|row| ContainerRow {
                            name: row.name.clone(),
                            project: row.project.clone(),
                            effigy_project: row.effigy_project.clone(),
                            checkout: row.checkout.clone(),
                            repo_root: row.repo_root.clone(),
                            scope: row.scope.clone(),
                            managed: row.managed,
                            barge_lease: row.barge_lease.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default())
        }

        fn teardown_project(
            &self,
            profile: &str,
            project: &str,
            remove_volumes: bool,
        ) -> Result<RetireOutcome, RunnerError> {
            self.teardown_calls
                .lock()
                .expect("teardown calls lock")
                .push(TeardownCall {
                    profile: profile.to_owned(),
                    project: project.to_owned(),
                    remove_volumes,
                });
            let configured = self
                .teardown_results
                .lock()
                .expect("teardown results lock")
                .get(&(profile.to_owned(), project.to_owned()))
                .cloned();
            match configured {
                Some(Ok(outcome)) => Ok(outcome),
                Some(Err(message)) => Err(RunnerError::task_invocation(message)),
                None => Ok(RetireOutcome::default()),
            }
        }
    }

    fn worker_request(
        profile: &str,
        project: &str,
        checkout: &Path,
        hold: bool,
    ) -> ContainerLeaseRequest {
        let canonical = std::fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf());
        ContainerLeaseRequest {
            profile: profile.to_owned(),
            project: project.to_owned(),
            owner: ContainerOwner::Checkout(canonical),
            class: ContainerClass::Worker,
            hold,
            wait: None,
        }
    }

    fn ok_outcome(profile: &str, name: &str) -> RetireOutcome {
        RetireOutcome {
            removed: vec![OwnedResource {
                kind: ResourceKind::Container,
                name: name.to_owned(),
                profile: profile.to_owned(),
                persist: false,
            }],
            ..RetireOutcome::default()
        }
    }

    #[test]
    fn not_managed_passes_through_when_profiles_missing_or_profile_undeclared() {
        let fixture = BargeFixture::new();
        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        let checkout = fixture._temp.path().join("repo");
        std::fs::create_dir_all(&checkout).expect("mkdir checkout");
        let req = worker_request("effigy", "demo-web", &checkout, false);

        // Missing container-profiles.json -> NotManaged.
        let outcome = acquire_container_lease_with(
            Some(&fixture.barge),
            &runtime,
            &req,
            Duration::from_secs(1),
            Duration::from_millis(25),
        )
        .expect("missing container-profiles.json passes through");
        assert!(matches!(outcome, LeaseAcquireOutcome::NotManaged));

        // Undeclared profile -> NotManaged.
        fixture.declare_profiles(&[("effigy-release", 1)]);
        let outcome = acquire_container_lease_with(
            Some(&fixture.barge),
            &runtime,
            &req,
            Duration::from_secs(1),
            Duration::from_millis(25),
        )
        .expect("undeclared profile passes through");
        assert!(matches!(outcome, LeaseAcquireOutcome::NotManaged));

        // Absent host Barge account -> NotManaged.
        let outcome = acquire_container_lease_with(
            None,
            &runtime,
            &req,
            Duration::from_secs(1),
            Duration::from_millis(25),
        )
        .expect("absent barge passes through");
        assert!(matches!(outcome, LeaseAcquireOutcome::NotManaged));
    }

    #[test]
    fn sweep_handles_owner_ended_and_hold_released_with_exact_volume_policy() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);

        let gone_checkout = fixture._temp.path().join("deleted-wt");
        std::fs::create_dir_all(&gone_checkout).expect("mkdir gone");
        let live_checkout = fixture._temp.path().join("live-wt");
        std::fs::create_dir_all(&live_checkout).expect("mkdir live");

        // 1. OwnerEnded -> remove_volumes: true
        let owner_ended_req = worker_request("effigy", "proj-gone", &gone_checkout, false);
        let owner_lease = match fixture
            .barge
            .container_lease(&owner_ended_req)
            .expect("lease owner_ended")
        {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected lease for proj-gone"),
        };
        let owner_lease_id = owner_lease.id().to_owned();
        drop(owner_lease);
        std::fs::remove_dir_all(&gone_checkout).expect("remove gone checkout");

        // 2. HoldReleased -> remove_volumes: false
        let hold_req = worker_request("effigy", "proj-held", &live_checkout, true);
        let held_lease = match fixture
            .barge
            .container_lease(&hold_req)
            .expect("lease held")
        {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected lease for proj-held"),
        };
        let held_lease_id = held_lease.id().to_owned();
        drop(held_lease);

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_teardown_result(
            "effigy",
            "proj-gone",
            Ok(ok_outcome("effigy", "proj-gone-app-1")),
        );
        runtime.set_teardown_result(
            "effigy",
            "proj-held",
            Ok(ok_outcome("effigy", "proj-held-app-1")),
        );

        let report =
            sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy").expect("sweep ok");
        assert!(report.ok());
        assert_eq!(report.returned_leases.len(), 2);

        let calls = runtime.teardown_calls();
        assert_eq!(calls.len(), 2);
        let gone_call = calls
            .iter()
            .find(|call| call.project == "proj-gone")
            .expect("proj-gone teardown");
        assert_eq!(gone_call.profile, "effigy");
        assert!(gone_call.remove_volumes);
        let held_call = calls
            .iter()
            .find(|call| call.project == "proj-held")
            .expect("proj-held teardown");
        assert_eq!(held_call.profile, "effigy");
        assert!(!held_call.remove_volumes);

        assert!(fixture.barge.container_status().expect("status").is_empty());
        assert!(report
            .returned_leases
            .iter()
            .any(|entry| entry.id == owner_lease_id && entry.reason == "owner_ended"));
        assert!(report
            .returned_leases
            .iter()
            .any(|entry| entry.id == held_lease_id && entry.reason == "hold_released"));
    }

    #[test]
    fn sweep_retains_lease_when_teardown_fails() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);
        let checkout = fixture._temp.path().join("wt-fail");
        std::fs::create_dir_all(&checkout).expect("mkdir");

        let req = worker_request("effigy", "proj-fail", &checkout, true);
        let lease = match fixture.barge.container_lease(&req).expect("lease") {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected lease"),
        };
        let lease_id = lease.id().to_owned();
        drop(lease);

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        let partial_failure = RetireOutcome {
            failed: vec![FailedResource {
                resource: OwnedResource {
                    kind: ResourceKind::Container,
                    name: "proj-fail-app-1".to_owned(),
                    profile: "effigy".to_owned(),
                    persist: false,
                },
                error: "nerdctl rm timed out".to_owned(),
            }],
            ..RetireOutcome::default()
        };
        runtime.set_teardown_result("effigy", "proj-fail", Ok(partial_failure));

        let report =
            sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy").expect("sweep");
        assert!(!report.ok());
        assert!(report.returned_leases.is_empty());
        assert_eq!(report.retained_failures.len(), 1);
        assert_eq!(report.retained_failures[0].lease_id, lease_id);

        // Lease must still exist in Barge for retry.
        let remaining = fixture.barge.container_status().expect("status");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, lease_id);

        // Retry after fixing teardown succeeds and returns the lease.
        runtime.set_teardown_result(
            "effigy",
            "proj-fail",
            Ok(ok_outcome("effigy", "proj-fail-app-1")),
        );
        let retry_report =
            sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy").expect("retry");
        assert!(retry_report.ok());
        assert_eq!(retry_report.returned_leases.len(), 1);
        assert!(fixture.barge.container_status().expect("status").is_empty());
    }

    #[test]
    fn sweep_tears_down_orphan_barge_lease_containers() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);

        let gone_checkout = fixture._temp.path().join("orphan-gone");
        let live_checkout = fixture._temp.path().join("orphan-live");
        std::fs::create_dir_all(&live_checkout).expect("mkdir live");

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_containers(
            "effigy",
            vec![
                ContainerRow {
                    name: "orphan-gone-app-1".to_owned(),
                    project: Some("orphan-gone-proj".to_owned()),
                    effigy_project: Some("orphan-gone-proj".to_owned()),
                    checkout: Some(gone_checkout.display().to_string()),
                    repo_root: None,
                    scope: None,
                    managed: true,
                    barge_lease: Some("dead-lease-1".to_owned()),
                },
                ContainerRow {
                    name: "orphan-live-app-1".to_owned(),
                    project: Some("orphan-live-proj".to_owned()),
                    effigy_project: Some("orphan-live-proj".to_owned()),
                    checkout: Some(live_checkout.display().to_string()),
                    repo_root: None,
                    scope: None,
                    managed: true,
                    barge_lease: Some("dead-lease-2".to_owned()),
                },
            ],
        );

        let report =
            sweep_all_running_profiles_with(Some(&fixture.barge), &runtime).expect("global sweep");
        assert!(report.ok());
        assert_eq!(report.orphans_torn_down.len(), 2);

        let calls = runtime.teardown_calls();
        let gone_call = calls
            .iter()
            .find(|c| c.project == "orphan-gone-proj")
            .expect("gone orphan teardown");
        assert!(gone_call.remove_volumes);
        let live_call = calls
            .iter()
            .find(|c| c.project == "orphan-live-proj")
            .expect("live orphan teardown");
        assert!(!live_call.remove_volumes);
    }

    #[test]
    fn full_wait_loop_sweeps_newly_ended_lease_and_times_out_cleanly() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 1)]);
        let checkout = fixture._temp.path().join("wt-wait");
        std::fs::create_dir_all(&checkout).expect("mkdir");

        let first_req = worker_request("effigy", "proj-first", &checkout, true);
        let first_lease = match fixture
            .barge
            .container_lease(&first_req)
            .expect("first lease")
        {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected first lease"),
        };

        let second_req = worker_request("effigy", "proj-second", &checkout, false);
        let runtime = FakeLeaseRuntime::new(&["effigy"]);

        // 1. Timeout when first_lease stays held.
        let timeout_err = acquire_container_lease_with(
            Some(&fixture.barge),
            &runtime,
            &second_req,
            Duration::from_millis(60),
            Duration::from_millis(20),
        )
        .expect_err("full pool times out cleanly");
        let msg = timeout_err.to_string();
        assert!(msg.contains("effigy"), "missing profile: {msg}");
        assert!(msg.contains("1/1"), "missing slots: {msg}");
        assert!(
            msg.contains("effigy container prune"),
            "missing prune hint: {msg}"
        );

        // 2. While waiting, first_lease drops its hold; sweep reclaims it and second_req succeeds.
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(45));
                drop(first_lease);
            });
            let acquired = acquire_container_lease_with(
                Some(&fixture.barge),
                &runtime,
                &second_req,
                Duration::from_secs(2),
                Duration::from_millis(30),
            )
            .expect("wait loop reclaims newly ended lease");
            assert!(matches!(
                acquired,
                LeaseAcquireOutcome::Leased(ref lease) if lease.project() == "proj-second"
            ));
        });
    }

    #[test]
    fn return_lease_for_project_returns_matching_lease_idempotently() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 2)]);
        let checkout = fixture._temp.path().join("wt-return");
        std::fs::create_dir_all(&checkout).expect("mkdir");

        let req = worker_request("effigy", "proj-return", &checkout, false);
        let _lease = match fixture.barge.container_lease(&req).expect("lease") {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected lease"),
        };

        assert!(
            return_lease_for_project_with(Some(&fixture.barge), "effigy", "proj-return")
                .expect("first return")
        );
        assert!(
            !return_lease_for_project_with(Some(&fixture.barge), "effigy", "proj-return")
                .expect("second return is idempotent no-op")
        );
        assert!(
            !return_lease_for_project_with(None, "effigy", "proj-return")
                .expect("unmanaged is no-op")
        );
    }

    #[test]
    fn class_resolution_distinguishes_interactive_and_worker() {
        assert_eq!(
            resolve_container_class(ContainerClassContext {
                profile: "effigy",
                is_worktree: false,
                is_ephemeral: false,
                is_ci: false,
                is_detached_worker: false,
                is_release_builder: false,
            }),
            ContainerClass::Interactive
        );
        for ctx in [
            ContainerClassContext {
                profile: "effigy",
                is_worktree: true,
                is_ephemeral: false,
                is_ci: false,
                is_detached_worker: false,
                is_release_builder: false,
            },
            ContainerClassContext {
                profile: "effigy",
                is_worktree: false,
                is_ephemeral: true,
                is_ci: false,
                is_detached_worker: false,
                is_release_builder: false,
            },
            ContainerClassContext {
                profile: "effigy",
                is_worktree: false,
                is_ephemeral: false,
                is_ci: true,
                is_detached_worker: false,
                is_release_builder: false,
            },
            ContainerClassContext {
                profile: "effigy",
                is_worktree: false,
                is_ephemeral: false,
                is_ci: false,
                is_detached_worker: true,
                is_release_builder: false,
            },
            ContainerClassContext {
                profile: "effigy-release",
                is_worktree: false,
                is_ephemeral: false,
                is_ci: false,
                is_detached_worker: false,
                is_release_builder: false,
            },
        ] {
            assert_eq!(resolve_container_class(ctx), ContainerClass::Worker);
        }
    }

    #[test]
    fn prune_command_renders_stable_json_schema() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);
        let runtime = FakeLeaseRuntime::new(&["effigy"]);

        let rendered =
            run_container_prune_command_with(Some(&fixture.barge), &runtime, Some("effigy"), true)
                .expect("prune json");
        let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(parsed["schema"], "effigy.container.prune.v1");
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["profile"], "effigy");
        assert_eq!(parsed["profiles_swept"], json!(["effigy"]));
    }

    #[test]
    fn resolve_container_owner_matches_worktree_lease_and_falls_back_to_checkout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state_dir = temp.path().join("barge-state");
        let mount = temp.path().join("builds-vol");
        std::fs::create_dir_all(&mount).expect("mkdir mount");
        let os = Arc::new(FakeOs::with_volume(
            &mount,
            "11111111-1111-1111-1111-111111111111",
            100 * 1024 * 1024 * 1024,
        ));
        let clock = Arc::new(ManualClock::new(1_700_000_000_000));
        let barge = Barge::open_with(&state_dir, os, clock).expect("open barge");

        barge
            .register_volume(
                barge::VolumeRegistration::new("Builds", mount.clone())
                    .uuid(Some("11111111-1111-1111-1111-111111111111"))
                    .reserve_bytes(0)
                    .floor_bytes(0),
            )
            .expect("register volume");
        barge
            .set_layout(
                barge::Class::Worktrees,
                barge::Placement {
                    volume: "Builds".to_owned(),
                    path: "worktrees".to_owned(),
                    absent: barge::Absent::Fail,
                },
            )
            .expect("set layout");

        let origin = temp.path().join("origin");
        std::fs::create_dir_all(&origin).expect("mkdir origin");
        let run_git = |args: &[&str]| -> String {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&origin)
                .args(args)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .expect("run git");
            assert!(output.status.success(), "git failed");
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        run_git(&["init", "--quiet", "-b", "main"]);
        std::fs::write(origin.join("README.md"), "ok\n").expect("write readme");
        run_git(&["add", "-A"]);
        run_git(&["commit", "--quiet", "-m", "base"]);
        let base = run_git(&["rev-parse", "HEAD"]);

        let canonical_origin = std::fs::canonicalize(&origin).expect("canonical origin");
        let repo_id = canonical_origin.to_string_lossy().to_ascii_lowercase();
        std::fs::write(
            state_dir.join("worktree-pools.json"),
            json!({
                "schema": barge::worktree::POOLS_SCHEMA,
                "pools": [{
                    "repo": repo_id,
                    "origin": canonical_origin.to_string_lossy(),
                    "warm": 1,
                    "max": 2
                }]
            })
            .to_string(),
        )
        .expect("write pools");

        let wt_lease = barge
            .worktree_lease(
                barge::worktree::LeaseRequest {
                    repo: canonical_origin.to_string_lossy().into_owned(),
                    base,
                    base_name: Some("main".to_owned()),
                    branch: "task/owner-test".to_owned(),
                    request: "req-owner-1".to_owned(),
                    label: None,
                },
                Duration::ZERO,
            )
            .expect("lease worktree");

        let wt_owner =
            resolve_container_owner(&barge, Path::new(&wt_lease.path)).expect("resolve wt owner");
        assert_eq!(
            wt_owner,
            ContainerOwner::Worktree(wt_lease.lease_id.clone())
        );

        let plain_checkout = temp.path().join("plain-checkout");
        std::fs::create_dir_all(&plain_checkout).expect("mkdir plain");
        let plain_owner =
            resolve_container_owner(&barge, &plain_checkout).expect("resolve plain owner");
        assert_eq!(
            plain_owner,
            ContainerOwner::Checkout(std::fs::canonicalize(&plain_checkout).expect("canonical"))
        );
    }

    #[test]
    fn sweep_skips_orphan_when_live_lease_holds_same_profile_and_project() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);
        let checkout = fixture._temp.path().join("shared-checkout");
        std::fs::create_dir_all(&checkout).expect("mkdir checkout");

        let live_req = worker_request("effigy", "proj-shared", &checkout, true);
        let live_lease = match fixture
            .barge
            .container_lease(&live_req)
            .expect("lease live")
        {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected live lease"),
        };
        let live_id = live_lease.id().to_owned();

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_containers(
            "effigy",
            vec![
                ContainerRow {
                    name: "proj-shared-stale-1".to_owned(),
                    project: Some("proj-shared".to_owned()),
                    effigy_project: Some("proj-shared".to_owned()),
                    checkout: Some(checkout.display().to_string()),
                    repo_root: None,
                    scope: None,
                    managed: true,
                    barge_lease: Some("stale-returned-lease".to_owned()),
                },
                ContainerRow {
                    name: "proj-shared-live-1".to_owned(),
                    project: Some("proj-shared".to_owned()),
                    effigy_project: Some("proj-shared".to_owned()),
                    checkout: Some(checkout.display().to_string()),
                    repo_root: None,
                    scope: None,
                    managed: true,
                    barge_lease: Some(live_id.clone()),
                },
            ],
        );

        let report =
            sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy").expect("sweep");
        assert!(report.ok());
        assert!(runtime.teardown_calls().is_empty());
        assert!(report.orphans_torn_down.is_empty());
        assert_eq!(report.skipped_orphans.len(), 1);
        assert_eq!(report.skipped_orphans[0].lease_id, "stale-returned-lease");
        assert_eq!(report.skipped_orphans[0].held_by_lease_id, live_id);
        assert_eq!(report.skipped_orphans[0].project, "proj-shared");

        let human = render_prune_human(&report, Some("effigy"));
        assert!(human.contains("skipped orphan"), "{human}");
        assert!(human.contains("proj-shared"), "{human}");

        drop(live_lease);
    }

    #[test]
    fn sweep_is_a_no_op_without_barge_and_does_not_tear_down_orphans() {
        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_containers(
            "effigy",
            vec![ContainerRow {
                name: "some-app-1".to_owned(),
                project: Some("some-proj".to_owned()),
                effigy_project: Some("some-proj".to_owned()),
                checkout: Some("/tmp/nonexistent-owner".to_owned()),
                repo_root: None,
                scope: None,
                managed: true,
                barge_lease: Some("foreign-lease-id".to_owned()),
            }],
        );

        let profile_report =
            sweep_profile_leases_with(None, &runtime, "effigy").expect("profile sweep");
        assert!(!profile_report.barge_managed);
        assert!(profile_report.ok());
        assert!(profile_report.orphans_torn_down.is_empty());
        assert!(runtime.teardown_calls().is_empty());

        let global_report = sweep_all_running_profiles_with(None, &runtime).expect("global sweep");
        assert!(!global_report.barge_managed);
        assert!(global_report.ok());
        assert!(global_report.orphans_torn_down.is_empty());
        assert!(runtime.teardown_calls().is_empty());
    }

    #[test]
    fn open_host_barge_and_sweep_surface_unknown_state_and_io_errors() {
        assert!(
            classify_open_barge_result(Err(barge::Error::not_found("missing")))
                .expect("not_found is unmanaged")
                .is_none()
        );
        assert!(
            classify_open_barge_result(Err(barge::Error::unsupported("unsupported")))
                .expect("unsupported is unmanaged")
                .is_none()
        );
        assert!(
            classify_open_barge_result(Err(barge::Error::unknown_state("corrupt state"))).is_err()
        );
        assert!(
            classify_open_barge_result(Err(barge::Error::io_message("permission denied"))).is_err()
        );

        let fixture = BargeFixture::new();
        let leases_dir = fixture.state_dir.join("container-leases");
        std::fs::create_dir_all(&leases_dir).expect("mkdir leases");
        std::fs::write(leases_dir.join("corrupt.json"), "{not valid json").expect("write corrupt");

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_containers(
            "effigy",
            vec![ContainerRow {
                name: "orphan-app-1".to_owned(),
                project: Some("orphan-proj".to_owned()),
                effigy_project: Some("orphan-proj".to_owned()),
                checkout: Some("/tmp/some-checkout".to_owned()),
                repo_root: None,
                scope: None,
                managed: true,
                barge_lease: Some("any-lease".to_owned()),
            }],
        );

        let err = sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy")
            .expect_err("corrupt barge state must error");
        assert!(!err.to_string().is_empty());
        assert!(
            runtime.teardown_calls().is_empty(),
            "unreadable barge state must never trigger orphan teardown"
        );
    }

    #[test]
    fn missing_parent_or_unmounted_volume_keeps_volumes_on_orphan_teardown() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 4)]);

        let missing_parent_checkout = fixture
            ._temp
            .path()
            .join("missing-parent")
            .join("wt-1")
            .join("checkout");
        let unmounted_vol_checkout = Path::new("/Volumes/UnmountedDrive/worktrees/wt-1/checkout");
        let mount_root = Path::new("/Volumes/UnmountedDrive");

        assert!(!is_checkout_confirmed_gone(Some(
            &missing_parent_checkout.display().to_string()
        )));
        assert!(!is_checkout_confirmed_gone(Some(
            &unmounted_vol_checkout.display().to_string()
        )));
        assert!(!is_checkout_confirmed_gone(Some(
            &mount_root.display().to_string()
        )));

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_containers(
            "effigy",
            vec![ContainerRow {
                name: "unmounted-app-1".to_owned(),
                project: Some("unmounted-proj".to_owned()),
                effigy_project: Some("unmounted-proj".to_owned()),
                checkout: Some(missing_parent_checkout.display().to_string()),
                repo_root: None,
                scope: None,
                managed: true,
                barge_lease: Some("dead-lease-missing-parent".to_owned()),
            }],
        );

        let report =
            sweep_profile_leases_with(Some(&fixture.barge), &runtime, "effigy").expect("sweep");
        assert!(report.ok());
        assert_eq!(report.orphans_torn_down.len(), 1);
        let calls = runtime.teardown_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].project, "unmounted-proj");
        assert!(!calls[0].remove_volumes, "missing parent must keep volumes");
    }

    #[test]
    fn acquire_timeout_refusal_surfaces_sweep_failures() {
        let fixture = BargeFixture::new();
        fixture.declare_profiles(&[("effigy", 1)]);
        let checkout = fixture._temp.path().join("wt-sweep-fail");
        std::fs::create_dir_all(&checkout).expect("mkdir");

        // Occupy slot 1 with an ended lease whose teardown fails.
        let ended_req = worker_request("effigy", "proj-stuck", &checkout, true);
        let ended_lease = match fixture.barge.container_lease(&ended_req).expect("lease") {
            ContainerLeaseOutcome::Leased(lease) => lease,
            _ => panic!("expected lease"),
        };
        drop(ended_lease);

        let runtime = FakeLeaseRuntime::new(&["effigy"]);
        runtime.set_teardown_result(
            "effigy",
            "proj-stuck",
            Err("colima nerdctl rm timed out".to_owned()),
        );

        let second_req = worker_request("effigy", "proj-next", &checkout, false);
        let err = acquire_container_lease_with(
            Some(&fixture.barge),
            &runtime,
            &second_req,
            Duration::from_millis(40),
            Duration::from_millis(20),
        )
        .expect_err("full pool times out");
        let msg = err.to_string();
        assert!(msg.contains("effigy/proj-stuck"), "{msg}");
        assert!(msg.contains("colima nerdctl rm timed out"), "{msg}");
        assert!(msg.contains("effigy container prune"), "{msg}");
    }
}
