use std::collections::BTreeSet;
use std::path::Path;

use effigy_manifest::{LoadedTaskManifest, ManifestDemoConfig};

use crate::{
    derive_gap_class, display_repo_path, load_active_attempt, load_active_terminal_session,
    load_attempt_history, load_latest_attempt, DemoActiveAttempt, DemoEntrypoint, DemoRecord,
    DemoRuntimeBackend, DemoStateError,
};

pub fn build_demo_record<FAlive, FBackend>(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    is_pid_alive: FAlive,
    resolve_runtime_backend: FBackend,
) -> Result<DemoRecord, DemoStateError>
where
    FAlive: Fn(u32) -> bool,
    FBackend:
        Fn(&Path, &LoadedTaskManifest, &DemoEntrypoint, &DemoActiveAttempt) -> DemoRuntimeBackend,
{
    let sources = demo_sources_for_id(repo_root, loaded, demo_id);
    let entrypoint = DemoEntrypoint::from_manifest(demo);
    let primary_source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| "effigy.toml".to_owned());
    let latest_attempt = load_latest_attempt(repo_root, demo_id, demo)?;
    let active_attempt = load_active_attempt(repo_root, demo_id, is_pid_alive)?;
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
        runtime_backend: resolve_runtime_backend(repo_root, loaded, &entrypoint, &active_attempt),
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
