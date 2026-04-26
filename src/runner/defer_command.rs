use effigy_cli::{DeferArgs, TaskInvocation};

use crate::runner::command_context::current_working_dir;
use crate::runner::deferral::{run_deferred_request, select_deferral};
use crate::runner::error::RunnerError;
use crate::runner::execute::api::build_execution_preflight;

pub(in crate::runner) fn run_defer(args: DeferArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let task = normalize_defer_task(args);
    let preflight = build_execution_preflight(&task, cwd)?;
    let Some(deferral) = select_deferral(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    ) else {
        return Err(RunnerError::task_invocation(format!(
            "no deferral is configured for `{}` from {}",
            task.name,
            preflight.invocation_cwd.display()
        )));
    };
    let cause = RunnerError::TaskNotFoundAny {
        name: task.name.clone(),
        catalogs: preflight
            .catalogs
            .iter()
            .map(|catalog| format!("{} ({})", catalog.alias, catalog.manifest_path.display()))
            .collect(),
    };
    run_deferred_request(&task, &preflight.runtime_args_raw, &deferral, &cause)
}

fn normalize_defer_task(args: DeferArgs) -> TaskInvocation {
    let mut task = args.task;
    if args.output_json && !task.args.iter().any(|arg| arg == "--json") {
        task.args.push("--json".to_owned());
    }
    if let Some(repo_override) = args.repo_override {
        task.args.push("--repo".to_owned());
        task.args.push(repo_override.display().to_string());
    }
    task
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn normalize_defer_task_applies_top_level_flags() {
        let task = normalize_defer_task(DeferArgs {
            task: TaskInvocation {
                name: "prep".to_owned(),
                args: vec!["--watch".to_owned()],
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/repo")),
            output_json: true,
        });

        assert_eq!(task.name, "prep");
        assert_eq!(
            task.args,
            vec![
                "--watch".to_owned(),
                "--json".to_owned(),
                "--repo".to_owned(),
                "/tmp/repo".to_owned(),
            ]
        );
    }

    #[test]
    fn run_defer_executes_configured_host_deferral() {
        let _guard = test_lock().lock().expect("lock test");
        let root = std::env::temp_dir().join(format!(
            "effigy-defer-command-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("mkdir root");
        std::fs::write(root.join("package.json"), "{}\n").expect("write package marker");
        std::fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\n",
        )
        .expect("write manifest");

        let original = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(&root).expect("set cwd");
        let out = run_defer(DeferArgs {
            task: TaskInvocation {
                name: "prep".to_owned(),
                args: vec![],
            },
            repo_override: None,
            output_json: false,
        })
        .expect("run defer");
        std::env::set_current_dir(original).expect("restore cwd");

        assert_eq!(out, "");
    }
}
