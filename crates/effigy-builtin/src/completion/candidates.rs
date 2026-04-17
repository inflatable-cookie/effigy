use std::path::Path;

use effigy_cli::TaskInvocation;

use super::output::render_completion_candidates_response;
use super::request::parse_completion_candidates_request;
use crate::BuiltinError;
use effigy_tasks::TaskRuntimeArgs;

mod cache;

use cache::{
    completion_candidates_cache_ttl_ms, completion_candidates_cache_ttl_source,
    load_completion_candidates_with_cache, CompletionCandidatesCacheState,
};

pub(super) struct CompletionCandidatesResult {
    pub(super) candidates: Vec<String>,
    pub(super) cache_hit: bool,
    pub(super) cache_state: &'static str,
    pub(super) manifest_count: usize,
    pub(super) cache_age_ms: Option<u128>,
    pub(super) cache_ttl_ms: Option<u64>,
    pub(super) effective_cache_ttl_ms: u64,
    pub(super) cache_ttl_source: &'static str,
}

pub(super) fn run_completion_candidates(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    let request = parse_completion_candidates_request(task, &runtime_args.passthrough)?;

    let repo_root = request
        .repo_override
        .unwrap_or_else(|| target_root.to_path_buf());
    let completion_candidates =
        collect_completion_candidates(&repo_root, request.prefix.as_deref())?;
    render_completion_candidates_response(
        request.output_json,
        &repo_root,
        request.prefix.as_deref(),
        &completion_candidates,
    )
}

fn collect_completion_candidates(
    repo_root: &Path,
    prefix: Option<&str>,
) -> Result<CompletionCandidatesResult, BuiltinError> {
    let (base_candidates, cache_state, manifest_count, cache_age_ms) =
        load_completion_candidates_with_cache(repo_root)?;
    let effective_cache_ttl_ms = completion_candidates_cache_ttl_ms();
    let cache_ttl_source = completion_candidates_cache_ttl_source();

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
        cache_ttl_ms: (cache_state == CompletionCandidatesCacheState::Hit)
            .then_some(effective_cache_ttl_ms),
        effective_cache_ttl_ms,
        cache_ttl_source,
    })
}
