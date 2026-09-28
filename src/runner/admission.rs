use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u8 = 1;
const HISTORY_LIMIT: usize = 512;
const DEFAULT_WAIT_SECS: u64 = 30 * 60;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const RSS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const LEASE_ENV: &str = "EFFIGY_ADMISSION_LEASE_ID";
static UNIQUE: AtomicU64 = AtomicU64::new(1);
static BOOT_IDENTITY: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Budget {
    pub(super) cpu_units: u32,
    pub(super) memory_mib: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Reservation {
    pub(super) cpu_units: u32,
    pub(super) memory_mib: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunRecord {
    run_id: String,
    lease_id: String,
    caller: String,
    repository: String,
    fairness_key: String,
    selector: String,
    state: String,
    ticket: u64,
    position: Option<usize>,
    queued_at: String,
    deadline_at: String,
    admitted_at: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    queue_wait_ms: Option<u128>,
    wall_ms: Option<u128>,
    cpu_user_ms: Option<u128>,
    cpu_system_ms: Option<u128>,
    peak_rss_bytes: Option<u64>,
    cpu_overrun: Option<bool>,
    memory_overrun: Option<bool>,
    owner_pid: u32,
    owner_start_identity: Option<String>,
    #[serde(default)]
    owner_process_group: Option<i32>,
    boot_identity: Option<String>,
    process_groups: Vec<i32>,
    budget: Budget,
    reservation: Reservation,
    exit_classification: Option<String>,
    log_reference: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Store {
    schema: String,
    schema_version: u8,
    budget: Budget,
    next_ticket: u64,
    scheduler_after: String,
    runs: Vec<RunRecord>,
}

#[derive(Debug, Serialize)]
struct StatusPayload<'a> {
    schema: &'static str,
    schema_version: u8,
    budget: &'a Budget,
    queued: Vec<QueuedStatus<'a>>,
    leases: Vec<RunRecord>,
}

#[derive(Debug, Serialize)]
struct QueuedStatus<'a> {
    run_id: &'a str,
    caller: &'a str,
    repository: &'a str,
    fairness_key: &'a str,
    selector: &'a str,
    state: &'a str,
    position: Option<usize>,
    queued_at: &'a str,
    deadline_at: &'a str,
    budget: &'a Budget,
    reservation: &'a Reservation,
}

#[derive(Debug, Serialize)]
struct RunsPayload<'a> {
    schema: &'static str,
    schema_version: u8,
    caller: &'a str,
    offset: usize,
    limit: usize,
    total: usize,
    runs: &'a [RunRecord],
}

pub(super) struct Request<'a> {
    pub(super) caller: &'a str,
    pub(super) repository: &'a Path,
    pub(super) selector: &'a str,
}

pub(super) fn default_caller_identity() -> String {
    #[cfg(unix)]
    {
        format!("interactive:{}", unsafe { libc::geteuid() })
    }
    #[cfg(not(unix))]
    {
        format!(
            "interactive:{}",
            std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_owned())
        )
    }
}

pub(super) struct AdmissionLease {
    root: PathBuf,
    run_id: String,
    lease_id: String,
    start: Instant,
    cpu_before: Option<(u128, u128)>,
    completed: bool,
}

pub(super) struct ProcessGroupRssMonitor {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ProcessGroupRssMonitor {
    pub(super) fn start(process_group: u32) -> Option<Self> {
        scoped_lease_id()?;
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let stop_thread = stop.clone();
        let peak_thread = peak.clone();
        let thread = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                if let Some(bytes) = process_group_rss_bytes(process_group as i32) {
                    peak_thread.fetch_max(bytes, Ordering::Relaxed);
                }
                std::thread::sleep(RSS_POLL_INTERVAL);
            }
            if let Some(bytes) = process_group_rss_bytes(process_group as i32) {
                peak_thread.fetch_max(bytes, Ordering::Relaxed);
            }
        });
        Some(Self {
            stop,
            peak,
            thread: Some(thread),
        })
    }

    pub(super) fn finish(mut self) -> Option<u64> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let peak = self.peak.load(Ordering::Relaxed);
        (peak > 0).then_some(peak)
    }
}

impl Drop for ProcessGroupRssMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct WaiterGuard {
    root: PathBuf,
    run_id: String,
    budget: Budget,
    active: bool,
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = transact(&self.root, &self.budget, |store| {
            if let Some(run) = store
                .runs
                .iter_mut()
                .find(|run| run.run_id == self.run_id && run.state == "waiting_for_capacity")
            {
                run.state = "cancelled".to_owned();
                run.ended_at = Some(Utc::now().to_rfc3339());
                run.exit_classification = Some("waiter_cancelled".to_owned());
            }
            Ok(())
        });
    }
}

impl AdmissionLease {
    pub(super) fn id(&self) -> &str {
        &self.lease_id
    }

    pub(super) fn finish(mut self, result: &Result<String, super::error::RunnerError>) {
        let state = if result.is_ok() {
            "succeeded"
        } else {
            "failed"
        };
        if !self.completed {
            finish_record(
                &self.root,
                &self.run_id,
                state,
                state,
                self.start,
                self.cpu_before,
            );
        }
        self.completed = true;
    }
}

impl Drop for AdmissionLease {
    fn drop(&mut self) {
        if !self.completed {
            finish_record(
                &self.root,
                &self.run_id,
                "cancelled",
                "cancelled",
                self.start,
                self.cpu_before,
            );
        }
    }
}

pub(super) fn acquire(request: Request<'_>) -> Result<AdmissionLease, String> {
    let root = state_root()?;
    ensure_secure_root(&root)?;

    if let Some(inherited) = std::env::var_os(LEASE_ENV).and_then(|value| value.into_string().ok())
    {
        if inherited_lease_is_valid(&root, &inherited) {
            return Ok(AdmissionLease {
                root,
                run_id: inherited.clone(),
                lease_id: inherited,
                start: Instant::now(),
                cpu_before: child_cpu_snapshot(),
                completed: true,
            });
        }
    }
    if let Some((lease_id, _cpu_units)) = ancestor_lease(&root) {
        return Ok(AdmissionLease {
            root,
            run_id: lease_id.clone(),
            lease_id,
            start: Instant::now(),
            cpu_before: child_cpu_snapshot(),
            completed: true,
        });
    }

    let budget = host_budget()?;
    let reservation = requested_reservation(&budget)?;
    if reservation.cpu_units > budget.cpu_units || reservation.memory_mib > budget.memory_mib {
        return Err(format!(
            "heavy task reservation ({} CPU units, {} MiB) exceeds admission budget ({} CPU units, {} MiB); adjust EFFIGY_ADMISSION_* settings",
            reservation.cpu_units,
            reservation.memory_mib,
            budget.cpu_units,
            budget.memory_mib
        ));
    }

    let now = Utc::now();
    let deadline = now + chrono::Duration::seconds(wait_timeout_secs()? as i64);
    let id = unique_id();
    let caller = request.caller.to_owned();
    let (repository, fairness_key) = repository_and_fairness_key(request.repository, &caller);
    let mut record = RunRecord {
        run_id: id.clone(),
        lease_id: id.clone(),
        caller,
        repository,
        fairness_key,
        selector: request.selector.to_owned(),
        state: "waiting_for_capacity".to_owned(),
        ticket: 0,
        position: None,
        queued_at: now.to_rfc3339(),
        deadline_at: deadline.to_rfc3339(),
        admitted_at: None,
        started_at: None,
        ended_at: None,
        queue_wait_ms: None,
        wall_ms: None,
        cpu_user_ms: None,
        cpu_system_ms: None,
        peak_rss_bytes: None,
        cpu_overrun: None,
        memory_overrun: None,
        owner_pid: std::process::id(),
        owner_start_identity: process_start_identity(std::process::id()),
        owner_process_group: verified_owner_process_group(std::process::id()),
        boot_identity: boot_identity(),
        process_groups: Vec::new(),
        budget: budget.clone(),
        reservation: reservation.clone(),
        exit_classification: None,
        log_reference: None,
    };
    transact(&root, &budget, |store| {
        cleanup_stale(store);
        record.ticket = store.next_ticket;
        store.next_ticket += 1;
        store.runs.push(record);
        prune_history(store);
        Ok(())
    })?;
    let mut waiter_guard = WaiterGuard {
        root: root.clone(),
        run_id: id.clone(),
        budget: budget.clone(),
        active: true,
    };

    let start = Instant::now();
    let cpu_before = child_cpu_snapshot();
    let mut reported_position = None;
    loop {
        let (state, position) = transact(&root, &budget, |store| {
            cleanup_stale(store);
            schedule(store);
            let index = store
                .runs
                .iter()
                .position(|run| run.run_id == id)
                .ok_or_else(|| "admission queue record disappeared".to_owned())?;
            let state = store.runs[index].state.clone();
            let position = store.runs[index].position;
            if state == "running" {
                store.runs[index].started_at = Some(Utc::now().to_rfc3339());
                store.runs[index].queue_wait_ms = Some(start.elapsed().as_millis());
                store.runs[index].position = None;
            }
            Ok((state, position))
        })?;
        if state == "running" {
            waiter_guard.active = false;
            return Ok(AdmissionLease {
                root,
                run_id: id.clone(),
                lease_id: id,
                start: Instant::now(),
                cpu_before,
                completed: false,
            });
        }
        if Utc::now() >= deadline {
            transact(&root, &budget, |store| {
                if let Some(run) = store.runs.iter_mut().find(|run| run.run_id == id) {
                    run.state = "capacity_timeout".to_owned();
                    run.ended_at = Some(Utc::now().to_rfc3339());
                    run.queue_wait_ms = Some(start.elapsed().as_millis());
                    run.exit_classification = Some("capacity_timeout".to_owned());
                    run.position = None;
                }
                Ok(())
            })?;
            return Err(format!(
                "heavy task capacity wait expired after {} seconds (run {id}); this is a capacity timeout, not a validation failure",
                wait_timeout_secs()?
            ));
        }
        let position = position.unwrap_or(1);
        if reported_position != Some(position) {
            eprintln!("waiting for QA capacity, position {position}");
            reported_position = Some(position);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

pub(super) fn current_lease_id() -> Option<String> {
    CURRENT_LEASE
        .with(|value| value.borrow().clone())
        .or_else(|| std::env::var(LEASE_ENV).ok())
}

pub(super) fn scoped_lease_id() -> Option<String> {
    CURRENT_LEASE.with(|value| value.borrow().clone())
}

pub(super) fn scoped_cpu_units() -> Option<u32> {
    let lease_id = scoped_lease_id()?;
    let root = state_root().ok()?;
    let budget = host_budget().ok()?;
    with_store_read(&root, &budget)
        .ok()?
        .runs
        .into_iter()
        .find(|run| run.lease_id == lease_id && is_capacity_holder(&run.state))
        .map(|run| run.reservation.cpu_units)
        .or(Some(1))
}

thread_local! {
    static CURRENT_LEASE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub(super) struct LeaseScope {
    previous_lease: Option<String>,
    previous_observer: Option<effigy_process::ProcessGroupObserver>,
}

impl LeaseScope {
    pub(super) fn enter(id: &str) -> Self {
        let previous = CURRENT_LEASE.with(|value| value.replace(Some(id.to_owned())));
        let observer: Option<effigy_process::ProcessGroupObserver> =
            state_root().ok().map(|root| {
                let lease_id = id.to_owned();
                Arc::new(move |pid| register_process_group_for(&root, &lease_id, pid))
                    as effigy_process::ProcessGroupObserver
            });
        let previous_observer = effigy_process::replace_process_group_observer(observer);
        Self {
            previous_lease: previous,
            previous_observer,
        }
    }
}

impl Drop for LeaseScope {
    fn drop(&mut self) {
        effigy_process::replace_process_group_observer(self.previous_observer.take());
        CURRENT_LEASE.with(|value| {
            value.replace(self.previous_lease.take());
        });
    }
}

pub(super) fn register_process_group(pid: u32) {
    let Some(lease_id) = current_lease_id() else {
        return;
    };
    let Ok(root) = state_root() else { return };
    register_process_group_for(&root, &lease_id, pid);
}

fn register_process_group_for(root: &Path, lease_id: &str, pid: u32) {
    if ensure_secure_root(root).is_err() {
        return;
    }
    if let Ok(budget) = host_budget() {
        let _ = transact(root, &budget, |store| {
            if let Some(run) = store
                .runs
                .iter_mut()
                .find(|run| run.lease_id == lease_id && run.state == "running")
            {
                run.process_groups
                    .retain(|existing| process_group_is_live(*existing) != Some(false));
                #[cfg(unix)]
                let group = pid as i32;
                #[cfg(not(unix))]
                let group = pid as i32;
                if !run.process_groups.contains(&group) {
                    run.process_groups.push(group);
                }
            }
            Ok(())
        });
    }
}

pub(super) fn unregister_process_group(pid: u32) {
    if process_group_is_live(pid as i32) != Some(false) {
        return;
    }
    let Some(lease_id) = current_lease_id() else {
        return;
    };
    let Ok(root) = state_root() else { return };
    let Ok(budget) = host_budget() else { return };
    let _ = transact(&root, &budget, |store| {
        if let Some(run) = store
            .runs
            .iter_mut()
            .find(|run| run.lease_id == lease_id && is_capacity_holder(&run.state))
        {
            run.process_groups.retain(|group| *group != pid as i32);
        }
        Ok(())
    });
}

pub(super) fn record_process_group_peak(bytes: u64) {
    let Some(lease_id) = current_lease_id() else {
        return;
    };
    let Ok(root) = state_root() else { return };
    let Ok(budget) = host_budget() else { return };
    let _ = transact(&root, &budget, |store| {
        if let Some(run) = store
            .runs
            .iter_mut()
            .find(|run| run.lease_id == lease_id && is_capacity_holder(&run.state))
        {
            run.peak_rss_bytes = Some(run.peak_rss_bytes.unwrap_or_default().max(bytes));
        }
        Ok(())
    });
}

pub(super) fn status_json() -> Result<String, String> {
    let root = state_root()?;
    ensure_secure_root(&root)?;
    let budget = host_budget()?;
    let store = with_store_read(&root, &budget)?;
    let mut queued = store
        .runs
        .iter()
        .filter(|run| run.state == "waiting_for_capacity")
        .collect::<Vec<_>>();
    queued.sort_by_key(|run| run.ticket);
    let queued = queued
        .into_iter()
        .map(|run| QueuedStatus {
            run_id: &run.run_id,
            caller: &run.caller,
            repository: &run.repository,
            fairness_key: &run.fairness_key,
            selector: &run.selector,
            state: &run.state,
            position: run.position,
            queued_at: &run.queued_at,
            deadline_at: &run.deadline_at,
            budget: &run.budget,
            reservation: &run.reservation,
        })
        .collect();
    let leases = store
        .runs
        .iter()
        .filter(|run| is_capacity_holder(&run.state))
        .cloned()
        .map(|mut run| {
            if run.state == "running" && !owner_is_live(&run) {
                run.state = "stale_owner_unknown".to_owned();
            }
            run
        })
        .collect();
    serde_json::to_string_pretty(&StatusPayload {
        schema: "effigy.admission.status.v1",
        schema_version: SCHEMA_VERSION,
        budget: &store.budget,
        queued,
        leases,
    })
    .map_err(|error| format!("failed to render admission status: {error}"))
}

pub(super) fn run_json(run_id: &str) -> Result<Option<String>, String> {
    let root = state_root()?;
    ensure_secure_root(&root)?;
    run_json_at(&root, run_id)
}

fn run_json_at(root: &Path, run_id: &str) -> Result<Option<String>, String> {
    let budget = host_budget()?;
    let store = with_store_read(root, &budget)?;
    let Some(run) = store.runs.iter().find(|run| run.run_id == run_id) else {
        return Ok(None);
    };
    let mut run = run.clone();
    if run.state == "running" && !owner_is_live(&run) {
        run.state = "stale_owner_unknown".to_owned();
    }
    let mut value = serde_json::to_value(run).map_err(|error| error.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "schema".to_owned(),
            serde_json::json!("effigy.admission.run.v1"),
        );
        object.insert(
            "schema_version".to_owned(),
            serde_json::json!(SCHEMA_VERSION),
        );
    }
    serde_json::to_string_pretty(&value)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub(super) fn runs_json(caller: &str, offset: usize, limit: usize) -> Result<String, String> {
    let root = state_root()?;
    ensure_secure_root(&root)?;
    let budget = host_budget()?;
    let store = with_store_read(&root, &budget)?;
    let mut runs = store
        .runs
        .iter()
        .filter(|run| run.caller == caller)
        .cloned()
        .map(|mut run| {
            if run.state == "running" && !owner_is_live(&run) {
                run.state = "stale_owner_unknown".to_owned();
            }
            run
        })
        .collect::<Vec<_>>();
    runs.sort_by_key(|run| std::cmp::Reverse(run.ticket));
    let total = runs.len();
    let bounded_limit = limit.clamp(1, 100);
    let selected = runs
        .into_iter()
        .skip(offset)
        .take(bounded_limit)
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&RunsPayload {
        schema: "effigy.admission.runs.v1",
        schema_version: SCHEMA_VERSION,
        caller,
        offset,
        limit: bounded_limit,
        total,
        runs: &selected,
    })
    .map_err(|error| format!("failed to render admission runs: {error}"))
}

fn finish_record(
    root: &Path,
    run_id: &str,
    state: &str,
    classification: &str,
    start: Instant,
    cpu_before: Option<(u128, u128)>,
) {
    let Ok(budget) = host_budget() else { return };
    let _ = transact(root, &budget, |store| {
        if let Some(run) = store
            .runs
            .iter_mut()
            .find(|run| run.run_id == run_id && is_capacity_holder(&run.state))
        {
            run.ended_at = Some(Utc::now().to_rfc3339());
            let wall_ms = start.elapsed().as_millis();
            run.wall_ms = Some(wall_ms);
            run.exit_classification = Some(classification.to_owned());
            if let (Some(before), Some(after)) = (cpu_before, child_cpu_snapshot()) {
                run.cpu_user_ms = Some(after.0.saturating_sub(before.0));
                run.cpu_system_ms = Some(after.1.saturating_sub(before.1));
                let cpu_ms = run
                    .cpu_user_ms
                    .unwrap_or_default()
                    .saturating_add(run.cpu_system_ms.unwrap_or_default());
                run.cpu_overrun =
                    Some(cpu_ms > u128::from(run.reservation.cpu_units).saturating_mul(wall_ms));
            }
            if let Some(peak) = run.peak_rss_bytes {
                run.memory_overrun = Some(
                    u128::from(peak)
                        > u128::from(run.reservation.memory_mib).saturating_mul(1024 * 1024),
                );
            }
            run.process_groups
                .retain(|group| process_group_is_live(*group) != Some(false));
            run.state = if run.process_groups.is_empty() {
                state.to_owned()
            } else {
                "owner_exited_waiting_children".to_owned()
            };
        }
        Ok(())
    });
}

fn inherited_lease_is_valid(root: &Path, lease_id: &str) -> bool {
    let Ok(budget) = host_budget() else {
        return false;
    };
    let Ok(store) = with_store_read(root, &budget) else {
        return false;
    };
    let Some(run) = store
        .runs
        .iter()
        .find(|run| run.lease_id == lease_id && is_capacity_holder(&run.state))
    else {
        return false;
    };
    let pid = std::process::id();
    if pid == run.owner_pid {
        return true;
    }
    #[cfg(unix)]
    {
        let group = nix::unistd::getpgid(None).map(|value| value.as_raw()).ok();
        group.is_some_and(|value| {
            run.process_groups.contains(&value) || run.owner_process_group == Some(value)
        }) || effigy_process::process_is_descendant_of(pid, run.owner_pid)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn ancestor_lease(root: &Path) -> Option<(String, u32)> {
    let budget = host_budget().ok()?;
    let store = with_store_read(root, &budget).ok()?;
    let pid = std::process::id();
    #[cfg(unix)]
    let process_group = nix::unistd::getpgid(None).ok().map(|value| value.as_raw());
    #[cfg(not(unix))]
    let process_group: Option<i32> = None;

    store
        .runs
        .into_iter()
        .filter(|run| is_capacity_holder(&run.state))
        .filter(|run| {
            let is_owner = pid == run.owner_pid && run.state == "running";
            let is_descendant = run.state == "running"
                && owner_is_live(run)
                && effigy_process::process_is_descendant_of(pid, run.owner_pid);
            let is_registered_group = process_group.is_some_and(|group| {
                (run.process_groups.contains(&group) || run.owner_process_group == Some(group))
                    && matches!(
                        run.state.as_str(),
                        "running" | "owner_exited_waiting_children"
                    )
            });
            is_owner || is_descendant || is_registered_group
        })
        .max_by_key(|run| run.ticket)
        .map(|run| (run.lease_id, run.reservation.cpu_units))
}

fn schedule(store: &mut Store) {
    loop {
        let mut waiting = store
            .runs
            .iter()
            .filter(|run| run.state == "waiting_for_capacity")
            .collect::<Vec<_>>();
        waiting.sort_by_key(|run| run.ticket);
        if waiting.is_empty() {
            break;
        }
        let mut repo_heads = Vec::<String>::new();
        for run in &waiting {
            if !repo_heads.contains(&run.fairness_key) {
                repo_heads.push(run.fairness_key.clone());
            }
        }
        if !store.scheduler_after.is_empty() {
            let split = repo_heads
                .iter()
                .position(|key| key > &store.scheduler_after)
                .unwrap_or(0);
            repo_heads.rotate_left(split);
        }
        let active_cpu: u32 = store
            .runs
            .iter()
            .filter(|run| is_capacity_holder(&run.state))
            .map(|run| run.reservation.cpu_units)
            .sum();
        let active_memory: u64 = store
            .runs
            .iter()
            .filter(|run| is_capacity_holder(&run.state))
            .map(|run| run.reservation.memory_mib)
            .sum();
        let candidate = repo_heads.into_iter().find_map(|key| {
            let head = waiting.iter().find(|run| run.fairness_key == key)?;
            (active_cpu.saturating_add(head.reservation.cpu_units) <= store.budget.cpu_units
                && active_memory.saturating_add(head.reservation.memory_mib)
                    <= store.budget.memory_mib)
                .then_some((head.run_id.clone(), key))
        });
        let Some((run_id, key)) = candidate else {
            break;
        };
        store.scheduler_after = key;
        if let Some(run) = store.runs.iter_mut().find(|run| run.run_id == run_id) {
            run.state = "running".to_owned();
            run.admitted_at = Some(Utc::now().to_rfc3339());
            run.position = None;
        }
    }
    let mut queued = store
        .runs
        .iter_mut()
        .filter(|run| run.state == "waiting_for_capacity")
        .collect::<Vec<_>>();
    queued.sort_by_key(|run| run.ticket);
    for (index, run) in queued.into_iter().enumerate() {
        run.position = Some(index + 1);
    }
}

fn cleanup_stale(store: &mut Store) {
    for run in &mut store.runs {
        if run.state == "waiting_for_capacity" && !owner_is_live(run) {
            run.state = "cancelled".to_owned();
            run.ended_at = Some(Utc::now().to_rfc3339());
            run.exit_classification = Some("owner_disappeared_while_waiting".to_owned());
        } else if run.state == "owner_exited_waiting_children" {
            if run
                .process_groups
                .iter()
                .all(|group| process_group_is_live(*group) == Some(false))
            {
                run.state = run
                    .exit_classification
                    .clone()
                    .unwrap_or_else(|| "recovered_stale_owner".to_owned());
            }
        } else if is_capacity_holder(&run.state) && !owner_is_live(run) {
            let boot_changed = run
                .boot_identity
                .as_deref()
                .zip(boot_identity().as_deref())
                .is_some_and(|(old, new)| old != new);
            let known_groups = run
                .process_groups
                .iter()
                .copied()
                .chain(run.owner_process_group)
                .collect::<Vec<_>>();
            let children_proven_gone = !known_groups.is_empty()
                && known_groups
                    .iter()
                    .all(|group| process_group_is_live(*group) == Some(false));
            if boot_changed || children_proven_gone {
                run.state = run
                    .exit_classification
                    .clone()
                    .unwrap_or_else(|| "recovered_stale_owner".to_owned());
                run.ended_at = Some(Utc::now().to_rfc3339());
                if run.exit_classification.is_none() {
                    run.exit_classification = Some("stale_owner_recovered".to_owned());
                }
            } else {
                run.state = "stale_owner_unknown".to_owned();
            }
        }
    }
}

fn is_capacity_holder(state: &str) -> bool {
    matches!(
        state,
        "running" | "stale_owner_unknown" | "owner_exited_waiting_children"
    )
}

fn owner_is_live(run: &RunRecord) -> bool {
    if run
        .boot_identity
        .as_deref()
        .zip(boot_identity().as_deref())
        .is_some_and(|(old, new)| old != new)
    {
        return false;
    }
    if run.boot_identity.is_none() || boot_identity().is_none() {
        return true;
    }
    match process_is_live(run.owner_pid) {
        Some(true) => match (
            run.owner_start_identity.as_deref(),
            process_start_identity(run.owner_pid).as_deref(),
        ) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => true,
        },
        Some(false) => false,
        None => true,
    }
}

fn process_is_live(pid: u32) -> Option<bool> {
    #[cfg(unix)]
    {
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None) {
            Ok(()) | Err(nix::errno::Errno::EPERM) => Some(true),
            Err(nix::errno::Errno::ESRCH) => Some(false),
            Err(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

fn process_group_is_live(group: i32) -> Option<bool> {
    #[cfg(unix)]
    {
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(-group), None) {
            Ok(()) => Some(true),
            Err(nix::errno::Errno::ESRCH) => Some(false),
            Err(nix::errno::Errno::EPERM) => Some(true),
            Err(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = group;
        None
    }
}

fn verified_owner_process_group(pid: u32) -> Option<i32> {
    #[cfg(unix)]
    {
        let group = nix::unistd::getpgid(None).ok()?.as_raw();
        (group == pid as i32).then_some(group)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

fn with_store(root: &Path, fallback_budget: &Budget) -> Result<Store, String> {
    let lock = open_lock(root)?;
    lock.lock_exclusive()
        .map_err(|error| format!("failed to lock admission state: {error}"))?;
    let store = load_store(root, fallback_budget)?;
    lock.unlock()
        .map_err(|error| format!("failed to unlock admission state: {error}"))?;
    Ok(store)
}

fn transact<T>(
    root: &Path,
    fallback_budget: &Budget,
    operation: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    let lock = open_lock(root)?;
    lock.lock_exclusive()
        .map_err(|error| format!("failed to lock admission state: {error}"))?;
    let mut store = load_store(root, fallback_budget)?;
    let output = operation(&mut store)?;
    save_store_unlocked(root, &store)?;
    lock.unlock()
        .map_err(|error| format!("failed to unlock admission state: {error}"))?;
    Ok(output)
}

fn with_store_read(root: &Path, fallback_budget: &Budget) -> Result<Store, String> {
    with_store(root, fallback_budget)
}

fn save_store_unlocked(root: &Path, store: &Store) -> Result<(), String> {
    let path = root.join("state.json");
    reject_symlink(&path)?;
    let bytes = serde_json::to_vec_pretty(store)
        .map_err(|error| format!("failed to encode admission state: {error}"))?;
    let temp = root.join(format!(
        "state.{}.{}.tmp",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(unix)]
    options.mode(if shared_directory_requested() {
        0o660
    } else {
        0o600
    });
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("failed to create admission state: {error}"))?;
    #[cfg(unix)]
    if shared_directory_requested() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o660))
            .map_err(|error| format!("failed to secure shared admission state: {error}"))?;
    }
    file.write_all(&bytes)
        .map_err(|error| format!("failed to write admission state: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync admission state: {error}"))?;
    fs::rename(&temp, &path)
        .map_err(|error| format!("failed to publish admission state: {error}"))?;
    Ok(())
}

fn load_store(root: &Path, budget: &Budget) -> Result<Store, String> {
    let path = root.join("state.json");
    reject_symlink(&path)?;
    let mut store = if path.exists() {
        verify_state_file(&path)?;
        let mut raw = Vec::new();
        File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut raw))
            .map_err(|error| format!("failed to read admission state: {error}"))?;
        serde_json::from_slice::<Store>(&raw)
            .map_err(|error| format!("admission state is invalid; refusing heavy work: {error}"))?
    } else {
        Store {
            schema: "effigy.admission.state.v1".to_owned(),
            schema_version: SCHEMA_VERSION,
            budget: budget.clone(),
            next_ticket: 1,
            scheduler_after: String::new(),
            runs: Vec::new(),
        }
    };
    if store.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported admission state schema version {}; upgrade or move {} safely",
            store.schema_version,
            path.display()
        ));
    }
    store.budget = budget.clone();
    Ok(store)
}

fn open_lock(root: &Path) -> Result<File, String> {
    let path = root.join("state.lock");
    reject_symlink(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(unix)]
    options.mode(if shared_directory_requested() {
        0o660
    } else {
        0o600
    });
    let file = options
        .open(&path)
        .map_err(|error| format!("failed to open admission lock: {error}"))?;
    #[cfg(unix)]
    if shared_directory_requested() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o660))
            .map_err(|error| format!("failed to secure shared admission lock: {error}"))?;
    }
    verify_state_file(&path)?;
    Ok(file)
}

fn ensure_secure_root(root: &Path) -> Result<(), String> {
    let existed = fs::symlink_metadata(root).is_ok();
    fs::create_dir_all(root).map_err(|error| {
        format!(
            "failed to create admission state directory {}: {error}",
            root.display()
        )
    })?;
    reject_symlink(root)?;
    let mut metadata = fs::metadata(root)
        .map_err(|error| format!("failed to inspect admission state directory: {error}"))?;
    if !metadata.is_dir() {
        return Err("admission state path is not a directory".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let uid = unsafe { libc::geteuid() };
        if !existed {
            let mode = if shared_directory_requested() {
                0o2770
            } else {
                0o700
            };
            fs::set_permissions(root, fs::Permissions::from_mode(mode))
                .map_err(|error| format!("failed to secure admission state directory: {error}"))?;
            metadata = fs::metadata(root).map_err(|error| {
                format!("failed to inspect secured admission state directory: {error}")
            })?;
        }
        let mode = metadata.permissions().mode();
        if metadata.uid() == uid && mode & 0o077 == 0 {
            return Ok(());
        }
        if !shared_directory_requested() {
            return Err(format!("admission state directory is owned by uid {} with mode {:o}; use a private mode-0700 directory or configure a trusted shared directory", metadata.uid(), mode & 0o7777));
        }
        let shared_group = metadata.gid();
        let owner_trusted = metadata.uid() == uid || metadata.uid() == 0;
        let group_secure = mode & 0o007 == 0 && mode & 0o2070 == 0o2070;
        if !owner_trusted || !group_secure || !current_user_in_group(shared_group) {
            return Err(format!("shared admission directory {} must be owned by root or the current user, setgid, group-readable/writable/searchable, world-private, and use a group the current user belongs to", root.display()));
        }
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "admission state path {} is a symlink",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to inspect admission state path {}: {error}",
            path.display()
        )),
    }
}

fn shared_directory_requested() -> bool {
    std::env::var_os("EFFIGY_ADMISSION_DIR").is_some()
}

#[cfg(unix)]
fn current_user_in_group(group: u32) -> bool {
    unsafe {
        if libc::getegid() == group {
            return true;
        }
        let count = libc::getgroups(0, std::ptr::null_mut());
        if count < 0 {
            return false;
        }
        let mut groups = vec![0 as libc::gid_t; count as usize];
        let actual = libc::getgroups(count, groups.as_mut_ptr());
        actual >= 0
            && groups
                .iter()
                .take(actual as usize)
                .any(|value| *value == group)
    }
}

#[cfg(not(unix))]
fn current_user_in_group(_group: u32) -> bool {
    false
}

fn verify_state_file(path: &Path) -> Result<(), String> {
    reject_symlink(path)?;
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = metadata.mode();
        let uid = unsafe { libc::geteuid() };
        if !shared_directory_requested() {
            if metadata.uid() != uid || mode & 0o077 != 0 {
                return Err(format!(
                    "admission state file {} must be owned by uid {uid} and private",
                    path.display()
                ));
            }
        } else if mode & 0o007 != 0
            || mode & 0o060 != 0o060
            || !current_user_in_group(metadata.gid())
        {
            return Err(format!(
                "shared admission state file {} must be group-readable/writable and world-private",
                path.display()
            ));
        }
    }
    Ok(())
}

fn state_root() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("EFFIGY_ADMISSION_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("EFFIGY_ADMISSION_DIR must be an absolute path".to_owned());
        }
        return Ok(path);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        "HOME is unset; set EFFIGY_ADMISSION_DIR to a private host-wide directory".to_owned()
    })?;
    let root = home.join(".cache").join("effigy").join("admission");
    if !root.is_absolute() {
        return Err("HOME must be absolute to locate host admission state".to_owned());
    }
    Ok(root)
}

fn host_budget() -> Result<Budget, String> {
    let cores = std::thread::available_parallelism()
        .map_err(|error| format!("cannot measure host logical CPUs: {error}"))?
        .get() as u32;
    let memory_bytes = physical_memory_bytes()
        .ok_or_else(|| "cannot measure host physical memory; heavy tasks fail closed".to_owned())?;
    let default = Budget {
        cpu_units: (cores / 2).max(1),
        memory_mib: (memory_bytes / 2) / (1024 * 1024),
    };
    let cpu_units = env_u32("EFFIGY_ADMISSION_CPU_BUDGET")?.unwrap_or(default.cpu_units);
    let memory_mib = env_u64("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB")?.unwrap_or(default.memory_mib);
    if cpu_units == 0 || memory_mib == 0 {
        return Err("admission CPU and memory budgets must both be greater than zero".to_owned());
    }
    Ok(Budget {
        cpu_units,
        memory_mib,
    })
}

fn requested_reservation(budget: &Budget) -> Result<Reservation, String> {
    let cpu_units = env_u32("EFFIGY_ADMISSION_CPU_UNITS")?.unwrap_or(budget.cpu_units);
    let memory_mib = env_u64("EFFIGY_ADMISSION_MEMORY_MIB")?.unwrap_or(budget.memory_mib);
    if cpu_units == 0 || memory_mib == 0 {
        return Err("admission task reservation must be greater than zero".to_owned());
    }
    Ok(Reservation {
        cpu_units,
        memory_mib,
    })
}

fn env_u32(key: &str) -> Result<Option<u32>, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| format!("{key} must be a positive integer"))
        })
        .transpose()
}

fn env_u64(key: &str) -> Result<Option<u64>, String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{key} must be a positive integer"))
        })
        .transpose()
}

fn wait_timeout_secs() -> Result<u64, String> {
    Ok(env_u64("EFFIGY_ADMISSION_TIMEOUT_SECS")?.unwrap_or(DEFAULT_WAIT_SECS))
}

fn physical_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let contents = fs::read_to_string("/proc/meminfo").ok()?;
        let line = contents
            .lines()
            .find(|line| line.starts_with("MemTotal:"))?;
        let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
        Some(kib * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn boot_identity() -> Option<String> {
    BOOT_IDENTITY.get_or_init(read_boot_identity).clone()
}

fn read_boot_identity() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .ok()
            .map(|value| value.trim().to_owned())
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn process_start_identity(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let raw = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let close = raw.rfind(')')?;
        raw[close + 1..]
            .split_whitespace()
            .nth(19)
            .map(str::to_owned)
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

fn child_cpu_snapshot() -> Option<(u128, u128)> {
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) != 0 {
            return None;
        }
        let user = (usage.ru_utime.tv_sec as u128) * 1000 + (usage.ru_utime.tv_usec as u128) / 1000;
        let system =
            (usage.ru_stime.tv_sec as u128) * 1000 + (usage.ru_stime.tv_usec as u128) / 1000;
        Some((user, system))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn process_group_rss_bytes(process_group: i32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size <= 0 {
            return None;
        }
        let mut total_pages = 0u64;
        let entries = fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().parse::<u32>().is_err() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some(close) = raw.rfind(')') else {
                continue;
            };
            let fields = raw[close + 1..].split_whitespace().collect::<Vec<_>>();
            let Some(group) = fields.get(2).and_then(|value| value.parse::<i32>().ok()) else {
                continue;
            };
            if group != process_group {
                continue;
            }
            if let Some(pages) = fields
                .get(21)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|pages| *pages > 0)
            {
                total_pages = total_pages.saturating_add(pages as u64);
            }
        }
        Some(total_pages.saturating_mul(page_size as u64))
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("ps")
            .args(["-Ao", "pgid=,rss="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let kib = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let group = parts.next()?.parse::<i32>().ok()?;
                let rss = parts.next()?.parse::<u64>().ok()?;
                (group == process_group).then_some(rss)
            })
            .sum::<u64>();
        Some(kib.saturating_mul(1024))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = process_group;
        None
    }
}

fn unique_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    )
}

fn canonical_or_absolute(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn repository_and_fairness_key(path: &Path, caller: &str) -> (String, String) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let common_dir = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !common_dir.is_empty() {
                return (common_dir.clone(), common_dir);
            }
        }
    }
    (canonical_or_absolute(path), caller.to_owned())
}

fn prune_history(store: &mut Store) {
    if store.runs.len() <= HISTORY_LIMIT {
        return;
    }
    let mut completed = store
        .runs
        .iter()
        .enumerate()
        .filter(|(_, run)| {
            !matches!(
                run.state.as_str(),
                "running" | "waiting_for_capacity" | "stale_owner_unknown"
            )
        })
        .map(|(index, run)| (index, run.ticket))
        .collect::<Vec<_>>();
    completed.sort_by_key(|(_, ticket)| *ticket);
    let remove_count = store.runs.len().saturating_sub(HISTORY_LIMIT);
    let remove = completed
        .into_iter()
        .take(remove_count)
        .map(|(index, _)| index)
        .collect::<std::collections::HashSet<_>>();
    store.runs = std::mem::take(&mut store.runs)
        .into_iter()
        .enumerate()
        .filter_map(|(index, run)| (!remove.contains(&index)).then_some(run))
        .collect();
}

#[cfg(test)]
mod tests {
    use super::{
        acquire, boot_identity, cleanup_stale, run_json_at, save_store_unlocked, schedule, Budget,
        Request, Reservation, RunRecord, Store,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn one_unconfigured_reservation_uses_the_entire_budget() {
        let budget = Budget {
            cpu_units: 4,
            memory_mib: 8192,
        };
        let reservation = Reservation {
            cpu_units: 4,
            memory_mib: 8192,
        };
        assert_eq!(reservation.cpu_units, budget.cpu_units);
        assert_eq!(reservation.memory_mib, budget.memory_mib);
    }

    #[test]
    fn store_is_versioned_and_starts_with_a_fairness_cursor() {
        let store = Store {
            schema: "effigy.admission.state.v1".to_owned(),
            schema_version: 1,
            budget: Budget {
                cpu_units: 1,
                memory_mib: 1,
            },
            next_ticket: 1,
            scheduler_after: String::new(),
            runs: Vec::new(),
        };
        let encoded = serde_json::to_string(&store).expect("encode");
        assert!(encoded.contains("effigy.admission.state.v1"));
        assert!(encoded.contains("scheduler_after"));
    }

    #[test]
    fn run_query_returns_a_versioned_record_for_a_persisted_run() {
        let root = tempfile::tempdir().expect("temp state directory");
        let record = run("run-1", "repo-a", 1, "succeeded", 1, 50);
        let mut coordinator = store(Budget {
            cpu_units: 2,
            memory_mib: 100,
        });
        coordinator.runs.push(record);
        save_store_unlocked(root.path(), &coordinator).expect("write coordinator state");

        let rendered = run_json_at(root.path(), "run-1")
            .expect("query succeeds")
            .expect("record exists");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");
        assert_eq!(value["schema"], "effigy.admission.run.v1");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["run_id"], "run-1");
        assert_eq!(value["state"], "succeeded");
    }

    #[test]
    fn scheduler_round_robins_repository_heads_and_keeps_each_repository_fifo() {
        let mut store = store(Budget {
            cpu_units: 2,
            memory_mib: 100,
        });
        store.scheduler_after = "repo-a".to_owned();
        store
            .runs
            .push(run("active-a", "repo-a", 1, "running", 1, 50));
        store
            .runs
            .push(run("b-1", "repo-b", 2, "waiting_for_capacity", 1, 50));
        store
            .runs
            .push(run("a-2", "repo-a", 3, "waiting_for_capacity", 1, 50));
        store
            .runs
            .push(run("c-1", "repo-c", 4, "waiting_for_capacity", 1, 50));

        schedule(&mut store);

        assert_eq!(state(&store, "b-1"), "running");
        assert_eq!(state(&store, "a-2"), "waiting_for_capacity");
        assert_eq!(state(&store, "c-1"), "waiting_for_capacity");
        assert_eq!(
            store
                .runs
                .iter()
                .find(|run| run.run_id == "a-2")
                .and_then(|run| run.position),
            Some(1)
        );

        store.budget.cpu_units = 4;
        store.budget.memory_mib = 200;
        schedule(&mut store);
        assert_eq!(state(&store, "a-2"), "running");
        assert_eq!(state(&store, "c-1"), "running");
        assert_eq!(store.scheduler_after, "repo-a");
    }

    #[test]
    fn scheduler_never_lets_a_small_request_pass_its_repository_head() {
        let mut store = store(Budget {
            cpu_units: 2,
            memory_mib: 100,
        });
        store
            .runs
            .push(run("blocker", "repo-x", 1, "running", 1, 50));
        store.runs.push(run(
            "large-head",
            "repo-a",
            2,
            "waiting_for_capacity",
            2,
            50,
        ));
        store.runs.push(run(
            "small-later",
            "repo-a",
            3,
            "waiting_for_capacity",
            1,
            50,
        ));
        store.runs.push(run(
            "other-repo",
            "repo-b",
            4,
            "waiting_for_capacity",
            1,
            50,
        ));

        schedule(&mut store);

        assert_eq!(state(&store, "large-head"), "waiting_for_capacity");
        assert_eq!(state(&store, "small-later"), "waiting_for_capacity");
        assert_eq!(state(&store, "other-repo"), "running");
    }

    #[cfg(unix)]
    #[test]
    fn stale_owner_recovery_waits_until_the_recorded_child_group_is_gone() {
        use std::os::unix::process::CommandExt;

        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn controlled child group");
        let group = child.id() as i32;
        let mut stale_run = run("stale", "repo-a", 1, "running", 1, 50);
        stale_run.owner_pid = 1_900_000_000;
        stale_run.boot_identity = boot_identity();
        stale_run.owner_process_group = Some(group);
        let mut store = store(Budget {
            cpu_units: 2,
            memory_mib: 100,
        });
        store.runs.push(stale_run);

        cleanup_stale(&mut store);
        assert_eq!(state(&store, "stale"), "stale_owner_unknown");

        child.kill().expect("stop controlled child");
        child.wait().expect("reap controlled child");
        cleanup_stale(&mut store);
        assert_eq!(state(&store, "stale"), "recovered_stale_owner");
        assert_eq!(
            store.runs[0].exit_classification.as_deref(),
            Some("stale_owner_recovered")
        );
    }

    #[test]
    fn stale_owner_without_a_proven_process_group_stays_reserved() {
        let mut stale_run = run("stale", "repo-a", 1, "running", 1, 50);
        stale_run.owner_pid = 1_900_000_000;
        stale_run.boot_identity = boot_identity();
        let mut store = store(Budget {
            cpu_units: 2,
            memory_mib: 100,
        });
        store.runs.push(stale_run);

        cleanup_stale(&mut store);
        assert_eq!(state(&store, "stale"), "stale_owner_unknown");
        assert!(store.runs[0].ended_at.is_none());
    }

    #[test]
    fn admission_is_shared_by_independent_processes_and_records_wait_time() {
        let root = tempfile::tempdir().expect("temp root");
        let state_dir = root.path().join("admission");
        let owner_repo = root.path().join("owner-repo");
        let waiter_repo = root.path().join("waiter-repo");
        fs::create_dir(&owner_repo).expect("owner repo");
        fs::create_dir(&waiter_repo).expect("waiter repo");
        let owner_ready = root.path().join("owner.ready");
        let waiter_ready = root.path().join("waiter.ready");
        let release_owner = root.path().join("release-owner");
        let release_waiter = root.path().join("release-waiter");
        let exe = std::env::current_exe().expect("test executable");

        let mut owner = fixture_process(
            &exe,
            "owner",
            &state_dir,
            &owner_repo,
            &owner_ready,
            &release_owner,
        );
        wait_for_file(&owner_ready, &mut owner);
        let mut waiter = fixture_process(
            &exe,
            "waiter",
            &state_dir,
            &waiter_repo,
            &waiter_ready,
            &release_waiter,
        );
        wait_for_queued_run(&state_dir, &mut waiter);
        assert!(
            !waiter_ready.exists(),
            "the second process must wait for the same host budget"
        );
        thread::sleep(Duration::from_millis(150));

        fs::write(&release_owner, "release").expect("release owner");
        wait_for_file(&waiter_ready, &mut waiter);
        fs::write(&release_waiter, "release").expect("release waiter");
        wait_child(owner);
        wait_child(waiter);

        let store: Store =
            serde_json::from_slice(&fs::read(state_dir.join("state.json")).expect("state file"))
                .expect("valid state");
        let waiter_run = store
            .runs
            .iter()
            .find(|run| run.caller == "waiter")
            .expect("waiter record");
        assert_eq!(waiter_run.state, "succeeded");
        assert!(waiter_run.queue_wait_ms.unwrap_or_default() >= 100);
        assert_eq!(waiter_run.reservation.cpu_units, 1);
        assert_eq!(waiter_run.reservation.memory_mib, 64);
    }

    #[test]
    fn identical_concurrent_invocations_get_distinct_leases_and_execute() {
        let root = tempfile::tempdir().expect("temp root");
        let state_dir = root.path().join("admission");
        fs::create_dir(&state_dir).expect("create shared test coordinator");
        #[cfg(unix)]
        fs::set_permissions(
            &state_dir,
            std::os::unix::fs::PermissionsExt::from_mode(0o2770),
        )
        .expect("secure shared test coordinator");
        let repository = root.path().join("same-repo");
        fs::create_dir(&repository).expect("repository");
        let repository_identity = fs::canonicalize(&repository)
            .expect("canonical repository")
            .to_string_lossy()
            .into_owned();
        let release = root.path().join("release-identical-invocations");
        let exe = std::env::current_exe().expect("test executable");

        let mut first = identical_invocation_process(&exe, "first", root.path(), &state_dir);
        let mut second = identical_invocation_process(&exe, "second", root.path(), &state_dir);
        wait_for_file(&root.path().join("started-first"), &mut first);
        wait_for_file(&root.path().join("started-second"), &mut second);

        let running: Store =
            serde_json::from_slice(&fs::read(state_dir.join("state.json")).expect("state file"))
                .expect("valid state");
        let runs = running
            .runs
            .iter()
            .filter(|run| run.caller == "same-caller")
            .collect::<Vec<_>>();
        assert_eq!(runs.len(), 2, "identical calls keep separate run records");
        assert_ne!(runs[0].run_id, runs[1].run_id);
        assert_ne!(runs[0].lease_id, runs[1].lease_id);
        assert!(runs.iter().all(|run| {
            run.repository == repository_identity && run.selector == "qa" && run.state == "running"
        }));

        fs::write(&release, "release both").expect("release invocations");
        wait_child(first);
        wait_child(second);
        assert!(root.path().join("executed-first").exists());
        assert!(root.path().join("executed-second").exists());

        let finished: Store =
            serde_json::from_slice(&fs::read(state_dir.join("state.json")).expect("state file"))
                .expect("valid state");
        assert_eq!(
            finished
                .runs
                .iter()
                .filter(|run| run.caller == "same-caller" && run.state == "succeeded")
                .count(),
            2,
            "both identical invocations execute and finish independently"
        );
    }

    fn identical_invocation_process(
        exe: &Path,
        role: &str,
        root: &Path,
        state_dir: &Path,
    ) -> Child {
        Command::new(exe)
            .args([
                "--exact",
                "runner::admission::tests::identical_invocation_process_fixture",
                "--nocapture",
            ])
            .env("EFFIGY_ADMISSION_TEST_ROLE", role)
            .env("EFFIGY_ADMISSION_TEST_ROOT", root)
            .env("EFFIGY_ADMISSION_DIR", state_dir)
            .env("EFFIGY_ADMISSION_CPU_BUDGET", "2")
            .env("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB", "128")
            .env("EFFIGY_ADMISSION_CPU_UNITS", "1")
            .env("EFFIGY_ADMISSION_MEMORY_MIB", "64")
            .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "10")
            .spawn()
            .expect("spawn identical invocation fixture")
    }

    #[test]
    fn identical_invocation_process_fixture() {
        let Ok(role) = std::env::var("EFFIGY_ADMISSION_TEST_ROLE") else {
            return;
        };
        let root =
            PathBuf::from(std::env::var_os("EFFIGY_ADMISSION_TEST_ROOT").expect("fixture root"));
        let repository = root.join("same-repo");
        let release = root.join("release-identical-invocations");
        let lease = acquire(Request {
            caller: "same-caller",
            repository: &repository,
            selector: "qa",
        })
        .expect("identical invocation receives its own lease");
        fs::write(root.join(format!("started-{role}")), "started").expect("started marker");
        while !release.exists() {
            thread::sleep(Duration::from_millis(10));
        }
        fs::write(root.join(format!("executed-{role}")), "executed").expect("executed marker");
        lease.finish(&Ok(String::new()));
    }

    #[test]
    fn descendant_process_shares_a_proven_parent_lease_without_the_env_token() {
        let root = tempfile::tempdir().expect("temp root");
        let state_dir = root.path().join("admission");
        let ready = root.path().join("descendant.ready");
        let exe = std::env::current_exe().expect("test executable");
        let mut owner = Command::new(exe)
            .args([
                "--exact",
                "runner::admission::tests::process_admission_descendant_owner_fixture",
                "--nocapture",
            ])
            .env("EFFIGY_ADMISSION_TEST_ROLE", "descendant-owner")
            .env("EFFIGY_ADMISSION_TEST_ROOT", root.path())
            .env("EFFIGY_ADMISSION_DIR", &state_dir)
            .env("EFFIGY_ADMISSION_CPU_BUDGET", "1")
            .env("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB", "64")
            .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "2")
            .spawn()
            .expect("spawn lease owner fixture");

        wait_for_file(&ready, &mut owner);
        wait_child(owner);

        let store: Store =
            serde_json::from_slice(&fs::read(state_dir.join("state.json")).expect("state file"))
                .expect("valid state");
        assert_eq!(
            store.runs.len(),
            1,
            "the descendant must reuse the owner run"
        );
        assert_eq!(store.runs[0].state, "succeeded");
    }

    #[test]
    fn process_admission_descendant_owner_fixture() {
        if std::env::var("EFFIGY_ADMISSION_TEST_ROLE").as_deref() != Ok("descendant-owner") {
            return;
        }
        let root = std::path::PathBuf::from(
            std::env::var_os("EFFIGY_ADMISSION_TEST_ROOT").expect("fixture root"),
        );
        let state_dir = std::env::var_os("EFFIGY_ADMISSION_DIR").expect("state directory");
        let lease = acquire(Request {
            caller: "descendant-owner",
            repository: &root,
            selector: "qa",
        })
        .expect("parent acquires admission");

        let child = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "runner::admission::tests::process_admission_descendant_fixture",
                "--nocapture",
            ])
            .env("EFFIGY_ADMISSION_TEST_ROLE", "descendant")
            .env("EFFIGY_ADMISSION_TEST_ROOT", &root)
            .env("EFFIGY_ADMISSION_DIR", state_dir)
            .env("EFFIGY_ADMISSION_CPU_BUDGET", "1")
            .env("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB", "64")
            .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "2")
            .env_remove("EFFIGY_ADMISSION_LEASE_ID")
            .spawn()
            .expect("spawn descendant fixture");
        wait_child(child);
        lease.finish(&Ok(String::new()));
        fs::write(root.join("descendant.ready"), "ready").expect("write marker");
    }

    #[test]
    fn process_admission_descendant_fixture() {
        if std::env::var("EFFIGY_ADMISSION_TEST_ROLE").as_deref() != Ok("descendant") {
            return;
        }
        let root = std::path::PathBuf::from(
            std::env::var_os("EFFIGY_ADMISSION_TEST_ROOT").expect("fixture root"),
        );
        let lease = acquire(Request {
            caller: "descendant",
            repository: &root,
            selector: "qa",
        })
        .expect("descendant reuses active ancestor lease");
        lease.finish(&Ok(String::new()));
    }

    #[test]
    fn process_admission_fixture() {
        let Ok(role) = std::env::var("EFFIGY_ADMISSION_TEST_ROLE") else {
            return;
        };
        let root = std::env::var_os("EFFIGY_ADMISSION_TEST_ROOT").expect("fixture root");
        let root = std::path::PathBuf::from(root);
        let repository = root.join(format!("{role}-repo"));
        let ready = root.join(format!("{role}.ready"));
        let release = root.join(format!("release-{role}"));
        let lease = acquire(Request {
            caller: &role,
            repository: &repository,
            selector: "qa",
        })
        .expect("admission succeeds");
        fs::write(&ready, "ready").expect("write ready marker");
        while !release.exists() {
            thread::sleep(Duration::from_millis(10));
        }
        lease.finish(&Ok(String::new()));
    }

    fn fixture_process(
        exe: &Path,
        role: &str,
        state_dir: &Path,
        _repository: &Path,
        _ready: &Path,
        _release: &Path,
    ) -> Child {
        Command::new(exe)
            .args([
                "--exact",
                "runner::admission::tests::process_admission_fixture",
                "--nocapture",
            ])
            .env("EFFIGY_ADMISSION_TEST_ROLE", role)
            .env(
                "EFFIGY_ADMISSION_TEST_ROOT",
                state_dir.parent().expect("fixture parent"),
            )
            .env("EFFIGY_ADMISSION_DIR", state_dir)
            .env("EFFIGY_ADMISSION_CPU_BUDGET", "1")
            .env("EFFIGY_ADMISSION_MEMORY_BUDGET_MIB", "64")
            .env("EFFIGY_ADMISSION_TIMEOUT_SECS", "10")
            .spawn()
            .expect("spawn fixture process")
    }

    fn wait_for_file(path: &Path, child: &mut Child) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            if let Some(status) = child.try_wait().expect("poll fixture child") {
                panic!("fixture exited before writing {}: {status}", path.display());
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {}",
                path.display()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_queued_run(state_dir: &Path, child: &mut Child) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().expect("poll waiter") {
                panic!("waiter exited before queueing: {status}");
            }
            if let Ok(bytes) = fs::read(state_dir.join("state.json")) {
                if let Ok(store) = serde_json::from_slice::<Store>(&bytes) {
                    if store
                        .runs
                        .iter()
                        .any(|run| run.caller == "waiter" && run.state == "waiting_for_capacity")
                    {
                        return;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for persistent capacity wait"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_child(mut child: Child) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().expect("poll fixture child") {
                assert!(status.success(), "fixture child failed: {status}");
                return;
            }
            assert!(Instant::now() < deadline, "fixture process did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn store(budget: Budget) -> Store {
        Store {
            schema: "effigy.admission.state.v1".to_owned(),
            schema_version: 1,
            budget,
            next_ticket: 1,
            scheduler_after: String::new(),
            runs: Vec::new(),
        }
    }

    fn run(
        run_id: &str,
        fairness_key: &str,
        ticket: u64,
        state: &str,
        cpu: u32,
        memory: u64,
    ) -> RunRecord {
        RunRecord {
            run_id: run_id.to_owned(),
            lease_id: run_id.to_owned(),
            caller: run_id.to_owned(),
            repository: fairness_key.to_owned(),
            fairness_key: fairness_key.to_owned(),
            selector: "qa".to_owned(),
            state: state.to_owned(),
            ticket,
            position: None,
            queued_at: "2026-09-28T00:00:00Z".to_owned(),
            deadline_at: "2026-09-28T00:30:00Z".to_owned(),
            admitted_at: None,
            started_at: None,
            ended_at: None,
            queue_wait_ms: None,
            wall_ms: None,
            cpu_user_ms: None,
            cpu_system_ms: None,
            peak_rss_bytes: None,
            cpu_overrun: None,
            memory_overrun: None,
            owner_pid: 1,
            owner_start_identity: None,
            owner_process_group: None,
            boot_identity: None,
            process_groups: Vec::new(),
            budget: Budget {
                cpu_units: 2,
                memory_mib: 100,
            },
            reservation: Reservation {
                cpu_units: cpu,
                memory_mib: memory,
            },
            exit_classification: None,
            log_reference: None,
        }
    }

    fn state<'a>(store: &'a Store, run_id: &str) -> &'a str {
        &store
            .runs
            .iter()
            .find(|run| run.run_id == run_id)
            .expect("run record")
            .state
    }
}
