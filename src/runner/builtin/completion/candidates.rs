use std::path::{Path, PathBuf};

use serde_json::json;

use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::TaskRuntimeArgs;
use super::help::render_completion_candidates_help;

mod cache;

use cache::{load_completion_candidates_with_cache, CompletionCandidatesCacheState};

struct CompletionCandidatesResult {
    candidates: Vec<String>,
    cache_hit: bool,
    cache_state: &'static str,
    manifest_count: usize,
    cache_age_ms: Option<u128>,
}

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
            "cache_state": completion_candidates.cache_state,
            "manifest_count": completion_candidates.manifest_count,
            "cache_age_ms": completion_candidates.cache_age_ms,
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
    let (base_candidates, cache_state, manifest_count, cache_age_ms) =
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
        cache_hit: cache_state == CompletionCandidatesCacheState::Hit,
        cache_state: cache_state.as_str(),
        manifest_count,
        cache_age_ms,
    })
}
