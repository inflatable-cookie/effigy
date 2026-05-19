use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::BuiltinRuntimePorts;
use crate::{PromptDecision, PromptPolicy};
use effigy_catalog::{StarterFile, StarterResolver};
use effigy_cli::{HelpTopic, TaskInvocation};
use effigy_core::fs_probe::PathPresenceCache;

use super::command_spec::run_builtin_command;
use super::render_builtin_help_topic;
use crate::BuiltinError;
#[path = "init/agent.rs"]
mod agent;
#[path = "init/inventory.rs"]
mod inventory;
#[path = "init/output.rs"]
mod output;
#[path = "init/request.rs"]
mod request;
#[path = "init/scaffold.rs"]
mod scaffold;
#[path = "init/wizard.rs"]
mod wizard;

pub(super) fn run_builtin_init(
    ports: &dyn BuiltinRuntimePorts,
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    run_builtin_command(
        args,
        |output_json| render_builtin_help_topic(HelpTopic::Init, "init", output_json),
        || request::parse_init_request(task, args),
        |request: request::InitRequest| run_init_request(ports, request, target_root),
    )
}

fn run_init_request(
    ports: &dyn BuiltinRuntimePorts,
    request: request::InitRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    match request.mode {
        request::InitMode::Ensure { mode } => {
            if should_prompt_init_wizard(&request, mode) {
                return wizard::run_init_wizard(target_root, || {
                    scaffold::load_starter(request::DEFAULT_STARTER)
                });
            }
            agent::run_agent_init(target_root, request.output_json, mode, || {
                scaffold::load_starter(request::DEFAULT_STARTER)
            })
        }
        request::InitMode::Checklist => {
            let assets =
                agent::load_agent_init_assets(|| scaffold::load_starter(request::DEFAULT_STARTER))?;
            let checks = agent::collect_agent_checks(
                target_root,
                &assets,
                request::AgentInitMode::Check,
                None,
            )?;
            let jobs = inventory::build_setup_inventory(target_root, &checks);
            output::render_init_checklist_response(request.output_json, target_root, &jobs)
        }
        request::InitMode::ApplyActions { action_ids } => {
            let assets =
                agent::load_agent_init_assets(|| scaffold::load_starter(request::DEFAULT_STARTER))?;
            let checks = agent::collect_agent_checks(
                target_root,
                &assets,
                request::AgentInitMode::Check,
                None,
            )?;
            let jobs = inventory::build_setup_inventory(target_root, &checks);
            let report = inventory::execute_selected_actions(
                ports,
                target_root,
                &assets,
                &jobs,
                &action_ids,
            )?;
            output::render_init_actions_response(request.output_json, &report)
        }
        request::InitMode::List => run_list(request.output_json),
        request::InitMode::Emit { starter_name } => run_emit(
            starter_name,
            target_root,
            request.output_json,
            request.force,
            request.dry_run,
        ),
    }
}

fn run_list(output_json: bool) -> Result<Option<String>, BuiltinError> {
    let resolver = StarterResolver::new();
    let starters = resolver.list();
    output::render_init_list_response(output_json, starters)
}

fn run_emit(
    starter_name: String,
    target_root: &Path,
    output_json: bool,
    force: bool,
    dry_run: bool,
) -> Result<Option<String>, BuiltinError> {
    let starter = scaffold::load_starter(&starter_name)?;

    // Resolve per-file target paths once up front so both the conflict
    // check and the emission loop agree on the same paths.
    let planned: Vec<(PathBuf, &StarterFile)> = starter
        .files
        .iter()
        .map(|file| (target_root.join(&file.target), file))
        .collect();

    let mut probe = PathPresenceCache::new();
    let blockers: Vec<&PathBuf> = planned
        .iter()
        .filter_map(|(path, file)| {
            if !probe.exists(path) {
                return None;
            }
            if skip_existing_readme(file, true, force) {
                return None;
            }
            Some(path)
        })
        .collect();

    if !blockers.is_empty() && !force && !dry_run {
        let listing = blockers
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(BuiltinError::task_invocation(format!(
            "starter `{}` path(s) already exists: {}. Use `effigy init --force` to overwrite or `effigy init --dry-run` to preview.",
            starter_name, listing
        )));
    }

    let mut emitted = Vec::with_capacity(planned.len());
    for (path, file) in &planned {
        let existed = probe.exists(path);
        let skip_readme = skip_existing_readme(file, existed, force);
        if dry_run {
            emitted.push(output::EmittedFile {
                target: file.target.clone(),
                path: path.clone(),
                contents: file.contents.clone(),
                existed,
                written: false,
                skipped: skip_readme,
            });
            continue;
        }

        if skip_readme {
            emitted.push(output::EmittedFile {
                target: file.target.clone(),
                path: path.clone(),
                contents: file.contents.clone(),
                existed,
                written: false,
                skipped: true,
            });
            continue;
        }

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| BuiltinError::task_invocation_failed_write(parent, error))?;
            }
        }
        std::fs::write(path, file.contents.as_bytes())
            .map_err(|error| BuiltinError::task_invocation_failed_write(path, error))?;
        emitted.push(output::EmittedFile {
            target: file.target.clone(),
            path: path.clone(),
            contents: file.contents.clone(),
            existed,
            written: true,
            skipped: false,
        });
    }

    output::render_init_response(
        output_json,
        &starter,
        emitted,
        output::InitOutcome {
            written: !dry_run,
            dry_run,
        },
    )
}

fn should_prompt_init_wizard(request: &request::InitRequest, mode: request::AgentInitMode) -> bool {
    should_prompt_init_wizard_for_terminals(
        request,
        mode,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )
}

fn should_prompt_init_wizard_for_terminals(
    request: &request::InitRequest,
    mode: request::AgentInitMode,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> bool {
    if !matches!(mode, request::AgentInitMode::Apply) || !request.implicit_default_apply {
        return false;
    }
    let policy = PromptPolicy {
        output_json: request.output_json,
        plan: false,
        explicit_non_interactive: false,
        stdin_is_tty,
        stdout_is_tty,
    };
    matches!(policy.decide(), PromptDecision::Prompt)
}

/// Root `README.md` from a starter is optional: never clobber an existing project
/// README unless `--force` is set.
fn skip_existing_readme(file: &StarterFile, path_exists: bool, force: bool) -> bool {
    file.target == "README.md" && path_exists && !force
}

#[cfg(test)]
mod tests {
    use super::request::{AgentInitMode, InitMode, InitRequest};
    use super::should_prompt_init_wizard_for_terminals;

    fn plain_request() -> InitRequest {
        InitRequest {
            mode: InitMode::Ensure {
                mode: AgentInitMode::Apply,
            },
            output_json: false,
            force: false,
            dry_run: false,
            implicit_default_apply: true,
        }
    }

    #[test]
    fn plain_tty_init_prompts_but_json_explicit_apply_and_non_tty_do_not() {
        assert!(should_prompt_init_wizard_for_terminals(
            &plain_request(),
            AgentInitMode::Apply,
            true,
            true,
        ));

        let mut json_request = plain_request();
        json_request.output_json = true;
        assert!(!should_prompt_init_wizard_for_terminals(
            &json_request,
            AgentInitMode::Apply,
            true,
            true,
        ));

        let mut explicit_request = plain_request();
        explicit_request.implicit_default_apply = false;
        assert!(!should_prompt_init_wizard_for_terminals(
            &explicit_request,
            AgentInitMode::Apply,
            true,
            true,
        ));

        assert!(!should_prompt_init_wizard_for_terminals(
            &plain_request(),
            AgentInitMode::Apply,
            false,
            true,
        ));
    }
}
