//! Host-process activation, replacement and readiness admission.

use super::super as host;
use super::*;

/// Starts (or replaces) the environment's host processes in dependency order.
///
/// Each supervisor is a detached kernel run; `up` keeps its owning pipe until
/// its declared readiness is observed and then detaches. A bring-up that
/// stops short never leaves a running sidecar: the failing path drops the
/// owning pipes, which is the kernel's owner-loss stop, and settles what it
/// started.
///
/// A listener that fails its own readiness keeps only its own route withheld:
/// the report carries that failure and the listeners that did publish, so the
/// caller can report exposure per declared route target (effigy#455). A
/// failure that is not the listener's own - a start, a state record or a stop
/// request - still ends the whole start.
///
/// `deadline` is the enclosing activation's observation budget. A listener
/// still starting when it is spent withholds its own route; a caller with no
/// enclosing budget passes `None` (a restart waits on its own readiness).
#[cfg(test)]
pub(in crate::runner) fn start_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<HostProcessListenerReport, RunnerError> {
    with_activation_waits(
        repo_root,
        &policy.name,
        crate::runner::environment::requested_holder(),
        |publisher| start_host_processes_in_activation(repo_root, policy, deadline, &publisher),
    )
}

pub(in crate::runner) fn start_host_processes_in_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
    publisher: &WaitPublisher,
) -> Result<HostProcessListenerReport, RunnerError> {
    let HostProcessStart {
        started,
        listeners,
        failures,
    } = start_host_processes_inner(
        repo_root,
        policy,
        None,
        ReplaceGate::Unheld,
        deadline,
        publisher,
    )?;
    detach_started(repo_root, policy, started, None)?;
    Ok(HostProcessListenerReport {
        ready: listeners,
        failures,
    })
}

/// Restarts only the named host processes, and every process that depends on
/// them, in `up`'s dependency order.
///
/// Each named process's current generation is settled by the same replace step
/// `up` runs, then a new generation starts, waits for its declared readiness
/// and publishes its route. Everything else in the environment - the
/// containers and the other host processes - is never touched, so a process
/// stopped with `effigy run stop <run>` is simply started again.
///
/// The replace step (stop plus spawn) holds `Gate(Env)`, and the readiness
/// wait runs after that gate is released: a restart that waits on a listener
/// which never answers still yields to `down` instead of holding the
/// environment for the whole gate budget (effigy#390).
///
/// An unknown name fails before any effect, naming the processes this
/// environment declares.
pub(in crate::runner) fn restart_host_processes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    requested: &[String],
) -> Result<HostProcessRestartReport, RunnerError> {
    let selection = restart_selection(policy, requested)?;
    let selected = selection.ordered.iter().cloned().collect::<BTreeSet<_>>();
    let HostProcessStart {
        started,
        listeners,
        failures,
    } = with_activation_waits(
        repo_root,
        &policy.name,
        crate::runner::environment::requested_holder(),
        |publisher| {
            start_host_processes_inner(
                repo_root,
                policy,
                Some(&selected),
                ReplaceGate::HeldForReplace,
                None,
                &publisher,
            )
        },
    )?;
    // A restart names the processes it replaces, so a listener that did not
    // publish is this restart's failure, not a partial environment: the
    // processes that did publish stay running and the refusal names them.
    let refusal = HostProcessListenerReport {
        ready: listeners.clone(),
        failures,
    }
    .refusal();
    let runs = detach_started(repo_root, policy, started, Some(&selected))?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut listeners = listeners
        .into_iter()
        .map(|listener| (listener.name.clone(), listener))
        .collect::<BTreeMap<_, _>>();
    if let Some(refusal) = refusal {
        // The named processes were replaced and settled; only the route the
        // listener would have served is absent. Report it as the refusal it is.
        return Err(refusal);
    }
    Ok(HostProcessRestartReport {
        environment: policy.name.clone(),
        processes: selection
            .ordered
            .into_iter()
            .map(|name| RestartedHostProcess {
                run: runs.get(&name).map(ToString::to_string),
                dependent: selection.dependent.contains(&name),
                listener: listeners.remove(&name),
                name,
            })
            .collect(),
    })
}

/// What a restart replaces: the named processes and every process whose
/// `depends_on` reaches one of them, in dependency order.
pub(in crate::runner::host_process) struct RestartSelection {
    pub(in crate::runner::host_process) ordered: Vec<String>,
    pub(in crate::runner::host_process) dependent: BTreeSet<String>,
}

pub(in crate::runner::host_process) fn restart_selection(
    policy: &EffectiveContainerPolicy,
    requested: &[String],
) -> Result<RestartSelection, RunnerError> {
    let declared = policy
        .host_processes
        .iter()
        .map(|process| process.name.as_str())
        .collect::<BTreeSet<_>>();
    for name in requested {
        if !declared.contains(name.as_str()) {
            return Err(RunnerError::task_invocation(format!(
                "environment `{}` declares no host process `{name}`; it declares {}",
                policy.name,
                if declared.is_empty() {
                    "none".to_owned()
                } else {
                    declared
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )));
        }
    }
    let mut selected = requested.iter().cloned().collect::<BTreeSet<String>>();
    // A dependent's readiness and route read its dependency, so it is
    // restarted too. Anything that does not reach a named process stays up.
    loop {
        let dependents = policy
            .host_processes
            .iter()
            .filter(|process| !selected.contains(&process.name))
            .filter(|process| {
                process
                    .depends_on
                    .iter()
                    .any(|dependency| selected.contains(dependency))
            })
            .map(|process| process.name.clone())
            .collect::<Vec<_>>();
        if dependents.is_empty() {
            break;
        }
        selected.extend(dependents);
    }
    let ordered = order_host_processes(&policy.host_processes)?
        .into_iter()
        .filter(|process| selected.contains(&process.name))
        .map(|process| process.name.clone())
        .collect();
    let dependent = selected
        .iter()
        .filter(|name| !requested.iter().any(|requested| requested == *name))
        .cloned()
        .collect();
    Ok(RestartSelection { ordered, dependent })
}

/// Releases the owning pipes of a bring-up that reached its readiness.
///
/// A process that exits before the detach has already settled its run: there
/// is no owner left to detach, and that is not a bring-up failure.
pub(super) fn detach_started(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    started: Vec<(String, effigy_kernel::supervise::SupervisorClient)>,
    selection: Option<&BTreeSet<String>>,
) -> Result<Vec<(String, RunId)>, RunnerError> {
    let cx = effects();
    let mut detached = Vec::with_capacity(started.len());
    for (name, client) in started {
        let run = client.run;
        if let Err(error) = client.detach(&cx, run) {
            if !run_is_closed(repo_root, run)? {
                let report = rollback_after_detach_failure(repo_root, policy, selection);
                return Err(RunnerError::task_invocation(format!(
                    "host process `{name}` could not detach after readiness: {error}{}",
                    if report.complete() {
                        String::new()
                    } else {
                        format!(
                            "\nhost-process rollback could not confirm every stop:\n{}",
                            report.lines().join("\n")
                        )
                    }
                )));
            }
        }
        detached.push((name, run));
    }
    Ok(detached)
}

/// The rollback after a detach that failed while its run was still live.
///
/// A restart settles exactly the processes it replaced; every other host
/// process of the environment keeps its run and its route. `up` names no
/// selection, so its rollback still settles the whole environment.
pub(in crate::runner::host_process) fn rollback_after_detach_failure(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selection: Option<&BTreeSet<String>>,
) -> HostProcessStopReport {
    stop_host_processes(repo_root, policy, selection)
}

/// Starts every supervisor and keeps its owning client, without detaching.
/// Dropping the clients is exactly the client death the kernel treats as an
/// owner-loss stop; tests use it to prove custody settles both children.
#[cfg(test)]
pub(in crate::runner::host_process) fn start_host_processes_owning(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<(String, effigy_kernel::supervise::SupervisorClient)>, RunnerError> {
    let HostProcessStart { started, .. } = with_activation_waits(
        repo_root,
        &policy.name,
        crate::runner::environment::requested_holder(),
        |publisher| {
            start_host_processes_inner(
                repo_root,
                policy,
                None,
                ReplaceGate::Unheld,
                None,
                &publisher,
            )
        },
    )?;
    Ok(started)
}

/// One successful bring-up: the supervisors still held by their owning
/// clients, the listener results the caller reports, and the listeners that
/// kept their own route withheld.
#[derive(Default)]
struct HostProcessStart {
    started: Vec<(String, effigy_kernel::supervise::SupervisorClient)>,
    listeners: Vec<ManagedHostListenerResult>,
    failures: Vec<ManagedHostListenerFailure>,
}

/// Whether the replace step (stop plus spawn) holds `Gate(Env)`.
///
/// `up` starts host processes after its reconcile run has finished, so it
/// holds no gate here. A restart's replace step does, and releases it before
/// the readiness wait, so a listener that never answers still yields to `down`
/// instead of holding the environment for the whole gate budget (effigy#390).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplaceGate {
    Unheld,
    HeldForReplace,
}

/// The kernel's implementation of the activation seam: real supervisors,
/// real listeners and real readiness. The ordering loop in
/// `host_process::waits` decides what starts and what every wait says; this
/// struct only performs the per-process operations it asks for, so the loop's
/// spawn decisions are the production ones in tests as well (effigy#458).
struct KernelActivation<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    dir: &'a Path,
    project_path: &'a str,
    checkout_root: &'a Path,
    facts: &'a HostProcessEnvFacts,
    listener_binds: &'a BTreeMap<String, String>,
    route_table: Option<&'a Path>,
    listener_root: Option<PathBuf>,
    state_root: &'static effigy_kernel::root::StateRoot,
    state_home: &'a Path,
    executable: &'a effigy_kernel::supervise::Executable,
    checkout: Option<effigy_kernel::id::CheckoutId>,
    gate: ReplaceGate,
    cx: &'a EffectContext,
    deadline: Option<Instant>,
    by_name: &'a std::collections::HashMap<&'a str, &'a effigy_containers::EffectiveHostProcess>,
    /// The dependency records the process about to launch observed.
    pending: Vec<host::HostProcessDependency>,
    /// The runtime generation `launch` gave the supervisor it started, which
    /// the listener's activation wait confirms.
    pending_generation: Option<String>,
    result: HostProcessStart,
}

impl KernelActivation<'_> {
    fn process(&self, name: &str) -> Result<&EffectiveHostProcess, RunnerError> {
        self.by_name.get(name).copied().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "host process `{name}` is not declared by environment `{}`",
                self.policy.name
            ))
        })
    }

    fn route_table(&self) -> Result<&Path, RunnerError> {
        self.route_table.ok_or_else(|| {
            RunnerError::task_invocation("managed host listener has no gateway route table")
        })
    }
}

/// A plain workload's launch barrier. The typed journal snapshot decides the
/// launch even when its best-effort event is lost; the pipe wakes the reader
/// and carries admission/settlement, including for an older supervisor.
fn await_plain_launch(
    client: &mut effigy_kernel::supervise::SupervisorClient,
    process: &str,
    cx: &EffectContext,
    source: &HostLifecycleSource,
    state_path: &Path,
    generation: &str,
) -> Result<(), RunnerError> {
    loop {
        let event = client.event(cx.operation());
        if let Some(decision) = plain_launch_observation(
            read_lifecycle(Some(source), state_path)?,
            generation,
            process,
        ) {
            return decision;
        }
        match event {
            Ok(effigy_kernel::supervise::Event::Active) => return Ok(()),
            Ok(effigy_kernel::supervise::Event::Closed { workload, .. }) => {
                return Err(RunnerError::task_invocation(format!(
                    "host process `{process}` settled without starting its workload ({workload})"
                )));
            }
            Ok(_) => {}
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "host process `{process}` was lost before its workload started: {error}"
                )));
            }
        }
    }
}

/// A durable observation decides launch even when the best-effort wakeup was
/// lost. A different activation never answers this launch barrier.
pub(in crate::runner::host_process) fn plain_launch_observation(
    state: Option<HostLifecycleObservation>,
    generation: &str,
    process: &str,
) -> Option<Result<(), RunnerError>> {
    let state = state.filter(|state| state.runtime_generation == generation)?;
    match state.status {
        host::HostLifecycleStatus::Running => Some(Ok(())),
        host::HostLifecycleStatus::Failed => Some(Err(RunnerError::task_invocation(format!(
            "host process `{process}` did not start its workload: {}",
            state.diagnostic.unwrap_or_default()
        )))),
        _ => None,
    }
}

impl host::HostActivation for KernelActivation<'_> {
    fn listener(&self, process: &EffectiveHostProcess) -> Option<EffectiveManagedHostListener> {
        resolved_host_process_listener(process, self.listener_binds)
    }

    fn dependency_ready(
        &mut self,
        dependency: &str,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<host::ReadinessOutcome, RunnerError> {
        let process = self.process(dependency)?;
        let state_path = host_process_state_path(self.dir, &process.name);
        let owner = managed_host_process_owner(self.project_path, self.policy, process);
        let source = HostLifecycleSource::for_policy(self.repo_root, self.policy, dependency)?;
        let state = match host::wait_for_listener_state(
            Some(&source),
            &state_path,
            dependency,
            &owner,
            self.route_table()?,
            self.deadline,
            &mut *on_wait,
        )? {
            host::ListenerWait::Ready(state) => state,
            // A dependency that failed its own readiness withholds this
            // process's route too, and nothing else: the rest of the
            // environment is still started (effigy#455).
            host::ListenerWait::Withheld {
                diagnostic,
                observed_status,
                budget_expired,
            } => {
                return Ok(host::ReadinessOutcome::Withheld {
                    diagnostic,
                    observed_status,
                    budget_expired,
                })
            }
        };
        if state.internal_url.is_none() || state.public_url.is_none() {
            return Err(RunnerError::task_invocation(format!(
                "managed host listener `{dependency}` is ready without current endpoint URLs"
            )));
        }
        self.pending.push(host::HostProcessDependency {
            source: Some(HostLifecycleSource::for_policy(
                self.repo_root,
                self.policy,
                dependency,
            )?),
            name: dependency.to_owned(),
            owner,
            state_file: state_path,
        });
        Ok(host::ReadinessOutcome::Ready)
    }

    fn discard_dependencies(&mut self) {
        self.pending.clear();
    }

    fn launch(&mut self, process: &EffectiveHostProcess) -> Result<(), RunnerError> {
        let resource = host_process_resource(self.policy, &process.name)?;
        let env = resolved_host_process_environment(process, self.facts)?;
        let listener = self.listener(process);
        // Read before the spec moves the listener: only a plain workload's
        // launch is decided on the event pipe (effigy#574).
        let plain = listener.is_none();
        let log_file = self
            .dir
            .join(format!("{}.log", host::sanitize(&process.name)));
        let state_path = host_process_state_path(self.dir, &process.name);
        let spec = HostProcessRuntimeSpec {
            run_id: None,
            ansi_front: crate::runner::managed_color::initiating_ansi(),
            output: None,
            schema: host::HOST_PROCESS_SPEC_SCHEMA.to_owned(),
            project_path: self.project_path.to_owned(),
            runtime_generation: {
                let generation = host::random_token()?;
                self.pending_generation = Some(generation.clone());
                generation
            },
            cwd: host::resolve_host_process_cwd(self.checkout_root, process)?,
            env,
            listener,
            owner: managed_host_process_owner(self.project_path, self.policy, process),
            dependencies: std::mem::take(&mut self.pending),
            listener_state_file: state_path.clone(),
            gateway_root: self.listener_root.clone(),
            environment: self.policy.name.clone(),
            profile: self.policy.profile.clone(),
            process_name: process.name.clone(),
            container_name: self.policy.name.clone(),
            run: process.run.clone(),
            log_file,
            restart: process.restart,
            restart_delay_ms: process.restart_delay_ms,
            shutdown_signal: process.shutdown_signal,
            shutdown_grace_secs: process.shutdown_grace_secs,
            checkout: self.checkout,
            state_home: self.state_home.to_path_buf(),
        };
        let payload = serde_json::to_vec(&spec)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        // A host-process supervisor starts users' applications, so it is never
        // a route for session access: its environment carries no vault
        // passphrase, and the processes it starts are workloads (effigy#338,
        // K12).
        let environment: Vec<(OsString, OsString)> =
            crate::runner::secret_session::workload_ambient_environment()
                .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
                .collect();
        // The replace step: settle the previous generation of this exact
        // process (from an earlier `up`) and start its successor. A restart
        // holds `Gate(Env)` for these two effects only, so a concurrent `down`
        // is never blocked by a later readiness wait (effigy#390).
        let launch_cwd = spec.cwd.clone();
        let process_name = process.name.clone();
        let cx = self.cx;
        let executable = self.executable;
        let state_root = self.state_root;
        let repo_root = self.repo_root;
        let policy = self.policy;
        let replace = move || -> Result<effigy_kernel::supervise::SupervisorClient, RunnerError> {
            stop_resource_run(repo_root, policy, &resource, &process_name, "replace")?;
            effigy_kernel::supervise::start_engine_at(
                cx,
                state_root,
                effigy_kernel::supervise::EngineLaunch {
                    payload,
                    cwd: launch_cwd,
                    env: environment,
                    handoff: None,
                    admission: None,
                },
                executable,
                effigy_kernel::supervise::ClientMode::Owning,
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "host process `{process_name}` did not start its kernel run: {error}"
                ))
            })
        };
        let mut client = match self.gate {
            ReplaceGate::Unheld => replace()?,
            ReplaceGate::HeldForReplace => crate::runner::environment::holding_environment(
                self.policy,
                effigy_containers::environment::Operation::Up,
                replace,
            )?,
        };
        // A plain workload's launch is decided on the owning client's event
        // pipe; carry its refusal out of the supervisor instead of reporting
        // a workload that can only settle failed (effigy#574).
        if plain {
            let source =
                HostLifecycleSource::for_policy(self.repo_root, self.policy, &process.name)?;
            let generation = self.pending_generation.as_deref().ok_or_else(|| {
                RunnerError::task_invocation("plain workload has no activation generation")
            })?;
            await_plain_launch(
                &mut client,
                &process.name,
                cx,
                &source,
                &state_path,
                generation,
            )?;
        }
        self.result.started.push((process.name.clone(), client));
        Ok(())
    }

    fn await_listener(
        &mut self,
        process: &EffectiveHostProcess,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<host::ReadinessOutcome, RunnerError> {
        let Some(listener) = self.listener(process) else {
            return Ok(host::ReadinessOutcome::Ready);
        };
        let owner = managed_host_process_owner(self.project_path, self.policy, process);
        let state_path = host_process_state_path(self.dir, &process.name);
        let log_file = self
            .dir
            .join(format!("{}.log", host::sanitize(&process.name)));
        let generation = self.pending_generation.take().ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "host process `{}` has no runtime generation to confirm",
                process.name
            ))
        })?;
        let source = HostLifecycleSource::for_policy(self.repo_root, self.policy, &process.name)?;
        let waited = (|| -> Result<host::ListenerWait, RunnerError> {
            host::wait_for_listener_activation(
                Some(&source),
                &state_path,
                &process.name,
                &generation,
                host::SUPERVISOR_ACTIVATION_PUBLICATION_BOUND,
            )?;
            host::wait_for_listener_state(
                Some(&source),
                &state_path,
                &process.name,
                &owner,
                self.route_table()?,
                self.deadline,
                &mut *on_wait,
            )
        })();
        match waited {
            Ok(host::ListenerWait::Ready(state)) => {
                self.result
                    .listeners
                    .push(listener_result(&process.name, &listener, &state));
                Ok(host::ReadinessOutcome::Ready)
            }
            // This listener never published its own route, so only its own
            // route is withheld.
            Ok(host::ListenerWait::Withheld {
                diagnostic,
                observed_status,
                budget_expired,
            }) => {
                // No withheld start may detach: a failure before the first
                // child never emits Active, and a retrying supervisor could
                // publish after this report. Settle it under the fenced stop
                // and release its owning pipe before reporting partial
                // exposure (effigy#455).
                settle_listener_generation(
                    self.repo_root,
                    self.policy,
                    &process.name,
                    "withheld start",
                )?;
                if self
                    .result
                    .started
                    .last()
                    .is_some_and(|(name, _)| name == &process.name)
                {
                    self.result.started.pop();
                }
                let diagnostic = format!(
                    "{diagnostic}; its generation was settled before this activation returned, so it cannot publish that route later — `effigy container {} restart {}` starts it again",
                    self.policy.name, process.name
                );
                self.result.failures.push(listener_failure(
                    process,
                    &listener,
                    &log_file,
                    diagnostic.clone(),
                    observed_status,
                ));
                Ok(host::ReadinessOutcome::Withheld {
                    diagnostic,
                    observed_status,
                    budget_expired,
                })
            }
            Err(error) => Err(error),
        }
    }

    fn reconcile(
        &mut self,
        process: &EffectiveHostProcess,
    ) -> Result<host::ReadinessOutcome, RunnerError> {
        let listener = self.listener(process).ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "host process `{}` declares no listener to reconcile",
                process.name
            ))
        })?;
        let state_path = host_process_state_path(self.dir, &process.name);
        let log_file = self
            .dir
            .join(format!("{}.log", host::sanitize(&process.name)));
        let owner = managed_host_process_owner(self.project_path, self.policy, process);
        let source = HostLifecycleSource::for_policy(self.repo_root, self.policy, &process.name)?;
        match reconcile_published_listener(
            &source,
            &state_path,
            &process.name,
            &owner,
            self.route_table,
            &listener,
        ) {
            host::ListenerWait::Ready(state) => {
                self.result
                    .listeners
                    .push(listener_result(&process.name, &listener, &state));
                Ok(host::ReadinessOutcome::Ready)
            }
            host::ListenerWait::Withheld {
                diagnostic,
                observed_status,
                budget_expired,
            } => {
                // Nothing verified is exposed for this listener, so a
                // generation that still owns a route for it must not keep
                // serving it: settle the exact owned generation under the same
                // fenced stop a withheld start uses, so the reported route and
                // the store agree (effigy#455).
                let stale_generation = !budget_expired;
                let detail = if budget_expired {
                    "not started: the activation's observation budget expired before this listener was reached, and no generation from a previous activation is published".to_owned()
                } else {
                    format!(
                        "not started: the activation's observation budget expired before this listener was reached, and its published generation is not verified now ({diagnostic})"
                    )
                };
                let detail = if stale_generation {
                    settle_listener_generation(
                        self.repo_root,
                        self.policy,
                        &process.name,
                        "reconciliation",
                    )?;
                    format!(
                        "{detail}; that generation was settled so it no longer serves a route this activation does not expose"
                    )
                } else {
                    detail
                };
                self.result.failures.push(listener_failure(
                    process,
                    &listener,
                    &log_file,
                    detail.clone(),
                    observed_status,
                ));
                Ok(host::ReadinessOutcome::Withheld {
                    diagnostic: detail,
                    observed_status,
                    budget_expired,
                })
            }
        }
    }

    fn withdraw(
        &mut self,
        process: &EffectiveHostProcess,
        detail: String,
        observed_status: Option<u16>,
    ) -> Result<String, RunnerError> {
        let listener = self.listener(process);
        let log_file = self
            .dir
            .join(format!("{}.log", host::sanitize(&process.name)));
        settle_listener_generation(
            self.repo_root,
            self.policy,
            &process.name,
            "withheld dependency",
        )?;
        let Some(listener) = listener else {
            // A process without a listener serves no route, so there is no
            // exposure to withhold; its previous generation is stopped and the
            // console says why it was not restarted.
            eprintln!(
                "[host process] `{}` was not started: {detail}",
                process.name
            );
            return Ok(detail);
        };
        let detail = format!(
            "{detail}; any generation it ran before was settled so it no longer serves a route this activation does not expose"
        );
        self.result.failures.push(listener_failure(
            process,
            &listener,
            &log_file,
            detail.clone(),
            observed_status,
        ));
        Ok(detail)
    }
}

fn start_host_processes_inner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selection: Option<&BTreeSet<String>>,
    gate: ReplaceGate,
    deadline: Option<Instant>,
    publisher: &WaitPublisher,
) -> Result<HostProcessStart, RunnerError> {
    if policy.host_processes.is_empty() {
        return Ok(HostProcessStart::default());
    }
    host::validate_host_process_preflight(repo_root, policy)?;
    let checkout_root = repo_root.canonicalize().map_err(|error| {
        RunnerError::task_invocation(format!("failed to resolve checkout: {error}"))
    })?;
    let project_path = checkout_root.display().to_string();
    let listener_root = listener_gateway_root(policy)?;
    let route_table = listener_root
        .as_deref()
        .map(|root| root.join("routes.json"));
    let dir = host_process_dir_for_policy(&checkout_root, policy);
    host::create_host_process_dir(&checkout_root, &dir)?;
    let cx = effects();
    let state_root = journal_root(&checkout_root)?;
    let state_home = crate::runner::task_engine::state_home_path().ok_or_else(|| {
        RunnerError::task_invocation("HOME is unset; host process runs need a checkout journal")
    })?;
    let executable = crate::runner::task_engine::startup_executable()?;
    let checkout = effigy_containers::environment::EnvRef::for_policy(policy)
        .map_err(crate::runner::environment::error)?
        .checkout;
    // Resolved environment facts every host-process `env` may reference, and
    // one concrete loopback port per dynamic listener, taken from the OS before
    // any child launches (effigy#321, effigy#346).
    let mut facts = HostProcessEnvFacts::for_policy(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    facts.set_gateway_https_port(crate::runner::gateway_command::gateway_https_port());
    let listener_binds = reserve_listener_binds(&dir, policy, &mut facts)?;
    let ordered = order_host_processes(&policy.host_processes)?;
    let by_name = policy
        .host_processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<std::collections::HashMap<_, _>>();
    // #458: every entry's own wait. A process this activation starts is
    // published as it goes; one another activation started is already running
    // and only its readiness is re-confirmed.
    let starting = ordered
        .iter()
        .filter(|process| selection.is_none_or(|selection| selection.contains(&process.name)))
        .map(|process| process.name.clone())
        .collect::<BTreeSet<_>>();
    let mut graph = host::HostWaitGraph::for_policy(policy, &starting, publisher, Instant::now());
    // The ordering loop and its per-process operations: the loop decides what
    // starts, what is withheld and what every entry's wait says; this struct
    // starts the real supervisors and waits for the real listeners.
    let mut kernel = KernelActivation {
        repo_root,
        policy,
        dir: &dir,
        project_path: &project_path,
        checkout_root: &checkout_root,
        facts: &facts,
        listener_binds: &listener_binds,
        route_table: route_table.as_deref(),
        listener_root: listener_root.clone(),
        state_root,
        state_home: &state_home,
        executable: &executable,
        checkout: Some(checkout),
        gate,
        cx: &cx,
        deadline,
        by_name: &by_name,
        pending: Vec::new(),
        pending_generation: None,
        result: HostProcessStart::default(),
    };
    let start_result =
        host::run_host_activation(&ordered, selection, &mut graph, &mut kernel, &|| {
            Instant::now()
        });
    if let Err(error) = start_result {
        graph.abandon_plan(
            &format!(
                "stopped after {}",
                error.to_string().lines().next().unwrap_or_default()
            ),
            Instant::now(),
        );
        // Dropping the owning pipes without a detach is the kernel's
        // owner-loss stop; settle explicitly so the refusal is complete before
        // it returns. Only the processes this start replaced are settled: a
        // restart leaves every other host process and its route running.
        let replaced_nothing = kernel.result.started.is_empty();
        drop(std::mem::take(&mut kernel.result.started));
        if gate == ReplaceGate::HeldForReplace && replaced_nothing {
            // A restart that could not take its gate, or whose first replace
            // failed before a successor started, settles no other process:
            // whatever else runs today is left alone.
            return Err(error);
        }
        let report = stop_host_processes(repo_root, policy, selection);
        let residue = report.lines();
        if report.complete() {
            return Err(error);
        }
        return Err(RunnerError::task_invocation(format!(
            "{error}\nhost-process rollback could not confirm every stop:\n{}",
            residue.join("\n")
        )));
    }
    // Every declared process started and every listener that was waited for
    // published its own verified route (or was reported withheld). The caller
    // releases the owning pipes after a successful detach; a dropped client is
    // the kernel's owner-loss stop.
    Ok(kernel.result)
}
