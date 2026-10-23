//! What a release Run does to the world: git, tasks and gates.
//!
//! The Run itself owns ordering, checkpoints and refusal; this module owns the
//! operations that leave the process. Tests substitute the same trait to inject
//! an ambiguous push or to script artifact tasks, so every proof exercises the
//! production stage logic rather than a parallel copy of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hosted_evidence::{
    collect_hosted_evidence_at, is_waiting_evidence, BoundRun, GhCliHostedEvidenceLookup,
    HostedEvidenceBinding, HostedEvidencePolling, HostedEvidenceSpec,
};
use crate::ReleaseError;

/// One task invocation from a release stage (contract 054 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRequest {
    /// The Effigy task name, or a gate's shell command.
    pub task: String,
    pub label: String,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    /// Where the task reads `effigy.release.input.v1`.
    pub input_path: PathBuf,
    /// Where the task writes `effigy.release.output.v1`.
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskResponse {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// What a hosted wait found for one gate (contract 054 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedEvidence {
    /// Exact-commit evidence for this commit, from this repository: the gate
    /// passes without running its command. `run` is the bound run.
    Accepted {
        repository: String,
        commit: String,
        run: BoundRun,
    },
    /// No evidence yet, or a run still in progress: the Run waits without
    /// holding a process open and re-checks when it resumes. `run` is the one
    /// dispatched run the check bound, when there is one, so the wait re-checks
    /// that run and never a sibling.
    Waiting {
        detail: String,
        run: Option<BoundRun>,
    },
    /// A failed run, the wrong commit or repository, an ambiguous answer or an
    /// unusable lookup: the gate fails closed.
    Failed {
        detail: String,
        run: Option<BoundRun>,
    },
}

fn command_error(context: &str, output: &std::process::Output) -> ReleaseError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim();
    ReleaseError::TaskInvocation(if detail.is_empty() {
        format!("{context} failed with {}", output.status)
    } else {
        format!("{context} failed with {}: {detail}", output.status)
    })
}

pub trait ReleaseEffects: std::fmt::Debug {
    fn current_branch(&mut self) -> Result<String, ReleaseError>;
    fn head_commit(&mut self) -> Result<String, ReleaseError>;
    fn working_tree_clean(&mut self) -> Result<bool, ReleaseError>;
    /// The commit a tag points at locally, or `None` when there is no such tag.
    fn local_tag_commit(&mut self, tag: &str) -> Result<Option<String>, ReleaseError>;
    /// The commit a tag points at on the remote, or `None` when the remote has
    /// no such tag. `Err` is an unknown outcome, never an absence.
    fn remote_tag_commit(&mut self, tag: &str) -> Result<Option<String>, ReleaseError>;
    fn create_annotated_tag(
        &mut self,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), ReleaseError>;
    /// Push one tag. An error is an unknown outcome (054 §7).
    fn push_tag(&mut self, tag: &str) -> Result<(), ReleaseError>;
    /// Push the release commit. An error is an unknown outcome.
    fn push_commit(&mut self, commit: &str, branch: &str) -> Result<(), ReleaseError>;
    fn commit_paths(&mut self, paths: &[PathBuf], message: &str) -> Result<String, ReleaseError>;
    /// Restore `paths` to HEAD in both the index and the working tree: a
    /// prepared change that never committed leaves nothing behind.
    fn discard_staged(&mut self, paths: &[PathBuf]) -> Result<(), ReleaseError>;
    fn run_task(&mut self, request: &TaskRequest) -> Result<TaskResponse, ReleaseError>;
    /// Exact-commit hosted evidence for one gate (contract 054 §3, §7).
    ///
    /// `binding` is the run this release dispatched: a resume re-checks the
    /// bound run, and a fresh wait accepts only a run created inside its
    /// dispatch window.
    fn hosted_evidence(
        &mut self,
        gate: &str,
        spec: &HostedEvidenceSpec,
        commit: &str,
        binding: &HostedEvidenceBinding,
    ) -> Result<HostedEvidence, ReleaseError>;
    /// The identity recorded as an approval's approver.
    fn operator(&mut self) -> Option<String>;
}

/// The bound run an evidence error names, when the check matched one.
fn bound_run(error: &crate::hosted_evidence::HostedEvidenceError) -> Option<BoundRun> {
    use crate::hosted_evidence::HostedEvidenceError;
    match error {
        HostedEvidenceError::Pending { url, run_id, .. }
        | HostedEvidenceError::Failed { url, run_id, .. } => Some(BoundRun {
            run_id: *run_id,
            url: url.clone(),
        }),
        HostedEvidenceError::Missing(_)
        | HostedEvidenceError::Ambiguous { .. }
        | HostedEvidenceError::WrongRepository { .. }
        | HostedEvidenceError::WrongSha { .. }
        | HostedEvidenceError::LookupFailed { .. } => None,
    }
}

/// The approver recorded on an approval checkpoint.
pub fn approver_name(effects: &mut dyn ReleaseEffects) -> String {
    effects.operator().unwrap_or_else(|| "operator".to_owned())
}

/// The production implementation: git through the `git` CLI, tasks through this
/// Effigy binary, so a task target runs with the same environment and admission
/// as any other Effigy task.
#[derive(Debug)]
pub struct GitEffects {
    pub repo_root: PathBuf,
    pub remote: String,
    pub task_runner: TaskRunner,
}

/// How a release stage runs a task.
#[derive(Debug, Clone)]
pub enum TaskRunner {
    /// This Effigy binary, resolved from the running executable.
    CurrentBinary,
}

impl GitEffects {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            remote: "origin".to_owned(),
            task_runner: TaskRunner::CurrentBinary,
        }
    }

    fn git(&self, args: &[&str]) -> Result<std::process::Output, ReleaseError> {
        Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(args)
            .output()
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to run `git {}` in {}: {error}",
                    args.join(" "),
                    self.repo_root.display()
                ))
            })
    }

    fn git_stdout(&self, args: &[&str]) -> Result<String, ReleaseError> {
        let output = self.git(args)?;
        if !output.status.success() {
            return Err(command_error(&format!("`git {}`", args.join(" ")), &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

impl ReleaseEffects for GitEffects {
    fn current_branch(&mut self) -> Result<String, ReleaseError> {
        let branch = self.git_stdout(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if branch.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release validate needs a checked-out branch; this work tree is detached".into(),
            ));
        }
        Ok(branch)
    }

    fn head_commit(&mut self) -> Result<String, ReleaseError> {
        self.git_stdout(&["rev-parse", "HEAD"])
    }

    fn working_tree_clean(&mut self) -> Result<bool, ReleaseError> {
        let status = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["status", "--porcelain=v1", "--untracked-files=all"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!("failed to inspect git working tree: {error}"))
            })?;
        if !status.status.success() {
            return Err(command_error("`git status`", &status));
        }
        Ok(status.stdout.is_empty())
    }

    fn local_tag_commit(&mut self, tag: &str) -> Result<Option<String>, ReleaseError> {
        // `show-ref --verify` reports a missing ref as exit 1 and nothing else;
        // any other failure is an unreadable tag, never an absence.
        let reference = format!("refs/tags/{tag}");
        let present = self.git(&["show-ref", "--verify", "--quiet", &reference])?;
        if !present.status.success() {
            return Ok(None);
        }
        let commit = self.git_stdout(&["rev-parse", &format!("{reference}^{{commit}}")])?;
        Ok(Some(commit))
    }

    fn remote_tag_commit(&mut self, tag: &str) -> Result<Option<String>, ReleaseError> {
        // `ls-remote` with a ref pattern does not always report the peeled
        // `^{}` entry, and an annotated tag's own object is not its commit, so
        // list the tags and prefer the peeled line for this tag.
        let output = self.git(&["ls-remote", "--tags", &self.remote])?;
        if !output.status.success() {
            return Err(command_error(
                &format!(
                    "`git ls-remote {remote} refs/tags/{tag}`",
                    remote = self.remote
                ),
                &output,
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut peeled = None;
        let mut plain = None;
        for line in stdout.lines() {
            let mut fields = line.split_whitespace();
            let (Some(sha), Some(reference)) = (fields.next(), fields.next()) else {
                continue;
            };
            if reference == format!("refs/tags/{tag}^{{}}") {
                peeled = Some(sha.to_owned());
            } else if reference == format!("refs/tags/{tag}") {
                plain = Some(sha.to_owned());
            }
        }
        Ok(peeled.or(plain))
    }

    fn create_annotated_tag(
        &mut self,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), ReleaseError> {
        let output = self.git(&["tag", "-a", tag, "-m", message, commit])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(&format!("`git tag -a {tag}`"), &output))
        }
    }

    fn push_tag(&mut self, tag: &str) -> Result<(), ReleaseError> {
        let reference = format!("refs/tags/{tag}");
        let output = self.git(&["push", &self.remote, &reference])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(
                &format!("`git push {remote} {reference}`", remote = self.remote),
                &output,
            ))
        }
    }

    fn push_commit(&mut self, commit: &str, branch: &str) -> Result<(), ReleaseError> {
        let reference = format!("{commit}:refs/heads/{branch}");
        let output = self.git(&["push", &self.remote, &reference])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(
                &format!("`git push {remote} {reference}`", remote = self.remote),
                &output,
            ))
        }
    }

    fn commit_paths(&mut self, paths: &[PathBuf], message: &str) -> Result<String, ReleaseError> {
        let staging = PathBuf::new();
        let mut add = Command::new("git");
        add.arg("-C").arg(&self.repo_root).arg("add").arg("--");
        for path in paths {
            let relative = path.strip_prefix(&self.repo_root).unwrap_or(path);
            if relative == staging {
                continue;
            }
            add.arg(relative);
        }
        let output = add.output().map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to stage the release changes: {error}"))
        })?;
        if !output.status.success() {
            return Err(command_error("`git add`", &output));
        }
        let output = self.git(&["commit", "-m", message])?;
        if !output.status.success() {
            return Err(command_error("`git commit`", &output));
        }
        self.head_commit()
    }

    fn discard_staged(&mut self, paths: &[PathBuf]) -> Result<(), ReleaseError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut command = Command::new("git");
        command.arg("-C").arg(&self.repo_root).args([
            "restore",
            "--source=HEAD",
            "--staged",
            "--worktree",
            "--",
        ]);
        for path in paths {
            let relative = path.strip_prefix(&self.repo_root).unwrap_or(path);
            command.arg(relative);
        }
        let output = command.output().map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to restore the prepared paths after a failed prepare: {error}"
            ))
        })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("`git restore`", &output))
        }
    }

    fn run_task(&mut self, request: &TaskRequest) -> Result<TaskResponse, ReleaseError> {
        let TaskRunner::CurrentBinary = self.task_runner;
        let binary = std::env::current_exe().map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to resolve this Effigy binary for `{}`: {error}",
                request.label
            ))
        })?;
        let mut command = Command::new(binary);
        command
            .arg("--repo")
            .arg(&request.cwd)
            .arg(&request.task)
            .current_dir(&request.cwd)
            .envs(&request.env);
        let output = command.output().map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to start `{}` ({}): {error}",
                request.task, request.label
            ))
        })?;
        Ok(TaskResponse {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            success: output.status.success(),
        })
    }

    fn hosted_evidence(
        &mut self,
        gate: &str,
        spec: &HostedEvidenceSpec,
        commit: &str,
        binding: &HostedEvidenceBinding,
    ) -> Result<HostedEvidence, ReleaseError> {
        let evaluated = collect_hosted_evidence_at(
            &self.repo_root,
            spec,
            commit,
            binding,
            HostedEvidencePolling::DISPATCH,
            &GhCliHostedEvidenceLookup,
        );
        Ok(match evaluated {
            Ok(run) => HostedEvidence::Accepted {
                repository: run.repository,
                commit: run.head_sha,
                run: BoundRun {
                    run_id: run.database_id,
                    url: run.url,
                },
            },
            Err(error) if is_waiting_evidence(&error) => HostedEvidence::Waiting {
                detail: error.diagnostic(gate),
                run: bound_run(&error),
            },
            Err(error) => HostedEvidence::Failed {
                detail: error.diagnostic(gate),
                run: bound_run(&error),
            },
        })
    }

    fn operator(&mut self) -> Option<String> {
        if let Ok(name) = self.git_stdout(&["config", "user.name"]) {
            if !name.is_empty() {
                return Some(name);
            }
        }
        std::env::var("USER").ok().filter(|user| !user.is_empty())
    }
}
