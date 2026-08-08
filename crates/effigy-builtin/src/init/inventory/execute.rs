use std::path::Path;

use effigy_cli::{
    BundleArgs, BundleSubcommand, Command, GraphArgs, GraphSubcommand, SecretsArgs,
    SecretsSubcommand, TaskInvocation,
};

use super::model::{InitActionReport, SetupActionOutcome, SetupActionStatus, SetupExecutionKind};
use super::SetupJob;
use crate::init::agent::{run_selected_agent_jobs, AgentInitAssets, AgentInitJob};
use crate::init::request::AgentInitMode;
use crate::{BuiltinError, BuiltinRuntimePorts};

pub(crate) fn execute_selected_actions(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    assets: &AgentInitAssets,
    jobs: &[SetupJob],
    selected_ids: &[String],
) -> Result<InitActionReport, BuiltinError> {
    let mut outcomes = Vec::new();
    let mut remaining = selected_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();

    for job in jobs {
        if !remaining.contains(&job.id) {
            continue;
        }
        remaining.remove(&job.id);
        outcomes.push(execute_one_action(ports, target_root, assets, job));
    }

    if !remaining.is_empty() {
        let mut unknown = remaining.into_iter().collect::<Vec<_>>();
        unknown.sort();
        return Err(BuiltinError::task_invocation(format!(
            "unknown init action id(s): {}",
            unknown.join(", ")
        )));
    }

    Ok(InitActionReport {
        selected_action_ids: selected_ids.to_vec(),
        outcomes,
    })
}

fn execute_one_action(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    assets: &AgentInitAssets,
    job: &SetupJob,
) -> SetupActionOutcome {
    if matches!(job.applicability, super::SetupApplicability::NotApplicable) {
        return SetupActionOutcome {
            id: job.id.clone(),
            status: SetupActionStatus::Blocked,
            summary: job.summary.clone(),
            reason: job.reason.clone(),
            command: job.recommended_command.clone(),
            output: None,
        };
    }

    match job.execution_kind {
        SetupExecutionKind::Guidance => SetupActionOutcome {
            id: job.id.clone(),
            status: SetupActionStatus::Guided,
            summary: job.summary.clone(),
            reason: job.reason.clone(),
            command: job.recommended_command.clone(),
            output: None,
        },
        SetupExecutionKind::Inspect | SetupExecutionKind::Apply if !job.can_run_noninteractive => {
            SetupActionOutcome {
                id: job.id.clone(),
                status: SetupActionStatus::Blocked,
                summary: job.summary.clone(),
                reason: if job.reason.is_empty() {
                    "this setup job cannot run non-interactively yet".to_owned()
                } else {
                    job.reason.clone()
                },
                command: job.recommended_command.clone(),
                output: None,
            }
        }
        SetupExecutionKind::Inspect | SetupExecutionKind::Apply => {
            let execution = if let Some(baseline_job) = baseline_job_for_id(&job.id) {
                execute_baseline_job(target_root, assets, baseline_job)
            } else {
                execute_delegated_job(ports, target_root, &job.id)
            };
            match execution {
                Ok((status, output, reason)) => SetupActionOutcome {
                    id: job.id.clone(),
                    status,
                    summary: job.summary.clone(),
                    reason,
                    command: job.recommended_command.clone(),
                    output,
                },
                Err(error) => SetupActionOutcome {
                    id: job.id.clone(),
                    status: SetupActionStatus::Failed,
                    summary: job.summary.clone(),
                    reason: error.to_string(),
                    command: job.recommended_command.clone(),
                    output: None,
                },
            }
        }
    }
}

fn baseline_job_for_id(id: &str) -> Option<AgentInitJob> {
    match id {
        "manifest.effigy_toml" => Some(AgentInitJob::Manifest),
        "readme.project_intro" => Some(AgentInitJob::Readme),
        "agents_md.effigy_contract" => Some(AgentInitJob::AgentsBlock),
        "skill.codex_project" => Some(AgentInitJob::SkillTree),
        "gitignore.effigy_local_state" => Some(AgentInitJob::Gitignore),
        _ => None,
    }
}

fn execute_baseline_job(
    target_root: &Path,
    assets: &AgentInitAssets,
    job: AgentInitJob,
) -> Result<(SetupActionStatus, Option<String>, String), BuiltinError> {
    let selected = std::collections::BTreeSet::from([job]);
    let checks = run_selected_agent_jobs(target_root, assets, AgentInitMode::Apply, &selected)?;
    let check = checks
        .into_iter()
        .next()
        .ok_or_else(|| BuiltinError::task_invocation("baseline init action returned no result"))?;
    let status = if check.changed() {
        SetupActionStatus::Applied
    } else {
        SetupActionStatus::Skipped
    };
    Ok((status, None, check.action_description()))
}

fn execute_delegated_job(
    ports: &dyn BuiltinRuntimePorts,
    target_root: &Path,
    id: &str,
) -> Result<(SetupActionStatus, Option<String>, String), BuiltinError> {
    let output = if let Some(command) = command_for_job(id, target_root) {
        ports.run_command(command)?
    } else if let Some(invocation) = task_invocation_for_job(id) {
        ports.run_manifest_task_with_cwd(&invocation, target_root.to_path_buf())?
    } else {
        return Err(BuiltinError::task_invocation(format!(
            "init action `{id}` has no executable adapter"
        )));
    };
    let status = match id {
        "graph_index.build"
        | "secrets_vault.init"
        | "bundle_sync.run"
        | "task_migration.package_json" => SetupActionStatus::Applied,
        _ => SetupActionStatus::Inspected,
    };
    Ok((status, Some(output), "command executed".to_owned()))
}

fn task_invocation_for_job(id: &str) -> Option<TaskInvocation> {
    let (name, args): (&str, &[&str]) = match id {
        "task_surface.scan" => ("tasks", &[]),
        "task_migration.package_json" => ("tasks", &["migrate", "--apply"]),
        "doctor.run" => ("doctor", &[]),
        "tasks.inspect" => ("tasks", &[]),
        "test_plan.inspect" => ("test", &["--plan"]),
        "validation_command.recommend" => ("test", &[]),
        _ => return None,
    };
    Some(TaskInvocation {
        name: name.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    })
}

fn command_for_job(id: &str, target_root: &Path) -> Option<Command> {
    let repo_override = Some(target_root.to_path_buf());
    match id {
        "graph_status.inspect" => Some(Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Status { refresh: false },
            repo_override,
            output_json: true,
        })),
        "graph_index.build" => Some(Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Index,
            repo_override,
            output_json: true,
        })),
        "secrets_surface.inspect" => Some(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Doctor,
            repo_override,
            output_json: false,
        })),
        "secrets_vault.init" => Some(Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override,
            output_json: false,
        })),
        "bundle_surface.inspect" => Some(Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Inspect,
            repo_override,
            output_json: false,
        })),
        "bundle_sync.run" => Some(Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Sync,
            repo_override,
            output_json: false,
        })),
        _ => None,
    }
}
