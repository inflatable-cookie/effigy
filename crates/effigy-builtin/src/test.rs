use effigy_cli::TaskInvocation;
use std::path::Path;

use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

mod execution;
mod planning;
mod render;
mod suite_selection;

pub(super) fn try_run_builtin_test(
    ports: &dyn BuiltinRuntimePorts,
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    let (flags, passthrough) = planning::extract_builtin_test_flags(&runtime_args.passthrough);
    let mut target_set = planning::resolve_builtin_test_targets(selector, resolved_root, catalogs)?;
    let runnable = planning::collect_builtin_test_runnable_targets(&target_set.targets);
    if runnable.is_empty() {
        return Ok(None);
    }
    let suite_selection = match suite_selection::select_builtin_test_suite(runnable, passthrough) {
        Ok(selection) => selection,
        Err(selection_error) => {
            return render::render_suite_selection_failure(
                task,
                resolved_root,
                flags,
                selection_error,
            );
        }
    };
    target_set.targets.retain(|target| {
        suite_selection
            .runnable
            .iter()
            .any(|entry| entry.root == target.root)
    });

    if flags.plan_mode {
        return render::render_builtin_test_plan(
            task,
            resolved_root,
            &target_set,
            suite_selection.requested_suite.as_deref(),
            &suite_selection.passthrough,
            suite_selection.runnable.len(),
            flags,
        )
        .map(Some);
    }

    let runnable = planning::apply_passthrough_to_runnable(
        suite_selection.runnable,
        &suite_selection.passthrough,
    );
    check_js_hydration(&runnable, resolved_root)?;
    reject_unusable_suite_runtimes(&runnable)?;
    let max_parallel = planning::builtin_test_max_parallel(catalogs, resolved_root);
    let should_tui = execution::should_run_builtin_test_tui(flags.tui, runnable.len());
    let results = if should_tui {
        execution::run_builtin_test_targets_tui(ports, runnable)?
    } else {
        execution::run_builtin_test_targets_parallel(
            ports,
            runnable,
            max_parallel,
            flags.output_json,
        )?
    };
    render::finalize_builtin_test_outcome(
        &results,
        &target_set.targets,
        suite_selection.requested_suite.as_deref(),
        &suite_selection.passthrough,
        flags.verbose_results,
        flags.output_json,
    )
}

/// Fail before executing when a selected suite's owning catalog declares a
/// runtime target builtin test suites cannot use. Without this the suite
/// would silently run on the host despite the declared container target.
fn reject_unusable_suite_runtimes(
    runnable: &[planning::BuiltinTestRunnable],
) -> Result<(), BuiltinError> {
    for suite in runnable {
        let crate::test::planning::BuiltinTargetRuntime::Unusable { reason } = &suite.runtime
        else {
            continue;
        };
        return Err(BuiltinError::task_invocation(reason.clone()));
    }
    Ok(())
}

fn check_js_hydration(
    runnable: &[planning::BuiltinTestRunnable],
    resolved_root: &Path,
) -> Result<(), BuiltinError> {
    let mut missing = Vec::new();
    for suite in runnable {
        if !suite.command.contains("bun ")
            || !suite.root.join("package.json").is_file()
            || suite
                .setup_command
                .as_deref()
                .is_some_and(|setup| setup.contains("bun install"))
        {
            continue;
        }
        let Some(install_root) = suite
            .root
            .ancestors()
            .take_while(|path| path.starts_with(resolved_root))
            .find(|path| path.join("bun.lock").is_file() || path.join("bun.lockb").is_file())
        else {
            continue;
        };
        if install_root.join("node_modules").is_dir() {
            continue;
        }
        let route = install_root
            .strip_prefix(resolved_root)
            .ok()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_owned());
        if !missing.contains(&route) {
            missing.push(route);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort();
    missing.truncate(3);
    Err(BuiltinError::task_invocation(format!(
        "JavaScript dependencies are missing for {}; prepare this checkout with `effigy bootstrap deps sync {}` (frozen Bun install), then rerun the test",
        missing.join(", "),
        missing[0]
    )))
}

pub(crate) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    planning::builtin_test_max_parallel(catalogs, resolved_root)
}

#[derive(Debug)]
struct BuiltinTestExecResult {
    name: String,
    runner: String,
    root: std::path::PathBuf,
    command: String,
    success: bool,
    code: Option<i32>,
}

#[cfg(test)]
mod hydration_tests {
    use super::*;

    #[test]
    fn fresh_bun_worktree_reports_frozen_preparation_route() {
        let root = std::env::temp_dir().join(format!(
            "effigy-bun-hydration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(root.join("bun.lock"), "").unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        let suite = planning::BuiltinTestRunnable {
            name: "app".into(),
            runner: "bun".into(),
            root: app,
            command: "bun test".into(),
            cargo_env: Default::default(),
            cargo_env_match: Default::default(),
            env: Default::default(),
            setup_command: None,
            teardown_command: None,
            teardown_policy: Default::default(),
            is_default: true,
            runtime: planning::BuiltinTargetRuntime::Host,
            nested_invocation: false,
        };
        let error = check_js_hydration(std::slice::from_ref(&suite), &root).unwrap_err();
        assert!(error.to_string().contains("effigy bootstrap deps sync ."));
        assert!(error.to_string().contains("frozen Bun install"));
        std::fs::create_dir(root.join("node_modules")).unwrap();
        check_js_hydration(&[suite], &root).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
