use std::path::{Path, PathBuf};

use effigy_cli::Command;

pub(in crate::runner) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Version => None,
        Command::Bundle(_) => None,
        Command::Changelog(_) => None,
        Command::Deploy(args) => args.repo_override.clone(),
        Command::Defer(args) => args.repo_override.clone(),
        Command::Exec(args) => args.repo_override.clone(),
        Command::System(args) => args.repo_override.clone(),
        Command::Workspace(args) => args.repo_override.clone(),
        Command::Gateway(_) => None,
        Command::Service(args) => args.repo_override.clone(),
        Command::Demo(args) => args.repo_override.clone(),
        Command::Docs(args) => args.repo_override.clone(),
        Command::Contracts(args) => args.repo_override.clone(),
        Command::Distribution(args) => args.repo_override.clone(),
        Command::Artifact(args) => args.repo_override.clone(),
        Command::Container(args) => args.repo_override.clone(),
        Command::Bootstrap(_) => None,
        Command::Release(args) => args.repo_override.clone(),
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::InternalGateway(_) => None,
        Command::InternalRhai(_) => None,
        Command::InternalContainerLeaseReaper(_) => None,
        Command::InternalHostProcessSupervise(_) => None,
        Command::InternalHostProcessStop(_) => None,
        Command::Task(_) => super::task_repo_override(cmd),
        Command::Help(_) => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::runner) enum EmbeddedRepoOverrideMode {
    Force,
    DefaultIfMissing,
}

pub(in crate::runner) fn apply_repo_target_to_embedded_command(
    mut command: Command,
    repo_root: &Path,
    mode: EmbeddedRepoOverrideMode,
) -> Command {
    let repo_root = repo_root.to_path_buf();
    match &mut command {
        Command::Deploy(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Defer(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Exec(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::System(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Workspace(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Service(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Demo(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Docs(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Contracts(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Distribution(args) => {
            assign_repo_override(&mut args.repo_override, &repo_root, mode)
        }
        Command::Artifact(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Container(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Release(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Doctor(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Tasks(args) => assign_repo_override(&mut args.repo_override, &repo_root, mode),
        Command::Version
        | Command::Bundle(_)
        | Command::Changelog(_)
        | Command::Gateway(_)
        | Command::Bootstrap(_)
        | Command::InternalGateway(_)
        | Command::InternalRhai(_)
        | Command::InternalContainerLeaseReaper(_)
        | Command::InternalHostProcessSupervise(_)
        | Command::InternalHostProcessStop(_)
        | Command::Task(_)
        | Command::Help(_) => {}
    }
    command
}

fn assign_repo_override(
    slot: &mut Option<PathBuf>,
    repo_root: &Path,
    mode: EmbeddedRepoOverrideMode,
) {
    match mode {
        EmbeddedRepoOverrideMode::Force => *slot = Some(repo_root.to_path_buf()),
        EmbeddedRepoOverrideMode::DefaultIfMissing => {
            if slot.is_none() {
                *slot = Some(repo_root.to_path_buf());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_repo_target_to_embedded_command, EmbeddedRepoOverrideMode};
    use effigy_cli::{
        Command, ContainerArgs, ContainerSubcommand, DeferArgs, DocsArgs, DocsSubcommand,
        TaskInvocation, WorkspaceArgs,
    };
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn embedded_force_overrides_existing_repo_target() {
        let command = Command::Docs(DocsArgs {
            subcommand: DocsSubcommand::CheckLinks { paths: Vec::new() },
            repo_override: Some(PathBuf::from("/tmp/original")),
            output_json: false,
        });

        let command = apply_repo_target_to_embedded_command(
            command,
            PathBuf::from("/tmp/embedded").as_path(),
            EmbeddedRepoOverrideMode::Force,
        );

        assert!(matches!(
            command,
            Command::Docs(args)
                if args.repo_override == Some(PathBuf::from("/tmp/embedded"))
        ));
    }

    #[test]
    fn embedded_default_preserves_existing_repo_target() {
        let command = Command::Defer(DeferArgs {
            task: TaskInvocation {
                name: "seed".to_owned(),
                args: Vec::new(),
            },
            repo_override: Some(PathBuf::from("/tmp/original")),
            output_json: false,
        });

        let command = apply_repo_target_to_embedded_command(
            command,
            PathBuf::from("/tmp/embedded").as_path(),
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        );

        assert!(matches!(
            command,
            Command::Defer(args)
                if args.repo_override == Some(PathBuf::from("/tmp/original"))
        ));
    }

    #[test]
    fn embedded_default_fills_missing_repo_target_for_workspace_commands() {
        let command = Command::Workspace(WorkspaceArgs {
            workspace: None,
            system: None,
            repo_override: None,
            output_json: false,
        });

        let command = apply_repo_target_to_embedded_command(
            command,
            PathBuf::from("/tmp/embedded").as_path(),
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        );

        assert!(matches!(
            command,
            Command::Workspace(args)
                if args.repo_override == Some(PathBuf::from("/tmp/embedded"))
        ));
    }

    #[test]
    fn embedded_default_fills_missing_repo_target_for_container_commands() {
        let command = Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Status {
                name: None,
                global: true,
            },
            repo_override: None,
            output_json: false,
        });

        let command = apply_repo_target_to_embedded_command(
            command,
            PathBuf::from("/tmp/embedded").as_path(),
            EmbeddedRepoOverrideMode::DefaultIfMissing,
        );

        assert!(matches!(
            command,
            Command::Container(args)
                if args.repo_override == Some(PathBuf::from("/tmp/embedded"))
        ));
    }

    #[test]
    fn embedded_repo_target_assignment_has_one_owner() {
        let runner_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner");
        let sentinels = [
            "Command::Deploy(args) => assign_repo_override",
            "Command::Container(args) => assign_repo_override",
            "Command::Tasks(args) => assign_repo_override",
        ];
        let mut hits = Vec::new();

        fn walk(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).expect("read runner dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }

        let mut files = Vec::new();
        walk(&runner_root, &mut files);
        files.sort();

        for path in files {
            let text = fs::read_to_string(&path).expect("read runner source");
            if sentinels.iter().all(|sentinel| text.contains(sentinel)) {
                hits.push(
                    path.strip_prefix(&runner_root)
                        .expect("runner-relative path")
                        .to_path_buf(),
                );
            }
        }

        assert_eq!(
            hits,
            vec![PathBuf::from("command_context/repo_override.rs")]
        );
    }
}
