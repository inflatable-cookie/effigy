use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value as JsonValue};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode,
    ManifestDemoStatus,
};
use crate::runner::util::with_local_node_bin_path;
use crate::ui::{KeyValue, NoticeLevel, Renderer, TableSpec};
use crate::{DemoArgs, DemoSubcommand, TaskInvocation};

use super::error::RunnerError;
use super::render::{encode_json, render_utf8, text_renderer};

const DEMO_RECEIPTS_DIR: &str = ".effigy/demo/receipts";

pub(super) fn run_demo(args: DemoArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;

    match args.subcommand {
        DemoSubcommand::List => render_demo_list(&repo_root, &loaded, args.output_json),
        DemoSubcommand::Inspect { demo_id } => {
            render_demo_inspect(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::Run { demo_id } => {
            render_demo_run(&repo_root, &loaded, &demo_id, args.output_json)
        }
    }
}

fn render_demo_list(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    output_json: bool,
) -> Result<String, RunnerError> {
    let demos = loaded
        .manifest
        .demos
        .iter()
        .map(|(demo_id, demo)| build_demo_record(repo_root, loaded, demo_id, demo))
        .collect::<Result<Vec<_>, _>>()?;

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.list.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "count": demos.len(),
                "demos": demos.iter().map(DemoRecord::to_json_summary).collect::<Vec<_>>(),
            }),
            true,
        );
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Registry")?;
    if demos.is_empty() {
        renderer.notice(
            NoticeLevel::Info,
            "No demos are declared in the current effigy.toml manifest.",
        )?;
        renderer.text("")?;
        return render_utf8(renderer.into_inner());
    }

    let rows = demos
        .iter()
        .map(|demo| {
            vec![
                demo.id.clone(),
                demo.title.clone(),
                display_status(demo.status, demo.latest_attempt.stale).to_owned(),
                demo.gap_class.to_owned(),
                demo.owner.clone(),
                demo.entrypoint.render_compact(),
            ]
        })
        .collect::<Vec<_>>();
    renderer.table(&TableSpec::new(
        vec![
            "ID".to_owned(),
            "Title".to_owned(),
            "Status".to_owned(),
            "Gap".to_owned(),
            "Owner".to_owned(),
            "Entrypoint".to_owned(),
        ],
        rows,
    ))?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to inspect proof intent, coverage, sources, and latest attempt details.",
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
        KeyValue::new(
            "status",
            display_status(record.status, record.latest_attempt.stale),
        ),
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

fn render_demo_run(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.run.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let attempt = execute_demo_attempt(repo_root, demo_id, demo, output_json)?;
    write_latest_attempt_receipt(repo_root, demo_id, demo, &attempt)?;
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        let rendered = encode_json(
            &json!({
                "schema": "effigy.demo.run.v1",
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
        return render_demo_run_text(&record, &attempt);
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

fn render_demo_run_text(
    record: &DemoRecord,
    attempt: &DemoExecutionAttempt,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.section("Demo Run")?;
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
        "Use `effigy demo inspect <DEMO_ID>` to review the recorded latest attempt.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner())
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
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(run_command).current_dir(repo_root);
    with_local_node_bin_path(&mut process, repo_root);

    if output_json {
        return match process.output() {
            Ok(output) => Ok(DemoExecutionAttempt {
                ok: output.status.success(),
                outcome: if output.status.success() {
                    "passed".to_owned()
                } else {
                    "failed".to_owned()
                },
                entrypoint_kind: "run".to_owned(),
                entrypoint_value: run_command.to_owned(),
                command: run_command.to_owned(),
                exit_code: output.status.code(),
                summary: Some(if output.status.success() {
                    format!("Demo `{demo_id}` completed via run entrypoint.")
                } else {
                    format!("Demo `{demo_id}` failed via run entrypoint.")
                }),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                recorded_at_epoch_ms: now_epoch_ms(),
            }),
            Err(error) => Ok(failed_demo_attempt(
                "run",
                run_command,
                run_command,
                None,
                format!("Demo `{demo_id}` failed to launch run entrypoint: {error}"),
                String::new(),
                String::new(),
            )),
        };
    }

    match process.status() {
        Ok(status) => Ok(DemoExecutionAttempt {
            ok: status.success(),
            outcome: if status.success() {
                "passed".to_owned()
            } else {
                "failed".to_owned()
            },
            entrypoint_kind: "run".to_owned(),
            entrypoint_value: run_command.to_owned(),
            command: run_command.to_owned(),
            exit_code: status.code(),
            summary: Some(if status.success() {
                format!("Demo `{demo_id}` completed via run entrypoint.")
            } else {
                format!("Demo `{demo_id}` failed via run entrypoint.")
            }),
            stdout: String::new(),
            stderr: String::new(),
            recorded_at_epoch_ms: now_epoch_ms(),
        }),
        Err(error) => Ok(failed_demo_attempt(
            "run",
            run_command,
            run_command,
            None,
            format!("Demo `{demo_id}` failed to launch run entrypoint: {error}"),
            String::new(),
            String::new(),
        )),
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

fn display_status(status: ManifestDemoStatus, stale: bool) -> String {
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
    latest_attempt: DemoLatestAttempt,
}

impl DemoRecord {
    fn to_json_summary(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
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
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "prerequisites": self.prerequisites,
            "dependencies": self.dependencies,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "sources": self.sources,
            "latest_attempt": self.latest_attempt.to_json(),
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
            "outcome": self.outcome,
            "summary": self.summary,
            "stale": self.stale,
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
