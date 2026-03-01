use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::json;

use crate::TaskInvocation;

use super::super::super::catalog::discover_catalogs;
use super::super::super::RunnerError;
use super::super::TaskRuntimeArgs;
use super::help::render_completion_candidates_help;
use super::scripts::command_names;

const CANDIDATE_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct CompletionCandidatesSnapshot {
    created_at: Instant,
    manifest_stamps: Vec<ManifestStamp>,
    candidates: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct ManifestStamp {
    path: PathBuf,
    modified_epoch_ns: Option<u128>,
}

struct CompletionCandidatesResult {
    candidates: Vec<String>,
    cache_hit: bool,
    manifest_count: usize,
}

static COMPLETION_CANDIDATES_CACHE: OnceLock<
    Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>,
> = OnceLock::new();

pub(super) fn run_completion_candidates(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut repo_override: Option<PathBuf> = None;
    let mut prefix: Option<String> = None;
    let mut i = 0usize;

    while i < runtime_args.passthrough.len() {
        match runtime_args.passthrough[i].as_str() {
            "candidates" => {
                i += 1;
            }
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--repo" => {
                let Some(value) = runtime_args.passthrough.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "completion candidates argument --repo requires a value".to_owned(),
                    ));
                };
                repo_override = Some(PathBuf::from(value));
                i += 2;
            }
            "--prefix" => {
                let Some(value) = runtime_args.passthrough.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "completion candidates argument --prefix requires a value".to_owned(),
                    ));
                };
                prefix = Some(value.clone());
                i += 2;
            }
            other => {
                return Err(RunnerError::TaskInvocation(format!(
                    "unknown argument(s) for built-in `{}`: candidates {other}",
                    task.name
                )));
            }
        }
    }

    if help {
        let text = render_completion_candidates_help();
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "completion-candidates",
                "text": text,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(text));
    }

    let repo_root = repo_override.unwrap_or_else(|| target_root.to_path_buf());
    let completion_candidates = collect_completion_candidates(&repo_root, prefix.as_deref())?;
    if output_json {
        let payload = json!({
            "schema": "effigy.completion.candidates.v1",
            "schema_version": 1,
            "ok": true,
            "repo": repo_root.display().to_string(),
            "prefix": prefix,
            "candidates": completion_candidates.candidates,
            "cache_hit": completion_candidates.cache_hit,
            "manifest_count": completion_candidates.manifest_count,
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    Ok(Some(completion_candidates.candidates.join("\n")))
}

fn collect_completion_candidates(
    repo_root: &Path,
    prefix: Option<&str>,
) -> Result<CompletionCandidatesResult, RunnerError> {
    let (base_candidates, cache_hit, manifest_count) =
        load_completion_candidates_with_cache(repo_root)?;

    let candidates = base_candidates
        .into_iter()
        .filter(|candidate| {
            prefix
                .map(|value| candidate.starts_with(value))
                .unwrap_or(true)
        })
        .collect::<Vec<String>>();

    Ok(CompletionCandidatesResult {
        candidates,
        cache_hit,
        manifest_count,
    })
}

fn load_completion_candidates_with_cache(
    repo_root: &Path,
) -> Result<(Vec<String>, bool, usize), RunnerError> {
    let now = Instant::now();
    let cache = COMPLETION_CANDIDATES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some((candidates, manifest_count)) =
        read_cached_completion_candidates(repo_root, now, cache)?
    {
        return Ok((candidates, true, manifest_count));
    }

    let (candidates, manifest_stamps) = discover_completion_candidates(repo_root)?;
    let manifest_count = manifest_stamps.len();
    let snapshot = CompletionCandidatesSnapshot {
        created_at: now,
        manifest_stamps,
        candidates: candidates.clone(),
    };

    let mut map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    map.insert(repo_root.to_path_buf(), snapshot);
    Ok((candidates, false, manifest_count))
}

fn read_cached_completion_candidates(
    repo_root: &Path,
    now: Instant,
    cache: &Mutex<HashMap<PathBuf, CompletionCandidatesSnapshot>>,
) -> Result<Option<(Vec<String>, usize)>, RunnerError> {
    let map = cache
        .lock()
        .map_err(|_| RunnerError::Ui("completion candidate cache lock poisoned".to_owned()))?;
    let Some(snapshot) = map.get(repo_root) else {
        return Ok(None);
    };
    if now.duration_since(snapshot.created_at) > CANDIDATE_CACHE_TTL {
        return Ok(None);
    }
    if !manifest_stamps_unchanged(&snapshot.manifest_stamps) {
        return Ok(None);
    }
    Ok(Some((
        snapshot.candidates.clone(),
        snapshot.manifest_stamps.len(),
    )))
}

fn discover_completion_candidates(
    repo_root: &Path,
) -> Result<(Vec<String>, Vec<ManifestStamp>), RunnerError> {
    let mut candidates: BTreeSet<String> = command_names().into_iter().map(str::to_owned).collect();
    let mut manifest_stamps: Vec<ManifestStamp> = Vec::new();
    match discover_catalogs(repo_root) {
        Ok(catalogs) => {
            for catalog in catalogs {
                manifest_stamps.push(read_manifest_stamp(&catalog.manifest_path));
                for task_name in catalog.manifest.tasks.keys() {
                    candidates.insert(task_name.clone());
                    candidates.insert(format!("{}/{}", catalog.alias, task_name));
                }
            }
        }
        Err(RunnerError::TaskCatalogsMissing { .. }) => {}
        Err(error) => return Err(error),
    }
    manifest_stamps.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((
        candidates.into_iter().collect::<Vec<String>>(),
        manifest_stamps,
    ))
}

fn manifest_stamps_unchanged(expected: &[ManifestStamp]) -> bool {
    expected
        .iter()
        .all(|stamp| read_manifest_stamp(&stamp.path) == *stamp)
}

fn read_manifest_stamp(path: &Path) -> ManifestStamp {
    let modified_epoch_ns = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());

    ManifestStamp {
        path: path.to_path_buf(),
        modified_epoch_ns,
    }
}
