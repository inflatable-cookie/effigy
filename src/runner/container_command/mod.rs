//! CLI command handler for `effigy container` subcommands.

use effigy_containers::ContainerCommandReport;

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use effigy_cli::{ContainerArgs, ContainerSubcommand};

use super::error::RunnerError;
use lifecycle::{
    run_container_down, run_container_eject, run_container_logs, run_container_reset,
    run_container_shell, run_container_stats_all, run_container_status, run_container_up,
};

pub(in crate::runner) use lifecycle::run_task_container_session;

mod gateway_registration;
mod lifecycle;
mod session;
mod signals;

pub(super) fn run_container(args: ContainerArgs) -> Result<String, RunnerError> {
    if let ContainerSubcommand::Status { name: _, all: true } = &args.subcommand {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container status --all` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return lifecycle::run_container_status_all(args.output_json);
    }
    if let ContainerSubcommand::Stats { all: true } = &args.subcommand {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container stats --all` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_stats_all(args.output_json);
    }

    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        ContainerSubcommand::Up {
            name,
            attach,
            detach,
        } => run_container_up(
            &repo_root,
            name.as_deref(),
            attach,
            detach,
            args.output_json,
        ),
        ContainerSubcommand::Down { name } => {
            run_container_down(&repo_root, name.as_deref(), args.output_json)
        }
        ContainerSubcommand::Status { name, all: false } => {
            run_container_status(&repo_root, name.as_deref(), args.output_json)
        }
        ContainerSubcommand::Status { all: true, .. } => unreachable!("handled above"),
        ContainerSubcommand::Stats { all: false } => unreachable!("parser rejects this shape"),
        ContainerSubcommand::Stats { all: true } => unreachable!("handled above"),
        ContainerSubcommand::Logs {
            name,
            service,
            follow,
        } => run_container_logs(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            follow,
            args.output_json,
        ),
        ContainerSubcommand::Shell {
            name,
            service,
            command,
        } => run_container_shell(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            command.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Reset { name, keep_data } => {
            run_container_reset(&repo_root, name.as_deref(), keep_data, args.output_json)
        }
        ContainerSubcommand::Eject { name } => {
            run_container_eject(&repo_root, name.as_deref(), args.output_json)
        }
    }
}

fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_cli::ContainerSubcommand;

    #[test]
    fn container_stats_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Stats { all: true },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("stats --all should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container stats --all` does not accept `--repo`"));
    }
}
