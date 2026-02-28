use std::collections::{BTreeSet, HashMap};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde_json::json;
use walkdir::WalkDir;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{OutputMode, PlainRenderer};
use crate::{render_help, HelpTopic, TaskInvocation};

use super::super::locking::{acquire_scopes, LockScope};
use super::super::{run_manifest_task_with_cwd, RunnerError, TaskRuntimeArgs};

const DEFAULT_DEBOUNCE_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchOwner {
    Effigy,
    External,
}

#[derive(Debug)]
struct WatchRequest {
    output_json: bool,
    help: bool,
    owner: Option<WatchOwner>,
    debounce_ms: u64,
    include: Vec<String>,
    exclude: Vec<String>,
    max_runs: Option<usize>,
    target: Option<TaskInvocation>,
}

#[derive(Debug)]
struct WatchMatcher {
    include: Option<GlobSet>,
    exclude: GlobSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    size: u64,
}

pub(super) fn run_builtin_watch(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(
            "`--verbose-root` is not supported for built-in `watch`".to_owned(),
        ));
    }

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
    run_manifest_task_with_cwd(&target, target_root.to_path_buf())?;
    runs += 1;
    if Some(runs) == max_runs {
        return render_watch_result_json(request.output_json, runs);
    }

    let mut snapshot = collect_snapshot(target_root, &matcher)?;
    loop {
        let _changes = wait_for_changes(target_root, &matcher, &mut snapshot, request.debounce_ms)?;
        run_manifest_task_with_cwd(&target, target_root.to_path_buf())?;
        runs += 1;
        if Some(runs) == max_runs {
            return render_watch_result_json(request.output_json, runs);
        }
    }
}

fn parse_watch_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<WatchRequest, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut owner: Option<WatchOwner> = None;
    let mut debounce_ms = DEFAULT_DEBOUNCE_MS;
    let mut include = Vec::<String>::new();
    let mut exclude = Vec::<String>::new();
    let mut max_runs: Option<usize> = None;
    let mut target: Option<TaskInvocation> = None;
    let mut i = 0usize;

    while i < args.len() {
        if target.is_some() {
            break;
        }
        let arg = &args[i];
        match arg.as_str() {
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--owner" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--owner` requires a value (`effigy` or `external`)".to_owned(),
                    ));
                };
                owner = match value.as_str() {
                    "effigy" => Some(WatchOwner::Effigy),
                    "external" => Some(WatchOwner::External),
                    _ => {
                        return Err(RunnerError::TaskInvocation(format!(
                            "invalid `--owner` value `{value}` (expected `effigy` or `external`)"
                        )));
                    }
                };
                i += 2;
            }
            "--debounce-ms" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--debounce-ms` requires a numeric value".to_owned(),
                    ));
                };
                let parsed = value.parse::<u64>().map_err(|_| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--debounce-ms` value `{value}` (expected a positive integer)"
                    ))
                })?;
                if parsed == 0 {
                    return Err(RunnerError::TaskInvocation(
                        "`--debounce-ms` must be greater than zero".to_owned(),
                    ));
                }
                debounce_ms = parsed;
                i += 2;
            }
            "--include" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--include` requires a glob value".to_owned(),
                    ));
                };
                include.push(value.clone());
                i += 2;
            }
            "--exclude" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--exclude` requires a glob value".to_owned(),
                    ));
                };
                exclude.push(value.clone());
                i += 2;
            }
            "--once" => {
                max_runs = Some(1);
                i += 1;
            }
            "--max-runs" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--max-runs` requires a numeric value".to_owned(),
                    ));
                };
                let parsed = value.parse::<usize>().map_err(|_| {
                    RunnerError::TaskInvocation(format!(
                        "invalid `--max-runs` value `{value}` (expected an integer >= 1)"
                    ))
                })?;
                if parsed == 0 {
                    return Err(RunnerError::TaskInvocation(
                        "`--max-runs` must be greater than zero".to_owned(),
                    ));
                }
                max_runs = Some(parsed);
                i += 2;
            }
            "--" => {
                return Err(RunnerError::TaskInvocation(
                    "watch requires `<task>` before passthrough arguments (`--`)".to_owned(),
                ));
            }
            _ if arg.starts_with('-') => {
                return Err(RunnerError::TaskInvocation(format!(
                    "unknown argument(s) for built-in `{}`: {}",
                    task.name, arg
                )));
            }
            _ => {
                target = Some(TaskInvocation {
                    name: arg.clone(),
                    args: args.iter().skip(i + 1).cloned().collect(),
                });
                i = args.len();
            }
        }
    }

    Ok(WatchRequest {
        output_json,
        help,
        owner,
        debounce_ms,
        include,
        exclude,
        max_runs,
        target,
    })
}

fn render_watch_help_payload(output_json: bool) -> Result<Option<String>, RunnerError> {
    let color_enabled = if output_json {
        false
    } else {
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
    };
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    render_help(&mut renderer, HelpTopic::Watch)?;
    let rendered = String::from_utf8(renderer.into_inner())
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))?;
    if output_json {
        let payload = json!({
            "schema": "effigy.help.v1",
            "schema_version": 1,
            "ok": true,
            "topic": "watch",
            "text": rendered,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }
    Ok(Some(rendered))
}

fn render_watch_result_json(output_json: bool, runs: usize) -> Result<Option<String>, RunnerError> {
    if !output_json {
        return Ok(Some(format!("watch complete after {runs} run(s).")));
    }
    let payload = json!({
        "schema": "effigy.watch.v1",
        "schema_version": 1,
        "ok": true,
        "runs": runs,
    });
    serde_json::to_string_pretty(&payload)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

fn build_matcher(include: &[String], exclude: &[String]) -> Result<WatchMatcher, RunnerError> {
    let include_set = if include.is_empty() {
        None
    } else {
        Some(build_glob_set(include, "include")?)
    };
    let mut excludes = vec![
        ".git/**".to_owned(),
        "node_modules/**".to_owned(),
        "target/**".to_owned(),
    ];
    excludes.extend(exclude.iter().cloned());
    let exclude_set = build_glob_set(&excludes, "exclude")?;
    Ok(WatchMatcher {
        include: include_set,
        exclude: exclude_set,
    })
}

fn build_glob_set(patterns: &[String], label: &str) -> Result<GlobSet, RunnerError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            RunnerError::TaskInvocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to compile `{label}` glob set: {error}"))
    })
}

fn wait_for_changes(
    root: &Path,
    matcher: &WatchMatcher,
    snapshot: &mut HashMap<PathBuf, FileStamp>,
    debounce_ms: u64,
) -> Result<Vec<String>, RunnerError> {
    let debounce = Duration::from_millis(debounce_ms);
    let poll_ms = (debounce_ms / 4).clamp(50, 800);
    let poll = Duration::from_millis(poll_ms);
    let mut changed = BTreeSet::<String>::new();
    loop {
        std::thread::sleep(poll);
        let next = collect_snapshot(root, matcher)?;
        for rel in snapshot_diff(snapshot, &next) {
            changed.insert(rel);
        }
        *snapshot = next;
        if changed.is_empty() {
            continue;
        }
        let mut quiet_deadline = std::time::Instant::now() + debounce;
        loop {
            std::thread::sleep(poll);
            let next = collect_snapshot(root, matcher)?;
            let delta = snapshot_diff(snapshot, &next);
            *snapshot = next;
            if delta.is_empty() {
                if std::time::Instant::now() >= quiet_deadline {
                    return Ok(changed.into_iter().collect());
                }
            } else {
                for rel in delta {
                    changed.insert(rel);
                }
                quiet_deadline = std::time::Instant::now() + debounce;
            }
        }
    }
}

fn collect_snapshot(
    root: &Path,
    matcher: &WatchMatcher,
) -> Result<HashMap<PathBuf, FileStamp>, RunnerError> {
    let mut snapshot = HashMap::<PathBuf, FileStamp>::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "watch scan failed under {}: {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(path);
        if rel.as_os_str().is_empty() {
            continue;
        }
        let rel_for_match = normalize_for_match(rel);
        if !matcher.matches(&rel_for_match) {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "watch metadata read failed for {}: {error}",
                path.display()
            ))
        })?;
        snapshot.insert(
            rel.to_path_buf(),
            FileStamp {
                modified: metadata.modified().ok(),
                size: metadata.len(),
            },
        );
    }
    Ok(snapshot)
}

fn snapshot_diff(
    old: &HashMap<PathBuf, FileStamp>,
    new: &HashMap<PathBuf, FileStamp>,
) -> Vec<String> {
    let mut changed = BTreeSet::<String>::new();
    for (path, stamp) in new {
        if old.get(path) != Some(stamp) {
            changed.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    for path in old.keys() {
        if !new.contains_key(path) {
            changed.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    changed.into_iter().collect()
}

impl WatchMatcher {
    fn matches(&self, rel_path: &str) -> bool {
        let rel = rel_path.trim_start_matches("./");
        if self.exclude.is_match(rel) {
            return false;
        }
        match self.include.as_ref() {
            Some(include) => include.is_match(rel),
            None => true,
        }
    }
}

fn normalize_for_match(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
