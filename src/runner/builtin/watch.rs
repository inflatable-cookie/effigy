use std::path::Path;

use crate::TaskInvocation;

use super::super::locking::{acquire_scopes, LockScope};
use super::super::{run_manifest_task_with_cwd, RunnerError, TaskRuntimeArgs};
use super::reject_verbose_root_for_builtin;

mod options;
mod output;
mod scan;

use options::{parse_watch_request, WatchOwner};
use output::{render_watch_help_payload, render_watch_result_json};
use scan::{build_matcher, collect_snapshot, wait_for_changes};

pub(super) fn run_builtin_watch(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;

    let request = parse_watch_request(task, &runtime_args.passthrough)?;
    if request.help {
        return render_watch_help_payload(request.output_json);
    }
    if request.output_json && request.max_runs.is_none() {
        return Err(RunnerError::TaskInvocation(
            "`--json` requires a bounded watch run (`--once` or `--max-runs <N>`).".to_owned(),
        ));
    }

    let owner = request.owner.ok_or_else(|| {
        RunnerError::TaskInvocation(
            "`--owner <effigy|external>` is required to avoid nested watcher conflicts.".to_owned(),
        )
    })?;
    if owner == WatchOwner::External {
        return Err(RunnerError::TaskInvocation(
            "watch owner `external` means task-managed watching is expected. Run the task directly (without `effigy watch`) to avoid nested watcher loops.".to_owned(),
        ));
    }

    let target = request.target.ok_or_else(|| {
        RunnerError::TaskInvocation(
            "watch requires a target task selector (for example `effigy watch --owner effigy test`)."
                .to_owned(),
        )
    })?;
    if target.name == "watch" {
        return Err(RunnerError::TaskInvocation(
            "watch target cannot be `watch` (nested watch loops are blocked by owner policy)."
                .to_owned(),
        ));
    }
    let watch_scope = LockScope::Task(format!("watch:{}", target.name));
    let _watch_lock = acquire_scopes(target_root, &[watch_scope])?;

    let matcher = build_matcher(&request.include, &request.exclude)?;
    let max_runs = request.max_runs;
    let mut runs = 0usize;
    run_watch_target(&target, target_root, request.output_json)?;
    runs += 1;
    if Some(runs) == max_runs {
        return render_watch_result_json(request.output_json, runs);
    }

    let mut snapshot = collect_snapshot(target_root, &matcher)?;
    loop {
        let _changes = wait_for_changes(target_root, &matcher, &mut snapshot, request.debounce_ms)?;
        run_watch_target(&target, target_root, request.output_json)?;
        runs += 1;
        if Some(runs) == max_runs {
            return render_watch_result_json(request.output_json, runs);
        }
    }
}

fn run_watch_target(
    target: &TaskInvocation,
    target_root: &Path,
    output_json: bool,
) -> Result<(), RunnerError> {
    let mut invocation = target.clone();
    if output_json {
        invocation.args.push("--json".to_owned());
    }
    let _ = run_manifest_task_with_cwd(&invocation, target_root.to_path_buf())?;
    Ok(())
}
