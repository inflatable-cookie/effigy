use super::execute::{concurrent_runner_task_process_names, load_active_attempt};
use super::*;

pub(super) fn query_is_empty(query: &DemoListQuery) -> bool {
    query.search.is_none()
        && query.owner.is_none()
        && query.tag.is_none()
        && query.mode.is_none()
        && query.cover.is_none()
        && query.status.is_none()
        && query.gap.is_none()
        && !query.stale_only
}

pub(super) fn demo_list_query_to_json(query: &DemoListQuery) -> JsonValue {
    json!({
        "search": query.search,
        "owner": query.owner,
        "tag": query.tag,
        "mode": query.mode.map(|value| value.as_str()),
        "cover": query.cover,
        "status": query.status.map(|value| value.as_str()),
        "gap": query.gap.map(|value| value.as_str()),
        "stale_only": query.stale_only,
        "group_by": query.group_by.map(|value| value.as_str()),
    })
}

pub(super) fn demo_list_query_to_key_values(query: &DemoListQuery) -> Vec<KeyValue> {
    let mut values = Vec::new();
    if let Some(search) = &query.search {
        values.push(KeyValue::new("search", search.clone()));
    }
    if let Some(owner) = &query.owner {
        values.push(KeyValue::new("owner", owner.clone()));
    }
    if let Some(tag) = &query.tag {
        values.push(KeyValue::new("tag", tag.clone()));
    }
    if let Some(mode) = query.mode {
        values.push(KeyValue::new("mode", mode.as_str().to_owned()));
    }
    if let Some(cover) = &query.cover {
        values.push(KeyValue::new("cover", cover.clone()));
    }
    if let Some(status) = query.status {
        values.push(KeyValue::new("status", status.as_str().to_owned()));
    }
    if let Some(gap) = query.gap {
        values.push(KeyValue::new("gap", gap.as_str().to_owned()));
    }
    if query.stale_only {
        values.push(KeyValue::new("stale-only", "yes".to_owned()));
    }
    if let Some(group_by) = query.group_by {
        values.push(KeyValue::new("group-by", group_by.as_str().to_owned()));
    }
    values
}

pub(super) fn build_demo_groups<'a>(
    demos: &'a [DemoRecord],
    group_by: DemoListGroupBy,
) -> Vec<DemoGroup<'a>> {
    build_extracted_demo_groups(demos, demo_record_group_by(group_by))
}

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

pub(super) fn demo_matches_query(record: &DemoRecord, query: &DemoListQuery) -> bool {
    record.matches_filters(
        query.search.as_deref(),
        query.owner.as_deref(),
        query.tag.as_deref(),
        query.mode.map(|value| value.as_str()),
        query.cover.as_deref(),
        query.status.map(|value| value.as_str()),
        query.gap.map(|value| value.as_str()),
        query.stale_only,
    )
}

pub(super) fn build_demo_record(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoRecord, RunnerError> {
    let sources = demo_sources_for_id(repo_root, loaded, demo_id);
    let entrypoint = DemoEntrypoint::from_manifest(demo);
    let primary_source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| "effigy.toml".to_owned());
    let latest_attempt = load_latest_attempt(repo_root, demo_id, demo)?;
    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    let attempt_history = load_attempt_history(repo_root, demo_id)?;
    let active_terminal_session = load_active_terminal_session(repo_root, &active_attempt);
    let gap_class = derive_gap_class(demo.status, latest_attempt.stale);

    Ok(DemoRecord {
        id: demo_id.to_owned(),
        title: demo.title.clone(),
        summary: demo.summary.clone(),
        proof: demo.proof.clone(),
        owner: demo.owner.clone(),
        mode: demo.mode,
        status: demo.status,
        covers: demo.covers.clone(),
        tags: demo.tags.clone(),
        prerequisites: demo.prerequisites.clone(),
        dependencies: demo.dependencies.clone(),
        entrypoint: entrypoint.clone(),
        sources,
        primary_source,
        gap_class,
        runtime_backend: demo_runtime_backend(repo_root, loaded, &entrypoint, &active_attempt),
        active_attempt,
        active_terminal_session,
        latest_attempt,
        attempt_history,
    })
}

fn demo_sources_for_id(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
) -> Vec<String> {
    let prefix = format!("demos.{demo_id}.");
    let mut seen = BTreeSet::new();
    loaded
        .value_sources
        .iter()
        .filter(|entry| entry.path == format!("demos.{demo_id}") || entry.path.starts_with(&prefix))
        .filter_map(|entry| {
            let rendered = display_repo_path(&entry.source, repo_root);
            seen.insert(rendered.clone()).then_some(rendered)
        })
        .collect::<Vec<_>>()
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
            catalogs: &catalogs,
            task_scope_cwd: repo_root,
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
