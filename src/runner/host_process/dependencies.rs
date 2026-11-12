//! Host-process ordering and generation-bound dependency environments.

use super::*;

pub(super) fn order_host_processes(
    processes: &[EffectiveHostProcess],
) -> Result<Vec<&EffectiveHostProcess>, RunnerError> {
    let by_name = processes
        .iter()
        .map(|process| (process.name.as_str(), process))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::with_capacity(processes.len());
    let mut visiting = HashSet::new();
    let mut complete = HashSet::new();
    fn visit<'a>(
        process: &'a EffectiveHostProcess,
        by_name: &HashMap<&'a str, &'a EffectiveHostProcess>,
        visiting: &mut HashSet<String>,
        complete: &mut HashSet<String>,
        ordered: &mut Vec<&'a EffectiveHostProcess>,
    ) -> Result<(), RunnerError> {
        if complete.contains(&process.name) {
            return Ok(());
        }
        if !visiting.insert(process.name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "managed host process dependency cycle includes `{}`",
                process.name
            )));
        }
        for dependency in &process.depends_on {
            let target = by_name.get(dependency.as_str()).ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "host process `{}` depends on missing process `{dependency}`",
                    process.name
                ))
            })?;
            if target.listener.is_none() {
                return Err(RunnerError::task_invocation(format!(
                    "host process `{}` depends on `{dependency}`, which has no managed listener",
                    process.name
                )));
            }
            visit(target, by_name, visiting, complete, ordered)?;
        }
        visiting.remove(&process.name);
        complete.insert(process.name.clone());
        ordered.push(process);
        Ok(())
    }
    for process in processes {
        visit(
            process,
            &by_name,
            &mut visiting,
            &mut complete,
            &mut ordered,
        )?;
    }
    Ok(ordered)
}

fn dependency_env_prefix(name: &str) -> String {
    format!(
        "EFFIGY_MANAGED_HOST_{}",
        effigy_manifest::secret_env_name(name)
    )
}

/// What one dependency's published listener record says right now.
enum DependencyRecord {
    Present(Box<HostLifecycleObservation>),
    /// The record is gone: the dependency was stopped or never started.
    Missing,
    /// The record is there but cannot be read as this owner's state.
    Unreadable,
}

fn read_dependency_record(dependency: &HostProcessDependency) -> DependencyRecord {
    match kernel_run::read_lifecycle(dependency.source.as_ref(), &dependency.state_file) {
        Ok(Some(state)) if state.owner == dependency.owner => {
            DependencyRecord::Present(Box::new(state))
        }
        Ok(None) => DependencyRecord::Missing,
        _ => DependencyRecord::Unreadable,
    }
}

pub(super) fn managed_route_matches(
    route_table_path: &Path,
    state: &HostLifecycleObservation,
    stable_owner: &str,
) -> Result<bool, RunnerError> {
    let Some(owner) = state.route_owner.as_ref() else {
        return Ok(false);
    };
    let Some(domain) = state.route_domain.as_deref() else {
        return Ok(false);
    };
    let table = RouteTable::load(route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(table.lookup(domain).is_some_and(|route| {
        route.target.as_deref() == Some(owner.address.as_str())
            && route.tls == owner.route_tls
            && route.managed_listener.as_ref() == Some(owner)
            && owner.owner == stable_owner
            && owner.runtime_generation == state.runtime_generation
    }))
}

type DependencyEnvironment = (Vec<(String, String)>, Vec<String>);

/// A dependent's view of one dependency generation.
fn dependency_generation(
    dependency: &HostProcessDependency,
    state: &HostLifecycleObservation,
) -> String {
    format!(
        "{}:{}:{}",
        dependency.name, state.runtime_generation, state.generation
    )
}

fn dependency_environment(
    spec: &HostProcessRuntimeSpec,
    states: &[HostLifecycleObservation],
) -> DependencyEnvironment {
    let mut env = BTreeMap::new();
    let mut generations = Vec::with_capacity(states.len());
    for (dependency, state) in spec.dependencies.iter().zip(states) {
        let prefix = dependency_env_prefix(&dependency.name);
        env.insert(
            format!("{prefix}_INTERNAL_URL"),
            state.internal_url.clone().unwrap_or_default(),
        );
        env.insert(
            format!("{prefix}_PUBLIC_URL"),
            state.public_url.clone().unwrap_or_default(),
        );
        env.insert(format!("{prefix}_GENERATION"), state.generation.clone());
        generations.push(dependency_generation(dependency, state));
    }
    (env.into_iter().collect(), generations)
}

/// The dependency generations a dependent is currently running against.
/// `None` means one dependency's record could not be read now, so nothing is
/// concluded from it.
pub(super) fn dependency_generations(spec: &HostProcessRuntimeSpec) -> Option<Vec<String>> {
    let mut generations = Vec::with_capacity(spec.dependencies.len());
    for dependency in &spec.dependencies {
        match read_dependency_record(dependency) {
            DependencyRecord::Present(state) => {
                generations.push(dependency_generation(dependency, &state));
            }
            // A dependency that is gone was stopped or replaced.
            DependencyRecord::Missing => {
                generations.push(format!("{}:missing", dependency.name));
            }
            // An unreadable record is never a state change.
            DependencyRecord::Unreadable => return None,
        }
    }
    Some(generations)
}

pub(super) fn wait_for_dependency_environment(
    spec: &HostProcessRuntimeSpec,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<DependencyEnvironment>, RunnerError> {
    loop {
        if stop() {
            return Ok(None);
        }
        let mut states = Vec::with_capacity(spec.dependencies.len());
        for dependency in &spec.dependencies {
            match read_dependency_record(dependency) {
                DependencyRecord::Present(state)
                    if state.status == HostLifecycleStatus::Ready
                        && state.route_owner.is_some() =>
                {
                    states.push(*state);
                }
                _ => {
                    states.clear();
                    break;
                }
            }
        }
        if states.len() == spec.dependencies.len() {
            return Ok(Some(dependency_environment(spec, &states)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum HostChildWait {
    Exited(i32),
    DependencyChanged,
    Shutdown,
}

/// A dependent is replaced only when its dependency's supervisor replaced its
/// child generation. A momentary probe miss or an unreadable dependency record
/// is not a change.
pub(super) fn dependency_transition(
    spec: &HostProcessRuntimeSpec,
    expected_dependencies: &[String],
) -> Result<bool, RunnerError> {
    let Some(current) = dependency_generations(spec) else {
        return Ok(false);
    };
    Ok(current != expected_dependencies)
}

pub(super) fn wait_for_host_child(
    child: &mut effigy_kernel::custody::OwnedProcess,
    spec: &HostProcessRuntimeSpec,
    expected_dependencies: &[String],
    stop: &mut dyn FnMut() -> bool,
) -> Result<HostChildWait, RunnerError> {
    loop {
        if let effigy_kernel::observe::Observed::Verified(exit, _) = child.poll() {
            return Ok(HostChildWait::Exited(exit.code));
        }
        if stop() {
            return Ok(HostChildWait::Shutdown);
        }
        if dependency_transition(spec, expected_dependencies)? {
            return Ok(HostChildWait::DependencyChanged);
        }
        thread::sleep(Duration::from_millis(100));
    }
}
