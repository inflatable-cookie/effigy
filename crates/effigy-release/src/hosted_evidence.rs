use std::path::Path;
use std::process::Command as ProcessCommand;
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

use crate::git::{git_head_sha, git_remote_url};
use crate::ReleaseError;

pub const DEFAULT_HOSTED_EVIDENCE_EVENT: &str = "workflow_dispatch";
pub const DEFAULT_HOSTED_EVIDENCE_BRANCH: &str = "main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceSpec {
    pub workflow: String,
    pub event: String,
    pub branch: String,
}

/// Which dispatched run a hosted wait may accept (contract 054 §3).
///
/// A release never takes "the newest run for this commit". A sibling dispatch —
/// another release, an operator's re-dispatch, a re-run — can be newer and
/// greener, and accepting it would certify a workflow this release did not
/// start. The wait binds the one run its own dispatch produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostedEvidenceBinding {
    /// The dispatch window, as an RFC 3339 instant: a run created before it
    /// belongs to an earlier dispatch and is never this release's evidence.
    pub created_after: Option<String>,
    /// The run this release already bound. A resume re-checks this run and
    /// never binds a sibling.
    pub run_id: Option<u64>,
}

impl HostedEvidenceBinding {
    /// No binding: the check accepts only a commitment that is unambiguous on
    /// its own, and refuses when several runs match.
    pub fn unbound() -> Self {
        Self::default()
    }

    /// A dispatch window that starts at `created_after` and has bound no run
    /// yet.
    pub fn opened_at(created_after: impl Into<String>) -> Self {
        Self {
            created_after: Some(created_after.into()),
            run_id: None,
        }
    }

    pub fn runs_after(&self, created_after: impl Into<String>) -> Self {
        Self {
            created_after: Some(created_after.into()),
            run_id: self.run_id,
        }
    }

    pub fn with_run_id(&self, run_id: u64) -> Self {
        Self {
            created_after: self.created_after.clone(),
            run_id: Some(run_id),
        }
    }
}

/// How long one check waits for a just-dispatched run to appear in `gh run
/// list`. A dispatch and its API listing are not atomic: a run created moments
/// ago can be missing from the first answer, so the check retries a bounded
/// number of times before it says there is no evidence yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedEvidencePolling {
    pub attempts: u32,
    pub interval: Duration,
}

impl HostedEvidencePolling {
    /// No wait at all: tests and one-shot checks answer on the first listing.
    pub const NONE: Self = Self {
        attempts: 0,
        interval: Duration::ZERO,
    };
    /// The production bound for a dispatch window: about ten seconds.
    pub const DISPATCH: Self = Self {
        attempts: 5,
        interval: Duration::from_secs(2),
    };
}

impl Default for HostedEvidencePolling {
    fn default() -> Self {
        Self::DISPATCH
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceQuery {
    pub repository: String,
    pub head_sha: String,
    pub workflow: String,
    pub event: String,
    pub branch: String,
    /// The dispatch binding this check must honour (054 §3).
    pub binding: HostedEvidenceBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedRun {
    pub repository: String,
    pub head_sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub url: String,
    pub database_id: Option<u64>,
    pub event: Option<String>,
    pub head_branch: Option<String>,
    /// The instant GitHub created the run, for the dispatch window.
    pub created_at: Option<String>,
}

/// The one dispatched run a check bound: what the record journals so a resume
/// re-checks that run instead of rebinding to a newer sibling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRun {
    pub run_id: Option<u64>,
    pub url: String,
}

/// Why a check found no run it may accept, in the operator's terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEvidence {
    pub sha: String,
    pub repository: String,
    pub workflow: String,
    pub event: String,
    pub branch: String,
    /// The dispatch window the check required, when the release recorded one.
    /// `None` for a check that binds only by uniqueness.
    pub created_after: Option<String>,
    /// Same-commit runs the check would not accept because they were created
    /// before the window: earlier dispatches, not this one.
    pub ignored: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedEvidenceError {
    /// Boxed: the operator-facing detail carries the ignored candidates too.
    Missing(Box<MissingEvidence>),
    Pending {
        sha: String,
        status: String,
        url: String,
        /// The bound run's id, so the wait re-checks this run next time.
        run_id: Option<u64>,
    },
    Failed {
        sha: String,
        conclusion: String,
        url: String,
        run_id: Option<u64>,
    },
    Ambiguous {
        detail: String,
    },
    WrongRepository {
        expected: String,
        actual: String,
        url: String,
    },
    WrongSha {
        expected: String,
        actual: String,
        url: String,
    },
    LookupFailed {
        detail: String,
    },
}

impl HostedEvidenceError {
    pub fn diagnostic(&self, gate_name: &str) -> String {
        match self {
            Self::Missing(missing) => {
                let MissingEvidence {
                    sha,
                    repository,
                    workflow,
                    event,
                    branch,
                    created_after,
                    ignored,
                } = missing.as_ref();
                let window = match created_after {
                    Some(created_after) => format!(
                        " created after {created_after} (the dispatch this release Run started)"
                    ),
                    None => String::new(),
                };
                let ignored = if ignored.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Runs for that commit from an earlier dispatch are not this Run's evidence and were not accepted: {}.",
                        ignored.join("; ")
                    )
                };
                format!(
                    "hosted evidence for gate `{gate_name}` is missing: no GitHub Actions run for workflow `{workflow}` event `{event}` branch `{branch}` commit `{sha}` in `{repository}`{window}.{ignored} Dispatch that workflow for this exact commit, wait for success, then retry."
                )
            }
            Self::Pending {
                sha,
                status,
                url,
                ..
            } => format!(
                "hosted evidence for gate `{gate_name}` is pending: run {url} status is `{status}` for commit `{sha}`. Wait for completion, then retry."
            ),
            Self::Failed {
                sha,
                conclusion,
                url,
                ..
            } => format!(
                "hosted evidence for gate `{gate_name}` failed: run {url} conclusion is `{conclusion}` for commit `{sha}`."
            ),
            Self::Ambiguous { detail } => {
                format!("hosted evidence for gate `{gate_name}` is ambiguous: {detail}")
            }
            Self::WrongRepository {
                expected,
                actual,
                url,
            } => format!(
                "hosted evidence for gate `{gate_name}` is from repository `{actual}`, not this repository `{expected}` (run {url})."
            ),
            Self::WrongSha {
                expected,
                actual,
                url,
            } => format!(
                "hosted evidence for gate `{gate_name}` is for commit `{actual}`, not the release commit `{expected}` (run {url}). Identical trees at another SHA are not accepted."
            ),
            Self::LookupFailed { detail } => format!(
                "hosted evidence for gate `{gate_name}` could not be verified through authenticated `gh`: {detail}"
            ),
        }
    }
}

pub trait HostedEvidenceLookup {
    fn list_runs(&self, query: &HostedEvidenceQuery) -> Result<Vec<HostedRun>, String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GhCliHostedEvidenceLookup;

impl HostedEvidenceLookup for GhCliHostedEvidenceLookup {
    fn list_runs(&self, query: &HostedEvidenceQuery) -> Result<Vec<HostedRun>, String> {
        let output = ProcessCommand::new("gh")
            .args([
                "run",
                "list",
                "--repo",
                &query.repository,
                "--workflow",
                &query.workflow,
                "--branch",
                &query.branch,
                "--commit",
                &query.head_sha,
                "--event",
                &query.event,
                "--limit",
                "20",
                "--json",
                "databaseId,headSha,status,conclusion,url,event,headBranch,createdAt",
            ])
            .output()
            .map_err(|error| format!("failed to invoke authenticated `gh`: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.trim();
            return Err(if detail.is_empty() {
                "`gh run list` failed".to_owned()
            } else {
                format!("`gh run list` failed: {detail}")
            });
        }
        parse_gh_run_list_json(&output.stdout)
    }
}

#[derive(Debug, Deserialize)]
struct GhRunListItem {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
    #[serde(rename = "headSha")]
    head_sha: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    url: Option<String>,
    event: Option<String>,
    #[serde(rename = "headBranch")]
    head_branch: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
}

pub fn parse_gh_run_list_json(raw: &[u8]) -> Result<Vec<HostedRun>, String> {
    let parsed: Vec<GhRunListItem> = serde_json::from_slice(raw)
        .map_err(|error| format!("`gh run list` JSON was not an array of runs: {error}"))?;
    parsed
        .into_iter()
        .map(|item| {
            let url = item.url.unwrap_or_default();
            let repository = github_repository_from_remote_url(&url).unwrap_or_default();
            Ok(HostedRun {
                repository,
                head_sha: item.head_sha.unwrap_or_default(),
                status: item.status.unwrap_or_default(),
                conclusion: item
                    .conclusion
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
                url,
                database_id: item.database_id,
                event: item.event,
                head_branch: item.head_branch,
                created_at: item
                    .created_at
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
            })
        })
        .collect()
}

pub fn github_repository_from_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = if let Some(path) = trimmed.strip_prefix("git@github.com:") {
        path
    } else {
        let rest = trimmed
            .strip_prefix("ssh://git@github.com/")
            .or_else(|| trimmed.strip_prefix("ssh://github.com/"))
            .or_else(|| trimmed.strip_prefix("https://github.com/"))
            .or_else(|| trimmed.strip_prefix("http://github.com/"))
            .or_else(|| trimmed.strip_prefix("git://github.com/"))?;
        rest
    };
    github_owner_repo_from_path(path)
}

fn github_owner_repo_from_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next().filter(|value| !value.is_empty())?;
    let repo = parts
        .next()
        .filter(|value| !value.is_empty())?
        .trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

pub fn hosted_evidence_spec_from_manifest(
    config: Option<&effigy_manifest::config_sections::ManifestReleaseHostedEvidenceConfig>,
) -> Result<Option<HostedEvidenceSpec>, ReleaseError> {
    let Some(config) = config else {
        return Ok(None);
    };
    let workflow = config.workflow.trim();
    if workflow.is_empty() {
        return Err(ReleaseError::TaskInvocation(
            "[release.hosted-evidence].workflow must not be empty".to_owned(),
        ));
    }
    let event = config
        .event
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_HOSTED_EVIDENCE_EVENT)
        .to_owned();
    let branch = config
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_HOSTED_EVIDENCE_BRANCH)
        .to_owned();
    Ok(Some(HostedEvidenceSpec {
        workflow: workflow.to_owned(),
        event,
        branch,
    }))
}

pub fn collect_hosted_evidence(
    root: &Path,
    spec: &HostedEvidenceSpec,
    lookup: &dyn HostedEvidenceLookup,
) -> Result<HostedRun, HostedEvidenceError> {
    collect_hosted_evidence_bound(
        root,
        spec,
        &HostedEvidenceBinding::unbound(),
        HostedEvidencePolling::NONE,
        lookup,
    )
}

/// Collect exact-commit evidence for an explicit commit, bound to the run the
/// release dispatched (contract 054 §3).
///
/// A release Run waits on the commit it certified, which is not the work
/// tree's HEAD once a later invocation has moved on, so the commit is an
/// argument rather than something re-read here.
pub fn collect_hosted_evidence_at(
    root: &Path,
    spec: &HostedEvidenceSpec,
    head_sha: &str,
    binding: &HostedEvidenceBinding,
    polling: HostedEvidencePolling,
    lookup: &dyn HostedEvidenceLookup,
) -> Result<HostedRun, HostedEvidenceError> {
    let origin =
        git_remote_url(root, "origin").map_err(|error| HostedEvidenceError::LookupFailed {
            detail: error.to_string(),
        })?;
    let repository = github_repository_from_remote_url(&origin).ok_or_else(|| {
        HostedEvidenceError::LookupFailed {
            detail: format!("origin remote is not a GitHub repository: {origin}"),
        }
    })?;
    let query = HostedEvidenceQuery {
        repository,
        head_sha: head_sha.trim().to_owned(),
        workflow: spec.workflow.clone(),
        event: spec.event.clone(),
        branch: spec.branch.clone(),
        binding: binding.clone(),
    };
    collect_bound_runs(&query, polling, lookup)
}

fn collect_hosted_evidence_bound(
    root: &Path,
    spec: &HostedEvidenceSpec,
    binding: &HostedEvidenceBinding,
    polling: HostedEvidencePolling,
    lookup: &dyn HostedEvidenceLookup,
) -> Result<HostedRun, HostedEvidenceError> {
    let head_sha = git_head_sha(root).map_err(|error| HostedEvidenceError::LookupFailed {
        detail: error.to_string(),
    })?;
    collect_hosted_evidence_at(root, spec, &head_sha, binding, polling, lookup)
}

/// Ask for the runs, evaluating each answer until a dispatch window has its
/// run or the bounded wait is spent.
///
/// Only a window with no run yet is retried: ambiguity, a failed run, the
/// wrong commit and an unusable lookup answer for good, and repeating them
/// would only delay the refusal.
pub(crate) fn collect_bound_runs(
    query: &HostedEvidenceQuery,
    polling: HostedEvidencePolling,
    lookup: &dyn HostedEvidenceLookup,
) -> Result<HostedRun, HostedEvidenceError> {
    let mut attempts_left = polling.attempts;
    loop {
        let runs = lookup
            .list_runs(query)
            .map_err(|detail| HostedEvidenceError::LookupFailed { detail })?;
        let evaluated = evaluate_hosted_runs(query, &runs);
        let retry = matches!(evaluated, Err(HostedEvidenceError::Missing(_)))
            && query.binding.created_after.is_some()
            && attempts_left > 0;
        if !retry {
            return evaluated;
        }
        attempts_left -= 1;
        std::thread::sleep(polling.interval);
    }
}

pub fn evaluate_hosted_runs(
    query: &HostedEvidenceQuery,
    runs: &[HostedRun],
) -> Result<HostedRun, HostedEvidenceError> {
    let mut matching = Vec::new();
    let mut wrong_sha = Vec::new();
    let mut wrong_repo = Vec::new();
    let mut malformed = Vec::new();

    for run in runs {
        if run.url.trim().is_empty() || run.head_sha.trim().is_empty() {
            malformed.push(run);
            continue;
        }
        if run.repository != query.repository {
            wrong_repo.push(run);
            continue;
        }
        if run.head_sha != query.head_sha {
            wrong_sha.push(run);
            continue;
        }
        matching.push(run);
    }

    if matching.is_empty() {
        if let Some(run) = wrong_repo.first() {
            return Err(HostedEvidenceError::WrongRepository {
                expected: query.repository.clone(),
                actual: run.repository.clone(),
                url: run.url.clone(),
            });
        }
        if let Some(run) = wrong_sha.first() {
            return Err(HostedEvidenceError::WrongSha {
                expected: query.head_sha.clone(),
                actual: run.head_sha.clone(),
                url: run.url.clone(),
            });
        }
        if !malformed.is_empty() {
            return Err(HostedEvidenceError::Ambiguous {
                detail: "hosted run is missing headSha or url".to_owned(),
            });
        }
        return Err(missing_evidence(query, Vec::new()));
    }

    if let Some(run_id) = query.binding.run_id {
        return match matching
            .iter()
            .find(|run| run.database_id == Some(run_id))
        {
            Some(run) => conclude_bound_run(query, run),
            None => Err(HostedEvidenceError::Ambiguous {
                detail: format!(
                    "this release bound run #{run_id} for gate `{}`, but `gh run list` did not return it for commit `{}`; the wait refuses to substitute one of: {}",
                    query.workflow,
                    query.head_sha,
                    describe_runs(&matching),
                ),
            }),
        };
    }

    let created_after = match query.binding.created_after.as_deref() {
        Some(created_after) => match parse_instant(created_after) {
            Some(instant) => Some(instant),
            None => {
                return Err(HostedEvidenceError::Ambiguous {
                    detail: format!(
                        "the recorded dispatch window `{created_after}` is not an RFC 3339 instant, so this check cannot tell this release's run from an earlier dispatch"
                    ),
                })
            }
        },
        None => None,
    };

    let mut in_window = Vec::new();
    let mut ignored = Vec::new();
    for run in matching {
        let inside = match (created_after, run_instant(run)) {
            (Some(window), Some(created)) => created >= window,
            // A run with no readable creation time cannot be placed in the
            // window, so it is never silently taken for this release's.
            (Some(_), None) => false,
            (None, _) => true,
        };
        if inside {
            in_window.push(run);
        } else {
            ignored.push(run);
        }
    }

    match in_window.as_slice() {
        [] => Err(missing_evidence(query, ignored)),
        [run] => conclude_bound_run(query, run),
        several => Err(HostedEvidenceError::Ambiguous {
            detail: format!(
                "{} runs match workflow `{}` for commit `{}` after the dispatch window, so this release cannot tell which run it started; re-run when only its own dispatch is live: {}",
                several.len(),
                query.workflow,
                query.head_sha,
                describe_runs(several),
            ),
        }),
    }
}

fn missing_evidence(query: &HostedEvidenceQuery, ignored: Vec<&HostedRun>) -> HostedEvidenceError {
    HostedEvidenceError::Missing(Box::new(MissingEvidence {
        sha: query.head_sha.clone(),
        repository: query.repository.clone(),
        workflow: query.workflow.clone(),
        event: query.event.clone(),
        branch: query.branch.clone(),
        created_after: query.binding.created_after.clone(),
        ignored: ignored.iter().map(|run| describe_run(run)).collect(),
    }))
}

fn conclude_bound_run(
    query: &HostedEvidenceQuery,
    run: &HostedRun,
) -> Result<HostedRun, HostedEvidenceError> {
    if is_successful_run(run) {
        return Ok(run.clone());
    }
    if run.status.trim() != "completed" {
        let status = if run.status.trim().is_empty() {
            "unknown".to_owned()
        } else {
            run.status.clone()
        };
        return Err(HostedEvidenceError::Pending {
            sha: query.head_sha.clone(),
            status,
            url: run.url.clone(),
            run_id: run.database_id,
        });
    }
    Err(HostedEvidenceError::Failed {
        sha: query.head_sha.clone(),
        conclusion: run
            .conclusion
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_owned()),
        url: run.url.clone(),
        run_id: run.database_id,
    })
}

fn parse_instant(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim()).ok()
}

fn run_instant(run: &HostedRun) -> Option<DateTime<FixedOffset>> {
    run.created_at.as_deref().and_then(parse_instant)
}

/// One run, named the way an operator can act on it.
fn describe_run(run: &HostedRun) -> String {
    let id = match run.database_id {
        Some(id) => format!("#{id}"),
        None => "unnumbered".to_owned(),
    };
    let state = match (run.status.trim(), run.conclusion.as_deref()) {
        (status, Some(conclusion)) if !status.is_empty() => format!("{status}/{conclusion}"),
        (status, _) if !status.is_empty() => status.to_owned(),
        _ => "unknown".to_owned(),
    };
    let created = run.created_at.as_deref().unwrap_or("unknown time");
    format!("{id} {} ({state}, created {created})", run.url)
}

fn describe_runs(runs: &[&HostedRun]) -> String {
    runs.iter()
        .map(|run| describe_run(run))
        .collect::<Vec<_>>()
        .join("; ")
}

fn is_successful_run(run: &HostedRun) -> bool {
    run.status.trim() == "completed"
        && run
            .conclusion
            .as_deref()
            .is_some_and(|value| value.trim() == "success")
}

/// Whether a hosted-evidence error means "not yet", which a Run waits on
/// rather than failing closed. Everything else — a failed run, the wrong
/// commit, the wrong repository, an ambiguous answer or an unusable lookup —
/// fails the gate.
pub fn is_waiting_evidence(error: &HostedEvidenceError) -> bool {
    matches!(
        error,
        HostedEvidenceError::Missing(_) | HostedEvidenceError::Pending { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> HostedEvidenceQuery {
        HostedEvidenceQuery {
            repository: "acme/demo".to_owned(),
            head_sha: "abc123".to_owned(),
            workflow: "ci.yml".to_owned(),
            event: "workflow_dispatch".to_owned(),
            branch: "main".to_owned(),
            binding: HostedEvidenceBinding::unbound(),
        }
    }

    fn windowed(created_after: &str) -> HostedEvidenceQuery {
        HostedEvidenceQuery {
            binding: HostedEvidenceBinding::opened_at(created_after),
            ..query()
        }
    }

    fn bound(run_id: u64) -> HostedEvidenceQuery {
        HostedEvidenceQuery {
            binding: HostedEvidenceBinding::opened_at("2026-10-22T00:10:00Z").with_run_id(run_id),
            ..query()
        }
    }

    fn run(repo: &str, sha: &str, status: &str, conclusion: Option<&str>, url: &str) -> HostedRun {
        run_at(
            repo,
            sha,
            status,
            conclusion,
            url,
            Some("2026-10-22T00:00:00Z"),
        )
    }

    fn run_at(
        repo: &str,
        sha: &str,
        status: &str,
        conclusion: Option<&str>,
        url: &str,
        created_at: Option<&str>,
    ) -> HostedRun {
        HostedRun {
            repository: repo.to_owned(),
            head_sha: sha.to_owned(),
            status: status.to_owned(),
            conclusion: conclusion.map(ToOwned::to_owned),
            url: url.to_owned(),
            database_id: Some(99),
            event: Some("workflow_dispatch".to_owned()),
            head_branch: Some("main".to_owned()),
            created_at: created_at.map(ToOwned::to_owned),
        }
    }

    fn numbered(mut run: HostedRun, id: u64) -> HostedRun {
        run.database_id = Some(id);
        run
    }

    #[test]
    fn github_repository_parses_https_ssh_and_git_urls() {
        assert_eq!(
            github_repository_from_remote_url("https://github.com/acme/demo.git"),
            Some("acme/demo".to_owned())
        );
        assert_eq!(
            github_repository_from_remote_url("git@github.com:acme/demo.git"),
            Some("acme/demo".to_owned())
        );
        assert_eq!(
            github_repository_from_remote_url("ssh://git@github.com/acme/demo.git"),
            Some("acme/demo".to_owned())
        );
        assert_eq!(
            github_repository_from_remote_url("https://github.com/acme/demo/actions/runs/123"),
            Some("acme/demo".to_owned())
        );
        assert_eq!(
            github_repository_from_remote_url("https://gitlab.com/acme/demo.git"),
            None
        );
    }

    #[test]
    fn evaluate_accepts_exact_sha_success_from_this_repository() {
        let accepted = evaluate_hosted_runs(
            &query(),
            &[run(
                "acme/demo",
                "abc123",
                "completed",
                Some("success"),
                "https://github.com/acme/demo/actions/runs/1",
            )],
        )
        .expect("success");
        assert_eq!(accepted.url, "https://github.com/acme/demo/actions/runs/1");
    }

    #[test]
    fn evaluate_prefers_a_bound_run_over_a_newer_sibling() {
        // A sibling dispatch of the same commit can be newer and greener. Once
        // this release bound its own run, the sibling never replaces it.
        let accepted = evaluate_hosted_runs(
            &bound(1),
            &[
                numbered(
                    run(
                        "acme/demo",
                        "abc123",
                        "completed",
                        Some("success"),
                        "https://github.com/acme/demo/actions/runs/2",
                    ),
                    2,
                ),
                numbered(
                    run(
                        "acme/demo",
                        "abc123",
                        "in_progress",
                        None,
                        "https://github.com/acme/demo/actions/runs/1",
                    ),
                    1,
                ),
            ],
        )
        .expect_err("the bound run is still running");
        assert!(matches!(accepted, HostedEvidenceError::Pending { .. }));
    }

    #[test]
    fn two_concurrent_runs_on_one_sha_bind_to_the_dispatched_one() {
        // The older green run belongs to an earlier dispatch; the one created
        // inside this Run's window is the run it started.
        let accepted = evaluate_hosted_runs(
            &windowed("2026-10-22T00:10:00Z"),
            &[
                numbered(
                    run_at(
                        "acme/demo",
                        "abc123",
                        "completed",
                        Some("success"),
                        "https://github.com/acme/demo/actions/runs/1",
                        Some("2026-10-22T00:05:00Z"),
                    ),
                    1,
                ),
                numbered(
                    run_at(
                        "acme/demo",
                        "abc123",
                        "in_progress",
                        None,
                        "https://github.com/acme/demo/actions/runs/2",
                        Some("2026-10-22T00:11:00Z"),
                    ),
                    2,
                ),
            ],
        )
        .expect_err("the dispatched run is the one still running");
        match accepted {
            HostedEvidenceError::Pending { url, .. } => {
                assert_eq!(url, "https://github.com/acme/demo/actions/runs/2")
            }
            other => panic!("expected the dispatched run, got {other:?}"),
        }
    }

    #[test]
    fn two_runs_inside_one_dispatch_window_refuse_with_the_candidates_named() {
        let ambiguous = evaluate_hosted_runs(
            &windowed("2026-10-22T00:10:00Z"),
            &[
                numbered(
                    run_at(
                        "acme/demo",
                        "abc123",
                        "completed",
                        Some("success"),
                        "https://github.com/acme/demo/actions/runs/3",
                        Some("2026-10-22T00:12:00Z"),
                    ),
                    3,
                ),
                numbered(
                    run_at(
                        "acme/demo",
                        "abc123",
                        "completed",
                        Some("success"),
                        "https://github.com/acme/demo/actions/runs/4",
                        Some("2026-10-22T00:13:00Z"),
                    ),
                    4,
                ),
            ],
        )
        .expect_err("two candidates in one window are not a binding");
        let detail = match ambiguous {
            HostedEvidenceError::Ambiguous { detail } => detail,
            other => panic!("expected an ambiguity refusal, got {other:?}"),
        };
        assert!(detail.contains("2 runs match"), "{detail}");
        assert!(detail.contains("actions/runs/3"), "{detail}");
        assert!(detail.contains("actions/runs/4"), "{detail}");
        // The gate fails closed instead of waiting for someone to change it.
        assert!(!is_waiting_evidence(&HostedEvidenceError::Ambiguous {
            detail,
        }));
    }

    #[test]
    fn a_window_with_only_earlier_runs_waits_and_names_them() {
        let missing = evaluate_hosted_runs(
            &windowed("2026-10-22T00:10:00Z"),
            &[numbered(
                run_at(
                    "acme/demo",
                    "abc123",
                    "completed",
                    Some("success"),
                    "https://github.com/acme/demo/actions/runs/1",
                    Some("2026-10-22T00:05:00Z"),
                ),
                1,
            )],
        )
        .expect_err("an earlier dispatch is not this one");
        assert!(is_waiting_evidence(&missing), "{missing:?}");
        let diagnostic = missing.diagnostic("ci");
        assert!(diagnostic.contains("created after 2026-10-22T00:10:00Z"));
        assert!(diagnostic.contains("actions/runs/1"), "{diagnostic}");
    }

    #[test]
    fn a_bound_run_missing_from_the_listing_refuses_a_substitute() {
        let refused = evaluate_hosted_runs(
            &bound(7),
            &[numbered(
                run(
                    "acme/demo",
                    "abc123",
                    "completed",
                    Some("success"),
                    "https://github.com/acme/demo/actions/runs/2",
                ),
                2,
            )],
        )
        .expect_err("the bound run is not in the listing");
        match refused {
            HostedEvidenceError::Ambiguous { detail } => {
                assert!(detail.contains("bound run #7"), "{detail}");
                assert!(detail.contains("actions/runs/2"), "{detail}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_run_without_a_creation_time_cannot_enter_a_window() {
        let missing = evaluate_hosted_runs(
            &windowed("2026-10-22T00:10:00Z"),
            &[numbered(
                run_at(
                    "acme/demo",
                    "abc123",
                    "completed",
                    Some("success"),
                    "https://github.com/acme/demo/actions/runs/1",
                    None,
                ),
                1,
            )],
        )
        .expect_err("an unplaceable run is not this release's");
        assert!(is_waiting_evidence(&missing), "{missing:?}");
    }

    #[test]
    fn a_malformed_dispatch_window_refuses_rather_than_guessing() {
        let refused = evaluate_hosted_runs(
            &windowed("yesterday"),
            &[run(
                "acme/demo",
                "abc123",
                "completed",
                Some("success"),
                "https://github.com/acme/demo/actions/runs/1",
            )],
        )
        .expect_err("an unreadable window cannot bind");
        assert!(matches!(refused, HostedEvidenceError::Ambiguous { .. }));
    }

    #[test]
    fn evaluate_rejects_missing_pending_failed_wrong_sha_wrong_repo_and_ambiguous() {
        let missing = evaluate_hosted_runs(&query(), &[]).expect_err("missing");
        assert!(matches!(missing, HostedEvidenceError::Missing(_)));

        let pending = evaluate_hosted_runs(
            &query(),
            &[run(
                "acme/demo",
                "abc123",
                "in_progress",
                None,
                "https://github.com/acme/demo/actions/runs/1",
            )],
        )
        .expect_err("pending");
        assert!(matches!(pending, HostedEvidenceError::Pending { .. }));

        let failed = evaluate_hosted_runs(
            &query(),
            &[run(
                "acme/demo",
                "abc123",
                "completed",
                Some("failure"),
                "https://github.com/acme/demo/actions/runs/1",
            )],
        )
        .expect_err("failed");
        assert!(matches!(failed, HostedEvidenceError::Failed { .. }));

        let wrong_sha = evaluate_hosted_runs(
            &query(),
            &[run(
                "acme/demo",
                "def456",
                "completed",
                Some("success"),
                "https://github.com/acme/demo/actions/runs/1",
            )],
        )
        .expect_err("wrong sha");
        assert!(matches!(wrong_sha, HostedEvidenceError::WrongSha { .. }));

        let wrong_repo = evaluate_hosted_runs(
            &query(),
            &[run(
                "other/repo",
                "abc123",
                "completed",
                Some("success"),
                "https://github.com/other/repo/actions/runs/1",
            )],
        )
        .expect_err("wrong repo");
        assert!(matches!(
            wrong_repo,
            HostedEvidenceError::WrongRepository { .. }
        ));

        let ambiguous = evaluate_hosted_runs(
            &query(),
            &[HostedRun {
                repository: "acme/demo".to_owned(),
                head_sha: String::new(),
                status: "completed".to_owned(),
                conclusion: Some("success".to_owned()),
                url: String::new(),
                database_id: None,
                event: None,
                head_branch: None,
                created_at: None,
            }],
        )
        .expect_err("ambiguous");
        assert!(matches!(ambiguous, HostedEvidenceError::Ambiguous { .. }));
    }

    #[test]
    fn parse_gh_run_list_json_reads_github_fields() {
        let raw = br#"[{
            "databaseId": 77,
            "headSha": "abc123",
            "status": "completed",
            "conclusion": "success",
            "url": "https://github.com/acme/demo/actions/runs/77",
            "event": "workflow_dispatch",
            "headBranch": "main",
            "createdAt": "2026-10-22T00:10:00Z"
        }]"#;
        let runs = parse_gh_run_list_json(raw).expect("parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].repository, "acme/demo");
        assert_eq!(runs[0].head_sha, "abc123");
        assert_eq!(runs[0].database_id, Some(77));
        assert_eq!(runs[0].created_at.as_deref(), Some("2026-10-22T00:10:00Z"));
    }
}
