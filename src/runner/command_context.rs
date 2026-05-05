#[path = "command_context/cwd.rs"]
mod cwd;
#[path = "command_context/repo_override.rs"]
mod repo_override;
#[path = "command_context/root.rs"]
mod root;
#[path = "command_context/runtime.rs"]
mod runtime;
#[path = "command_context/tasks.rs"]
mod tasks;

use effigy_cli::Command;

use super::util::parse_task_runtime_args;

pub(in crate::runner) use cwd::{canonicalize_or_original, current_working_dir};
pub(super) use repo_override::{
    apply_repo_target_to_embedded_command, command_repo_override, EmbeddedRepoOverrideMode,
};
pub(super) use root::{resolve_command_root, resolve_repo_root};
pub(super) use runtime::{active_runtime_context, with_runtime_context};
pub(super) use tasks::task_selection_precedence_notes;

pub(in crate::runner) fn resolve_active_repo_root(
    repo_override: Option<std::path::PathBuf>,
) -> Result<effigy_core::resolver::ResolvedTarget, super::error::RunnerError> {
    let cwd = current_working_dir()?;
    resolve_repo_root(cwd, repo_override)
}

fn task_repo_override(cmd: &Command) -> Option<std::path::PathBuf> {
    parse_task_runtime_args(match cmd {
        Command::Task(task) => &task.args,
        _ => return None,
    })
    .ok()
    .and_then(|parsed| parsed.repo_override)
}

pub fn command_repo_override_for_context(cmd: &Command) -> Option<std::path::PathBuf> {
    command_repo_override(cmd)
}

#[cfg(test)]
mod tests {
    use super::{current_working_dir, resolve_repo_root, with_runtime_context};
    use effigy_context::{CapturedEnv, EffigyRuntimeContext};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_repo(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-command-context-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("mkdir temp repo");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctx\"\n").expect("write manifest");
        root
    }

    #[test]
    fn cwd_helper_uses_active_runtime_context() {
        let root = temp_repo("cwd");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("mkdir nested");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(nested.clone()))
            .captured_env(CapturedEnv::default())
            .capture()
            .expect("capture context");

        let cwd = with_runtime_context(&context, current_working_dir).expect("cwd");

        assert_eq!(cwd, nested);
    }

    #[test]
    fn root_helper_reuses_active_runtime_context_target() {
        let root = temp_repo("root");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("mkdir nested");
        let context = EffigyRuntimeContext::builder()
            .cwd_override(Some(nested.clone()))
            .captured_env(CapturedEnv::default())
            .capture()
            .expect("capture context");

        let resolved =
            with_runtime_context(&context, || resolve_repo_root(nested, None)).expect("root");

        assert_eq!(resolved, context.resolved_target().clone());
    }
}
