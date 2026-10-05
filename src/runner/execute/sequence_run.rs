use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand};
use std::thread;
use std::time::{Duration, Instant};

use effigy_cli::{Command, TaskInvocation};
use effigy_core::shell::with_local_node_bin_path;
use effigy_env::resolver::ResolvedEnv;
use effigy_env::secret::SecretString;
use effigy_execution::ExecutionOutputMode;
use effigy_managed::{
    build_run_sequence_schedule, render_step_command_template, StepEnvAccumulator,
};
use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep, TaskSelection};
use effigy_routing::{resolve_catalog_by_prefix, select_catalog_and_task_on_surface};
use effigy_tasks::{parse_task_reference_invocation, render_task_selector, TaskSurface};

use super::super::cache::ops::update_task_cache_entry;
use super::context::ExecutionTaskContext;
use super::preflight::ExecutionPreflight;
use crate::runner::command_context::EmbeddedRepoOverrideMode;
use crate::runner::embedded_runner::{
    parse_embedded_command, run_embedded_command, run_embedded_draft,
    run_embedded_draft_with_output_mode, run_embedded_task, run_embedded_task_with_output_mode,
};
use crate::runner::error::RunnerError;
use crate::runner::script_command::execute_repo_rhai_script;

pub(super) fn maybe_run_in_process_sequence(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    context: &ExecutionTaskContext<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<Option<String>, RunnerError> {
    if preflight.output_json {
        return Ok(None);
    }

    let ManifestManagedRun::Sequence(steps) =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: preflight.selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?
    else {
        return Ok(None);
    };

    let overall_failed = run_in_process_sequence_steps_inner(
        preflight,
        selection,
        steps,
        env_schema_resolved,
        secret_env,
        &preflight.selector.task_name,
        &preflight.runtime_args_exec.passthrough,
    )?;

    if let Some(code) = overall_failed {
        return Err(RunnerError::TaskCommandFailure {
            command: context.command().to_owned(),
            code: Some(
                if preflight.output_mode == ExecutionOutputMode::Passthrough {
                    code
                } else {
                    1
                },
            ),
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    update_task_cache_entry(
        context.resolved_root,
        context.repo_for_task(),
        &selection.catalog.manifest_path,
        &preflight.selector.task_name,
        selection.task,
        context.command(),
    )?;

    if preflight.runtime_args_raw.verbose_root {
        return Ok(Some(context.render_resolution_trace()));
    }
    Ok(Some(String::new()))
}

pub(super) fn maybe_run_fully_in_process_sequence(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    context: &ExecutionTaskContext<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<Option<String>, RunnerError> {
    let Some(run_spec) = selection.task.run.as_ref() else {
        return Ok(None);
    };
    if !run_spec_is_fully_in_process_capable(run_spec) {
        return Ok(None);
    }
    maybe_run_in_process_sequence(
        preflight,
        selection,
        context,
        env_schema_resolved,
        secret_env,
    )
}

pub(super) fn run_in_process_sequence_steps(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    steps: &[ManifestManagedRunStep],
    env_schema_resolved: &Option<ResolvedEnv>,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<(), RunnerError> {
    let overall_failed = run_in_process_sequence_steps_inner(
        preflight,
        selection,
        steps,
        env_schema_resolved,
        secret_env,
        task_name,
        passthrough,
    )?;
    if let Some(code) = overall_failed {
        return Err(RunnerError::TaskCommandFailure {
            command: task_name.to_owned(),
            code: Some(
                if preflight.output_mode == ExecutionOutputMode::Passthrough {
                    code
                } else {
                    1
                },
            ),
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    Ok(())
}

fn run_in_process_sequence_steps_inner(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    steps: &[ManifestManagedRunStep],
    env_schema_resolved: &Option<ResolvedEnv>,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<Option<i32>, RunnerError> {
    let _output_mode = ScopedOutputMode::set(preflight.output_mode);
    let execution_root = preflight.task_execution_root(&selection.catalog.catalog_root);
    let mut task_env = env_schema_resolved
        .as_ref()
        .map(|resolved| resolved.plain_env())
        .unwrap_or_default();
    for (key, value) in &selection.task.env {
        task_env.insert(key.clone(), value.clone());
    }
    let mut env_state = StepEnvAccumulator::new(
        selection.task.env_file.as_ref(),
        preflight.runtime_args_raw.env_schema_override.as_deref(),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for (key, value) in &task_env {
        env_state.chained_env().get(key);
        let _ = value;
    }

    let mut planned_steps = Vec::with_capacity(steps.len());
    for step in steps {
        env_state
            .apply_from_step(
                task_name,
                step,
                &selection.catalog.manifest.env,
                execution_root,
                &preflight.catalogs,
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let step_env = merged_step_env(
            &task_env,
            env_state.chained_env(),
            execution_root,
            preflight
                .task_source
                .as_ref()
                .map(|source| source.source_root.as_path()),
        );
        planned_steps.push(StepPlan {
            action: resolve_step_action(step, preflight, selection, passthrough)?,
            policy: step_policy(step),
            env: step_env,
        });
    }

    let schedule = build_run_sequence_schedule(task_name, steps)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Some(levels) = schedule {
        run_scheduled_steps(
            &planned_steps,
            &levels,
            selection,
            execution_root,
            secret_env,
            task_name,
            passthrough,
        )
    } else {
        run_step_indexes_in_order(
            planned_steps.iter().enumerate().map(|(index, _)| index),
            &planned_steps,
            selection,
            execution_root,
            secret_env,
            task_name,
            passthrough,
        )
    }
}

fn merged_step_env(
    task_env: &BTreeMap<String, String>,
    chained_env: &BTreeMap<String, String>,
    repo_root: &Path,
    skill_root: Option<&Path>,
) -> BTreeMap<String, String> {
    let mut merged = BTreeMap::new();
    for (key, value) in task_env {
        merged.insert(key.clone(), render_env_value(value, repo_root, skill_root));
    }
    for (key, value) in chained_env {
        merged.insert(key.clone(), render_env_value(value, repo_root, skill_root));
    }
    merged
}

fn run_spec_is_fully_in_process_capable(run: &ManifestManagedRun) -> bool {
    let ManifestManagedRun::Sequence(steps) = run else {
        return false;
    };
    steps.iter().all(step_is_fully_in_process_capable)
}

pub(super) fn step_is_fully_in_process_capable(step: &ManifestManagedRunStep) -> bool {
    match step {
        ManifestManagedRunStep::Command(command) => command
            .strip_prefix("task:")
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
        ManifestManagedRunStep::Step(table) => {
            let table = table.as_ref();
            match (
                table.run.as_deref(),
                table.task.as_deref(),
                table.draft.as_deref(),
                table.rhai.as_deref(),
            ) {
                (Some(_), None, None, None) => false,
                (None, Some(task_ref), None, None) => !task_ref.trim().is_empty(),
                (None, None, Some(draft_ref), None) => !draft_ref.trim().is_empty(),
                (None, None, None, Some(path)) => !path.trim().is_empty(),
                (None, None, None, None) => table.env.is_some() || table.env_file.is_some(),
                _ => false,
            }
        }
    }
}

fn render_env_value(value: &str, repo_root: &Path, skill_root: Option<&Path>) -> String {
    let repo = repo_root.display().to_string();
    let rendered = value.replace("{project}", &repo).replace("{repo}", &repo);
    skill_root.map_or(rendered.clone(), |root| {
        rendered.replace("{skill}", &root.display().to_string())
    })
}

#[derive(Clone, Copy)]
struct StepPolicy {
    retry: usize,
    retry_delay_ms: u64,
    timeout_ms: Option<u64>,
    fail_fast: bool,
}

fn step_policy(step: &ManifestManagedRunStep) -> StepPolicy {
    match step {
        ManifestManagedRunStep::Command(_) => StepPolicy {
            retry: 0,
            retry_delay_ms: 0,
            timeout_ms: None,
            fail_fast: true,
        },
        ManifestManagedRunStep::Step(table) => StepPolicy {
            retry: table.retry.unwrap_or(0),
            retry_delay_ms: table.retry_delay_ms.unwrap_or(0),
            timeout_ms: table.timeout_ms,
            fail_fast: table.fail_fast.unwrap_or(true),
        },
    }
}

struct StepPlan {
    action: StepAction,
    policy: StepPolicy,
    env: BTreeMap<String, String>,
}

fn run_scheduled_steps(
    planned_steps: &[StepPlan],
    levels: &[Vec<usize>],
    selection: &TaskSelection<'_>,
    execution_root: &Path,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<Option<i32>, RunnerError> {
    let mut overall_failed = None;
    let max_parallel = dag_max_parallel();
    for level in levels {
        for batch in level.chunks(max_parallel) {
            if batch
                .iter()
                .all(|index| planned_steps[*index].action.is_shell())
            {
                let batch_failed =
                    run_parallel_shell_batch(batch, planned_steps, execution_root, secret_env)?;
                overall_failed = overall_failed.or(batch_failed);
            } else {
                let batch_failed = run_step_indexes_in_order(
                    batch.iter().copied(),
                    planned_steps,
                    selection,
                    execution_root,
                    secret_env,
                    task_name,
                    passthrough,
                )?;
                overall_failed = overall_failed.or(batch_failed);
            }
            if overall_failed.is_some()
                && batch
                    .iter()
                    .any(|index| planned_steps[*index].policy.fail_fast)
            {
                return Ok(overall_failed);
            }
        }
    }
    Ok(overall_failed)
}

fn run_step_indexes_in_order<I>(
    indexes: I,
    planned_steps: &[StepPlan],
    selection: &TaskSelection<'_>,
    execution_root: &Path,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<Option<i32>, RunnerError>
where
    I: IntoIterator<Item = usize>,
{
    let mut overall_failed = None;
    for index in indexes {
        let step = &planned_steps[index];
        match run_step_with_retry(
            step,
            selection,
            execution_root,
            secret_env,
            task_name,
            passthrough,
        ) {
            Ok(()) => {}
            Err(error) if step.policy.fail_fast => return Err(error),
            Err(error) => {
                overall_failed.get_or_insert(error.task_exit_status().unwrap_or(1));
            }
        }
    }
    Ok(overall_failed)
}

fn run_parallel_shell_batch(
    batch: &[usize],
    planned_steps: &[StepPlan],
    execution_root: &Path,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<Option<i32>, RunnerError> {
    let mut handles = Vec::with_capacity(batch.len());
    for index in batch {
        let step = &planned_steps[*index];
        let StepAction::Command(command) = &step.action else {
            unreachable!("parallel shell batch only supports command steps");
        };
        let command = command.clone();
        let cwd = execution_root.to_path_buf();
        let env = step.env.clone();
        let policy = step.policy;
        let secrets = secret_env.map(|entries| {
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).clone()))
                .collect::<Vec<_>>()
        });
        handles.push(thread::spawn(move || {
            run_shell_step_with_retry_owned(&command, &cwd, &env, secrets.as_deref(), policy)
        }));
    }

    let mut overall_failed = None;
    let mut first_fail_fast_error = None;
    for (offset, handle) in handles.into_iter().enumerate() {
        let result = handle
            .join()
            .map_err(|_| RunnerError::task_invocation("parallel run-array step panicked"))?;
        match result {
            Ok(()) => {}
            Err(error) if planned_steps[batch[offset]].policy.fail_fast => {
                if first_fail_fast_error.is_none() {
                    first_fail_fast_error = Some(error);
                }
            }
            Err(error) => {
                overall_failed.get_or_insert(error.task_exit_status().unwrap_or(1));
            }
        }
    }

    if let Some(error) = first_fail_fast_error {
        return Err(error);
    }

    Ok(overall_failed)
}

fn run_step_with_retry(
    step: &StepPlan,
    selection: &TaskSelection<'_>,
    execution_root: &Path,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<(), RunnerError> {
    let mut attempt = 0;
    loop {
        match run_single_step(
            step,
            selection,
            execution_root,
            secret_env,
            task_name,
            passthrough,
        ) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= step.policy.retry => return Err(error),
            Err(_) => {
                attempt += 1;
                if step.policy.retry_delay_ms > 0 {
                    thread::sleep(Duration::from_millis(step.policy.retry_delay_ms));
                }
            }
        }
    }
}

fn run_single_step(
    step: &StepPlan,
    _selection: &TaskSelection<'_>,
    execution_root: &Path,
    secret_env: Option<&[(&str, &SecretString)]>,
    task_name: &str,
    passthrough: &[String],
) -> Result<(), RunnerError> {
    match &step.action {
        StepAction::Command(command) => run_shell_step_with_retry(
            command,
            execution_root,
            &step.env,
            secret_env,
            StepPolicy {
                retry: 0,
                retry_delay_ms: 0,
                timeout_ms: step.policy.timeout_ms,
                fail_fast: step.policy.fail_fast,
            },
        ),
        StepAction::Task { invocation, cwd } => {
            ensure_timeout_supported(&step.action, step.policy.timeout_ms)?;
            let _env_guard = ScopedEnvOverride::set(&step.env);
            let output = if nested_output_mode() == ExecutionOutputMode::Passthrough {
                run_embedded_task_with_output_mode(
                    invocation,
                    cwd,
                    ExecutionOutputMode::Passthrough,
                )?
            } else {
                run_embedded_task(invocation, cwd)?
            };
            render_nested_output(&output)
        }
        StepAction::Draft { invocation, cwd } => {
            ensure_timeout_supported(&step.action, step.policy.timeout_ms)?;
            let _env_guard = ScopedEnvOverride::set(&step.env);
            let output = if nested_output_mode() == ExecutionOutputMode::Passthrough {
                run_embedded_draft_with_output_mode(
                    invocation,
                    cwd,
                    ExecutionOutputMode::Passthrough,
                )?
            } else {
                run_embedded_draft(invocation, cwd)?
            };
            render_nested_output(&output)
        }
        StepAction::Builtin { command, cwd } => {
            ensure_timeout_supported(&step.action, step.policy.timeout_ms)?;
            let _env_guard = ScopedEnvOverride::set(&step.env);
            let output = run_embedded_command(
                execution_root,
                *command.clone(),
                cwd,
                EmbeddedRepoOverrideMode::Force,
            )?;
            render_nested_output(&output)
        }
        StepAction::Rhai { path } => {
            ensure_timeout_supported(&step.action, step.policy.timeout_ms)?;
            let _env_guard = ScopedEnvOverride::set(&step.env);
            execute_repo_rhai_script(execution_root, task_name, path, passthrough)
        }
        StepAction::Noop => Ok(()),
    }
}

enum StepAction {
    Command(String),
    Task {
        invocation: TaskInvocation,
        cwd: PathBuf,
    },
    Draft {
        invocation: TaskInvocation,
        cwd: PathBuf,
    },
    Builtin {
        command: Box<Command>,
        cwd: PathBuf,
    },
    Rhai {
        path: PathBuf,
    },
    Noop,
}

impl StepAction {
    fn is_shell(&self) -> bool {
        matches!(self, Self::Command(_))
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Command(_) => "command",
            Self::Task { .. } => "task",
            Self::Draft { .. } => "draft",
            Self::Builtin { .. } => "builtin task",
            Self::Rhai { .. } => "rhai script",
            Self::Noop => "env-only step",
        }
    }
}

fn resolve_step_action(
    step: &ManifestManagedRunStep,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    passthrough: &[String],
) -> Result<StepAction, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return resolve_task_step(task_ref, preflight, selection);
            }
            let rendered = render_step_command_template(
                command,
                preflight.task_execution_root(&selection.catalog.catalog_root),
                selection.catalog.bundle_root.as_deref(),
                &effigy_tasks::render_template_args(passthrough),
            );
            Ok(StepAction::Command(render_skill_token(rendered, preflight)))
        }
        ManifestManagedRunStep::Step(table) => {
            let table = table.as_ref();
            match (
                table.run.as_deref(),
                table.task.as_deref(),
                table.draft.as_deref(),
                table.rhai.as_deref(),
            ) {
                (Some(run), None, None, None) => {
                    let rendered = render_step_command_template(
                        run,
                        preflight.task_execution_root(&selection.catalog.catalog_root),
                        selection.catalog.bundle_root.as_deref(),
                        &effigy_tasks::render_template_args(passthrough),
                    );
                    Ok(StepAction::Command(render_skill_token(rendered, preflight)))
                }
                (None, Some(task_ref), None, None) => {
                    resolve_task_step(task_ref, preflight, selection)
                }
                (None, None, Some(draft_ref), None) => {
                    resolve_draft_step(draft_ref, preflight, selection)
                }
                (None, None, None, Some(path)) => Ok(StepAction::Rhai {
                    path: PathBuf::from(render_script_path(
                        path,
                        &selection.catalog.catalog_root,
                        selection.catalog.bundle_root.as_deref(),
                        preflight.task_source.is_some(),
                    )),
                }),
                (None, None, None, None) if table.env.is_some() || table.env_file.is_some() => {
                    Ok(StepAction::Noop)
                }
                _ => Err(RunnerError::task_invocation(format!(
                    "task `{}` run step is invalid: define exactly one of `run`, `task`, `draft`, or `rhai`",
                    preflight.selector.task_name
                ))),
            }
        }
    }
}

fn resolve_task_step(
    task_ref: &str,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<StepAction, RunnerError> {
    let (mut selector, mut args) =
        parse_task_reference_invocation(task_ref).map_err(RunnerError::task_invocation)?;
    args.extend(preflight.runtime_args_exec.passthrough.clone());
    if effigy_managed::BUILTIN_TASKS
        .iter()
        .any(|(name, _)| *name == selector.task_name)
    {
        let catalog_root = if let Some(prefix) = selector.prefix.as_deref() {
            resolve_catalog_by_prefix(prefix, &preflight.catalogs, &preflight.invocation_cwd)
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "unknown catalog prefix `{prefix}` for built-in task `{}`",
                        selector.task_name
                    ))
                })?
                .catalog_root
                .clone()
        } else {
            selection.catalog.catalog_root.clone()
        };
        let cwd = preflight.task_execution_root(&catalog_root).to_path_buf();
        let command = parse_builtin_step_command(&selector.task_name, &args, &cwd)?;
        return Ok(StepAction::Builtin {
            command: Box::new(command),
            cwd,
        });
    }
    if let Some(prefix) = selector.prefix.as_deref() {
        resolve_catalog_by_prefix(prefix, &preflight.catalogs, &preflight.invocation_cwd)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "unknown catalog prefix `{prefix}` for task `{}`",
                    selector.task_name
                ))
            })?;
    } else {
        selector.prefix = Some(selection.catalog.alias.clone());
    }
    let invocation = TaskInvocation {
        name: render_task_selector(&selector),
        args,
    };
    let cwd = preflight.resolved.resolved_root.clone();
    Ok(StepAction::Task { invocation, cwd })
}

/// Resolve one explicit `{ draft = "..." }` composition step.
///
/// Draft-to-draft composition never falls back from an unresolved published
/// reference. The target is resolved on the draft surface before any side
/// effect, so a missing or ambiguous draft fails with source-aware diagnostics.
fn resolve_draft_step(
    draft_ref: &str,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<StepAction, RunnerError> {
    let (mut selector, mut args) =
        parse_task_reference_invocation(draft_ref).map_err(RunnerError::task_invocation)?;
    args.extend(preflight.runtime_args_exec.passthrough.clone());
    if let Some(prefix) = selector.prefix.as_deref() {
        resolve_catalog_by_prefix(prefix, &preflight.catalogs, &preflight.invocation_cwd)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "unknown catalog prefix `{prefix}` for draft `{}`",
                    selector.task_name
                ))
            })?;
    } else {
        selector.prefix = Some(selection.catalog.alias.clone());
    }
    select_catalog_and_task_on_surface(
        TaskSurface::Draft,
        &selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = TaskInvocation {
        name: render_task_selector(&selector),
        args,
    };
    let cwd = preflight.resolved.resolved_root.clone();
    Ok(StepAction::Draft { invocation, cwd })
}

pub(in crate::runner) fn render_script_path(
    path: &str,
    repo_root: &Path,
    bundle_root: Option<&Path>,
    absolute_source_path: bool,
) -> String {
    let repo = repo_root.display().to_string();
    let mut rendered = path.replace("{project}", &repo).replace("{repo}", &repo);
    if let Some(bundle_root) = bundle_root {
        let bundle = bundle_root.display().to_string();
        rendered = rendered
            .replace("{{ bundle.root }}", &bundle)
            .replace("{{bundle}}", &bundle);
    }
    let rendered = PathBuf::from(rendered);
    if rendered.is_absolute() || !absolute_source_path {
        rendered.display().to_string()
    } else {
        repo_root.join(rendered).display().to_string()
    }
}

fn render_skill_token(rendered: String, preflight: &ExecutionPreflight) -> String {
    let Some(source) = preflight.task_source.as_ref() else {
        return rendered;
    };
    rendered.replace(
        "{skill}",
        &effigy_core::shell::shell_quote(&source.source_root.display().to_string()),
    )
}

fn parse_builtin_step_command(
    task_name: &str,
    args: &[String],
    repo_root: &Path,
) -> Result<Command, RunnerError> {
    let mut argv = vec![task_name.to_owned()];
    argv.extend(args.iter().cloned());
    parse_embedded_command(repo_root, &argv, false, EmbeddedRepoOverrideMode::Force)
}

fn run_shell_step_with_retry(
    command: &str,
    cwd: &Path,
    step_env: &BTreeMap<String, String>,
    secret_env: Option<&[(&str, &SecretString)]>,
    policy: StepPolicy,
) -> Result<(), RunnerError> {
    let mut attempt = 0;
    loop {
        match run_shell_step_once(command, cwd, step_env, secret_env, policy.timeout_ms) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= policy.retry => return Err(error),
            Err(_) => {
                attempt += 1;
                if policy.retry_delay_ms > 0 {
                    thread::sleep(Duration::from_millis(policy.retry_delay_ms));
                }
            }
        }
    }
}

fn run_shell_step_with_retry_owned(
    command: &str,
    cwd: &Path,
    step_env: &BTreeMap<String, String>,
    secret_env: Option<&[(String, SecretString)]>,
    policy: StepPolicy,
) -> Result<(), RunnerError> {
    let mut attempt = 0;
    loop {
        match run_shell_step_once_owned(command, cwd, step_env, secret_env, policy.timeout_ms) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= policy.retry => return Err(error),
            Err(_) => {
                attempt += 1;
                if policy.retry_delay_ms > 0 {
                    thread::sleep(Duration::from_millis(policy.retry_delay_ms));
                }
            }
        }
    }
}

fn run_shell_step_once(
    command: &str,
    cwd: &Path,
    step_env: &BTreeMap<String, String>,
    secret_env: Option<&[(&str, &SecretString)]>,
    timeout_ms: Option<u64>,
) -> Result<(), RunnerError> {
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(command).current_dir(cwd);
    with_local_node_bin_path(&mut process, cwd);
    process.envs(
        step_env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    if let Some(secrets) = secret_env {
        for (key, secret) in secrets {
            process.env(key, secret.expose());
        }
    }
    finish_shell_process(process, command, timeout_ms)
}

fn run_shell_step_once_owned(
    command: &str,
    cwd: &Path,
    step_env: &BTreeMap<String, String>,
    secret_env: Option<&[(String, SecretString)]>,
    timeout_ms: Option<u64>,
) -> Result<(), RunnerError> {
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(command).current_dir(cwd);
    with_local_node_bin_path(&mut process, cwd);
    process.envs(
        step_env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    if let Some(secrets) = secret_env {
        for (key, secret) in secrets {
            process.env(key, secret.expose());
        }
    }
    finish_shell_process(process, command, timeout_ms)
}

fn finish_shell_process(
    mut process: ProcessCommand,
    command: &str,
    timeout_ms: Option<u64>,
) -> Result<(), RunnerError> {
    let own_group = timeout_ms.is_some() || crate::runner::owned_children::signal_scope_active();
    if own_group {
        crate::runner::owned_children::configure_owned_process_group(&mut process);
    }
    if let Some(timeout_ms) = timeout_ms {
        return wait_for_shell_step_with_timeout(
            process
                .spawn()
                .map_err(|error| RunnerError::TaskCommandLaunch {
                    command: command.to_owned(),
                    error,
                })?,
            command,
            timeout_ms,
        );
    }

    let status = if own_group {
        let child = process
            .spawn()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: command.to_owned(),
                error,
            })?;
        crate::runner::owned_children::wait_for_owned_child_unbounded(child).map_err(|error| {
            RunnerError::TaskCommandLaunch {
                command: command.to_owned(),
                error,
            }
        })?
    } else {
        process
            .status()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: command.to_owned(),
                error,
            })?
    };
    if status.success() {
        return Ok(());
    }
    Err(RunnerError::TaskCommandFailure {
        command: command.to_owned(),
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn wait_for_shell_step_with_timeout(
    child: Child,
    command: &str,
    timeout_ms: u64,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    match crate::runner::owned_children::wait_for_owned_child(
        child,
        deadline,
        Duration::from_millis(10),
    )
    .map_err(|error| RunnerError::TaskCommandLaunch {
        command: command.to_owned(),
        error,
    })? {
        crate::runner::owned_children::OwnedChildOutcome::Exited(status) if status.success() => {
            Ok(())
        }
        crate::runner::owned_children::OwnedChildOutcome::Exited(status) => {
            Err(RunnerError::TaskCommandFailure {
                command: command.to_owned(),
                code: status.code(),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
        crate::runner::owned_children::OwnedChildOutcome::TimedOut => {
            Err(RunnerError::TaskCommandFailure {
                command: command.to_owned(),
                code: Some(124),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }
}

fn dag_max_parallel() -> usize {
    std::env::var("EFFIGY_DAG_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(4)
}

fn ensure_timeout_supported(
    action: &StepAction,
    timeout_ms: Option<u64>,
) -> Result<(), RunnerError> {
    if timeout_ms.is_some() && !action.is_shell() {
        return Err(RunnerError::task_invocation(format!(
            "run-array timeout is only supported for shell command steps in-process; `{}` steps cannot set `timeout_ms`",
            action.kind()
        )));
    }
    Ok(())
}

fn render_nested_output(output: &str) -> Result<(), RunnerError> {
    let passthrough = nested_output_mode() == ExecutionOutputMode::Passthrough;
    if (passthrough && output.is_empty()) || (!passthrough && output.trim().is_empty()) {
        return Ok(());
    }
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(output.as_bytes())
        .map_err(RunnerError::Cwd)?;
    if !passthrough && !output.ends_with('\n') {
        stdout.write_all(b"\n").map_err(RunnerError::Cwd)?;
    }
    stdout.flush().map_err(RunnerError::Cwd)
}

thread_local! {
    static NESTED_OUTPUT_MODE: std::cell::Cell<ExecutionOutputMode> = const {
        std::cell::Cell::new(ExecutionOutputMode::Capture)
    };
}

fn nested_output_mode() -> ExecutionOutputMode {
    NESTED_OUTPUT_MODE.with(std::cell::Cell::get)
}

struct ScopedOutputMode(ExecutionOutputMode);

impl ScopedOutputMode {
    fn set(mode: ExecutionOutputMode) -> Self {
        Self(NESTED_OUTPUT_MODE.with(|current| current.replace(mode)))
    }
}

impl Drop for ScopedOutputMode {
    fn drop(&mut self) {
        NESTED_OUTPUT_MODE.with(|current| current.set(self.0));
    }
}

struct ScopedEnvOverride {
    original: Vec<(String, Option<OsString>)>,
}

impl ScopedEnvOverride {
    fn set(entries: &BTreeMap<String, String>) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push((key.clone(), std::env::var_os(key)));
            std::env::set_var(key, value);
        }
        Self { original }
    }
}

impl Drop for ScopedEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_builtin_step_command;
    use super::{resolve_task_step, StepAction};
    use crate::runner::execute::preflight::ExecutionPreflight;
    use effigy_cli::Command;
    use effigy_core::resolver::{ResolutionMode, ResolvedTarget};
    use effigy_env::secret::SecretString;
    use effigy_execution::{ExecutionDiscoveryPlan, ExecutionSurface};
    use effigy_manifest::{LoadedCatalog, TaskManifest, TaskSelection};
    use effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector};
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn parse_builtin_step_command_applies_repo_override_to_builtin_command() {
        let command = parse_builtin_step_command(
            "container",
            &["up".to_owned(), "--detach".to_owned()],
            Path::new("/tmp/repo"),
        )
        .expect("parse builtin step command");

        assert!(matches!(
            command,
            Command::Container(args)
                if args.repo_override == Some(PathBuf::from("/tmp/repo"))
        ));
    }

    fn manifest_from_toml(body: &str) -> TaskManifest {
        toml::from_str(body).expect("parse manifest")
    }

    fn loaded_catalog(
        alias: &str,
        root: &str,
        manifest: TaskManifest,
        depth: usize,
    ) -> LoadedCatalog {
        let root = PathBuf::from(root);
        LoadedCatalog {
            alias: alias.to_owned(),
            manifest_path: root.join("effigy.toml"),
            catalog_root: root,
            bundle_root: None,
            manifest,
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth,
            draft_sources: Default::default(),
        }
    }

    #[test]
    fn resolve_task_step_keeps_workspace_scope_for_nested_child_tasks() {
        let root = loaded_catalog(
            "acowtancy",
            "/workspace-root/acowtancy",
            manifest_from_toml(""),
            0,
        );
        let child = loaded_catalog(
            "farmyard",
            "/workspace-root/acowtancy/farmyard",
            manifest_from_toml(
                r#"
[tasks."state:apply:schema"]
run = [{ task = "db:migrate" }]
"#,
            ),
            1,
        );
        let preflight = ExecutionPreflight {
            invocation_cwd: PathBuf::from("/workspace-root/acowtancy"),
            execution_surface: ExecutionSurface::RunArray,
            runtime_args_raw: TaskRuntimeArgs {
                repo_override: None,
                verbose_root: false,
                env_schema_override: None,
                lock_wait_ms: None,
                passthrough: Vec::new(),
            },
            runtime_args_exec: TaskRuntimeArgs {
                repo_override: None,
                verbose_root: false,
                env_schema_override: None,
                lock_wait_ms: None,
                passthrough: Vec::new(),
            },
            output_json: false,
            plan: false,
            output_mode: effigy_execution::ExecutionOutputMode::Capture,
            resolved: ResolvedTarget {
                resolved_root: PathBuf::from("/workspace-root/acowtancy"),
                resolution_mode: ResolutionMode::AutoNearest,
                evidence: Vec::new(),
                warnings: Vec::new(),
            },
            discovery_plan: ExecutionDiscoveryPlan {
                invocation_cwd: PathBuf::from("/workspace-root/acowtancy"),
                resolved_root: PathBuf::from("/workspace-root/acowtancy"),
                selector: TaskSelector {
                    prefix: Some("farmyard".to_owned()),
                    task_name: "state:apply:schema".to_owned(),
                },
                repo_override: None,
            },
            selector: TaskSelector {
                prefix: Some("farmyard".to_owned()),
                task_name: "state:apply:schema".to_owned(),
            },
            catalogs: vec![root, child],
            secret_targets: Vec::new(),
            task_source: None,
        };
        let selection = TaskSelection {
            catalog: &preflight.catalogs[1],
            task: preflight.catalogs[1]
                .manifest
                .tasks
                .get("state:apply:schema")
                .expect("state task exists"),
            mode: CatalogSelectionMode::ExplicitPrefix,
            evidence: vec!["selected catalog via explicit prefix".to_owned()],
            surface: effigy_tasks::TaskSurface::Published,
        };

        let action = resolve_task_step("db:migrate", &preflight, &selection).expect("resolve step");

        match action {
            StepAction::Task { invocation, cwd } => {
                assert_eq!(invocation.name, "farmyard/db:migrate");
                assert_eq!(cwd, PathBuf::from("/workspace-root/acowtancy"));
            }
            other => panic!("expected nested task action, got {:?}", other.kind()),
        }
    }

    #[cfg(unix)]
    fn empty_step_env() -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::new()
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_success_and_nonzero_exit() {
        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        let dir = tempfile::Builder::new()
            .prefix("effigy-seq-exit-")
            .tempdir()
            .expect("tempdir");
        super::run_shell_step_once("true", dir.path(), &empty_step_env(), None, None)
            .expect("true must succeed");
        let borrowed_fail =
            super::run_shell_step_once("false", dir.path(), &empty_step_env(), None, None)
                .expect_err("false must fail");
        assert_eq!(borrowed_fail.task_exit_status(), Some(1));

        let secret = SecretString::new("timeout-proof".to_owned());
        let owned = vec![("EFFIGY_TIMEOUT_PROOF".to_owned(), secret)];
        super::run_shell_step_once_owned(
            "true",
            dir.path(),
            &empty_step_env(),
            Some(owned.as_slice()),
            None,
        )
        .expect("owned true must succeed");
        let owned_fail = super::run_shell_step_once_owned(
            "false",
            dir.path(),
            &empty_step_env(),
            Some(owned.as_slice()),
            None,
        )
        .expect_err("owned false must fail");
        assert_eq!(owned_fail.task_exit_status(), Some(1));
    }

    #[cfg(unix)]
    fn assert_sequence_timeout_reaps(
        run: impl FnOnce(&std::path::Path, &str) -> Result<(), crate::runner::error::RunnerError>,
    ) {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::time::{Duration, Instant};

        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-timeout-");
        fixture.spawn_unrelated_sibling();
        let command = fixture.hang_command();
        let started = Instant::now();
        let error = run(fixture.cwd(), &command).expect_err("hang must time out");
        let pids = fixture.wait_for_recorded_pids();
        assert_eq!(error.task_exit_status(), Some(124), "{error}");
        fixture.assert_owned_gone(&pids);
        fixture.assert_sibling_alive();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timeout must not wait out /bin/sleep 300"
        );
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_borrowed_reaps_leader_and_descendant() {
        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        assert_sequence_timeout_reaps(|cwd, command| {
            super::run_shell_step_once(command, cwd, &empty_step_env(), None, Some(2_000))
        });
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_owned_reaps_leader_and_descendant() {
        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        let secret = SecretString::new("timeout-proof".to_owned());
        let owned = vec![("EFFIGY_TIMEOUT_PROOF".to_owned(), secret)];
        assert_sequence_timeout_reaps(|cwd, command| {
            super::run_shell_step_once_owned(
                command,
                cwd,
                &empty_step_env(),
                Some(owned.as_slice()),
                Some(2_000),
            )
        });
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_reaps_term_immune_descendant() {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::time::{Duration, Instant};

        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-timeout-term-immune-");
        fixture.spawn_unrelated_sibling();
        let command = fixture.hang_command_ignoring_term();
        let started = Instant::now();
        let error = super::run_shell_step_once(
            &command,
            fixture.cwd(),
            &empty_step_env(),
            None,
            Some(2_000),
        )
        .expect_err("hang must time out");
        let pids = fixture.wait_for_recorded_pids();
        assert_eq!(error.task_exit_status(), Some(124), "{error}");
        fixture.wait_until_owned_gone(&pids);
        fixture.assert_sibling_alive();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timeout must not wait out /bin/sleep 300"
        );
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_interrupt_reaps_leader_and_descendant() {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::thread;
        use std::time::{Duration, Instant};

        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-interrupt-");
        fixture.spawn_unrelated_sibling();
        let command = fixture.hang_command();
        let cwd = fixture.cwd().to_path_buf();
        let started = Instant::now();
        let handle = thread::spawn(move || {
            super::run_shell_step_once(&command, &cwd, &empty_step_env(), None, Some(30_000))
        });
        TimeoutDescendantFixture::wait_until_signal_scope_active();
        let pids = fixture.wait_for_recorded_pids();
        unsafe {
            libc::raise(libc::SIGINT);
        }
        let _ = handle.join().expect("sequence interrupt wait thread");
        fixture.wait_until_owned_gone(&pids);
        fixture.assert_sibling_alive();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "interrupt must not wait out /bin/sleep 300"
        );
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_interrupt_without_forwarding_fails_reap_oracle() {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::panic::AssertUnwindSafe;
        use std::thread;
        use std::time::{Duration, Instant};

        let _seam = crate::runner::owned_children::disable_group_cleanup_for_test();
        let _forwarding = crate::runner::owned_children::disable_forwarding_for_test();
        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-interrupt-negative-");
        fixture.spawn_unrelated_sibling();
        let command = fixture.hang_command();
        let cwd = fixture.cwd().to_path_buf();
        let handle = thread::spawn(move || {
            super::run_shell_step_once(&command, &cwd, &empty_step_env(), None, Some(2_000))
        });
        TimeoutDescendantFixture::wait_until_signal_scope_active();
        let pids = fixture.wait_for_recorded_pids();
        unsafe {
            libc::raise(libc::SIGINT);
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            assert!(
                pids.iter()
                    .any(|pid| TimeoutDescendantFixture::pid_alive(*pid)),
                "forwarding disabled: a descendant must still be alive: {pids:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let oracle = std::panic::catch_unwind(AssertUnwindSafe(|| {
            fixture.assert_owned_gone(&pids);
        }));
        assert!(
            oracle.is_err(),
            "reap oracle must fail while interrupt forwarding is disabled"
        );
        fixture.assert_sibling_alive();
        let _ = handle
            .join()
            .expect("sequence interrupt negative wait thread");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_group_cleanup_disabled_fails_reap_oracle() {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::panic::AssertUnwindSafe;

        let _seam = crate::runner::owned_children::disable_group_cleanup_for_test();
        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-timeout-negative-");
        fixture.spawn_unrelated_sibling();
        let command = fixture.hang_command();
        let error = super::run_shell_step_once(
            &command,
            fixture.cwd(),
            &empty_step_env(),
            None,
            Some(2_000),
        )
        .expect_err("pid-only timeout must still return 124");
        assert_eq!(error.task_exit_status(), Some(124), "{error}");
        let pids = fixture.wait_for_recorded_pids();
        assert!(
            pids.iter()
                .any(|pid| TimeoutDescendantFixture::pid_alive(*pid)),
            "group cleanup disabled: a descendant must still be alive: {pids:?}"
        );
        let oracle = std::panic::catch_unwind(AssertUnwindSafe(|| {
            fixture.assert_owned_gone(&pids);
        }));
        assert!(
            oracle.is_err(),
            "reap oracle must fail while group cleanup is disabled"
        );
        fixture.assert_sibling_alive();
    }

    #[cfg(unix)]
    #[test]
    fn timeout_descendants_sequence_fixture_guard_reaps_on_failure() {
        use crate::runner::owned_children::timeout_descendant_proof::TimeoutDescendantFixture;
        use std::os::unix::process::CommandExt;
        use std::panic::AssertUnwindSafe;
        use std::process::{Command, Stdio};

        let _lock = crate::runner::owned_children::hold_group_cleanup_test_lock();
        let mut fixture = TimeoutDescendantFixture::new("effigy-seq-timeout-guard-");
        let command = fixture.hang_command();
        let child = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(fixture.cwd())
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn hang without production reap");
        fixture.adopt_direct_child(child);
        let pids = fixture.wait_for_recorded_pids();
        assert!(
            pids.iter()
                .all(|pid| TimeoutDescendantFixture::pid_alive(*pid)),
            "fixture failure path starts with a live tree: {pids:?}"
        );
        let panicked = std::panic::catch_unwind(AssertUnwindSafe(|| {
            panic!("fixture failure");
        }));
        assert!(panicked.is_err());
        drop(fixture);
        let alive = pids
            .iter()
            .copied()
            .filter(|pid| TimeoutDescendantFixture::pid_alive(*pid))
            .collect::<Vec<_>>();
        assert!(
            alive.is_empty(),
            "fixture RAII must reap recorded pids after failure: {alive:?}"
        );
    }
}
