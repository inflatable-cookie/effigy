use std::collections::{BTreeMap, BTreeSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestDemoStatus,
};
use crate::tui::run_demo_browser_tui;
use crate::runner::util::with_local_node_bin_path;
use crate::ui::{KeyValue, NoticeLevel, Renderer, TableSpec};
use crate::{
    DemoArgs, DemoListGroupBy, DemoListQuery, DemoListStatus, DemoSubcommand, TaskInvocation,
};

use super::error::RunnerError;
use super::render::{encode_json, render_utf8, text_renderer};

const DEMO_RECEIPTS_DIR: &str = ".effigy/demo/receipts";
const DEMO_ACTIVE_DIR: &str = ".effigy/demo/active";

pub(super) fn run_demo(args: DemoArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;

    match args.subcommand {
        DemoSubcommand::Browser { group_by } => {
            if args.output_json {
                return demo_error(
                    true,
                    "effigy.demo.browser.v1",
                    "demo browser does not support json mode".to_owned(),
                    json!({ "repo_root": repo_root.display().to_string() }),
                );
            }
            run_demo_browser_tui(repo_root, group_by)?;
            Ok(String::new())
        }
        DemoSubcommand::List { query } => {
            render_demo_list(&repo_root, &loaded, &query, args.output_json)
        }
        DemoSubcommand::Inspect { demo_id } => {
            render_demo_inspect(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::Run { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Run,
        ),
        DemoSubcommand::Rerun { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Rerun,
        ),
        DemoSubcommand::Stop { demo_id } => {
            render_demo_stop(&repo_root, &loaded, &demo_id, args.output_json)
        }
    }
}

fn render_demo_list(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    query: &DemoListQuery,
    output_json: bool,
) -> Result<String, RunnerError> {
    let all_demos = loaded
        .manifest
        .demos
        .iter()
        .map(|(demo_id, demo)| build_demo_record(repo_root, loaded, demo_id, demo))
        .collect::<Result<Vec<_>, _>>()?;
    let demos = all_demos
        .into_iter()
        .filter(|demo| demo.matches_query(query))
        .collect::<Vec<_>>();
    let groups = query
        .group_by
        .map(|group_by| build_demo_groups(&demos, group_by));

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.list.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "query": demo_list_query_to_json(query),
                "group_by": query.group_by.map(|value| value.as_str()),
                "count": demos.len(),
                "total_count": loaded.manifest.demos.len(),
                "groups": groups.as_ref().map(|groups| groups.iter().map(DemoGroup::to_json).collect::<Vec<_>>()),
                "demos": demos.iter().map(DemoRecord::to_json_summary).collect::<Vec<_>>(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Registry")?;
    if demos.is_empty() {
        if query_is_empty(query) {
            renderer.notice(
                NoticeLevel::Info,
                "No demos are declared in the current effigy.toml manifest.",
            )?;
        } else {
            renderer.notice(
                NoticeLevel::Info,
                "No demos matched the current discovery query.",
            )?;
        }
        renderer.text("")?;
        return render_utf8(renderer.into_inner());
    }

    if !query_is_empty(query) {
        renderer.key_values(&demo_list_query_to_key_values(query))?;
        renderer.text("")?;
    }

    if let Some(groups) = groups {
        for group in groups {
            renderer.section(&format!("Group: {}", group.label))?;
            renderer.table(&demo_table_spec(&group.demos))?;
            renderer.text("")?;
        }
    } else {
        let demo_refs = demos.iter().collect::<Vec<_>>();
        renderer.table(&demo_table_spec(&demo_refs))?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to inspect proof intent, coverage, action availability, active state, and latest attempt details.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_inspect(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.inspect.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.inspect.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "demo": record.to_json_detail(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Inspect")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("summary", record.summary.clone()),
        KeyValue::new("proof", record.proof.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("mode", record.mode.as_str().to_owned()),
        KeyValue::new("base-status", record.status.as_str().to_owned()),
        KeyValue::new("effective-status", record.effective_status()),
        KeyValue::new("freshness", record.freshness_label().to_owned()),
        KeyValue::new("gap", record.gap_class.to_owned()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("defined-in", record.primary_source.clone()),
    ])?;
    renderer.text("")?;

    if !record.covers.is_empty() {
        renderer.bullet_list("covers", &record.covers)?;
        renderer.text("")?;
    }
    if !record.tags.is_empty() {
        renderer.bullet_list("tags", &record.tags)?;
        renderer.text("")?;
    }
    if record.sources.len() > 1 {
        renderer.bullet_list("sources", &record.sources)?;
        renderer.text("")?;
    }
    if !record.prerequisites.is_empty() {
        renderer.bullet_list("prerequisites", &record.prerequisites)?;
        renderer.text("")?;
    }
    if !record.dependencies.is_empty() {
        renderer.bullet_list("dependencies", &record.dependencies)?;
        renderer.text("")?;
    }

    renderer.section("Actions")?;
    renderer.key_values(&record.actions().to_key_values())?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&record.active_attempt.to_key_values())?;
    renderer.text("")?;

    renderer.section("Latest Attempt")?;
    let mut latest_values = vec![
        KeyValue::new("state", record.latest_attempt.state_label()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ];
    if let Some(outcome) = &record.latest_attempt.outcome {
        latest_values.push(KeyValue::new("outcome", outcome.clone()));
    }
    if let Some(summary) = &record.latest_attempt.summary {
        latest_values.push(KeyValue::new("summary", summary.clone()));
    }
    if let Some(parse_error) = &record.latest_attempt.parse_error {
        latest_values.push(KeyValue::new("receipt-parse", parse_error.clone()));
    }
    renderer.key_values(&latest_values)?;
    if !record.latest_attempt.artifacts.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("artifacts", &record.latest_attempt.artifacts)?;
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_execute(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    invocation: DemoInvocationKind,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            invocation.schema(),
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    if active_attempt.active {
        return demo_error(
            output_json,
            invocation.schema(),
            format!(
                "demo `{demo_id}` already has an active attempt; stop it before starting a fresh run"
            ),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }

    let attempt = execute_demo_attempt(repo_root, demo_id, demo, output_json)?;
    write_latest_attempt_receipt(repo_root, demo_id, demo, &attempt)?;
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        let rendered = encode_json(
            &json!({
                "schema": invocation.schema(),
                "schema_version": 1,
                "ok": attempt.ok,
                "repo_root": repo_root.display().to_string(),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "execution": attempt.to_json(),
                "active_attempt": record.active_attempt.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        )?;
        if attempt.ok {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }

    if attempt.ok {
        return render_demo_execute_text(&record, &attempt, invocation.title());
    }

    Err(RunnerError::task_invocation(format!(
        "demo `{demo_id}` failed; latest attempt written to {}",
        record
            .latest_attempt
            .receipt_path
            .as_deref()
            .unwrap_or("<none>")
    )))
}

fn render_demo_stop(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    match demo_entrypoint(demo) {
        DemoEntrypoint::Task(task_name) => {
            return demo_error(
                output_json,
                "effigy.demo.stop.v1",
                format!(
                    "demo `{demo_id}` uses task entrypoint `{task_name}`; stop is not supported until task execution exposes cancellable handles"
                ),
                json!({
                    "demo_id": demo_id,
                    "entrypoint": { "kind": "task", "value": task_name },
                    "active_attempt": active_attempt.to_json(),
                }),
            );
        }
        DemoEntrypoint::Run(_) => {}
    }

    if !active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` has no active attempt to stop"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }
    if !active_attempt.stoppable {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but not stoppable through the current runtime"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }

    let mut persisted = read_active_attempt_record(repo_root, demo_id)?.ok_or_else(|| {
        RunnerError::task_invocation(format!("demo `{demo_id}` has no active attempt to stop"))
    })?;
    if persisted.phase == PersistedDemoActivePhase::StopRequested {
        return render_demo_stop_result(
            repo_root,
            loaded,
            demo_id,
            output_json,
            "stop already requested",
            demo_active_attempt_from_record(
                repo_root,
                demo_id,
                &persisted,
                render_active_attempt_path(repo_root, demo_id),
            ),
        );
    }

    let Some(target_pid) = persisted.target_pid else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but has no stoppable process handle"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    };

    if !pid_is_alive(target_pid) {
        clear_active_attempt_state(repo_root, demo_id);
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is no longer running"),
            json!({
                "demo_id": demo_id,
                "active_attempt": DemoActiveAttempt::inactive(Some(render_active_attempt_path(repo_root, demo_id))).to_json(),
            }),
        );
    }

    request_demo_termination(target_pid)?;
    persisted.phase = PersistedDemoActivePhase::StopRequested;
    write_active_attempt_record(repo_root, demo_id, &persisted)?;
    render_demo_stop_result(
        repo_root,
        loaded,
        demo_id,
        output_json,
        "stop requested",
        demo_active_attempt_from_record(
            repo_root,
            demo_id,
            &persisted,
            render_active_attempt_path(repo_root, demo_id),
        ),
    )
}

fn render_demo_stop_result(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    summary: &str,
    reported_active_attempt: DemoActiveAttempt,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.stop.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "message": format!("demo `{demo_id}` {summary}"),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "active_attempt": reported_active_attempt.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Stop")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("state", reported_active_attempt.state_label().to_owned()),
        KeyValue::new(
            "stoppable",
            if reported_active_attempt.stoppable {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ),
    ])?;
    renderer.text("")?;
    let message = format!("demo `{demo_id}` {summary}");
    renderer.notice(NoticeLevel::Info, &message)?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn render_demo_execute_text(
    record: &DemoRecord,
    attempt: &DemoExecutionAttempt,
    section_title: &str,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.section(section_title)?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("outcome", attempt.outcome.clone()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ])?;
    if let Some(summary) = &attempt.summary {
        renderer.text("")?;
        renderer.notice(NoticeLevel::Info, summary)?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to review the recorded latest attempt and any active state.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
}

fn query_is_empty(query: &DemoListQuery) -> bool {
    query.search.is_none()
        && query.owner.is_none()
        && query.tag.is_none()
        && query.mode.is_none()
        && query.cover.is_none()
        && query.status.is_none()
        && query.gap.is_none()
        && !query.stale_only
}

fn demo_list_query_to_json(query: &DemoListQuery) -> JsonValue {
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

fn demo_list_query_to_key_values(query: &DemoListQuery) -> Vec<KeyValue> {
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

fn demo_table_spec(demos: &[&DemoRecord]) -> TableSpec {
    TableSpec::new(
        vec![
            "ID".to_owned(),
            "Title".to_owned(),
            "Owner".to_owned(),
            "Mode".to_owned(),
            "Status".to_owned(),
            "Gap".to_owned(),
            "Actions".to_owned(),
            "Entrypoint".to_owned(),
        ],
        demos
            .iter()
            .map(|demo| {
                vec![
                    demo.id.clone(),
                    demo.title.clone(),
                    demo.owner.clone(),
                    demo.mode.as_str().to_owned(),
                    demo.effective_status(),
                    demo.gap_class.to_owned(),
                    demo.actions().summary_label(),
                    demo.entrypoint.render_compact(),
                ]
            })
            .collect(),
    )
}

fn build_demo_groups<'a>(demos: &'a [DemoRecord], group_by: DemoListGroupBy) -> Vec<DemoGroup<'a>> {
    let mut groups: BTreeMap<String, Vec<&DemoRecord>> = BTreeMap::new();
    for demo in demos {
        match group_by {
            DemoListGroupBy::Owner => {
                groups.entry(demo.owner.clone()).or_default().push(demo);
            }
            DemoListGroupBy::Tag => {
                if demo.tags.is_empty() {
                    groups
                        .entry("(untagged)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for tag in &demo.tags {
                        groups.entry(tag.clone()).or_default().push(demo);
                    }
                }
            }
            DemoListGroupBy::Mode => {
                groups
                    .entry(demo.mode.as_str().to_owned())
                    .or_default()
                    .push(demo);
            }
            DemoListGroupBy::Cover => {
                if demo.covers.is_empty() {
                    groups
                        .entry("(unmapped)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for cover in &demo.covers {
                        groups.entry(cover.clone()).or_default().push(demo);
                    }
                }
            }
            DemoListGroupBy::Status => {
                groups
                    .entry(demo.effective_status())
                    .or_default()
                    .push(demo);
            }
            DemoListGroupBy::Gap => {
                groups
                    .entry(demo.gap_class.to_owned())
                    .or_default()
                    .push(demo);
            }
        }
    }

    groups
        .into_iter()
        .map(|(label, demos)| DemoGroup { label, demos })
        .collect()
}

fn availability_label(available: bool, reason: Option<&str>) -> String {
    if available {
        "yes".to_owned()
    } else if let Some(reason) = reason {
        format!("no ({reason})")
    } else {
        "no".to_owned()
    }
}

fn build_demo_record(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoRecord, RunnerError> {
    let sources = demo_sources_for_id(repo_root, loaded, demo_id);
    let primary_source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| "effigy.toml".to_owned());
    let latest_attempt = load_latest_attempt(repo_root, demo_id, demo)?;
    let active_attempt = load_active_attempt(repo_root, demo_id)?;
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
        entrypoint: demo_entrypoint(demo),
        sources,
        primary_source,
        gap_class,
        active_attempt,
        latest_attempt,
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

fn demo_entrypoint(demo: &ManifestDemoConfig) -> DemoEntrypoint {
    if let Some(task) = &demo.task {
        DemoEntrypoint::Task(task.clone())
    } else if let Some(run) = &demo.run {
        DemoEntrypoint::Run(run.clone())
    } else {
        DemoEntrypoint::Run("<invalid>".to_owned())
    }
}

fn load_latest_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoLatestAttempt, RunnerError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    let mut artifacts = demo.artifacts.clone();
    let rendered_receipt_path = display_repo_path(&receipt_path, repo_root);
    if !receipt_path.exists() {
        return Ok(DemoLatestAttempt {
            recorded: false,
            receipt_path: Some(rendered_receipt_path),
            outcome: None,
            summary: None,
            stale: false,
            artifacts,
            parse_error: None,
        });
    }

    let content = fs::read_to_string(&receipt_path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&receipt_path, error))?;
    let parsed = match serde_json::from_str::<JsonValue>(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(DemoLatestAttempt {
                recorded: true,
                receipt_path: Some(rendered_receipt_path),
                outcome: None,
                summary: None,
                stale: false,
                artifacts,
                parse_error: Some(error.to_string()),
            });
        }
    };

    if let Some(receipt_artifacts) = parsed.get("artifacts").and_then(normalize_artifact_refs) {
        for artifact in receipt_artifacts {
            if !artifacts.contains(&artifact) {
                artifacts.push(artifact);
            }
        }
    }

    Ok(DemoLatestAttempt {
        recorded: true,
        receipt_path: Some(rendered_receipt_path),
        outcome: parsed
            .get("status")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        summary: parsed
            .get("summary")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        stale: parsed
            .get("stale")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || parsed
                .get("freshness")
                .and_then(JsonValue::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("stale")),
        artifacts,
        parse_error: None,
    })
}

fn load_active_attempt(repo_root: &Path, demo_id: &str) -> Result<DemoActiveAttempt, RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    let rendered_path = display_repo_path(&path, repo_root);
    let Some(record) = read_active_attempt_record(repo_root, demo_id)? else {
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    };

    let target_alive = record.target_pid.is_none_or(pid_is_alive);
    let owner_alive = pid_is_alive(record.owner_pid);
    if !owner_alive || !target_alive {
        clear_active_attempt_state(repo_root, demo_id);
        return Ok(DemoActiveAttempt::inactive(Some(rendered_path)));
    }

    Ok(demo_active_attempt_from_record(
        repo_root,
        demo_id,
        &record,
        rendered_path,
    ))
}

fn demo_active_attempt_from_record(
    _repo_root: &Path,
    _demo_id: &str,
    record: &PersistedDemoActiveAttempt,
    rendered_path: String,
) -> DemoActiveAttempt {
    DemoActiveAttempt {
        active: true,
        state: record.phase.rendered().to_owned(),
        attempt_id: Some(record.attempt_id.clone()),
        state_path: Some(rendered_path),
        owner_pid: Some(record.owner_pid),
        target_pid: record.target_pid,
        stoppable: record.stoppable,
        started_at_epoch_ms: Some(record.started_at_epoch_ms),
        entrypoint_kind: Some(record.entrypoint_kind.clone()),
        entrypoint_value: Some(record.entrypoint_value.clone()),
        command: Some(record.command.clone()),
        parse_error: None,
    }
}

fn read_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
) -> Result<Option<PersistedDemoActiveAttempt>, RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
    let parsed = serde_json::from_str::<PersistedDemoActiveAttempt>(&content)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
    Ok(Some(parsed))
}

fn normalize_artifact_refs(value: &JsonValue) -> Option<Vec<String>> {
    let entries = value.as_array()?;
    let mut rendered = Vec::new();
    for entry in entries {
        match entry {
            JsonValue::String(path) if !path.trim().is_empty() => rendered.push(path.clone()),
            JsonValue::Object(map) => {
                if let Some(path) = map.get("path").and_then(JsonValue::as_str) {
                    if !path.trim().is_empty() {
                        rendered.push(path.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Some(rendered)
}

fn execute_demo_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    match demo_entrypoint(demo) {
        DemoEntrypoint::Task(task_name) => {
            execute_task_backed_demo(repo_root, demo_id, &task_name, output_json)
        }
        DemoEntrypoint::Run(run_command) => {
            execute_run_backed_demo(repo_root, demo_id, &run_command, output_json)
        }
    }
}

fn execute_task_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let attempt_id = build_attempt_id(demo_id);
    let _active_guard = register_active_attempt(
        repo_root,
        demo_id,
        PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: None,
            stoppable: false,
            entrypoint_kind: "task".to_owned(),
            entrypoint_value: task_name.to_owned(),
            command: task_name.to_owned(),
        },
    )?;

    if output_json {
        let task = TaskInvocation {
            name: task_name.to_owned(),
            args: vec!["--json".to_owned()],
        };
        return match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
            Ok(rendered) => parse_task_backed_attempt_json(demo_id, task_name, &rendered),
            Err(RunnerError::CommandJsonFailure { rendered }) => {
                parse_task_backed_attempt_json(demo_id, task_name, &rendered)
            }
            Err(error) => Ok(failed_demo_attempt(
                "task",
                task_name,
                task_name,
                None,
                format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
                String::new(),
                String::new(),
            )),
        };
    }

    let task = TaskInvocation {
        name: task_name.to_owned(),
        args: Vec::new(),
    };
    match run_manifest_task_with_cwd(&task, repo_root.to_path_buf()) {
        Ok(_) => Ok(successful_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            Some(format!(
                "Demo `{demo_id}` completed via task `{task_name}`."
            )),
            String::new(),
            String::new(),
        )),
        Err(RunnerError::TaskCommandFailure { code, .. }) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            code,
            format!("Demo `{demo_id}` failed via task `{task_name}`."),
            String::new(),
            String::new(),
        )),
        Err(error) => Ok(failed_demo_attempt(
            "task",
            task_name,
            task_name,
            None,
            format!("Demo `{demo_id}` failed to run task `{task_name}`: {error}"),
            String::new(),
            String::new(),
        )),
    }
}

fn parse_task_backed_attempt_json(
    demo_id: &str,
    task_name: &str,
    rendered: &str,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let parsed: JsonValue = serde_json::from_str(rendered).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse json task payload for demo `{demo_id}` task `{task_name}`: {error}"
        ))
    })?;
    let ok = parsed
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let exit_code = parsed
        .get("exit_code")
        .and_then(JsonValue::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    let stdout = parsed
        .get("stdout")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let stderr = parsed
        .get("stderr")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let command = parsed
        .get("command")
        .and_then(JsonValue::as_str)
        .unwrap_or(task_name)
        .to_owned();

    Ok(DemoExecutionAttempt {
        ok,
        outcome: if ok {
            "passed".to_owned()
        } else {
            "failed".to_owned()
        },
        entrypoint_kind: "task".to_owned(),
        entrypoint_value: task_name.to_owned(),
        command,
        exit_code,
        summary: Some(if ok {
            format!("Demo `{demo_id}` completed via task `{task_name}`.")
        } else {
            format!("Demo `{demo_id}` failed via task `{task_name}`.")
        }),
        stdout,
        stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    })
}

fn execute_run_backed_demo(
    repo_root: &Path,
    demo_id: &str,
    run_command: &str,
    output_json: bool,
) -> Result<DemoExecutionAttempt, RunnerError> {
    let mut child = build_run_backed_process(repo_root, run_command, output_json)?
        .spawn()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to launch run entrypoint: {error}"
            ))
        })?;

    let attempt_id = build_attempt_id(demo_id);
    let _active_guard = register_active_attempt(
        repo_root,
        demo_id,
        PersistedDemoActiveAttempt {
            schema: "effigy.demo.active.v1".to_owned(),
            schema_version: 1,
            attempt_id,
            demo_id: demo_id.to_owned(),
            phase: PersistedDemoActivePhase::Running,
            started_at_epoch_ms: now_epoch_ms(),
            owner_pid: std::process::id(),
            target_pid: Some(child.id()),
            stoppable: true,
            entrypoint_kind: "run".to_owned(),
            entrypoint_value: run_command.to_owned(),
            command: run_command.to_owned(),
        },
    )?;

    if output_json {
        let output = child.wait_with_output().map_err(|error| {
            RunnerError::task_invocation(format!(
                "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
            ))
        })?;
        let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
        return Ok(run_attempt_from_output(
            demo_id,
            run_command,
            output.status.code(),
            output.status.success(),
            stop_requested,
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let status = child.wait().map_err(|error| {
        RunnerError::task_invocation(format!(
            "Demo `{demo_id}` failed to wait for run entrypoint: {error}"
        ))
    })?;
    let stop_requested = active_attempt_is_stop_requested(repo_root, demo_id);
    Ok(run_attempt_from_output(
        demo_id,
        run_command,
        status.code(),
        status.success(),
        stop_requested,
        String::new(),
        String::new(),
    ))
}

fn build_run_backed_process(
    repo_root: &Path,
    run_command: &str,
    capture_output: bool,
) -> Result<ProcessCommand, RunnerError> {
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(run_command).current_dir(repo_root);
    if capture_output {
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        process.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    #[cfg(unix)]
    unsafe {
        process.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    with_local_node_bin_path(&mut process, repo_root);
    Ok(process)
}

fn run_attempt_from_output(
    demo_id: &str,
    run_command: &str,
    exit_code: Option<i32>,
    success: bool,
    stop_requested: bool,
    stdout: String,
    stderr: String,
) -> DemoExecutionAttempt {
    if stop_requested {
        return terminated_demo_attempt(
            "run",
            run_command,
            run_command,
            exit_code,
            format!("Demo `{demo_id}` was terminated after stop was requested."),
            stdout,
            stderr,
        );
    }
    if success {
        return successful_demo_attempt(
            "run",
            run_command,
            run_command,
            exit_code,
            Some(format!("Demo `{demo_id}` completed via run entrypoint.")),
            stdout,
            stderr,
        );
    }
    failed_demo_attempt(
        "run",
        run_command,
        run_command,
        exit_code,
        format!("Demo `{demo_id}` failed via run entrypoint."),
        stdout,
        stderr,
    )
}

fn active_attempt_is_stop_requested(repo_root: &Path, demo_id: &str) -> bool {
    read_active_attempt_record(repo_root, demo_id)
        .ok()
        .flatten()
        .is_some_and(|record| record.phase == PersistedDemoActivePhase::StopRequested)
}

fn register_active_attempt(
    repo_root: &Path,
    demo_id: &str,
    record: PersistedDemoActiveAttempt,
) -> Result<DemoActiveAttemptGuard, RunnerError> {
    write_active_attempt_record(repo_root, demo_id, &record)?;
    Ok(DemoActiveAttemptGuard {
        path: effective_active_attempt_path(repo_root, demo_id),
    })
}

fn write_active_attempt_record(
    repo_root: &Path,
    demo_id: &str,
    record: &PersistedDemoActiveAttempt,
) -> Result<(), RunnerError> {
    let path = effective_active_attempt_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }
    let rendered = serde_json::to_string_pretty(record)
        .map_err(|error| RunnerError::task_invocation_failed_render(&path, error))?;
    fs::write(&path, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

fn clear_active_attempt_state(repo_root: &Path, demo_id: &str) {
    let path = effective_active_attempt_path(repo_root, demo_id);
    let _ = fs::remove_file(path);
}

fn request_demo_termination(target_pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        let raw = target_pid as i32;
        match signal::kill(Pid::from_raw(-raw), Signal::SIGTERM) {
            Ok(()) => Ok(()),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "failed to send stop signal to demo process group `{target_pid}`: {error}"
            ))),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = target_pid;
        Err(RunnerError::task_invocation(
            "demo stop is not supported on this platform in the current runtime".to_owned(),
        ))
    }
}

fn successful_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: Option<String>,
    stdout: String,
    stderr: String,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: true,
        outcome: "passed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary,
        stdout,
        stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn failed_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "failed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn terminated_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "terminated".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), RunnerError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| RunnerError::task_invocation_failed_write(parent, error))?;
    }

    let rendered = serde_json::to_string_pretty(&json!({
        "schema": "effigy.demo.receipt.v1",
        "schema_version": 1,
        "demo_id": demo_id,
        "ok": attempt.ok,
        "status": attempt.outcome,
        "summary": attempt.summary,
        "stale": false,
        "recorded_at_epoch_ms": attempt.recorded_at_epoch_ms,
        "entrypoint": {
            "kind": attempt.entrypoint_kind,
            "value": attempt.entrypoint_value,
        },
        "command": attempt.command,
        "exit_code": attempt.exit_code,
        "artifacts": demo.artifacts,
    }))
    .map_err(|error| RunnerError::task_invocation_failed_render(&receipt_path, error))?;

    fs::write(&receipt_path, rendered)
        .map_err(|error| RunnerError::task_invocation_failed_write(&receipt_path, error))
}

fn effective_receipt_path(repo_root: &Path, demo_id: &str, demo: &ManifestDemoConfig) -> PathBuf {
    if let Some(path) = &demo.receipt {
        return repo_root.join(path);
    }
    repo_root
        .join(DEMO_RECEIPTS_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

fn effective_active_attempt_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root
        .join(DEMO_ACTIVE_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

fn render_active_attempt_path(repo_root: &Path, demo_id: &str) -> String {
    display_repo_path(
        &effective_active_attempt_path(repo_root, demo_id),
        repo_root,
    )
}

fn build_attempt_id(demo_id: &str) -> String {
    format!(
        "{}-{}",
        sanitize_demo_id_for_filename(demo_id),
        now_epoch_ms()
    )
}

fn sanitize_demo_id_for_filename(demo_id: &str) -> String {
    demo_id
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let raw = pid as i32;
    match signal::kill(Pid::from_raw(raw), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(not(unix))]
fn pid_is_alive(pid: u32) -> bool {
    pid != 0
}

fn derive_gap_class(status: ManifestDemoStatus, stale: bool) -> &'static str {
    if stale {
        return "stale";
    }
    match status {
        ManifestDemoStatus::Planned => "planned",
        ManifestDemoStatus::Missing => "missing",
        ManifestDemoStatus::Broken => "broken",
        ManifestDemoStatus::Ready
        | ManifestDemoStatus::Running
        | ManifestDemoStatus::Passed
        | ManifestDemoStatus::Failed => "existing",
    }
}

fn display_status(
    status: ManifestDemoStatus,
    stale: bool,
    active_attempt: &DemoActiveAttempt,
) -> String {
    if active_attempt.active {
        return match active_attempt.state.as_str() {
            "stop-requested" => "running (stop-requested)".to_owned(),
            _ => "running".to_owned(),
        };
    }
    if stale {
        format!("{} (stale)", status.as_str())
    } else {
        status.as_str().to_owned()
    }
}

fn display_repo_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn demo_error(
    output_json: bool,
    schema: &str,
    message: String,
    extra: JsonValue,
) -> Result<String, RunnerError> {
    if output_json {
        let mut payload = serde_json::Map::new();
        payload.insert("schema".to_owned(), JsonValue::String(schema.to_owned()));
        payload.insert("schema_version".to_owned(), JsonValue::from(1));
        payload.insert("ok".to_owned(), JsonValue::Bool(false));
        payload.insert("message".to_owned(), JsonValue::String(message.clone()));
        if let JsonValue::Object(extra_map) = extra {
            payload.extend(extra_map);
        }
        let rendered = encode_json(&JsonValue::Object(payload), true)?;
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    Err(RunnerError::task_invocation(message))
}

#[derive(Debug, Clone, Copy)]
enum DemoInvocationKind {
    Run,
    Rerun,
}

impl DemoInvocationKind {
    fn schema(&self) -> &'static str {
        match self {
            Self::Run => "effigy.demo.run.v1",
            Self::Rerun => "effigy.demo.rerun.v1",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Run => "Demo Run",
            Self::Rerun => "Demo Rerun",
        }
    }
}

#[derive(Debug, Clone)]
struct DemoRecord {
    id: String,
    title: String,
    summary: String,
    proof: String,
    owner: String,
    mode: ManifestDemoMode,
    status: ManifestDemoStatus,
    covers: Vec<String>,
    tags: Vec<String>,
    prerequisites: Vec<String>,
    dependencies: Vec<String>,
    entrypoint: DemoEntrypoint,
    sources: Vec<String>,
    primary_source: String,
    gap_class: &'static str,
    active_attempt: DemoActiveAttempt,
    latest_attempt: DemoLatestAttempt,
}

impl DemoRecord {
    fn effective_status(&self) -> String {
        display_status(self.status, self.latest_attempt.stale, &self.active_attempt)
    }

    fn freshness_label(&self) -> &'static str {
        if self.latest_attempt.stale {
            "stale"
        } else {
            "current"
        }
    }

    fn actions(&self) -> DemoActionAvailability {
        let can_run = !self.active_attempt.active;
        let can_rerun = !self.active_attempt.active;
        let can_stop = self.active_attempt.active && self.active_attempt.stoppable;
        DemoActionAvailability {
            run_available: can_run,
            run_reason: (!can_run).then(|| {
                "an active attempt already exists; stop it before starting a fresh run".to_owned()
            }),
            stop_available: can_stop,
            stop_reason: if can_stop {
                None
            } else if self.active_attempt.active {
                Some("the active attempt is not stoppable through the current runtime".to_owned())
            } else {
                Some("no active attempt is currently running".to_owned())
            },
            rerun_available: can_rerun,
            rerun_reason: (!can_rerun)
                .then(|| "an active attempt already exists; stop it before rerunning".to_owned()),
        }
    }

    fn to_json_summary(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
        })
    }

    fn to_json_detail(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "proof": self.proof,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "prerequisites": self.prerequisites,
            "dependencies": self.dependencies,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "sources": self.sources,
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
        })
    }

    fn matches_query(&self, query: &DemoListQuery) -> bool {
        if let Some(search) = &query.search {
            let needle = search.to_ascii_lowercase();
            let haystacks = [&self.id, &self.title, &self.summary];
            if !haystacks
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&needle))
            {
                return false;
            }
        }
        if let Some(owner) = &query.owner {
            if &self.owner != owner {
                return false;
            }
        }
        if let Some(tag) = &query.tag {
            if !self.tags.iter().any(|value| value == tag) {
                return false;
            }
        }
        if let Some(mode) = query.mode {
            if self.mode.as_str() != mode.as_str() {
                return false;
            }
        }
        if let Some(cover) = &query.cover {
            if !self.covers.iter().any(|value| value == cover) {
                return false;
            }
        }
        if let Some(status) = query.status {
            if self.browser_status() != status {
                return false;
            }
        }
        if let Some(gap) = query.gap {
            if self.gap_class != gap.as_str() {
                return false;
            }
        }
        if query.stale_only && !self.latest_attempt.stale {
            return false;
        }
        true
    }

    fn browser_status(&self) -> DemoListStatus {
        if self.active_attempt.active {
            return DemoListStatus::Running;
        }
        match self.status {
            ManifestDemoStatus::Planned => DemoListStatus::Planned,
            ManifestDemoStatus::Ready => DemoListStatus::Ready,
            ManifestDemoStatus::Running => DemoListStatus::Running,
            ManifestDemoStatus::Passed => DemoListStatus::Passed,
            ManifestDemoStatus::Failed => DemoListStatus::Failed,
            ManifestDemoStatus::Broken => DemoListStatus::Broken,
            ManifestDemoStatus::Missing => DemoListStatus::Missing,
        }
    }
}

#[derive(Debug, Clone)]
struct DemoActionAvailability {
    run_available: bool,
    run_reason: Option<String>,
    stop_available: bool,
    stop_reason: Option<String>,
    rerun_available: bool,
    rerun_reason: Option<String>,
}

impl DemoActionAvailability {
    fn summary_label(&self) -> String {
        let mut actions = Vec::new();
        if self.run_available {
            actions.push("run");
        }
        if self.stop_available {
            actions.push("stop");
        }
        if self.rerun_available {
            actions.push("rerun");
        }
        if actions.is_empty() {
            "none".to_owned()
        } else {
            actions.join(", ")
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "run": {
                "available": self.run_available,
                "reason": self.run_reason,
            },
            "stop": {
                "available": self.stop_available,
                "reason": self.stop_reason,
            },
            "rerun": {
                "available": self.rerun_available,
                "reason": self.rerun_reason,
            },
        })
    }

    fn to_key_values(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new(
                "run",
                availability_label(self.run_available, self.run_reason.as_deref()),
            ),
            KeyValue::new(
                "stop",
                availability_label(self.stop_available, self.stop_reason.as_deref()),
            ),
            KeyValue::new(
                "rerun",
                availability_label(self.rerun_available, self.rerun_reason.as_deref()),
            ),
        ]
    }
}

#[derive(Debug, Clone)]
struct DemoGroup<'a> {
    label: String,
    demos: Vec<&'a DemoRecord>,
}

impl DemoGroup<'_> {
    fn to_json(&self) -> JsonValue {
        json!({
            "label": self.label,
            "count": self.demos.len(),
            "demos": self.demos.iter().map(|demo| demo.to_json_summary()).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone)]
enum DemoEntrypoint {
    Task(String),
    Run(String),
}

impl DemoEntrypoint {
    fn render_compact(&self) -> String {
        match self {
            Self::Task(task) => format!("task:{task}"),
            Self::Run(run) => format!("run:{run}"),
        }
    }

    fn render_full(&self) -> String {
        match self {
            Self::Task(task) => format!("task `{task}`"),
            Self::Run(run) => format!("run `{run}`"),
        }
    }

    fn to_json(&self) -> JsonValue {
        match self {
            Self::Task(task) => json!({ "kind": "task", "value": task }),
            Self::Run(run) => json!({ "kind": "run", "value": run }),
        }
    }
}

#[derive(Debug, Clone)]
struct DemoActiveAttempt {
    active: bool,
    state: String,
    attempt_id: Option<String>,
    state_path: Option<String>,
    owner_pid: Option<u32>,
    target_pid: Option<u32>,
    stoppable: bool,
    started_at_epoch_ms: Option<u128>,
    entrypoint_kind: Option<String>,
    entrypoint_value: Option<String>,
    command: Option<String>,
    parse_error: Option<String>,
}

impl DemoActiveAttempt {
    fn inactive(state_path: Option<String>) -> Self {
        Self {
            active: false,
            state: "not-active".to_owned(),
            attempt_id: None,
            state_path,
            owner_pid: None,
            target_pid: None,
            stoppable: false,
            started_at_epoch_ms: None,
            entrypoint_kind: None,
            entrypoint_value: None,
            command: None,
            parse_error: None,
        }
    }

    fn state_label(&self) -> &str {
        &self.state
    }

    fn to_key_values(&self) -> Vec<KeyValue> {
        let mut values = vec![
            KeyValue::new("state", self.state.clone()),
            KeyValue::new(
                "stoppable",
                if self.stoppable {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                },
            ),
            KeyValue::new(
                "state-path",
                self.state_path
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
        ];
        if let Some(attempt_id) = &self.attempt_id {
            values.push(KeyValue::new("attempt-id", attempt_id.clone()));
        }
        if let Some(owner_pid) = self.owner_pid {
            values.push(KeyValue::new("owner-pid", owner_pid.to_string()));
        }
        if let Some(target_pid) = self.target_pid {
            values.push(KeyValue::new("target-pid", target_pid.to_string()));
        }
        if let Some(started_at_epoch_ms) = self.started_at_epoch_ms {
            values.push(KeyValue::new(
                "started-at-epoch-ms",
                started_at_epoch_ms.to_string(),
            ));
        }
        if let (Some(kind), Some(value)) = (&self.entrypoint_kind, &self.entrypoint_value) {
            values.push(KeyValue::new("entrypoint", format!("{kind}:{value}")));
        }
        if let Some(command) = &self.command {
            values.push(KeyValue::new("command", command.clone()));
        }
        if let Some(parse_error) = &self.parse_error {
            values.push(KeyValue::new("parse-error", parse_error.clone()));
        }
        values
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "active": self.active,
            "state": self.state,
            "attempt_id": self.attempt_id,
            "state_path": self.state_path,
            "owner_pid": self.owner_pid,
            "target_pid": self.target_pid,
            "stoppable": self.stoppable,
            "started_at_epoch_ms": self.started_at_epoch_ms,
            "entrypoint": {
                "kind": self.entrypoint_kind,
                "value": self.entrypoint_value,
            },
            "command": self.command,
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoLatestAttempt {
    recorded: bool,
    receipt_path: Option<String>,
    outcome: Option<String>,
    summary: Option<String>,
    stale: bool,
    artifacts: Vec<String>,
    parse_error: Option<String>,
}

impl DemoLatestAttempt {
    fn state_label(&self) -> &'static str {
        if self.recorded {
            "recorded"
        } else {
            "no-recorded-attempt"
        }
    }

    fn to_json(&self) -> JsonValue {
        json!({
            "recorded": self.recorded,
            "state": self.state_label(),
            "receipt_path": self.receipt_path,
            "receipt_present": self.receipt_path.is_some(),
            "outcome": self.outcome,
            "summary": self.summary,
            "freshness": if self.stale { "stale" } else { "current" },
            "stale": self.stale,
            "artifact_count": self.artifacts.len(),
            "artifacts": self.artifacts,
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone)]
struct DemoExecutionAttempt {
    ok: bool,
    outcome: String,
    entrypoint_kind: String,
    entrypoint_value: String,
    command: String,
    exit_code: Option<i32>,
    summary: Option<String>,
    stdout: String,
    stderr: String,
    recorded_at_epoch_ms: u128,
}

impl DemoExecutionAttempt {
    fn to_json(&self) -> JsonValue {
        json!({
            "ok": self.ok,
            "outcome": self.outcome,
            "entrypoint": {
                "kind": self.entrypoint_kind,
                "value": self.entrypoint_value,
            },
            "command": self.command,
            "exit_code": self.exit_code,
            "summary": self.summary,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "recorded_at_epoch_ms": self.recorded_at_epoch_ms,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedDemoActiveAttempt {
    schema: String,
    schema_version: u8,
    attempt_id: String,
    demo_id: String,
    phase: PersistedDemoActivePhase,
    started_at_epoch_ms: u128,
    owner_pid: u32,
    target_pid: Option<u32>,
    stoppable: bool,
    entrypoint_kind: String,
    entrypoint_value: String,
    command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum PersistedDemoActivePhase {
    Running,
    StopRequested,
}

impl PersistedDemoActivePhase {
    fn rendered(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::StopRequested => "stop-requested",
        }
    }
}

struct DemoActiveAttemptGuard {
    path: PathBuf,
}

impl Drop for DemoActiveAttemptGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
