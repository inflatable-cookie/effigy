use std::path::PathBuf;

use crate::{unknown_argument, CliParseError, Command};

/// The environment name a task run reads its lock wait from. `--lock-wait-ms`
/// is exported under this name so every nested Effigy a task step starts
/// inherits the same wait (effigy#1018).
pub const LOCK_WAIT_ENV: &str = "EFFIGY_LOCK_WAIT_MS";

const LOCK_WAIT_SUPPORTED_FORMS: &str = "`--lock-wait-ms` applies to a task or a skill run: `effigy --lock-wait-ms <MS> <task>` or `effigy --lock-wait-ms <MS> skill run <skill>/<task>`";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalCliOptions {
    pub json_mode: bool,
    /// Show detailed progress subjects and diagnostic events.
    pub verbose: bool,
    pub repo_override: Option<PathBuf>,
    pub task_verbose_root: bool,
    pub task_env_schema: Option<PathBuf>,
    pub task_lock_wait_ms: Option<String>,
    pub task_plan: bool,
    /// `--run-id <key>`: a caller-chosen idempotency key for one task run.
    pub run_id: Option<String>,
}

pub fn strip_global_cli_flags(
    args: Vec<String>,
) -> Result<(Vec<String>, GlobalCliOptions), CliParseError> {
    let mut stripped = Vec::with_capacity(args.len());
    let mut options = GlobalCliOptions::default();
    let mut args = args.into_iter();
    let mut parsing_leading_globals = true;

    while let Some(arg) = args.next() {
        if !parsing_leading_globals {
            stripped.push(arg);
            stripped.extend(args);
            break;
        }

        match arg.as_str() {
            "--json" => options.json_mode = true,
            "--verbose" => options.verbose = true,
            "--repo" => {
                options.repo_override = Some(PathBuf::from(args.next().ok_or(
                    CliParseError::MissingFlagValue {
                        flag: "--repo".to_owned(),
                    },
                )?));
            }
            "--env-schema" => {
                options.task_env_schema = Some(PathBuf::from(args.next().ok_or(
                    CliParseError::MissingFlagValue {
                        flag: "--env-schema".to_owned(),
                    },
                )?));
            }
            "--host-run-request-id" => {
                return Err(CliParseError::InvalidArguments(
                    crate::RETIRED_HOST_RUN_REQUEST_ID.to_owned(),
                ));
            }
            "--run-id" => {
                if options.run_id.is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`--run-id` may be supplied only once".to_owned(),
                    ));
                }
                options.run_id = Some(args.next().ok_or(CliParseError::MissingFlagValue {
                    flag: "--run-id".to_owned(),
                })?);
            }
            "--verbose-root" => options.task_verbose_root = true,
            "--lock-wait-ms" => {
                let wait = args.next().ok_or(CliParseError::MissingFlagValue {
                    flag: "--lock-wait-ms".to_owned(),
                })?;
                if wait.trim().parse::<u64>().is_err() {
                    return Err(CliParseError::InvalidArguments(format!(
                        "`--lock-wait-ms` takes a non-negative integer millisecond count, not `{wait}`"
                    )));
                }
                options.task_lock_wait_ms = Some(wait);
            }
            "--plan" => options.task_plan = true,
            "--help" | "-h" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            "--version" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            "--" => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                parsing_leading_globals = false;
                stripped.push(arg);
            }
        }
    }

    Ok((stripped, options))
}

/// Exports `--lock-wait-ms` as `EFFIGY_LOCK_WAIT_MS` for this process and
/// every child it spawns, so a nested `effigy <selector>` a task step runs
/// waits on the same lock as the outer run. Only an explicit flag writes the
/// variable: an inherited value stays unless the flag names a wait.
///
/// The flag's value is validated by `strip_global_cli_flags`. Call once,
/// before any task runs and while the process is still single-threaded.
pub fn export_lock_wait_env(options: &GlobalCliOptions) {
    if let Some(wait) = options.task_lock_wait_ms.as_deref() {
        std::env::set_var(LOCK_WAIT_ENV, wait.trim());
    }
}

pub(super) fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg);
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg);
    }
    (stripped, json_mode)
}

pub fn apply_global_cli_options(
    mut cmd: Command,
    options: &GlobalCliOptions,
) -> Result<Command, CliParseError> {
    // A repository task: Effigy's flags are the ones before the selector,
    // and the task receives everything after it (D1-34). Built-in commands
    // routed as tasks keep their own flag positions, with or without a
    // catalog prefix (`catalog_a/tasks`).
    if let Command::Task(task) = &mut cmd {
        let routed_name = task.name.rsplit('/').next().unwrap_or(&task.name);
        if !effigy_core::builtin_tasks::is_builtin_task_name(routed_name)
            && !task
                .args
                .iter()
                .any(|arg| arg == effigy_core::task_args::TASK_ARGS_BOUNDARY)
        {
            let mut effigy = Vec::new();
            if let Some(repo) = options.repo_override.as_ref() {
                effigy.extend(["--repo".to_owned(), repo.display().to_string()]);
            }
            if options.task_verbose_root {
                effigy.push("--verbose-root".to_owned());
            }
            if let Some(schema) = options.task_env_schema.as_ref() {
                effigy.extend(["--env-schema".to_owned(), schema.display().to_string()]);
            }
            if let Some(wait) = options.task_lock_wait_ms.as_ref() {
                effigy.extend(["--lock-wait-ms".to_owned(), wait.clone()]);
            }
            if options.task_plan {
                effigy.push("--plan".to_owned());
            }
            if options.json_mode {
                effigy.push("--json".to_owned());
            }
            let task_args = std::mem::take(&mut task.args);
            task.args = effigy_core::task_args::join_task_args(effigy, task_args);
            if let Some(key) = options.run_id.as_ref() {
                task.args.push(crate::INTERNAL_RUN_ID_ARG.to_owned());
                task.args.push(key.clone());
            }
            return Ok(cmd);
        }
    }
    if let Some(wait) = options.task_lock_wait_ms.as_ref() {
        match &mut cmd {
            Command::Task(task) => {
                if !runtime_flag_present_before_passthrough(&task.args, "--lock-wait-ms") {
                    task.args.insert(0, wait.clone());
                    task.args.insert(0, "--lock-wait-ms".to_owned());
                }
            }
            // A skill run's wait travels in the environment (see
            // `export_lock_wait_env`), so there is no argument to place.
            Command::Skill(args)
                if matches!(args.subcommand, crate::SkillSubcommand::Run { .. }) => {}
            _ => {
                return Err(CliParseError::InvalidArguments(
                    LOCK_WAIT_SUPPORTED_FORMS.to_owned(),
                ))
            }
        }
    }
    if options.task_plan {
        match &mut cmd {
            Command::Task(task) => {
                if !runtime_flag_present_before_passthrough(&task.args, "--plan") {
                    task.args.insert(0, "--plan".to_owned());
                }
            }
            _ => return Err(unknown_argument("--plan")),
        }
    }
    if let Some(repo_override) = options.repo_override.as_ref() {
        match &mut cmd {
            Command::Bundle(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Changelog(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Fmt(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Deploy(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Deps(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Secrets(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Defer(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Exec(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::State(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Workspace(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Service(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Rhai(_) => return Err(unknown_argument("--repo")),
            Command::Skill(args) => match &mut args.subcommand {
                crate::SkillSubcommand::Run {
                    repo_override: slot,
                    ..
                } => {
                    slot.get_or_insert_with(|| repo_override.clone());
                }
                crate::SkillSubcommand::Tasks { .. } => return Err(unknown_argument("--repo")),
            },
            Command::Docs(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Contracts(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Container(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Release(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Doctor(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Tasks(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Runs(args) => {
                args.repo_override
                    .get_or_insert_with(|| repo_override.clone());
            }
            Command::Task(task) => {
                if !runtime_flag_present_before_passthrough(&task.args, "--repo") {
                    task.args.insert(0, repo_override.display().to_string());
                    task.args.insert(0, "--repo".to_owned());
                }
            }
            Command::Version
            | Command::Bootstrap(_)
            | Command::Uninstall(_)
            | Command::Gateway(_)
            | Command::InternalGateway(_)
            | Command::InternalGatewayIdentity(_)
            | Command::InternalGuestProbe(_)
            | Command::InternalScriptRun(_)
            | Command::InternalContainerLeaseReaper(_) => return Err(unknown_argument("--repo")),
            Command::Help(_) | Command::HelpGroup(_) => {}
        }
    }

    if options.task_verbose_root {
        match &mut cmd {
            Command::Task(task) => {
                if !runtime_flag_present_before_passthrough(&task.args, "--verbose-root") {
                    task.args.insert(0, "--verbose-root".to_owned());
                }
            }
            _ => return Err(unknown_argument("--verbose-root")),
        }
    }

    if let Some(env_schema) = options.task_env_schema.as_ref() {
        match &mut cmd {
            Command::Task(task) => {
                if !runtime_flag_present_before_passthrough(&task.args, "--env-schema") {
                    task.args.insert(0, env_schema.display().to_string());
                    task.args.insert(0, "--env-schema".to_owned());
                }
            }
            _ => return Err(unknown_argument("--env-schema")),
        }
    }

    if let Some(key) = options.run_id.as_ref() {
        // A built-in routed as a task (`tasks`, `test`) is not a task run.
        match &mut cmd {
            Command::Task(task)
                if !effigy_core::builtin_tasks::is_builtin_task_name(
                    task.name.rsplit('/').next().unwrap_or(&task.name),
                ) =>
            {
                task.args.push(crate::INTERNAL_RUN_ID_ARG.to_owned());
                task.args.push(key.clone());
            }
            _ => {
                return Err(CliParseError::InvalidArguments(
                    "`--run-id` names one repository task run: `effigy --run-id <key> <task>`"
                        .to_owned(),
                ))
            }
        }
    }

    if options.json_mode {
        if let Command::Skill(args) = &cmd {
            if args.stdio_passthrough() {
                return Err(CliParseError::InvalidArguments(
                    "`--json` and `--stdio passthrough` are incompatible: JSON mode lets Effigy own a versioned envelope while passthrough lets the task own raw stdio; choose one".to_owned(),
                ));
            }
        }
        cmd = apply_global_json_flag(cmd, true);
    }

    Ok(cmd)
}

pub(super) fn apply_global_json_flag(mut cmd: Command, json_mode: bool) -> Command {
    if !json_mode {
        return cmd;
    }

    match &mut cmd {
        Command::Version => {}
        Command::Bundle(args) => args.output_json = true,
        Command::Deploy(args) => args.output_json = true,
        Command::Deps(args) => args.output_json = true,
        Command::Secrets(args) => args.output_json = true,
        Command::Defer(args) => args.output_json = true,
        Command::Exec(args) => args.output_json = true,
        Command::State(args) => args.output_json = true,
        Command::Workspace(args) => args.output_json = true,
        Command::Gateway(args) => args.output_json = true,
        Command::Service(args) => args.output_json = true,
        Command::Task(task) => {
            if !runtime_flag_present_before_passthrough(&task.args, "--json") {
                // After a nested built-in's subcommand word, which may follow
                // a leading `--repo <PATH>`.
                let insert_at = task
                    .args
                    .iter()
                    .take(3)
                    .position(|arg| matches!(arg.as_str(), "unlock" | "cache" | "completion"))
                    .map_or(0, |index| index + 1);
                task.args.insert(insert_at, "--json".to_owned());
            }
        }
        Command::Changelog(args) => args.output_json = true,
        Command::Fmt(args) => args.output_json = true,
        Command::Rhai(args) => args.output_json = true,
        Command::Skill(args) => args.output_json = true,
        Command::Docs(args) => args.output_json = true,
        Command::Contracts(args) => args.output_json = true,
        Command::Container(args) => args.output_json = true,
        Command::Bootstrap(args) => args.output_json = true,
        Command::Uninstall(args) => args.output_json = true,
        Command::Release(args) => args.output_json = true,
        Command::Tasks(args) => args.output_json = true,
        Command::Runs(args) => args.output_json = true,
        Command::Doctor(args) => args.output_json = true,
        Command::InternalGateway(_) => {}
        Command::InternalGatewayIdentity(_) => {}
        Command::InternalGuestProbe(_) => {}
        Command::InternalScriptRun(_) => {}
        Command::InternalContainerLeaseReaper(_) => {}
        Command::Help(_) | Command::HelpGroup(_) => {}
    }
    cmd
}

pub(super) fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Version => false,
        Command::Bundle(args) => args.output_json,
        Command::Deploy(args) => args.output_json,
        Command::Deps(args) => args.output_json,
        Command::Secrets(args) => args.output_json,
        Command::Defer(args) => args.output_json,
        Command::Exec(args) => args.output_json,
        Command::State(args) => args.output_json,
        Command::Workspace(args) => args.output_json,
        Command::Gateway(args) => args.output_json,
        Command::Service(args) => args.output_json,
        Command::Changelog(args) => args.output_json,
        Command::Fmt(args) => args.output_json,
        Command::Rhai(args) => args.output_json,
        Command::Skill(args) => args.output_json,
        Command::Docs(args) => args.output_json,
        Command::Contracts(args) => args.output_json,
        Command::Container(args) => args.output_json,
        Command::Bootstrap(args) => args.output_json,
        Command::Uninstall(args) => args.output_json,
        Command::Release(args) => args.output_json,
        Command::Tasks(args) => args.output_json,
        Command::Runs(args) => args.output_json,
        Command::Doctor(args) => args.output_json,
        Command::Task(task) => runtime_flag_present_before_passthrough(&task.args, "--json"),
        Command::InternalGateway(_) => false,
        Command::InternalGatewayIdentity(_) => false,
        Command::InternalGuestProbe(_) => false,
        Command::InternalScriptRun(_) => false,
        Command::InternalContainerLeaseReaper(_) => false,
        Command::Help(_) | Command::HelpGroup(_) => false,
    }
}

pub fn runtime_flag_present_before_passthrough(args: &[String], flag: &str) -> bool {
    if let Some(split) = effigy_core::task_args::split_task_args(args) {
        return split.effigy.iter().any(|arg| arg == flag);
    }
    args.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == flag)
}

#[cfg(test)]
mod progress_verbose_tests {
    use super::*;

    #[test]
    fn leading_verbose_is_global_and_task_arguments_stay_untouched() {
        let (args, options) =
            strip_global_cli_flags(vec!["--verbose".into(), "container".into(), "up".into()])
                .unwrap();
        assert!(options.verbose);
        assert_eq!(args, ["container", "up"]);
        let (args, options) =
            strip_global_cli_flags(vec!["build".into(), "--verbose".into()]).unwrap();
        assert!(!options.verbose);
        assert_eq!(args, ["build", "--verbose"]);
    }
}

#[cfg(test)]
mod lock_wait_flag_tests {
    use crate::{parse_command, CliParseError, Command, SkillSubcommand};

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn lock_wait_flag_is_accepted_before_a_skill_run() {
        let command = parse_command(argv(&[
            "--lock-wait-ms",
            "600000",
            "skill",
            "run",
            "--path",
            "/opt/skills/example",
            "example/check",
        ]))
        .expect("skill run takes the lock wait");
        assert!(
            matches!(
                command,
                Command::Skill(ref args) if matches!(args.subcommand, SkillSubcommand::Run { .. })
            ),
            "{command:?}"
        );
    }

    #[test]
    fn lock_wait_flag_is_refused_with_its_supported_forms_where_it_cannot_apply() {
        let error = parse_command(argv(&[
            "--lock-wait-ms",
            "5",
            "skill",
            "tasks",
            "--path",
            "/opt/skills/example",
        ]))
        .expect_err("skill tasks runs no task");
        let message = error.to_string();
        assert!(message.contains("skill run"), "{message}");
        assert!(!message.contains("unknown argument"), "{message}");
    }

    #[test]
    fn lock_wait_flag_must_name_a_millisecond_count() {
        let error = crate::strip_global_cli_flags(argv(&["--lock-wait-ms", "soon", "build"]))
            .expect_err("a non-numeric wait is refused at parse time");
        assert!(
            matches!(error, CliParseError::InvalidArguments(ref message) if message.contains("non-negative")),
            "{error:?}"
        );
    }
}
