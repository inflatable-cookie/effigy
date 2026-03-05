use std::path::{Path, PathBuf};

use serde_json::json;

use crate::TaskInvocation;

use super::super::super::RunnerError;
use super::super::arg_parser::{BuiltinArgParser, ParseLoopAction};
use super::super::response::render_optional_text_or_schema_json_lazy;
use super::super::TaskRuntimeArgs;
use super::surface::COMPLETION_CANDIDATES_SUBCOMMAND;

mod cache;

use cache::{
    completion_candidates_cache_ttl_ms, completion_candidates_cache_ttl_source,
    load_completion_candidates_with_cache, CompletionCandidatesCacheState,
};

struct CompletionCandidatesResult {
    candidates: Vec<String>,
    cache_hit: bool,
    cache_state: &'static str,
    manifest_count: usize,
    cache_age_ms: Option<u128>,
    cache_ttl_ms: Option<u64>,
    effective_cache_ttl_ms: u64,
    cache_ttl_source: &'static str,
}

struct CompletionCandidatesRequest {
    output_json: bool,
    repo_override: Option<PathBuf>,
    prefix: Option<String>,
}

pub(super) fn run_completion_candidates(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let request = parse_completion_candidates_request(task, &runtime_args.passthrough)?;

    let repo_root = request
        .repo_override
        .unwrap_or_else(|| target_root.to_path_buf());
    let completion_candidates =
        collect_completion_candidates(&repo_root, request.prefix.as_deref())?;
    render_optional_text_or_schema_json_lazy(
        request.output_json,
        "effigy.completion.candidates.v1",
        || completion_candidates.candidates.join("\n"),
        || {
            json!({
                "repo": repo_root.display().to_string(),
                "prefix": request.prefix.as_deref(),
                "candidates": &completion_candidates.candidates,
                "cache_hit": completion_candidates.cache_hit,
                "cache_state": completion_candidates.cache_state,
                "manifest_count": completion_candidates.manifest_count,
                "cache_age_ms": completion_candidates.cache_age_ms,
                "cache_ttl_ms": completion_candidates.cache_ttl_ms,
                "effective_cache_ttl_ms": completion_candidates.effective_cache_ttl_ms,
                "cache_ttl_source": completion_candidates.cache_ttl_source,
            })
        },
    )
}

fn parse_completion_candidates_request(
    task: &TaskInvocation,
    args: &[String],
) -> Result<CompletionCandidatesRequest, RunnerError> {
    let mut parser = BuiltinArgParser::new(args);
    let mut output_json = false;
    let mut repo_override: Option<PathBuf> = None;
    let mut prefix: Option<String> = None;
    parser.parse_loop_require_no_unknown_with_prefix(
        &task.name,
        COMPLETION_CANDIDATES_SUBCOMMAND,
        |parser, arg| {
            if arg == COMPLETION_CANDIDATES_SUBCOMMAND
                || parser.consume_json_flag(arg, &mut output_json)
            {
                return Ok(ParseLoopAction::Handled);
            }
            match arg {
                "--repo" => {
                    let value =
                        parser.context_string_flag_value("completion candidates", "--repo")?;
                    repo_override = Some(PathBuf::from(value));
                    Ok(ParseLoopAction::Handled)
                }
                "--prefix" => {
                    let value =
                        parser.context_string_flag_value("completion candidates", "--prefix")?;
                    prefix = Some(value);
                    Ok(ParseLoopAction::Handled)
                }
                _ => Ok(ParseLoopAction::Unknown),
            }
        },
    )?;

    Ok(CompletionCandidatesRequest {
        output_json,
        repo_override,
        prefix,
    })
}

fn collect_completion_candidates(
    repo_root: &Path,
    prefix: Option<&str>,
) -> Result<CompletionCandidatesResult, RunnerError> {
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
