//! One typed, ordered sequence engine. Output mode chooses sinks only.
use super::{context::ExecutionTaskContext, preflight::ExecutionPreflight};
use crate::runner::command_context::EmbeddedRepoOverrideMode;
use crate::runner::embedded_runner::{parse_embedded_command, run_embedded_task_with_output_mode};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{
    run_effigy_child, run_plain, CustodyWait, PlainCommand, SpawnScope, TaskRun,
};
use effigy_cli::{Command, TaskInvocation};
use effigy_env::{resolver::ResolvedEnv, secret::SecretString};
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_managed::{render_step_command_template, StepEnvAccumulator};
use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep, TaskSelection};
use effigy_routing::{resolve_catalog_by_prefix, select_catalog_and_task};
use effigy_tasks::{parse_task_reference_invocation, render_task_selector};
use std::path::{Path, PathBuf};

pub(super) fn maybe_run_in_process_sequence(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    context: &ExecutionTaskContext<'_>,
    env_schema: &Option<ResolvedEnv>,
    secrets: Option<&[(&str, &SecretString)]>,
    run: &mut TaskRun,
    workload: &mut Option<WorkloadOutcome>,
) -> Result<Option<String>, RunnerError> {
    let Some(ManifestManagedRun::Sequence(steps)) = &selection.task.run else {
        return Ok(None);
    };
    let report = run_steps(
        preflight,
        selection,
        steps,
        env_schema,
        secrets,
        (
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
        ),
        run,
    )?;
    *workload = Some(report.outcome.clone());
    if preflight.output_json {
        let mut payload: serde_json::Value =
            serde_json::from_str(&super::json_payload::render_task_command_json(
                &preflight.selector,
                context.repo_for_task(),
                context.command(),
                Some(report.code),
                &report.stdout,
                &report.stderr,
            )?)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        payload["steps"] = serde_json::Value::Array(report.steps);
        payload["failed_step"] = serde_json::json!(report.failed_step);
        let rendered = effigy_ui::encode_json(&payload, true)?;
        return if report.code == 0 {
            Ok(Some(rendered))
        } else {
            Err(RunnerError::CommandJsonFailure { rendered })
        };
    }
    if report.code != 0 {
        return Err(RunnerError::TaskCommandFailure {
            command: format!(
                "{} step `{}`",
                preflight.selector.task_name,
                report.failed_step.unwrap_or_default()
            ),
            code: Some(report.code),
            stdout: report.stdout,
            stderr: report.stderr,
        });
    }
    Ok(Some(if preflight.runtime_args_raw.verbose_root {
        context.render_resolution_trace()
    } else {
        String::new()
    }))
}

pub(super) fn run_in_process_sequence_steps(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    steps: &[ManifestManagedRunStep],
    env_schema: &Option<ResolvedEnv>,
    secrets: Option<&[(&str, &SecretString)]>,
    owner: (&str, &[String]),
    run: &TaskRun,
) -> Result<(), RunnerError> {
    let report = run_steps(preflight, selection, steps, env_schema, secrets, owner, run)?;
    if report.code == 0 {
        Ok(())
    } else {
        Err(RunnerError::TaskCommandFailure {
            command: format!(
                "{} step `{}`",
                owner.0,
                report.failed_step.unwrap_or_default()
            ),
            code: Some(report.code),
            stdout: report.stdout,
            stderr: report.stderr,
        })
    }
}

/// Classify the complete declared tree before admission. Dynamic adapters
/// declare durable capability; static task references resolve exactly as steps do.
pub(in crate::runner::execute) fn foreground_tree(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<bool, RunnerError> {
    classify_tree(
        preflight,
        selection,
        &preflight.selector.task_name,
        &mut Vec::new(),
    )
}

pub(in crate::runner::execute) fn foreground_test_sources(
    preflight: &ExecutionPreflight,
    sources: Vec<effigy_builtin::TestRunSource>,
) -> Result<bool, RunnerError> {
    classify_test_sources(preflight, sources, &mut Vec::new())
}

fn classify_test_sources(
    preflight: &ExecutionPreflight,
    sources: Vec<effigy_builtin::TestRunSource>,
    ancestors: &mut Vec<(PathBuf, String)>,
) -> Result<bool, RunnerError> {
    for source in sources {
        let Some(catalog) = preflight
            .catalogs
            .iter()
            .find(|catalog| catalog.catalog_root == source.root)
        else {
            // Auto-detected suites outside a manifest are ordinary shell
            // commands. They have no declared Effigy descendants to resolve.
            continue;
        };
        let task = effigy_manifest::ManifestTask {
            run: Some(source.run),
            run_in: Some(effigy_manifest::ManifestTaskRunIn::Host),
            scratch: Some(false),
            ..Default::default()
        };
        let selection = TaskSelection {
            catalog,
            task: &task,
            mode: effigy_core::task_selection::CatalogSelectionMode::CwdNearest,
            evidence: Vec::new(),
            surface: effigy_core::task_selection::TaskSurface::Published,
        };
        if !classify_tree(
            preflight,
            &selection,
            &format!("test:{}", source.label),
            ancestors,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn classify_tree(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    task_name: &str,
    ancestors: &mut Vec<(PathBuf, String)>,
) -> Result<bool, RunnerError> {
    let identity = (selection.catalog.catalog_root.clone(), task_name.to_owned());
    if ancestors.len() >= 64 || ancestors.contains(&identity) {
        return Err(RunnerError::task_invocation(
            "task tree is cyclic or exceeds 64 declared levels",
        ));
    }
    if selection.task.durable
        || selection.task.mode.as_deref() == Some("tui")
        || !selection.task.concurrent.is_empty()
        || !super::pipeline::standard::checked_host_binding(preflight, selection, task_name)?
        || selection.task.uses_scratch() && crate::runner::scratch::wanted()
        // The declared vault generation hook runs during `secrets =
        // "required"` startup, before this task's body. It is a durable
        // descendant the manifest names, so the tree class must include it
        // before admission rather than refuse it mid-flight.
        || super::pipeline::standard::startup_can_run_vault_generate_hook(preflight, selection)?
    {
        return Ok(false);
    }
    let Some(run) = selection.task.run.as_ref() else {
        return Ok(false);
    };
    ancestors.push(identity);
    let steps = match run {
        ManifestManagedRun::Command(command) => {
            vec![ManifestManagedRunStep::Command(command.clone())]
        }
        ManifestManagedRun::Sequence(steps) => steps.clone(),
    };
    let result = (|| {
        for step in &steps {
            match resolve_step_action(
                step,
                preflight,
                selection,
                &preflight.runtime_args_exec.passthrough,
            )? {
                StepAction::Command(_) | StepAction::Noop => {}
                StepAction::Rhai { .. } => return Ok(false),
                StepAction::Builtin { parsed, cwd, .. } => {
                    if !classify_builtin(&parsed, &cwd, ancestors)? {
                        return Ok(false);
                    }
                }
                StepAction::Task { invocation, cwd } => {
                    if !classify_invocation(&invocation, &cwd, ancestors)? {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    })();
    ancestors.pop();
    result
}

/// Resolve nested invocations through the same precedence and discovery as
/// execution. A task named `config` may shadow that builtin, and a remapped
/// builtin may defer to an adapter with durable effects.
fn classify_invocation(
    invocation: &TaskInvocation,
    cwd: &Path,
    ancestors: &mut Vec<(PathBuf, String)>,
) -> Result<bool, RunnerError> {
    if invocation
        .args
        .iter()
        .any(|arg| arg == effigy_cli::INTERNAL_RUN_ID_ARG)
    {
        return Ok(false);
    }
    let child = super::preflight::build_execution_preflight(invocation, cwd.into())?;
    if child.help {
        return Ok(true);
    }
    match super::route::resolve_route(invocation, &child)? {
        super::route::Route::Task { selection, .. } => {
            if child.plan {
                Ok(true)
            } else {
                classify_tree(&child, &selection, &child.selector.task_name, ancestors)
            }
        }
        super::route::Route::Builtin if child.selector.task_name == "test" => {
            match effigy_builtin::test_run_requirement(
                &child.selector,
                &child.runtime_args_exec,
                &child.invocation_cwd,
                &child.catalogs,
            )? {
                effigy_builtin::TestRunRequirement::Observe => Ok(true),
                effigy_builtin::TestRunRequirement::Local(sources) => {
                    classify_test_sources(&child, sources, ancestors)
                }
                _ => Ok(false),
            }
        }
        _ => Ok(false),
    }
}

fn classify_builtin(
    command: &Command,
    cwd: &Path,
    ancestors: &mut Vec<(PathBuf, String)>,
) -> Result<bool, RunnerError> {
    use effigy_cli::{RunsAction, TasksQaCommand};
    Ok(match command {
        Command::Task(task) => return classify_invocation(task, cwd, ancestors),
        Command::Version | Command::Help(_) | Command::HelpGroup(_) => true,
        Command::Doctor(args) => !args.fix && !args.deep && !args.refresh,
        Command::Tasks(args) => matches!(
            args.qa,
            None | Some(
                TasksQaCommand::GroupsList { .. }
                    | TasksQaCommand::GroupStatus { .. }
                    | TasksQaCommand::GroupLogs { .. }
                    | TasksQaCommand::GroupRun { plan: true, .. }
            )
        ),
        Command::Runs(args) => matches!(
            args.action,
            RunsAction::List { .. } | RunsAction::Status { .. } | RunsAction::Logs { .. }
        ),
        Command::Gateway(args) => matches!(args.subcommand, effigy_cli::GatewaySubcommand::Status),
        Command::Deps(args) => matches!(args.subcommand, effigy_cli::DepsSubcommand::Status { .. }),
        Command::Bundle(args) => matches!(args.subcommand, effigy_cli::BundleSubcommand::Inspect),
        Command::Service(args) => matches!(args.subcommand, effigy_cli::ServiceSubcommand::List),
        Command::Changelog(args) => !matches!(
            args.subcommand,
            effigy_cli::ChangelogSubcommand::Format { write: true }
        ),
        Command::Skill(args) => {
            matches!(args.subcommand, effigy_cli::SkillSubcommand::Tasks { .. })
        }
        _ => false,
    })
}

struct SequenceReport {
    code: i32,
    stdout: String,
    stderr: String,
    steps: Vec<serde_json::Value>,
    failed_step: Option<String>,
    outcome: WorkloadOutcome,
}

fn run_steps(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    steps: &[ManifestManagedRunStep],
    env_schema: &Option<ResolvedEnv>,
    secrets: Option<&[(&str, &SecretString)]>,
    owner: (&str, &[String]),
    run: &TaskRun,
) -> Result<SequenceReport, RunnerError> {
    let (task_name, passthrough) = owner;
    let _signals = CustodyWait::enter();
    if steps.is_empty() {
        return Err(RunnerError::task_invocation(
            "a run sequence must contain at least one step",
        ));
    }
    let mut labels = std::collections::BTreeSet::new();
    for step in steps {
        if let ManifestManagedRunStep::Step(table) = step {
            if let Some(label) = &table.id {
                if label.trim().is_empty() || !labels.insert(label) {
                    return Err(RunnerError::task_invocation(format!(
                        "task `{task_name}` has an empty or duplicate step id `{label}`"
                    )));
                }
            }
        }
    }
    let root = preflight.task_execution_root(&selection.catalog.catalog_root);
    let mut task_env = env_schema
        .as_ref()
        .map(|env| env.plain_env())
        .unwrap_or_default();
    task_env.extend(selection.task.env.clone());
    let mut env = StepEnvAccumulator::new(
        selection.task.env_file.as_ref(),
        preflight.runtime_args_raw.env_schema_override.as_deref(),
    )?;
    let mut planned = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        env.apply_from_step(
            task_name,
            step,
            &selection.catalog.manifest.env,
            root,
            &preflight.catalogs,
        )?;
        let mut step_env = task_env.clone();
        step_env.extend(env.chained_env().clone());
        for value in step_env.values_mut() {
            *value = value
                .replace("{repo}", &root.display().to_string())
                .replace("{project}", &root.display().to_string());
            if let Some(source) = &preflight.task_source {
                *value = value.replace("{skill}", &source.source_root.display().to_string());
            }
        }
        let (label, timeout) = match step {
            ManifestManagedRunStep::Command(_) => ((index + 1).to_string(), None),
            ManifestManagedRunStep::Step(table) => (
                table.id.clone().unwrap_or_else(|| (index + 1).to_string()),
                table.timeout_ms,
            ),
        };
        let subject = step_subject(step);
        planned.push((
            resolve_step_action(step, preflight, selection, passthrough)?,
            step_env,
            label,
            timeout,
            subject,
        ));
    }
    let mut report = SequenceReport {
        code: 0,
        stdout: String::new(),
        stderr: String::new(),
        steps: Vec::new(),
        failed_step: None,
        outcome: WorkloadOutcome::Succeeded,
    };
    let total = planned.len();
    for (index, (action, env, label, timeout, subject)) in planned.into_iter().enumerate() {
        if report.code != 0 {
            report
                .steps
                .push(serde_json::json!({"index":index, "step":label, "state":"not_started"}));
            continue;
        }
        if let Some((code, outcome)) = crate::runner::task_engine::pending_stop(&label) {
            report.code = code;
            report.outcome = outcome;
            report.failed_step = Some(label.clone());
            report
                .steps
                .push(serde_json::json!({"index":index, "step":label, "state":"not_started"}));
            continue;
        }
        // A step can run for minutes, so each one is a live step that names
        // which step of how many is running.
        let position = index + 1;
        let description = selection
            .task
            .description
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        let subject = if total == 1 {
            description
        } else {
            subject.as_deref()
        };
        let progress = sequence_step_label(task_name, position, total, subject);
        let step = effigy_ui::steps::step(&progress);
        let mut child = run.step(index, &label)?;
        let child_id = child.id();
        let result = {
            let _parent = child.enter();
            let _env = SpawnScope::enter(&env, secrets, preflight.output_json, timeout);
            execute_step(
                &action,
                preflight,
                selection,
                root,
                (task_name, passthrough),
                timeout,
                &mut child,
            )
        };
        let (code, stdout, stderr, workload) = match result {
            Ok(exit) => exit,
            Err(error) => {
                let rendered = error
                    .rendered_output()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
                let pending = crate::runner::task_engine::pending_stop(&label);
                let code = pending
                    .as_ref()
                    .map(|(code, _)| *code)
                    .or_else(|| {
                        rendered
                            .as_ref()
                            .and_then(|value| value["exit_code"].as_i64())
                            .map(|code| code as i32)
                            .or(error.task_exit_status())
                    })
                    .unwrap_or_else(|| error.process_exit_code());
                let (stdout, stderr) = match (&rendered, &error) {
                    (Some(value), _) => (
                        value["stdout"].as_str().unwrap_or_default().to_owned(),
                        value["stderr"].as_str().unwrap_or_default().to_owned(),
                    ),
                    (_, RunnerError::TaskCommandFailure { stdout, stderr, .. })
                        if !stdout.is_empty() || !stderr.is_empty() =>
                    {
                        (stdout.clone(), stderr.clone())
                    }
                    _ => (String::new(), error.to_string()),
                };
                (
                    code,
                    stdout,
                    stderr,
                    pending.map(|(_, outcome)| outcome).unwrap_or_else(|| {
                        if code == 124 {
                            WorkloadOutcome::TimedOut {
                                phase: label.clone(),
                            }
                        } else {
                            crate::runner::task_engine::workload_for_exit(code)
                        }
                    }),
                )
            }
        };
        let outcome_label = progress.strip_prefix("running ").unwrap_or(&progress);
        if code == 0 {
            step.succeed(&format!("{outcome_label} finished"));
        } else {
            step.fail(&format!("{outcome_label} failed with exit code {code}"));
        }
        for warning in child.close(workload.clone()) {
            eprintln!("warning: step `{label}`: {warning}");
        }
        report.steps.push(serde_json::json!({"index":index, "step":label, "run_id":child_id.to_string(), "outcome":workload, "exit_code":code, "stdout":stdout, "stderr":stderr}));
        report.stdout.push_str(&stdout);
        report.stderr.push_str(&stderr);
        if code != 0 {
            report.code = code;
            report.outcome = workload;
            report.failed_step = Some(label);
        }
    }
    Ok(report)
}

/// A human subject from an explicit ID, task reference or command.
fn step_subject(step: &ManifestManagedRunStep) -> Option<String> {
    let command = match step {
        ManifestManagedRunStep::Command(command) => Some(command.as_str()),
        ManifestManagedRunStep::Step(table) => {
            if let Some(id) = table.id.as_deref().filter(|id| !id.trim().is_empty()) {
                return Some(id.to_owned());
            }
            if let Some(task) = table.task.as_deref() {
                return Some(task.to_owned());
            }
            table.run.as_deref().or(table.rhai.as_deref())
        }
    };
    command
        .and_then(|text| text.split_whitespace().next())
        .map(str::to_owned)
}

pub(in crate::runner::execute) fn sequence_step_label(
    task_name: &str,
    position: usize,
    total: usize,
    subject: Option<&str>,
) -> String {
    let mut label = format!("running {task_name}");
    if total > 1 {
        label.push_str(&format!(" step {position}/{total}"));
    }
    if let Some(subject) = subject {
        label.push_str(&format!(": {subject}"));
    }
    label
}

type StepExit = (i32, String, String, WorkloadOutcome);

/// The source a command opens is the one the lock selects when it runs.
///
/// A sequence resolves every step before the first one runs, and a step is
/// itself a Prepare path (`task:container up`, `task:bundle sync`). Such a
/// step publishes a new source generation and removes the one the later
/// steps were rendered against, so each command is pointed at the live
/// object as it starts instead of the generation it named.
fn live_source_command(selection: &TaskSelection<'_>, command: &str) -> String {
    effigy_manifest::relocate_source_paths(&selection.catalog.manifest_path, command)
}

fn execute_step(
    action: &StepAction,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    root: &Path,
    owner: (&str, &[String]),
    timeout_ms: Option<u64>,
    run: &mut TaskRun,
) -> Result<StepExit, RunnerError> {
    let (task_name, passthrough) = owner;
    match action {
        StepAction::Task { invocation, cwd } => {
            let output =
                run_embedded_task_with_output_mode(invocation, cwd, preflight.output_mode)?;
            nested_exit(output, preflight.output_json)
        }
        StepAction::Builtin { parsed, .. }
            if matches!(parsed.as_ref(), Command::Exec(_))
                && !crate::runner::container_runtime::inside_container_handoff() =>
        {
            let Command::Exec(args) = parsed.as_ref() else {
                unreachable!("matched exec")
            };
            let exit =
                crate::runner::exec_command::run_exec_step(args, run, preflight.output_json)?;
            Ok((
                exit.code,
                String::from_utf8_lossy(&exit.stdout).into_owned(),
                String::from_utf8_lossy(&exit.stderr).into_owned(),
                exit.workload,
            ))
        }
        StepAction::Command(command)
            if !super::pipeline::standard::runs_plain_host_command(preflight, selection) =>
        {
            let command = live_source_command(selection, command);
            super::pipeline::standard::run_sequence_container_command(
                preflight, selection, &command, run,
            )
            .and_then(|output| nested_exit(output, preflight.output_json))
        }
        _ => {
            let command = match action {
                StepAction::Command(command) | StepAction::Builtin { command, .. } => {
                    live_source_command(selection, command)
                }
                StepAction::Rhai { path } => {
                    let executable =
                        effigy_core::effigy_invocation::resolve_effigy_invocation_prefix(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/Cargo.toml"
                        ))
                        .map_err(RunnerError::Cwd)?;
                    let path = live_source_command(selection, &path.display().to_string());
                    format!(
                        "{executable} script run --file {} --repo-root {} --task-name {}",
                        effigy_core::shell::shell_quote(&path),
                        effigy_core::shell::shell_quote(&root.display().to_string()),
                        effigy_core::shell::shell_quote(task_name)
                    )
                }
                StepAction::Noop => ":".to_owned(),
                _ => unreachable!("references returned above"),
            };
            let cwd = match action {
                StepAction::Builtin { cwd, .. } => cwd.as_path(),
                _ => root,
            };
            let helper = matches!(action, StepAction::Builtin { .. } | StepAction::Rhai { .. });
            let mut environment = Vec::new();
            if helper {
                environment.push((
                    "EFFIGY_INTERNAL_SUPPRESS_HEADER".to_owned(),
                    crate::runner::task_engine::EnvOverride::Plain("1".to_owned()),
                ));
            }
            if matches!(action, StepAction::Rhai { .. }) {
                // Rhai `args` must match what a run step's `{args}` template
                // receives: Effigy consumes one leading `--` boundary flag of
                // its own, and later `--` tokens stay the task's.
                for (key, value) in [
                    (
                        "EFFIGY_RHAI_ARGS_JSON",
                        serde_json::to_string(effigy_tasks::command_passthrough_args(passthrough))
                            .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
                    ),
                    (
                        "EFFIGY_RHAI_CATALOG_ROOT",
                        selection.catalog.catalog_root.display().to_string(),
                    ),
                    (
                        "EFFIGY_RHAI_INVOCATION_CWD",
                        preflight.invocation_cwd.display().to_string(),
                    ),
                ] {
                    environment.push((
                        key.to_owned(),
                        crate::runner::task_engine::EnvOverride::Plain(value),
                    ));
                }
            }
            let plain = PlainCommand {
                command: &command,
                cwd,
                env: environment,
                capture: preflight.output_json || helper,
                timeout_ms,
            };
            // A builtin or script step is Effigy's own child process: it
            // receives the session grant. A user command is a workload and
            // never does.
            let exit = if helper {
                run_effigy_child(run, plain)?
            } else {
                run_plain(run, plain)?
            };
            if helper && !preflight.output_json {
                print!("{}", String::from_utf8_lossy(&exit.stdout));
                if exit.code == 0 {
                    eprint!("{}", String::from_utf8_lossy(&exit.stderr));
                }
            }
            Ok((
                exit.code,
                String::from_utf8_lossy(&exit.stdout).into_owned(),
                String::from_utf8_lossy(&exit.stderr).into_owned(),
                exit.workload,
            ))
        }
    }
}

fn nested_exit(output: String, capture: bool) -> Result<StepExit, RunnerError> {
    if capture {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
            if value["schema"] != "effigy.task.run.v1" {
                return Ok((0, output, String::new(), WorkloadOutcome::Succeeded));
            }
            let code = value["exit_code"].as_i64().unwrap_or(0) as i32;
            return Ok((
                code,
                value["stdout"].as_str().unwrap_or_default().to_owned(),
                value["stderr"].as_str().unwrap_or_default().to_owned(),
                crate::runner::task_engine::workload_for_exit(code),
            ));
        }
    } else if !output.is_empty() {
        print!("{output}");
    }
    Ok((
        0,
        if capture { output } else { String::new() },
        String::new(),
        WorkloadOutcome::Succeeded,
    ))
}

enum StepAction {
    Command(String),
    Task {
        invocation: TaskInvocation,
        cwd: PathBuf,
    },
    Builtin {
        command: String,
        parsed: Box<Command>,
        cwd: PathBuf,
    },
    Rhai {
        path: PathBuf,
    },
    Noop,
}
impl StepAction {
    #[cfg(test)]
    fn kind(&self) -> &'static str {
        match self {
            Self::Command(_) => "command",
            Self::Task { .. } => "task",
            Self::Builtin { .. } => "builtin",
            Self::Rhai { .. } => "rhai",
            Self::Noop => "env",
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
                table.rhai.as_deref(),
            ) {
                (Some(run), None, None) => {
                    let rendered = render_step_command_template(
                        run,
                        preflight.task_execution_root(&selection.catalog.catalog_root),
                        selection.catalog.bundle_root.as_deref(),
                        &effigy_tasks::render_template_args(passthrough),
                    );
                    Ok(StepAction::Command(render_skill_token(rendered, preflight)))
                }
                (None, Some(task_ref), None) => resolve_task_step(task_ref, preflight, selection),
                (None, None, Some(path)) => Ok(StepAction::Rhai {
                    path: PathBuf::from(render_script_path(
                        path,
                        &selection.catalog.catalog_root,
                        selection.catalog.bundle_root.as_deref(),
                        preflight.task_source.is_some(),
                    )),
                }),
                (None, None, None) if table.env.is_some() || table.env_file.is_some() => {
                    Ok(StepAction::Noop)
                }
                _ => Err(RunnerError::task_invocation(format!(
                    "task `{}` run step is invalid: define exactly one of `run`, `task`, or `rhai`",
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
    // The same `{repo}`/`{cwd}` spellings a run command supports work inside a
    // task reference, so a nested catalog has a cwd-proof argument spelling.
    let task_ref = effigy_managed::references::expand_reference_placeholders(
        task_ref,
        preflight.task_execution_root(&selection.catalog.catalog_root),
        &selection.catalog.catalog_root,
    );
    let (mut selector, mut args) = parse_task_reference_invocation(&task_ref).map_err(|error| {
        RunnerError::task_invocation(format!("run step task ref `{task_ref}`: {error}"))
    })?;
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
        let parsed = parse_builtin_step_command(&selector.task_name, &args, &cwd)?;
        let command = effigy_managed::render_builtin_reference_invocation(
            &selector.task_name,
            &effigy_tasks::render_template_args(&args),
            &cwd,
        )?;
        return Ok(StepAction::Builtin {
            command,
            parsed: Box::new(parsed),
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
    select_catalog_and_task(&selector, &preflight.catalogs, &preflight.invocation_cwd).map_err(
        |error| RunnerError::task_invocation(format!("run step task ref `{task_ref}`: {error}")),
    )?;
    let invocation = TaskInvocation {
        name: render_task_selector(&selector),
        args,
    };
    let cwd = preflight.resolved.resolved_root.clone();
    Ok(StepAction::Task { invocation, cwd })
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

#[cfg(test)]
mod tests {
    use super::parse_builtin_step_command;
    use super::{resolve_task_step, sequence_step_label, StepAction};
    use crate::runner::execute::preflight::ExecutionPreflight;
    use effigy_cli::Command;
    use effigy_core::resolver::{ResolutionMode, ResolvedTarget};
    use effigy_execution::ExecutionDiscoveryPlan;
    use effigy_manifest::{LoadedCatalog, TaskManifest, TaskSelection};
    use effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector};
    use std::path::{Path, PathBuf};

    #[test]
    fn a_sequence_step_names_the_task_its_position_and_its_label() {
        assert_eq!(
            sequence_step_label("dev", 2, 4, Some("build")),
            "running dev step 2/4: build"
        );
    }

    #[test]
    fn single_step_labels_omit_ordinals_and_prefer_description() {
        assert_eq!(sequence_step_label("dev", 1, 1, None), "running dev");
        assert_eq!(
            sequence_step_label("dev", 1, 1, Some("starting services")),
            "running dev: starting services"
        );
    }

    fn classify_manifest(body: &str) -> bool {
        let fixture = effigy_test_support::Fixture::new("q010-classification-").unwrap();
        std::fs::write(fixture.path().join("effigy.toml"), body).unwrap();
        let invocation = effigy_cli::TaskInvocation {
            name: "start".into(),
            args: Vec::new(),
        };
        let preflight =
            super::super::preflight::build_execution_preflight(&invocation, fixture.path().into())
                .unwrap();
        let selection = effigy_routing::select_catalog_and_task(
            &preflight.selector,
            &preflight.catalogs,
            &preflight.invocation_cwd,
        )
        .unwrap();
        super::foreground_tree(&preflight, &selection).unwrap()
    }

    #[test]
    fn opaque_durable_descendants_are_declared_before_tree_admission() {
        assert!(classify_manifest(
            "[tasks.start]\nscratch = false\nrun = 'true'\n"
        ));
        assert!(!classify_manifest(
            "[tasks.start]\nscratch = false\ndurable = true\nrun = 'effigy --run-id child inner'\n"
        ));
        assert!(!classify_manifest(
            "[tasks.start]\nscratch = false\nrun = [{task = 'inner'}]\n[tasks.inner]\ndurable = true\nrun = 'true'\n"
        ));
    }

    #[test]
    fn nested_config_reference_classifies_the_declared_override() {
        let prefix = "[tasks.start]\nscratch = false\nrun = [{task = 'config'}]\n";
        assert!(classify_manifest(&format!(
            "{prefix}[tasks.config]\nscratch = false\nrun = 'true'\n"
        )));
        assert!(!classify_manifest(&format!(
            "{prefix}[tasks.config]\nmode = 'tui'\nrun = 'true'\n"
        )));
    }

    #[test]
    fn deferred_config_reference_stays_durable_before_execution() {
        assert!(!classify_manifest(
            "[defer]\nrun = 'true'\nbuiltins = ['config']\n[tasks.start]\nscratch = false\nrun = [{task = 'config'}]\n"
        ));
    }

    fn vault_generate_prelude(generate: Option<&str>) -> String {
        let generate = generate.map(|line| format!("{line}\n")).unwrap_or_default();
        format!(
            "[secrets]\nbackend = 'effigy-vault'\n[secrets.vault]\npath = '.effigy/secrets/local.vault'\n{generate}[secrets.keys.api_token]\nrequired = true\ntargets = ['tasks']\n"
        )
    }

    #[test]
    fn declared_vault_generate_hook_selects_durability_before_task_startup() {
        let hook = vault_generate_prelude(Some("generate = { task = 'secrets:generate-dev' }"));
        let start = "[tasks.start]\nscratch = false\nsecrets = 'required'\nrun = 'true'\n";
        assert!(
            !classify_manifest(&format!("{hook}{start}")),
            "a task whose required-secret startup can run the declared hook is durable"
        );
        assert!(
            classify_manifest(&format!(
                "{hook}[tasks.start]\nscratch = false\nrun = 'true'\n"
            )),
            "without `secrets = 'required'` startup never resolves the hook"
        );
        assert!(
            classify_manifest(&format!(
                "{hook}[tasks.start]\nscratch = false\nsecrets = 'required'\nrun = [{{task = 'other'}}]\n[tasks.other]\nscratch = false\nrun = 'true'\n"
            )),
            "a task reference is not a direct shell, so secret startup never runs"
        );
        assert!(
            classify_manifest(&format!(
                "{}[tasks.start]\nscratch = false\nsecrets = 'required'\nrun = 'true'\n",
                vault_generate_prelude(None)
            )),
            "a manifest without a declared hook earns no durable admission"
        );
        assert!(
            classify_manifest(&format!(
                "{}[tasks.start]\nscratch = false\nsecrets = 'required'\nrun = 'true'\n",
                vault_generate_prelude(Some("generate = { task = 'secrets:generate-dev' }"))
                    .replace("targets = ['tasks']", "targets = ['containers']")
            )),
            "a key outside the task targets never reaches the hook"
        );
    }

    #[test]
    fn diagnostic_builtin_sequence_is_foreground_but_qa_execution_is_durable() {
        assert!(classify_manifest(
            "[tasks.start]\nscratch = false\nrun = [{task = 'tasks'}, {task = 'help test'}, {task = 'gateway status'}]\n"
        ));
        assert!(!classify_manifest(
            "[tasks.start]\nscratch = false\nrun = [{task = 'tasks qa-group run checks'}]\n"
        ));
    }

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
        LoadedCatalog::for_test(alias, PathBuf::from(root), manifest, depth)
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
[tasks."db:migrate"]
run = "true"
[tasks."state:apply:schema"]
run = [{ task = "db:migrate" }]
"#,
            ),
            1,
        );
        let preflight = ExecutionPreflight {
            invocation: effigy_cli::TaskInvocation {
                name: "state:apply:schema".to_owned(),
                args: Vec::new(),
            },
            invocation_cwd: PathBuf::from("/workspace-root/acowtancy"),
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
            help: false,
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
}
