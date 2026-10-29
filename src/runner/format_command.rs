//! `effigy fmt`: format an explicit file set with the nearest rustfmt config.
//!
//! `cargo fmt --manifest-path` reads a Cargo workspace, so a repository that
//! path-links another checkout can reformat crates outside the change. This
//! route takes the files themselves — the list `git diff --name-only` produces
//! — and runs `rustfmt` once per file with `skip_children`, so only the named
//! files are written and no linked workspace is followed.
//!
//! Each file is formatted with rustfmt's own configuration discovery, run from
//! the file's own directory, so the nearest `rustfmt.toml` applies.

use super::task_engine;
use effigy_kernel::{
    ctx::OpenReason,
    custody::{Lifetime, ProcessSpec, Terminal},
    observe::Observed,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_cli::{FmtArgs, FmtSubcommand};

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

/// The edition every format task in this repository pins today. rustfmt
/// defaults to 2015 and would format 2018+ source differently.
const RUST_EDITION: &str = "2021";

pub(super) fn run_fmt(args: FmtArgs) -> Result<String, RunnerError> {
    let repo_root = resolve_active_repo_root(args.repo_override)?.resolved_root;
    let (files, check) = match args.subcommand {
        FmtSubcommand::Rust { files } => (files, false),
        FmtSubcommand::RustCheck { files } => (files, true),
    };
    format_files(&repo_root, &files, check)
}

/// Format (or check) the named files, which must all live in `repo_root`.
fn format_files(repo_root: &Path, files: &[PathBuf], check: bool) -> Result<String, RunnerError> {
    let repo_root = std::fs::canonicalize(repo_root).map_err(|error| {
        RunnerError::task_invocation(format!(
            "`effigy fmt rust` could not resolve the repository root {}: {error}",
            repo_root.display()
        ))
    })?;
    let files = resolve_rust_files(&repo_root, files)?;

    let mut changed = Vec::new();
    for file in &files {
        if run_rustfmt(&repo_root, file, check)? {
            changed.push(display(&repo_root, file));
        }
    }

    if check {
        if changed.is_empty() {
            return Ok(format!(
                "Rust formatting clean: {} file(s) already formatted",
                files.len()
            ));
        }
        let mut lines = vec![format!(
            "Rust formatting needs attention: {} of {} file(s)",
            changed.len(),
            files.len()
        )];
        lines.extend(
            changed
                .iter()
                .map(|file| format!("  needs formatting: {file}")),
        );
        return Err(RunnerError::task_invocation(lines.join("\n")));
    }

    let mut lines = vec![format!(
        "Rust formatting applied: {} file(s) with their nearest rustfmt config",
        files.len()
    )];
    lines.extend(
        files
            .iter()
            .map(|file| format!("  - {}", display(&repo_root, file))),
    );
    Ok(lines.join("\n"))
}

fn display(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Resolve the requested files to absolute, deduplicated, in-repo paths.
///
/// The explicit file set is the whole safety surface: every path must be an
/// existing `.rs` file inside the target repository, so neither a stray
/// argument nor a path-linked checkout outside it can be reformatted.
fn resolve_rust_files(
    canonical_root: &Path,
    files: &[PathBuf],
) -> Result<Vec<PathBuf>, RunnerError> {
    let invocation_cwd = super::command_context::current_working_dir()?;

    let mut resolved: Vec<PathBuf> = Vec::new();
    for file in files {
        let candidate = if file.is_absolute() {
            file.clone()
        } else {
            invocation_cwd.join(file)
        };
        let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
            RunnerError::task_invocation(format!(
                "`effigy fmt rust` cannot read `{}`: {error}",
                file.display()
            ))
        })?;
        if canonical
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("rs")
        {
            return Err(RunnerError::task_invocation(format!(
                "`effigy fmt rust` only formats Rust source files, and `{}` is not a `.rs` file",
                file.display()
            )));
        }
        if !canonical.starts_with(canonical_root) {
            return Err(RunnerError::task_invocation(format!(
                "`effigy fmt rust` stays inside the target repository {}; `{}` resolves to {} outside it",
                canonical_root.display(),
                file.display(),
                canonical.display()
            )));
        }
        if !resolved.iter().any(|existing| existing == &canonical) {
            resolved.push(canonical);
        }
    }

    if resolved.is_empty() {
        return Err(RunnerError::task_invocation(
            "`effigy fmt rust` requires at least one file path".to_owned(),
        ));
    }
    Ok(resolved)
}

/// Run rustfmt for one file. Returns whether the file changed (or, in check
/// mode, would change).
fn run_rustfmt(repo_root: &Path, file: &Path, check: bool) -> Result<bool, RunnerError> {
    let directory = file.parent().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`effigy fmt rust` cannot format {}: it has no parent directory",
            display(repo_root, file)
        ))
    })?;
    let file_name = file.file_name().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`effigy fmt rust` cannot format {}: it has no file name",
            display(repo_root, file)
        ))
    })?;
    let mut argv = vec![
        effigy_containers::compose::resolve_host_cli_program("rustfmt"),
        "--edition".into(),
        RUST_EDITION.into(),
        "--config".into(),
        "skip_children=true".into(),
    ];
    if check {
        argv.push("--check".into());
    }
    argv.push(file_name.to_os_string());
    // Formatting mutates explicit checkout files, so use the task engine's
    // memory custody rather than a read-only probe or an unowned child.
    let mut run = task_engine::TaskRun::standalone_foreground(repo_root, "fmt rust")?;
    let result = (|| -> Result<(i32, Vec<u8>, Vec<u8>), RunnerError> {
        let spec = ProcessSpec {
            argv,
            cwd: directory.into(),
            env: task_engine::process_environment(directory, Vec::new())?,
            grace: Duration::from_secs(3),
            lifetime: Lifetime::Open(OpenReason::Shell),
            terminal: Terminal::None,
            label: "rustfmt".into(),
        };
        let mut process = run
            .spawn(spec, false)
            .map_err(|error| RunnerError::task_invocation(format!("rustfmt custody: {error:?}")))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        loop {
            let (out, err) = process.take_stream_output();
            stdout.extend(out);
            stderr.extend(err);
            if let Observed::Verified(exit, _) = process.poll() {
                let closed = process.stop(&task_engine::cleanup_context(Duration::from_secs(30)));
                if !matches!(closed.group, Observed::Verified(..) | Observed::Absent(_))
                    || !closed.residue.is_empty()
                    || closed.capture != effigy_kernel::custody::CaptureState::Complete
                {
                    return Err(RunnerError::task_invocation(
                        "rustfmt closure or output is unresolved",
                    ));
                }
                let (out, err) = process.take_stream_output();
                stdout.extend(out);
                stderr.extend(err);
                run.complete(&process, exit).map_err(|e| {
                    RunnerError::task_invocation(format!("rustfmt completion: {e:?}"))
                })?;
                return Ok((exit.code, stdout, stderr));
            }
            if !task_engine::routed_signals().is_empty() {
                return Err(RunnerError::task_invocation("rustfmt interrupted"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    let code = result.as_ref().map(|r| r.0).unwrap_or(1);
    let warnings = run.close(task_engine::workload_for_exit(code));
    if !warnings.is_empty() {
        return Err(RunnerError::task_invocation(warnings.join("; ")));
    }
    let (code, stdout, stderr) = result?;
    if code == 0 {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
    if stderr.is_empty() && !stdout.is_empty() && (check || stdout.contains("Diff in")) {
        // rustfmt reports each difference on stdout and exits non-zero.
        return Ok(true);
    }
    Err(RunnerError::task_invocation(format!(
        "`effigy fmt rust` failed for {}: {}",
        display(repo_root, file),
        if stderr.is_empty() { stdout } else { stderr }
    )))
}

#[cfg(test)]
#[path = "format_command/tests.rs"]
mod tests;
