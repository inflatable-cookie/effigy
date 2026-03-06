use std::path::Path;

use crate::TaskInvocation;

use super::super::super::execute::run_manifest_task_with_cwd;
use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::output::render_watch_result_json;
use super::request::WatchRequest;
use super::scan::{build_matcher, collect_snapshot, wait_for_changes};
use crate::runner::error::RunnerError;

pub(super) fn run_watch_runtime(
    request: WatchRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let target = request.validated_target()?;
    let watch_scope = LockScope::Task(format!("watch:{}", target.name));
    let _watch_lock = acquire_scopes(target_root, &[watch_scope])?;

    let matcher = build_matcher(&request.include, &request.exclude)?;
    let max_runs = request.max_runs;
    let mut runs = 0usize;
    run_watch_target(target, target_root, request.output_json)?;
    runs += 1;
    if Some(runs) == max_runs {
        return render_watch_result_json(request.output_json, runs);
    }

    let mut snapshot = collect_snapshot(target_root, &matcher)?;
    loop {
        let _changes = wait_for_changes(target_root, &matcher, &mut snapshot, request.debounce_ms)?;
        run_watch_target(target, target_root, request.output_json)?;
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
