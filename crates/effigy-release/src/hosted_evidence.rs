use std::path::Path;
use std::process::Command as ProcessCommand;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedEvidenceQuery {
    pub repository: String,
    pub head_sha: String,
    pub workflow: String,
    pub event: String,
    pub branch: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedEvidenceError {
    Missing {
        sha: String,
        repository: String,
        workflow: String,
        event: String,
        branch: String,
    },
    Pending {
        sha: String,
        status: String,
        url: String,
    },
    Failed {
        sha: String,
        conclusion: String,
        url: String,
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
            Self::Missing {
                sha,
                repository,
                workflow,
                event,
                branch,
            } => format!(
                "hosted evidence for gate `{gate_name}` is missing: no GitHub Actions run for workflow `{workflow}` event `{event}` branch `{branch}` commit `{sha}` in `{repository}`. Dispatch that workflow for this exact commit, wait for success, then retry."
            ),
            Self::Pending { sha, status, url } => format!(
                "hosted evidence for gate `{gate_name}` is pending: run {url} status is `{status}` for commit `{sha}`. Wait for completion, then retry."
            ),
            Self::Failed {
                sha,
                conclusion,
                url,
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
                "databaseId,headSha,status,conclusion,url,event,headBranch",
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
    let head_sha = git_head_sha(root).map_err(|error| HostedEvidenceError::LookupFailed {
        detail: error.to_string(),
    })?;
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
        head_sha,
        workflow: spec.workflow.clone(),
        event: spec.event.clone(),
        branch: spec.branch.clone(),
    };
    let runs = lookup
        .list_runs(&query)
        .map_err(|detail| HostedEvidenceError::LookupFailed { detail })?;
    evaluate_hosted_runs(&query, &runs)
}

pub fn evaluate_hosted_runs(
    query: &HostedEvidenceQuery,
    runs: &[HostedRun],
) -> Result<HostedRun, HostedEvidenceError> {
    if runs.is_empty() {
        return Err(HostedEvidenceError::Missing {
            sha: query.head_sha.clone(),
            repository: query.repository.clone(),
            workflow: query.workflow.clone(),
            event: query.event.clone(),
            branch: query.branch.clone(),
        });
    }

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
        return Err(HostedEvidenceError::Missing {
            sha: query.head_sha.clone(),
            repository: query.repository.clone(),
            workflow: query.workflow.clone(),
            event: query.event.clone(),
            branch: query.branch.clone(),
        });
    }

    if let Some(run) = matching.iter().find(|run| is_successful_run(run)) {
        return Ok((*run).clone());
    }
    if let Some(run) = matching.iter().find(|run| run.status.trim() != "completed") {
        let status = if run.status.trim().is_empty() {
            "unknown".to_owned()
        } else {
            run.status.clone()
        };
        return Err(HostedEvidenceError::Pending {
            sha: query.head_sha.clone(),
            status,
            url: run.url.clone(),
        });
    }
    let failed = matching[0];
    Err(HostedEvidenceError::Failed {
        sha: query.head_sha.clone(),
        conclusion: failed
            .conclusion
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_owned()),
        url: failed.url.clone(),
    })
}

fn is_successful_run(run: &HostedRun) -> bool {
    run.status.trim() == "completed"
        && run
            .conclusion
            .as_deref()
            .is_some_and(|value| value.trim() == "success")
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
        }
    }

    fn run(repo: &str, sha: &str, status: &str, conclusion: Option<&str>, url: &str) -> HostedRun {
        HostedRun {
            repository: repo.to_owned(),
            head_sha: sha.to_owned(),
            status: status.to_owned(),
            conclusion: conclusion.map(ToOwned::to_owned),
            url: url.to_owned(),
            database_id: Some(99),
            event: Some("workflow_dispatch".to_owned()),
            head_branch: Some("main".to_owned()),
        }
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
    fn evaluate_prefers_successful_run_over_older_failure() {
        let accepted = evaluate_hosted_runs(
            &query(),
            &[
                run(
                    "acme/demo",
                    "abc123",
                    "completed",
                    Some("success"),
                    "https://github.com/acme/demo/actions/runs/2",
                ),
                run(
                    "acme/demo",
                    "abc123",
                    "completed",
                    Some("failure"),
                    "https://github.com/acme/demo/actions/runs/1",
                ),
            ],
        )
        .expect("success");
        assert_eq!(accepted.url, "https://github.com/acme/demo/actions/runs/2");
    }

    #[test]
    fn evaluate_rejects_missing_pending_failed_wrong_sha_wrong_repo_and_ambiguous() {
        let missing = evaluate_hosted_runs(&query(), &[]).expect_err("missing");
        assert!(matches!(missing, HostedEvidenceError::Missing { .. }));

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
            "headBranch": "main"
        }]"#;
        let runs = parse_gh_run_list_json(raw).expect("parse");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].repository, "acme/demo");
        assert_eq!(runs[0].head_sha, "abc123");
        assert_eq!(runs[0].database_id, Some(77));
    }
}
