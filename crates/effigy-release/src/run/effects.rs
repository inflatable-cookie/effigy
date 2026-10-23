//! What a release Run does to the world: git, tasks and gates.
//!
//! The Run itself owns ordering, checkpoints and refusal; this module owns the
//! operations that leave the process. Tests substitute the same trait to inject
//! an ambiguous push or to script artifact tasks, so every proof exercises the
//! production stage logic rather than a parallel copy of it.

use effigy_kernel::run::Run;
use effigy_kernel::supervise::Executable;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ReleaseError;

/// One task invocation from a release stage (contract 054 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRequest {
    /// The Effigy task name, or a gate's shell command.
    pub task: String,
    pub label: String,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskResponse {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
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
    fn current_branch(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError>;
    fn head_commit(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError>;
    fn working_tree_clean(&mut self, run: &mut Run<'_>) -> Result<bool, ReleaseError>;
    /// The commit a tag points at locally, or `None` when there is no such tag.
    fn local_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError>;
    /// The commit a tag points at on the remote, or `None` when the remote has
    /// no such tag. `Err` is an unknown outcome, never an absence.
    fn remote_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError>;
    fn create_annotated_tag(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), ReleaseError>;
    /// Push one tag. An error is an unknown outcome (054 §7).
    fn push_tag(&mut self, run: &mut Run<'_>, tag: &str) -> Result<(), ReleaseError>;
    /// Push the release commit. An error is an unknown outcome.
    fn push_commit(
        &mut self,
        run: &mut Run<'_>,
        commit: &str,
        branch: &str,
    ) -> Result<(), ReleaseError>;
    fn commit_paths(
        &mut self,
        run: &mut Run<'_>,
        paths: &[PathBuf],
        message: &str,
    ) -> Result<String, ReleaseError>;
    /// Restore `paths` to HEAD in both the index and the working tree: a
    /// prepared change that never committed leaves nothing behind.
    fn discard_staged(&mut self, run: &mut Run<'_>, paths: &[PathBuf]) -> Result<(), ReleaseError>;
    fn run_task(
        &mut self,
        run: &mut Run<'_>,
        request: &TaskRequest,
    ) -> Result<TaskResponse, ReleaseError>;
    /// The identity recorded as an approval's approver.
    fn operator(&mut self, run: &mut Run<'_>) -> Option<String>;
}

/// The approver recorded on an approval checkpoint.
pub fn approver_name(effects: &mut dyn ReleaseEffects, run: &mut Run<'_>) -> String {
    effects
        .operator(run)
        .unwrap_or_else(|| "operator".to_owned())
}

/// The production implementation: git through the `git` CLI, tasks through this
/// Effigy binary, so a task target runs with the same environment and admission
/// as any other Effigy task.
#[derive(Debug)]
pub struct GitEffects {
    pub repo_root: PathBuf,
    pub remote: String,
    pub task_runner: TaskRunner,
    executable: Result<Executable, String>,
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
            executable: Executable::current().map_err(|error| error.to_string()),
        }
    }

    /// Embedders supply an executable implementing the kernel helper entries.
    pub fn with_executable(repo_root: &Path, executable: Executable) -> Self {
        Self {
            executable: Ok(executable),
            ..Self::new(repo_root)
        }
    }

    fn git(&self, run: &mut Run<'_>, args: &[&str]) -> Result<std::process::Output, ReleaseError> {
        self.git_args(run, args.iter().map(OsString::from).collect())
    }
    fn git_args(
        &self,
        run: &mut Run<'_>,
        args: Vec<OsString>,
    ) -> Result<std::process::Output, ReleaseError> {
        let program = find_program("git")?;
        let argv = [
            program.into_os_string(),
            "-C".into(),
            self.repo_root.clone().into_os_string(),
        ]
        .into_iter()
        .chain(args)
        .collect();
        self.execute(
            run,
            argv,
            &self.repo_root.clone(),
            &BTreeMap::from([("GIT_OPTIONAL_LOCKS".into(), "0".into())]),
            "release git",
        )
    }

    fn execute(
        &self,
        run: &mut Run<'_>,
        argv: Vec<OsString>,
        cwd: &Path,
        extra: &BTreeMap<String, String>,
        label: &str,
    ) -> Result<std::process::Output, ReleaseError> {
        use effigy_kernel::{
            ctx::*,
            custody::*,
            observe::Observed,
            run::Phase,
            supervise::{LifecyclePlan, Placement as LifecyclePlacement},
        };
        let refused = |error: &dyn std::fmt::Display| {
            ReleaseError::TaskInvocation(format!("release process: {error}"))
        };
        if !run.owns_sentinel() {
            let executable = self
                .executable
                .as_ref()
                .map_err(|error| refused(error))?
                .clone();
            run.enable_custody(&LifecyclePlan {
                executable,
                placement: LifecyclePlacement::Inline,
            })
            .map_err(|error| refused(&error))?;
        }
        let op = OperationContext::new(
            Deadline::At(std::time::Instant::now() + Duration::from_secs(12 * 60 * 60)),
            CancelToken::default(),
            Limits {
                capture_tail: 4 << 20,
                log_rotate: 8 << 20,
                log_keep: 2,
                line_max: 65536,
                queue_bytes: 1 << 20,
                probe_concurrency: 2,
            },
            EffectClass::Mutate,
            Placement::Host,
        );
        let cx = op.effects().map_err(|error| {
            ReleaseError::TaskInvocation(format!("release process context: {error:?}"))
        })?;
        let mut env = std::env::vars_os()
            .filter(|(key, _)| {
                !matches!(
                    key.to_str(),
                    Some(
                        "EFFIGY_CUSTODY_NONCE" | "EFFIGY_PROBE_NONCE" | "EFFIGY_PARENT_CAPABILITY"
                    )
                )
            })
            .map(|(key, value)| (key, EnvValue::Plain(value)))
            .collect::<BTreeMap<_, _>>();
        env.extend(
            extra
                .iter()
                .map(|(key, value)| (key.into(), EnvValue::Plain(value.into()))),
        );
        let spec = ProcessSpec {
            argv,
            cwd: cwd.into(),
            env,
            grace: Duration::from_secs(3),
            lifetime: Lifetime::Finite(Duration::from_secs(12 * 60 * 60)),
            terminal: Terminal::None,
            label: label.into(),
        };
        // Stages interpret adapter statuses (for example, a missing tag exits
        // 1). Every process remains in custody; the stage owns workload success.
        let mut process = Custody::spawn_auxiliary(run, spec, &cx).map_err(|error| {
            ReleaseError::TaskInvocation(format!("release process admission: {error:?}"))
        })?;
        if matches!(
            run.phase().map_err(|error| refused(&error))?,
            Phase::Admitted | Phase::Starting
        ) {
            run.advance(Phase::Active)
                .map_err(|error| refused(&error))?;
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = loop {
            let (out, err) = process.take_stream_output();
            if stdout.len() + stderr.len() + out.len() + err.len() > 4 << 20 {
                return Err(ReleaseError::TaskInvocation(
                    "release process output exceeds 4 MiB".into(),
                ));
            }
            stdout.extend(out);
            stderr.extend(err);
            if let Observed::Verified(exit, _) = process.poll() {
                break exit;
            }
            op.check("release process wait").map_err(|error| {
                ReleaseError::TaskInvocation(format!("release process interrupted: {error:?}"))
            })?;
            std::thread::sleep(Duration::from_millis(5));
        };
        let cleanup = op.cleanup(Duration::from_secs(30)).map_err(|error| {
            ReleaseError::TaskInvocation(format!("release process cleanup context: {error:?}"))
        })?;
        let closed = process.stop(&cleanup);
        if !matches!(closed.group, Observed::Verified(..) | Observed::Absent(_))
            || !closed.residue.is_empty()
            || closed.capture != CaptureState::Complete
        {
            return Err(ReleaseError::TaskInvocation(
                "release process closure or capture is unresolved".into(),
            ));
        }
        let (out, err) = process.take_stream_output();
        stdout.extend(out);
        stderr.extend(err);
        if stdout.len() + stderr.len() > 4 << 20 {
            return Err(ReleaseError::TaskInvocation(
                "release process output exceeds 4 MiB".into(),
            ));
        }
        process
            .complete(run, exit)
            .map_err(|error| refused(&error))?;
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(exit.code << 8),
            stdout,
            stderr,
        })
    }

    fn git_stdout(&self, run: &mut Run<'_>, args: &[&str]) -> Result<String, ReleaseError> {
        let output = self.git(run, args)?;
        if !output.status.success() {
            return Err(command_error(&format!("`git {}`", args.join(" ")), &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

impl ReleaseEffects for GitEffects {
    fn current_branch(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError> {
        let branch = self.git_stdout(run, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if branch.is_empty() {
            return Err(ReleaseError::TaskInvocation(
                "release validate needs a checked-out branch; this work tree is detached".into(),
            ));
        }
        Ok(branch)
    }

    fn head_commit(&mut self, run: &mut Run<'_>) -> Result<String, ReleaseError> {
        self.git_stdout(run, &["rev-parse", "HEAD"])
    }

    fn working_tree_clean(&mut self, run: &mut Run<'_>) -> Result<bool, ReleaseError> {
        let status = self.git(run, &["status", "--porcelain=v1", "--untracked-files=all"])?;
        if !status.status.success() {
            return Err(command_error("`git status`", &status));
        }
        Ok(status.stdout.is_empty())
    }

    fn local_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError> {
        // `show-ref --verify` reports a missing ref as exit 1 and nothing else;
        // any other failure is an unreadable tag, never an absence.
        let reference = format!("refs/tags/{tag}");
        let present = self.git(run, &["show-ref", "--verify", "--quiet", &reference])?;
        if present.status.code() == Some(1) {
            return Ok(None);
        }
        if !present.status.success() {
            return Err(command_error("`git show-ref`", &present));
        }
        let commit = self.git_stdout(run, &["rev-parse", &format!("{reference}^{{commit}}")])?;
        Ok(Some(commit))
    }

    fn remote_tag_commit(
        &mut self,
        run: &mut Run<'_>,
        tag: &str,
    ) -> Result<Option<String>, ReleaseError> {
        // `ls-remote` with a ref pattern does not always report the peeled
        // `^{}` entry, and an annotated tag's own object is not its commit, so
        // list the tags and prefer the peeled line for this tag.
        let output = self.git(run, &["ls-remote", "--tags", &self.remote])?;
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
        run: &mut Run<'_>,
        tag: &str,
        commit: &str,
        message: &str,
    ) -> Result<(), ReleaseError> {
        let output = self.git(run, &["tag", "-a", tag, "-m", message, commit])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(&format!("`git tag -a {tag}`"), &output))
        }
    }

    fn push_tag(&mut self, run: &mut Run<'_>, tag: &str) -> Result<(), ReleaseError> {
        let reference = format!("refs/tags/{tag}");
        let output = self.git(run, &["push", &self.remote, &reference])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(
                &format!("`git push {remote} {reference}`", remote = self.remote),
                &output,
            ))
        }
    }

    fn push_commit(
        &mut self,
        run: &mut Run<'_>,
        commit: &str,
        branch: &str,
    ) -> Result<(), ReleaseError> {
        let reference = format!("{commit}:refs/heads/{branch}");
        let output = self.git(run, &["push", &self.remote, &reference])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error(
                &format!("`git push {remote} {reference}`", remote = self.remote),
                &output,
            ))
        }
    }

    fn commit_paths(
        &mut self,
        run: &mut Run<'_>,
        paths: &[PathBuf],
        message: &str,
    ) -> Result<String, ReleaseError> {
        let mut args = vec!["add".into(), "--".into()];
        args.extend(paths.iter().filter_map(|path| {
            let relative = path.strip_prefix(&self.repo_root).unwrap_or(path);
            (!relative.as_os_str().is_empty()).then(|| relative.as_os_str().to_owned())
        }));
        let output = self.git_args(run, args)?;
        if !output.status.success() {
            return Err(command_error("`git add`", &output));
        }
        let output = self.git(run, &["commit", "-m", message])?;
        if !output.status.success() {
            return Err(command_error("`git commit`", &output));
        }
        self.head_commit(run)
    }

    fn discard_staged(&mut self, run: &mut Run<'_>, paths: &[PathBuf]) -> Result<(), ReleaseError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = ["restore", "--source=HEAD", "--staged", "--worktree", "--"]
            .map(OsString::from)
            .to_vec();
        args.extend(paths.iter().map(|path| {
            path.strip_prefix(&self.repo_root)
                .unwrap_or(path)
                .as_os_str()
                .to_owned()
        }));
        let output = self.git_args(run, args)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("`git restore`", &output))
        }
    }

    fn run_task(
        &mut self,
        run: &mut Run<'_>,
        request: &TaskRequest,
    ) -> Result<TaskResponse, ReleaseError> {
        let TaskRunner::CurrentBinary = self.task_runner;
        let binary = std::env::current_exe().map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to resolve this Effigy binary for `{}`: {error}",
                request.label
            ))
        })?;
        let output = self.execute(
            run,
            vec![
                binary.into_os_string(),
                "--repo".into(),
                request.cwd.clone().into_os_string(),
                request.task.clone().into(),
            ],
            &request.cwd,
            &request.env,
            &request.label,
        )?;
        Ok(TaskResponse {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            success: output.status.success(),
        })
    }

    fn operator(&mut self, run: &mut Run<'_>) -> Option<String> {
        if let Ok(name) = self.git_stdout(run, &["config", "user.name"]) {
            if !name.is_empty() {
                return Some(name);
            }
        }
        std::env::var("USER").ok().filter(|user| !user.is_empty())
    }
}

fn find_program(name: &str) -> Result<PathBuf, ReleaseError> {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find_map(|path| {
            let metadata = std::fs::metadata(&path).ok()?;
            (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .then(|| path.canonicalize().ok())
                .flatten()
        })
        .ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release executable `{name}` is unavailable on PATH"
            ))
        })
}
