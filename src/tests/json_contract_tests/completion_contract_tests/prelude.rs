pub(super) use super::super::prelude::*;

pub(super) fn with_completion_cache_default() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)])
}

pub(super) fn run_completion_task(root: PathBuf, args: &[&str]) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
}
