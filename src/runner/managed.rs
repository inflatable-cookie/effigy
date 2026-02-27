use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process_manager::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::catalog::select_catalog_and_task;
use super::{
    parse_task_selector, shell_quote, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    ManifestManagedProcess, ManifestManagedProfile, ManifestManagedRun, ManifestManagedRunStep,
    ManifestTask, RunnerError, TaskRuntimeArgs, TaskSelector, DEFAULT_MANAGED_SHELL_RUN,
};

pub(super) fn resolve_managed_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    runtime_args: &TaskRuntimeArgs,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Option<ManagedTaskPlan>, RunnerError> {
    let Some(mode) = task.mode.as_deref() else {
        return Ok(None);
    };
    if mode != "tui" {
        return Err(RunnerError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile_name = runtime_args
        .passthrough
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_owned());
    let profile = task.profiles.get(&profile_name).ok_or_else(|| {
        let mut available = task.profiles.keys().cloned().collect::<Vec<String>>();
        available.sort();
        RunnerError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name.clone(),
            available,
        }
    })?;

    let profile_entries = profile.start_entries();
    if profile_entries.is_empty() {
        return Err(RunnerError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile_name,
        });
    }

    let start_delay_ms = profile.start_delay_ms();
    let mut processes = Vec::with_capacity(profile_entries.len());
    if task.processes.is_empty() {
        for (idx, entry) in profile_entries.iter().enumerate() {
            let (name, run, cwd) = resolve_direct_profile_entry(
                &selector.task_name,
                entry,
                idx + 1,
                catalogs,
                task_scope_cwd,
            )?;
            processes.push(ManagedProcessSpec {
                name,
                run,
                cwd,
                start_after_ms: *start_delay_ms.get(entry).unwrap_or(&0),
            });
        }
    } else {
        for process_name in &profile_entries {
            let process = task.processes.get(process_name).ok_or_else(|| {
                RunnerError::TaskManagedProcessNotFound {
                    task: selector.task_name.clone(),
                    profile: profile_name.clone(),
                    process: process_name.clone(),
                }
            })?;
            let (process_run, process_cwd) = resolve_managed_process_run(
                &selector.task_name,
                process_name,
                process,
                catalogs,
                task_scope_cwd,
            )?;
            processes.push(ManagedProcessSpec {
                name: process_name.clone(),
                run: process_run,
                cwd: process_cwd,
                start_after_ms: *start_delay_ms.get(process_name).unwrap_or(&0),
            });
        }
    }

    if task.shell.unwrap_or(false) {
        let shell_name = "shell".to_owned();
        if processes.iter().any(|process| process.name == shell_name) {
            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                task: selector.task_name.clone(),
                process: shell_name,
                detail: "reserved process name `shell` is already defined".to_owned(),
            });
        }
        let shell_run = catalog
            .manifest
            .shell
            .as_ref()
            .and_then(|shell| shell.run.clone())
            .unwrap_or_else(|| DEFAULT_MANAGED_SHELL_RUN.to_owned());
        processes.push(ManagedProcessSpec {
            name: "shell".to_owned(),
            run: shell_run,
            cwd: task_scope_cwd.to_path_buf(),
            start_after_ms: 0,
        });
    }

    let tab_order =
        resolve_managed_tab_order(&selector.task_name, &profile_name, &processes, profile)?;

    Ok(Some(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name,
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: runtime_args.passthrough.iter().skip(1).cloned().collect(),
    }))
}

fn resolve_managed_tab_order(
    task_name: &str,
    profile_name: &str,
    processes: &[ManagedProcessSpec],
    profile: &ManifestManagedProfile,
) -> Result<Vec<String>, RunnerError> {
    let process_names = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let Some(tab_entries) = profile.tab_entries() else {
        return Ok(process_names);
    };

    let mut tab_order = Vec::with_capacity(process_names.len());
    for tab in &tab_entries {
        if !process_names.iter().any(|name| name == tab) {
            return Err(RunnerError::TaskManagedProfileTabOrderInvalid {
                task: task_name.to_owned(),
                profile: profile_name.to_owned(),
                detail: format!("tab `{tab}` is not a configured process in this profile"),
            });
        }
        if tab_order.iter().any(|name| name == tab) {
            return Err(RunnerError::TaskManagedProfileTabOrderInvalid {
                task: task_name.to_owned(),
                profile: profile_name.to_owned(),
                detail: format!("tab `{tab}` is duplicated"),
            });
        }
        tab_order.push(tab.to_owned());
    }
    for process_name in process_names {
        if !tab_order.iter().any(|name| name == &process_name) {
            tab_order.push(process_name);
        }
    }
    Ok(tab_order)
}

fn resolve_managed_process_run(
    managed_task_name: &str,
    process_name: &str,
    process: &ManifestManagedProcess,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    match (&process.run, &process.task) {
        (Some(run), None) => resolve_managed_run_value(
            managed_task_name,
            process_name,
            run,
            catalogs,
            task_scope_cwd,
        ),
        (None, Some(task_ref)) => resolve_task_reference_run(
            managed_task_name,
            process_name,
            task_ref,
            catalogs,
            task_scope_cwd,
        ),
        (Some(_), Some(_)) => Err(RunnerError::TaskManagedProcessInvalidDefinition {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            detail: "define either `run` or `task`, not both".to_owned(),
        }),
        (None, None) => Err(RunnerError::TaskManagedProcessInvalidDefinition {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            detail: "missing both `run` and `task`".to_owned(),
        }),
    }
}

fn resolve_managed_run_value(
    managed_task_name: &str,
    process_name: &str,
    run: &ManifestManagedRun,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    match run {
        ManifestManagedRun::Command(command) => Ok((command.clone(), task_scope_cwd.to_path_buf())),
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                    task: managed_task_name.to_owned(),
                    process: process_name.to_owned(),
                    detail: "run array must include at least one step".to_owned(),
                });
            }
            let mut commands = Vec::with_capacity(steps.len());
            for (idx, step) in steps.iter().enumerate() {
                let step_num = idx + 1;
                let step_command = match step {
                    ManifestManagedRunStep::Command(command) => {
                        if let Some(task_ref) = command
                            .strip_prefix("task:")
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            let (task_run, task_cwd) = resolve_task_reference_run(
                                managed_task_name,
                                process_name,
                                task_ref,
                                catalogs,
                                task_scope_cwd,
                            )?;
                            format!(
                                "(cd {} && {})",
                                shell_quote(&task_cwd.display().to_string()),
                                task_run
                            )
                        } else {
                            command.clone()
                        }
                    }
                    ManifestManagedRunStep::Step(step) => match (&step.run, &step.task) {
                        (Some(run), None) => run.clone(),
                        (None, Some(task_ref)) => {
                            let (task_run, task_cwd) = resolve_task_reference_run(
                                managed_task_name,
                                process_name,
                                task_ref,
                                catalogs,
                                task_scope_cwd,
                            )?;
                            format!(
                                "(cd {} && {})",
                                shell_quote(&task_cwd.display().to_string()),
                                task_run
                            )
                        }
                        (Some(_), Some(_)) => {
                            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                                task: managed_task_name.to_owned(),
                                process: process_name.to_owned(),
                                detail: format!(
                                    "run step {step_num} defines both `run` and `task`; choose one"
                                ),
                            });
                        }
                        (None, None) => {
                            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                                task: managed_task_name.to_owned(),
                                process: process_name.to_owned(),
                                detail: format!(
                                    "run step {step_num} is missing both `run` and `task`"
                                ),
                            });
                        }
                    },
                };
                commands.push(step_command);
            }
            Ok((commands.join(" && "), task_scope_cwd.to_path_buf()))
        }
    }
}

fn resolve_task_reference_run(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    let selector = parse_task_selector(task_ref).map_err(|error| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: error.to_string(),
        }
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskManagedTaskReferenceInvalid {
                task: managed_task_name.to_owned(),
                process: process_name.to_owned(),
                reference: task_ref.to_owned(),
                detail: error.to_string(),
            }
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: format!(
                "referenced task `{}` in {} has no `run` command",
                selector.task_name,
                selection.catalog.manifest_path.display()
            ),
        }
    })?;
    let run_rendered = render_task_run_spec(
        &selector.task_name,
        run_spec,
        "",
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| RunnerError::TaskManagedTaskReferenceInvalid {
        task: managed_task_name.to_owned(),
        process: process_name.to_owned(),
        reference: task_ref.to_owned(),
        detail: error.to_string(),
    })?;
    Ok((run_rendered, selection.catalog.catalog_root.clone()))
}

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if depth > 12 {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run expansion exceeded maximum nested task references (12)"
        )));
    }
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    match run {
        ManifestManagedRun::Command(command) => Ok(command
            .replace("{repo}", &repo_rendered)
            .replace("{args}", args_rendered)),
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "task `{task_name}` has an empty run array"
                )));
            }
            let mut commands = Vec::with_capacity(steps.len());
            for step in steps {
                commands.push(resolve_task_run_step(
                    task_name,
                    step,
                    args_rendered,
                    repo_root,
                    catalogs,
                    task_scope_cwd,
                    depth + 1,
                )?);
            }
            Ok(commands.join(" && "))
        }
    }
}

fn resolve_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStep,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                resolve_task_reference_step(
                    task_name,
                    task_ref,
                    args_rendered,
                    catalogs,
                    task_scope_cwd,
                    depth,
                )
            } else {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(command
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
        }
        ManifestManagedRunStep::Step(step) => match (&step.run, &step.task) {
            (Some(run), None) => {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(run
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
            (None, Some(task_ref)) => resolve_task_reference_step(
                task_name,
                task_ref,
                args_rendered,
                catalogs,
                task_scope_cwd,
                depth,
            ),
            (Some(_), Some(_)) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: define either `run` or `task`, not both"
            ))),
            (None, None) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: missing both `run` and `task`"
            ))),
        },
    }
}

fn resolve_task_reference_step(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    let selector = parse_task_selector(task_ref).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` is invalid: {error}"
        ))
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step task ref `{task_ref}` failed: {error}"
            ))
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
            selection.catalog.manifest_path.display()
        ))
    })?;
    let nested = render_task_run_spec(
        &selector.task_name,
        run_spec,
        args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        depth,
    )?;
    Ok(format!(
        "(cd {} && {})",
        shell_quote(&selection.catalog.catalog_root.display().to_string()),
        nested
    ))
}

fn resolve_direct_profile_entry(
    managed_task_name: &str,
    entry: &str,
    ordinal: usize,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, String, PathBuf), RunnerError> {
    let selector = parse_task_selector(entry).map_err(|error| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: format!("entry-{ordinal}"),
            reference: entry.to_owned(),
            detail: error.to_string(),
        }
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskManagedTaskReferenceInvalid {
                task: managed_task_name.to_owned(),
                process: format!("entry-{ordinal}"),
                reference: entry.to_owned(),
                detail: error.to_string(),
            }
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: format!("entry-{ordinal}"),
            reference: entry.to_owned(),
            detail: format!(
                "referenced task `{}` in {} has no `run` command",
                selector.task_name,
                selection.catalog.manifest_path.display()
            ),
        }
    })?;
    let run = render_task_run_spec(
        &selector.task_name,
        run_spec,
        "",
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| RunnerError::TaskManagedTaskReferenceInvalid {
        task: managed_task_name.to_owned(),
        process: format!("entry-{ordinal}"),
        reference: entry.to_owned(),
        detail: error.to_string(),
    })?;
    let name = selector
        .prefix
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or(selector.task_name);
    Ok((name, run, selection.catalog.catalog_root.clone()))
}

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Plan")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile),
        KeyValue::new("repo-root", repo_root.display().to_string()),
        KeyValue::new("manifest", manifest_path.display().to_string()),
        KeyValue::new("processes", plan.processes.len().to_string()),
        KeyValue::new("tab-order", plan.tab_order.join(", ")),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Interactive TUI runtime is available for this task.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Set EFFIGY_MANAGED_STREAM=1 to run selected profile processes in stream mode.",
    )?;
    renderer.text("")?;
    let rows = plan
        .processes
        .into_iter()
        .map(|process| {
            vec![
                process.name,
                process.cwd.display().to_string(),
                process.run,
                process.start_after_ms.to_string(),
            ]
        })
        .collect::<Vec<Vec<String>>>();
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "cwd".to_owned(),
            "run".to_owned(),
            "start-after-ms".to_owned(),
        ],
        rows,
    ))?;
    if !plan.passthrough.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("profile-args", &plan.passthrough)?;
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let tui_override = std::env::var("EFFIGY_MANAGED_TUI").ok();
    let should_stream = std::env::var("EFFIGY_MANAGED_STREAM")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    if should_stream {
        return run_managed_task_runtime(task_name, repo_root, plan);
    }

    let should_tui = match tui_override.as_deref() {
        Some("1") => true,
        Some(value) if value.eq_ignore_ascii_case("true") => true,
        Some("0") => false,
        Some(value) if value.eq_ignore_ascii_case("false") => false,
        _ => std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    };
    if should_tui {
        return run_managed_task_tui(task_name, repo_root, plan);
    }

    render_managed_task_plan(task_name, repo_root, manifest_path, plan)
}

fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        ..
    } = plan;
    let specs = processes
        .into_iter()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let outcome = run_multiprocess_tui(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
    )
    .map_err(|error| {
        RunnerError::Ui(format!(
            "managed tui runtime failed for task `{task_name}`: {error}"
        ))
    })?;
    if fail_on_non_zero && !outcome.non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile,
            processes: outcome.non_zero_exits,
        });
    }
    Ok(String::new())
}

fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let specs = plan
        .processes
        .iter()
        .map(|process| ProcessSpec {
            name: process.name.clone(),
            run: process.run.clone(),
            cwd: process.cwd.clone(),
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Runtime")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile.clone()),
        KeyValue::new("processes", expected.to_string()),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Running managed profile in temporary stream mode.",
    )?;
    renderer.text("")?;

    let mut exit_count = 0usize;
    let mut drained_after_exit = 0usize;
    let mut non_zero_exits = Vec::<(String, String)>::new();
    while exit_count < expected || drained_after_exit < 3 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if exit_count >= expected {
                drained_after_exit = 0;
            }
            match event.kind {
                ProcessEventKind::Stdout => {
                    renderer.text(&format!("[{}] {}", event.process, event.payload))?;
                }
                ProcessEventKind::Stderr => {
                    renderer.text(&format!("[{} stderr] {}", event.process, event.payload))?;
                }
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => {
                    exit_count += 1;
                    if event.payload != "exit=0" {
                        non_zero_exits.push((event.process.clone(), event.payload.clone()));
                    }
                    renderer.notice(
                        NoticeLevel::Info,
                        &format!("process `{}` {}", event.process, event.payload),
                    )?;
                }
            }
        } else if exit_count >= expected {
            drained_after_exit += 1;
        }
    }

    supervisor.terminate_all();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if plan.fail_on_non_zero && !non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile: plan.profile,
            processes: non_zero_exits,
        });
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: expected,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
