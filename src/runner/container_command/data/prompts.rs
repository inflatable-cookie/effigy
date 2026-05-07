use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use effigy_bootstrap::BootstrapStagedDbSeed;
use effigy_builtin::{PromptDecision, PromptPolicy};

use super::RunnerError;

pub(super) fn maybe_confirm_container_data_pull_production(
    container_name: &str,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !container_data_pull_production_prompt_required(
        container_name,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_container_data_pull_production_from_io(container_name, &mut stdin, &mut stdout)
}

pub(super) fn maybe_confirm_container_data_seed(
    container_name: &str,
    staged_db_seeds: &[BootstrapStagedDbSeed],
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !container_data_seed_prompt_required(
        container_name,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_container_data_seed_from_io(container_name, staged_db_seeds, &mut stdin, &mut stdout)
}

pub(super) fn container_data_pull_production_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data pull-production` requires confirmation before pulling production data into the local generated-compose environment. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

pub(super) fn container_data_import_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data import` requires confirmation before importing archive data into the local generated-compose environment. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

pub(super) fn container_data_seed_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data seed` requires confirmation before resetting and importing local database dumps. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

pub(super) fn destructive_container_action_prompt_required(
    command_label: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "{command_label} requires confirmation because it deletes persistent local container data. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

pub(super) fn confirm_destructive_container_action_from_io<R: BufRead, W: Write>(
    description: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    write_confirmation_prompt(
        output,
        &format!("{description}\nThis deletes persistent local data.\n"),
    )?;
    read_confirmation(
        input,
        "destructive container action cancelled during confirmation",
    )
}

pub(super) fn confirm_container_data_import_from_io<R: BufRead, W: Write>(
    container_name: &str,
    volume_name: &str,
    archive_path: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    write_confirmation_prompt(
        output,
        &format!(
            "Import archive into local container `{container_name}`.\nVolume: {volume_name}\nArchive: {}\nThis may overwrite local generated-compose data.\n",
            archive_path.display()
        ),
    )?;
    read_confirmation(input, "container data import cancelled during confirmation")
}

pub(super) fn confirm_container_data_pull_production_from_io<R: BufRead, W: Write>(
    container_name: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    write_confirmation_prompt(
        output,
        &format!(
            "Pull production data into local container `{container_name}`.\nThis may overwrite local generated-compose data.\n"
        ),
    )?;
    read_confirmation(
        input,
        "container data pull-production cancelled during confirmation",
    )
}

pub(super) fn confirm_container_data_seed_from_io<R: BufRead, W: Write>(
    container_name: &str,
    staged_db_seeds: &[BootstrapStagedDbSeed],
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    let seed_lines = staged_db_seeds
        .iter()
        .map(|seed| match seed.target.as_deref() {
            Some(target) => format!("{target}: {}", seed.source_path.display()),
            None => seed.source_path.display().to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    write_confirmation_prompt(
        output,
        &format!(
            "Reset and seed local database(s) for container `{container_name}`.\nSQL dumps:\n{seed_lines}\nThis may overwrite local generated-compose data.\n"
        ),
    )?;
    read_confirmation(input, "container data seed cancelled during confirmation")
}

fn write_confirmation_prompt<W: Write>(output: &mut W, message: &str) -> Result<(), RunnerError> {
    writeln!(output, "{message}")
        .and_then(|_| output.write_all(b"Continue? [y/N]: "))
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive container data prompt: {error}"
            ))
        })
}

fn read_confirmation<R: BufRead>(input: &mut R, cancelled: &str) -> Result<(), RunnerError> {
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive container data input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(cancelled))
}
