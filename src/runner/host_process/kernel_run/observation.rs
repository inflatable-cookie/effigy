//! Lifecycle and journal observation for host-owned runs.

use super::super as host;
use super::*;

const HOST_LIFECYCLE_EFFECT: &str = "host:lifecycle";

/// Identifies the existing owner journal, including the home inherited by a
/// detached supervisor. A dependency never follows a different checkout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::runner::host_process) struct HostLifecycleSource {
    repo: PathBuf,
    resource: ResourceId,
    state_home: PathBuf,
}

impl HostLifecycleSource {
    pub(super) fn for_policy(
        repo: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            repo: repo.to_owned(),
            resource: host_process_resource(policy, name)?,
            state_home: crate::runner::task_engine::state_home_path().ok_or_else(|| {
                RunnerError::task_invocation("host lifecycle journal home is unavailable")
            })?,
        })
    }
}

/// The latest typed observation, including retained observations after settle.
/// The observation is already true when intended: its complete payload is the
/// atomic publication, including before the applied callback. Corrupt latest
/// payloads never fall back to an older snapshot.
pub(in crate::runner::host_process) fn lifecycle_from_effects(
    effects: &[Effect],
) -> Result<Option<HostLifecycleObservation>, RunnerError> {
    effects
        .iter()
        .rev()
        .find(|effect| effect.spec.kind == HOST_LIFECYCLE_EFFECT)
        .map(|effect| {
            serde_json::from_value(effect.spec.payload.clone()).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "host lifecycle snapshot is unreadable: {error}"
                ))
            })
        })
        .transpose()
}

fn lifecycle_snapshot(
    effects: &[Effect],
    legacy_path: &Path,
    run: RunId,
) -> Result<Option<HostLifecycleObservation>, RunnerError> {
    let state = match lifecycle_from_effects(effects)? {
        Some(state) => Some(state),
        None => read_lifecycle(None, legacy_path)?,
    };
    Ok(state.and_then(|state| bind_lifecycle(state, run, custody_nonce(effects))))
}

pub(in crate::runner::host_process) fn publish_lifecycle(
    run: &mut effigy_kernel::run::Run<'_>,
    state: &HostLifecycleObservation,
) -> Result<(), RunnerError> {
    let result = (|| -> Result<(), RunError> {
        let ticket = run.intend(effigy_kernel::run::EffectSpec {
            target: run.fence().clone(),
            kind: HOST_LIFECYCLE_EFFECT.to_owned(),
            payload: serde_json::to_value(state)?,
            settlement: effigy_kernel::run::Settlement::Retain,
            required: false,
        })?;
        run.applied(&ticket, serde_json::Value::Null)
    })();
    result.map_err(|error| {
        RunnerError::task_invocation(format!(
            "host lifecycle observation could not be recorded: {error}"
        ))
    })
}

/// All readers use the run's latest observation. A run without lifecycle
/// effects belongs to an older supervisor and retains its existing file reader
/// until stopped; new supervisors never write that file.
pub(in crate::runner::host_process) fn read_lifecycle(
    source: Option<&HostLifecycleSource>,
    legacy_path: &Path,
) -> Result<Option<HostLifecycleObservation>, RunnerError> {
    let Some(source) = source else {
        let Some(mut state): Option<HostLifecycleObservation> = optional_private_json(legacy_path)?
        else {
            return Ok(None);
        };
        if state.schema != HOST_LISTENER_STATE_SCHEMA {
            return Err(RunnerError::task_invocation(
                "unsupported legacy host listener schema",
            ));
        }
        if matches!(
            state.status,
            host::HostLifecycleStatus::Starting | host::HostLifecycleStatus::Unavailable
        ) && host::read_live_process_identity(state.supervisor_pid).is_err_and(|error| {
            error.kind() == std::io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH)
        }) {
            // Preserve a final publication that raced the vanished owner.
            state = owner_observation(read_private_json(legacy_path)?, false);
        }
        return Ok(Some(state));
    };
    let unavailable =
        || RunnerError::task_invocation("host lifecycle journal observation is unavailable");
    let Some((root, reader)) = observed_journal(observe_root_in(&source.state_home, &source.repo))?
    else {
        return Ok(None);
    };
    let rows = observed_value(
        history(&reader, &source.resource, 1),
        "host lifecycle history",
    )?
    .ok_or_else(unavailable)?;
    let Some(run) = rows.into_iter().next() else {
        return Ok(None);
    };
    let effects = observed_value(
        effigy_kernel::run::effects(&reader, run.id),
        "host lifecycle effects",
    )?
    .ok_or_else(unavailable)?;
    let Some(mut state) = lifecycle_snapshot(&effects, legacy_path, run.id)? else {
        return Ok(None);
    };
    if matches!(
        state.status,
        host::HostLifecycleStatus::Starting
            | host::HostLifecycleStatus::Ready
            | host::HostLifecycleStatus::Unavailable
    ) {
        let live = match Gate::probe(&observe(), &root, &source.resource) {
            Observed::Verified(GateState::Held(_), _) => run.phase != Phase::Closed,
            Observed::Verified(GateState::Free, _) => false,
            _ => return Err(unavailable()),
        };
        if !live {
            // The owner may have committed its final failure after our first
            // snapshot. With its gate free, no later observation can follow.
            let final_effects = observed_value(
                effigy_kernel::run::effects(&reader, run.id),
                "host lifecycle final effects",
            )?
            .ok_or_else(unavailable)?;
            let Some(final_state) = lifecycle_snapshot(&final_effects, legacy_path, run.id)? else {
                return Ok(None);
            };
            state = owner_observation(final_state, false);
        }
    }
    Ok(Some(state))
}

/// What `container status` shows for one declared host process.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessStatus {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    pub state: HostProcessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(serialize_with = "serialize_listener_status")]
    pub listener: Option<host::HostLifecycleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Panes and status observe the same owner run and typed lifecycle snapshot.
/// No child text is an authority for lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostPaneIdentity {
    repo: PathBuf,
    resource: ResourceId,
}
#[derive(Debug)]
pub(in crate::runner) struct HostPaneObservation {
    pub phase: Phase,
    pub live: bool,
    pub exit: Option<i32>,
    pub lifecycle: Option<(host::HostLifecycleStatus, Option<String>)>,
}
struct HostRunObservation {
    run: RunSummary,
    pid: Option<u32>,
    last_exit: Option<i32>,
    pane: HostPaneObservation,
}

impl HostPaneIdentity {
    pub(in crate::runner) fn new(
        repo: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
    ) -> Option<Self> {
        Some(Self {
            repo: repo.to_owned(),
            resource: host_process_resource(policy, name).ok()?,
        })
    }
    pub(in crate::runner) fn read(&self, listener: Option<&Path>) -> Option<HostPaneObservation> {
        self.observe(listener)
            .ok()
            .flatten()
            .map(|observation| observation.pane)
    }

    fn observe(&self, listener: Option<&Path>) -> Result<Option<HostRunObservation>, String> {
        let Some((root, reader)) =
            observed_journal(observe_root(&self.repo)).map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let rows = history(&reader, &self.resource, 1);
        let Some(run) = rows
            .verified()
            .ok_or("host run observation is unknown")?
            .first()
            .cloned()
        else {
            return Ok(None);
        };
        let effects = effigy_kernel::run::effects(&reader, run.id);
        let effects = effects
            .verified()
            .ok_or("host effects observation is unknown")?;
        let latest = effects
            .iter()
            .rev()
            .find(|effect| effect.spec.kind == "kernel:process");
        let nonce = latest.and_then(|effect| effect.receipt.as_ref()?.get("nonce")?.as_str());
        let pid = latest
            .and_then(|effect| effect.receipt.as_ref()?.get("pgid")?.as_u64())
            .and_then(|pid| u32::try_from(pid).ok());
        let last_exit = effects
            .iter()
            .rev()
            .filter(|effect| effect.spec.kind == "kernel:process")
            .find_map(|effect| effect.receipt.as_ref()?.get("exit")?.as_i64())
            .and_then(|code| i32::try_from(code).ok())
            .or_else(|| run_exit(&run));
        let exit = latest
            .and_then(|effect| effect.receipt.as_ref()?.get("exit")?.as_i64())
            .and_then(|code| i32::try_from(code).ok());
        let live = match Gate::probe(&observe(), &root, &self.resource) {
            Observed::Verified(GateState::Held(_), _) => true,
            Observed::Verified(GateState::Free, _) => false,
            _ => return Err("host owner observation is unknown".to_owned()),
        };
        let lifecycle = match lifecycle_from_effects(effects)
            .map_err(|_| "host lifecycle snapshot is unreadable")?
        {
            Some(state) => {
                // A malformed or stale current observation is unknown, never missing.
                let state = owner_observation(state, live && run.phase != Phase::Closed);
                Some(
                    pane_listener(state, run.id, nonce)
                        .ok_or("host lifecycle snapshot has a stale custody generation")?,
                )
            }
            None => match listener {
                Some(path) => read_lifecycle(None, path)
                    .map_err(|_| "legacy host lifecycle snapshot is unreadable")?
                    .and_then(|state| pane_listener(state, run.id, nonce)),
                None => None,
            },
        };
        let exit = if matches!(
            run.workload,
            Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled { .. })
        ) {
            None
        } else {
            exit.or_else(|| run_exit(&run))
        };
        Ok(Some(HostRunObservation {
            pid,
            last_exit,
            pane: HostPaneObservation {
                lifecycle,
                phase: run.phase,
                live,
                exit,
            },
            run,
        }))
    }
}

pub(in crate::runner::host_process) fn pane_listener(
    state: HostLifecycleObservation,
    run: RunId,
    nonce: Option<&str>,
) -> Option<(host::HostLifecycleStatus, Option<String>)> {
    let state = bind_lifecycle(state, run, nonce)?;
    let url = if state.status == host::HostLifecycleStatus::Ready {
        state.public_url.or(state.internal_url)
    } else {
        None
    };
    Some((state.status, url))
}

fn custody_nonce(effects: &[Effect]) -> Option<&str> {
    effects
        .iter()
        .rev()
        .find(|effect| effect.spec.kind == "kernel:process")?
        .receipt
        .as_ref()?
        .get("nonce")?
        .as_str()
}

/// Ownership loss ends an unfinished listener, while terminal diagnostics and
/// a plain workload's historical launch remain the observations they were.
pub(super) fn owner_observation(
    mut state: HostLifecycleObservation,
    live: bool,
) -> HostLifecycleObservation {
    if !live
        && matches!(
            state.status,
            host::HostLifecycleStatus::Starting
                | host::HostLifecycleStatus::Ready
                | host::HostLifecycleStatus::Unavailable
        )
    {
        state.status = host::HostLifecycleStatus::OwnerGone;
        state.diagnostic = Some("its supervisor no longer holds the host owner gate".to_owned());
    }
    state
}

/// Ready/running is a fact about the latest child, never merely the owner run.
/// Pre-spawn and terminal observations may have no custody identity.
fn bind_lifecycle(
    state: HostLifecycleObservation,
    run: RunId,
    nonce: Option<&str>,
) -> Option<HostLifecycleObservation> {
    if state.schema != HOST_LISTENER_STATE_SCHEMA
        || state.run.as_deref() != Some(run.to_string().as_str())
        || (state.custody_nonce.is_some() && state.custody_nonce.as_deref() != nonce)
        || (matches!(
            state.status,
            host::HostLifecycleStatus::Ready | host::HostLifecycleStatus::Running
        ) && (nonce.is_none() || state.custody_nonce.as_deref() != nonce))
    {
        return None;
    }
    Some(state)
}

/// The newest run of `resource` that has not closed.
pub(super) fn latest_open_run(
    repo_root: &Path,
    resource: &ResourceId,
) -> Result<Option<RunSummary>, RunnerError> {
    let Some((_, reader)) = observed_journal(observe_root(repo_root))? else {
        return Ok(None);
    };
    Ok(
        observed_value(history(&reader, resource, 4), "host run history")?
            .and_then(|rows| rows.into_iter().find(|row| row.phase != Phase::Closed)),
    )
}

fn observed_journal(
    observed: Option<Observed<effigy_kernel::root::StateRoot>>,
) -> Result<
    Option<(
        effigy_kernel::root::StateRoot,
        effigy_kernel::store::StoreReader,
    )>,
    RunnerError,
> {
    let observed =
        observed.ok_or_else(|| RunnerError::task_invocation("host journal home is unavailable"))?;
    let Some(root) = observed_value(observed, "host journal")? else {
        return Ok(None);
    };
    let Some(reader) = observed_value(Store::read(&observe(), &root), "host journal read")? else {
        return Ok(None);
    };
    Ok(Some((root, reader)))
}

/// Absence permits an empty result; an unknown observation must retain its cause.
fn observed_value<T>(observed: Observed<T>, step: &str) -> Result<Option<T>, RunnerError> {
    match observed {
        Observed::Verified(value, _) => Ok(Some(value)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(RunnerError::task_invocation(format!(
            "{step} is unavailable ({reason}); retry after restoring journal access"
        ))),
    }
}

/// Whether `id` has already closed in the checkout journal.
pub(super) fn run_is_closed(repo_root: &Path, id: RunId) -> Result<bool, RunnerError> {
    Ok(run_summary_of(repo_root, id)?.is_some_and(|row| row.phase == Phase::Closed))
}

pub(super) fn run_summary_of(
    repo_root: &Path,
    id: RunId,
) -> Result<Option<RunSummary>, RunnerError> {
    let Some((_, reader)) = observed_journal(observe_root(repo_root))? else {
        return Ok(None);
    };
    observed_value(summary(&reader, id), "host run summary")
}

#[derive(Debug, Default)]
pub(super) struct ReceiptFacts {
    pub(super) pid: Option<u32>,
    pub(super) exit: Option<i32>,
}

pub(super) fn custody_effect(repo_root: &Path, run: RunId) -> Result<ReceiptFacts, RunnerError> {
    let Some((_, reader)) = observed_journal(observe_root(repo_root))? else {
        return Ok(ReceiptFacts::default());
    };
    let Some(effects) = observed_value(
        effigy_kernel::run::effects(&reader, run),
        "host custody effects",
    )?
    else {
        return Ok(ReceiptFacts::default());
    };
    let custody = effects
        .iter()
        .rev()
        .filter(|effect| effect.spec.kind == "kernel:process")
        .filter_map(|effect| effect.receipt.as_ref())
        .collect::<Vec<_>>();
    let pid = custody
        .first()
        .and_then(|receipt| receipt.get("pgid"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    // The last exit is the newest generation that actually ended; a running
    // generation carries no exit until it does.
    let exit = custody
        .iter()
        .find_map(|receipt| receipt.get("exit").and_then(serde_json::Value::as_i64))
        .and_then(|exit| i32::try_from(exit).ok());
    Ok(ReceiptFacts { pid, exit })
}

/// `container status` rows: each declared host process's run, state and last
/// exit.
pub(in crate::runner) fn host_process_status(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Vec<HostProcessStatus> {
    let dir = host_process_dir_for_policy(repo_root, policy);
    policy
        .host_processes
        .iter()
        .map(|hp| {
            let mut row = HostProcessStatus {
                name: hp.name.clone(),
                run: None,
                state: HostProcessState::Unknown,
                phase: None,
                diagnostic: None,
                pid: None,
                last_exit: None,
                listener: None,
                log: Some(
                    hp_log_path(repo_root, policy, &hp.name)
                        .display()
                        .to_string(),
                ),
            };
            let resource = match host_process_resource(policy, &hp.name) {
                Ok(resource) => resource,
                Err(error) => {
                    row.diagnostic = Some(error.to_string());
                    return row;
                }
            };
            let identity = HostPaneIdentity {
                repo: repo_root.to_owned(),
                resource,
            };
            match identity.observe(Some(&host_process_state_path(&dir, &hp.name))) {
                Ok(None) => row.state = HostProcessState::Absent,
                Err(error) => row.diagnostic = Some(error),
                Ok(Some(observed)) => {
                    let run = observed.run;
                    let view = observed.pane;
                    row.run = Some(run.id.to_string());
                    row.state = if run.phase == Phase::Closed {
                        HostProcessState::Closed
                    } else if view.live {
                        HostProcessState::Running
                    } else {
                        HostProcessState::Interrupted
                    };
                    row.phase = Some(run.phase);
                    row.pid = observed.pid;
                    row.last_exit = observed.last_exit;
                    row.listener = hp
                        .listener
                        .as_ref()
                        .and(view.lifecycle)
                        .map(|(status, _)| status);
                }
            }
            row
        })
        .collect()
}

fn serialize_listener_status<S: serde::Serializer>(
    status: &Option<host::HostLifecycleStatus>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    status
        .map(host::HostLifecycleStatus::as_str)
        .serialize(serializer)
}

fn run_exit(run: &RunSummary) -> Option<i32> {
    match run.workload.as_ref()? {
        effigy_kernel::outcome::WorkloadOutcome::Succeeded => Some(0),
        effigy_kernel::outcome::WorkloadOutcome::Failed { code } => Some(*code),
        _ => None,
    }
}
