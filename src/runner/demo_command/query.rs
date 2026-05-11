use super::execute::concurrent_runner_task_process_names;
use super::*;

fn demo_record_group_by(group_by: DemoListGroupBy) -> DemoRecordGroupBy {
    match group_by {
        DemoListGroupBy::Owner => DemoRecordGroupBy::Owner,
        DemoListGroupBy::Tag => DemoRecordGroupBy::Tag,
        DemoListGroupBy::Mode => DemoRecordGroupBy::Mode,
        DemoListGroupBy::Cover => DemoRecordGroupBy::Cover,
        DemoListGroupBy::Status => DemoRecordGroupBy::Status,
        DemoListGroupBy::Gap => DemoRecordGroupBy::Gap,
    }
}

pub(super) fn demo_list_request(query: &DemoListQuery) -> effigy_demo::DemoListRequest {
    effigy_demo::DemoListRequest {
        search: query.search.clone(),
        owner: query.owner.clone(),
        tag: query.tag.clone(),
        mode: query.mode.map(|value| value.as_str().to_owned()),
        cover: query.cover.clone(),
        status: query.status.map(|value| value.as_str().to_owned()),
        gap: query.gap.map(|value| value.as_str().to_owned()),
        stale_only: query.stale_only,
        group_by: query.group_by.map(demo_record_group_by),
    }
}

pub(super) fn demo_history_request(
    limit: Option<usize>,
    outcome: Option<DemoHistoryOutcome>,
    selected_attempt_id: Option<&str>,
    selected_attempt_ordinal: Option<usize>,
) -> effigy_demo::DemoHistoryRequest {
    effigy_demo::DemoHistoryRequest {
        limit,
        outcome: outcome.map(|value| value.as_str().to_owned()),
        selected_attempt_id: selected_attempt_id.map(str::to_owned),
        selected_attempt_ordinal,
    }
}

pub(super) fn build_demo_record(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoRecord, RunnerError> {
    effigy_demo::build_demo_record(
        repo_root,
        loaded,
        demo_id,
        demo,
        super::execute::pid_is_alive,
        demo_runtime_backend,
    )
    .map_err(Into::into)
}

pub(super) fn render_demo_run_command(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    run: &ManifestManagedRun,
) -> Result<String, RunnerError> {
    let catalogs = effigy_routing::discover_catalogs_allow_missing(repo_root)?;
    let task_env = BTreeMap::new();
    render_task_run_spec(
        run,
        RunSpecContext {
            task_name: demo_id,
            task_env: &task_env,
            task_env_file: None,
            env_profiles: &loaded.manifest.env,
            args_rendered: "",
            args_raw: &[],
            repo_root,
            bundle_root: loaded.bundle_root.as_deref(),
            catalogs: &catalogs,
            task_scope_cwd: repo_root,
            invocation_cwd: repo_root,
            runtime_env_schema_override: None,
            depth: 0,
            resolver: &effigy_routing::resolve_task_selection,
        },
    )
    .map_err(Into::into)
}

fn demo_runtime_backend(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    entrypoint: &DemoEntrypoint,
    active_attempt: &DemoActiveAttempt,
) -> DemoRuntimeBackend {
    if active_attempt.active {
        active_attempt.runtime_backend()
    } else {
        demo_runtime_backend_from_entrypoint(repo_root, loaded, entrypoint)
    }
}

fn demo_runtime_backend_from_entrypoint(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    entrypoint: &DemoEntrypoint,
) -> DemoRuntimeBackend {
    match entrypoint {
        DemoEntrypoint::Task(task_name) => {
            if loaded
                .manifest
                .tasks
                .get(task_name)
                .is_some_and(task_is_concurrent_runner_backed)
                || super::execute::demo_task_selection(repo_root, task_name)
                    .ok()
                    .flatten()
                    .and_then(|selection| {
                        selection.task().ok().map(task_is_concurrent_runner_backed)
                    })
                    .unwrap_or(false)
            {
                let managed_process_names =
                    concurrent_runner_task_process_names(repo_root, task_name).unwrap_or_default();
                concurrent_runner_runtime_backend(managed_process_names)
            } else {
                match entrypoint {
                    DemoEntrypoint::Task(_) => DemoRuntimeBackend::task(),
                    DemoEntrypoint::Run(_) => DemoRuntimeBackend::run(),
                }
            }
        }
        DemoEntrypoint::Run(_) => DemoRuntimeBackend::run(),
    }
}
