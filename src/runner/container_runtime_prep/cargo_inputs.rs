//! Refreshing the container's view of the Cargo inputs the host changed, so an
//! incremental build never reuses an object file for edited source.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

/// Run one preparation command through the verified container.
///
/// `tail` is the `compose exec` tail only (`exec … <service> <command>`): the
/// plan builder spells the single `compose -f … -p …` prefix, so a caller can
/// never hand the executor a second one.
pub(super) fn run_verified_prep_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    tail: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        tail.to_vec(),
        effigy_containers::ContainerAction::Exec,
        label,
    )
    .map_err(RunnerError::from)?;
    crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )
}

pub(super) fn refresh_changed_cargo_inputs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    refresh_changed_cargo_inputs_using(repo_root, |changed| {
        for batch in changed.chunks(128) {
            let tail = touch_invocation_tail(policy, working_dir, batch);
            run_verified_prep_capture(
                repo_root,
                policy,
                binding,
                &tail,
                "refresh changed Cargo inputs",
            )
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot refresh changed Cargo inputs in selected checkout: {error}"
                ))
            })?;
        }
        Ok(())
    })
}

/// The one container command a refresh batch runs, as the `compose exec` tail
/// the plan builder prefixes: `exec -T -w <working dir> <service> touch -c --
/// <paths>`. The paths are the host's own path bytes, so the container
/// refreshes exactly the files the host changed.
fn touch_invocation_tail(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    batch: &[PathBuf],
) -> Vec<OsString> {
    let mut tail = vec![
        OsString::from("exec"),
        OsString::from("-T"),
        OsString::from("-w"),
        working_dir.as_os_str().to_owned(),
        OsString::from(policy.primary_service.as_str()),
        OsString::from("touch"),
        OsString::from("-c"),
        OsString::from("--"),
    ];
    tail.extend(batch.iter().map(|path| path.as_os_str().to_owned()));
    tail
}

pub(super) fn refresh_changed_cargo_inputs_using(
    repo_root: &Path,
    touch_in_container: impl FnOnce(&[PathBuf]) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let changed = changed_cargo_inputs(repo_root)?;
    if changed.is_empty() {
        return Ok(());
    }
    touch_in_container(&changed)
}

/// Every Cargo input the host has changed or added since `HEAD`.
///
/// `git diff HEAD` only names tracked files, so a source file that is new and
/// untracked would otherwise keep the container's stale view of that path; the
/// untracked set is included for the same reason. Names are carried as path
/// bytes, never as lossy text: a name that cannot be decoded is still the name
/// the container must refresh, so `touch` receives the exact bytes and a
/// platform that cannot carry them refuses instead of refreshing the wrong
/// path.
fn changed_cargo_inputs(repo_root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    let mut changed = git_paths(repo_root, &["diff", "--name-only", "-z", "HEAD", "--"])?;
    changed.extend(git_paths(
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?);
    changed.retain(|path| is_cargo_input(path) && repo_root.join(path).is_file());
    changed.sort();
    changed.dedup();
    Ok(changed)
}

fn git_paths(repo_root: &Path, args: &[&str]) -> Result<Vec<PathBuf>, RunnerError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| {
            RunnerError::task_invocation(format!("cannot list changed Cargo inputs: {error}"))
        })?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "cannot list changed Cargo inputs in {}: {}",
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(path_from_bytes)
        .collect()
}

fn is_cargo_input(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
        || path.file_name().is_some_and(|name| {
            name == std::ffi::OsStr::new("Cargo.toml") || name == std::ffi::OsStr::new("Cargo.lock")
        })
}

/// A repository path is carried as its own bytes.
#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, RunnerError> {
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, RunnerError> {
    std::str::from_utf8(bytes).map(PathBuf::from).map_err(|_| {
        RunnerError::task_invocation(
            "cannot refresh changed Cargo inputs: a repository path is not valid UTF-8 on this platform",
        )
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::super::tests::temp_test_dir;
    use super::refresh_changed_cargo_inputs_using;

    fn init_repo(root: &std::path::Path) {
        let run_git = |args: &[&str]| {
            assert!(std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        };
        run_git(&["init", "-q"]);
        fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 1 }\n").unwrap();
        run_git(&["add", "lib.rs"]);
        run_git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ]);
    }

    #[test]
    fn host_edit_refreshes_cargo_input_before_container_check() {
        host_edit_refreshes_cargo_input_before_container_check_using(|| {});
    }

    fn host_edit_refreshes_cargo_input_before_container_check_using(after_serial: impl FnOnce()) {
        let _locks =
            crate::runner::owned_children::hold_signal_proof_test_locks_after_serial(after_serial);
        let root = temp_test_dir("cargo-host-edit");
        init_repo(&root);
        fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 2 }\n").unwrap();
        let source = fs::File::open(root.join("lib.rs")).unwrap();
        source
            .set_modified(UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        let artifact = root.join("artifact");
        fs::write(&artifact, "old").unwrap();
        fs::File::open(&artifact)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(2))
            .unwrap();
        refresh_changed_cargo_inputs_using(&root, |changed| {
            assert_eq!(changed, &[PathBuf::from("lib.rs")]);
            source.set_modified(SystemTime::now()).unwrap();
            Ok(())
        })
        .unwrap();
        assert!(
            fs::metadata(root.join("lib.rs"))
                .unwrap()
                .modified()
                .unwrap()
                > fs::metadata(artifact).unwrap().modified().unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_untracked_new_cargo_input_is_refreshed_too() {
        let root = temp_test_dir("cargo-untracked-input");
        init_repo(&root);
        fs::write(root.join("new.rs"), "pub fn added() -> u8 { 3 }\n").unwrap();
        fs::write(root.join("notes.md"), "not a Cargo input\n").unwrap();
        refresh_changed_cargo_inputs_using(&root, |changed| {
            assert_eq!(changed, &[PathBuf::from("new.rs")]);
            Ok(())
        })
        .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_refresh_batch_plan_carries_one_compose_prefix_and_the_touch_command() {
        let root = temp_test_dir("cargo-refresh-plan");
        let compose = root.join("docker-compose.yml");
        fs::write(&compose, "services: {}\n").unwrap();
        let policy = crate::runner::test_support::effective_container_policy(
            "stack",
            "demo-stack",
            "workspace",
            compose,
        );
        let tail = super::touch_invocation_tail(
            &policy,
            std::path::Path::new("/workspace"),
            &[PathBuf::from("lib.rs")],
        );
        let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
            &root,
            &policy,
            tail,
            effigy_containers::ContainerAction::Exec,
            "refresh changed Cargo inputs",
        )
        .expect("preparation plan");

        // The plan is what the executor is handed: it must carry the Compose
        // prefix once, never twice.
        let rendered = plan
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            rendered
                .iter()
                .filter(|token| token.as_str() == "compose")
                .count(),
            1,
            "the plan carries exactly one Compose prefix: {rendered:?}"
        );
        let exec = rendered
            .iter()
            .position(|token| token == "exec")
            .expect("compose exec");
        assert_eq!(
            &rendered[exec..],
            [
                "exec",
                "-T",
                "-w",
                "/workspace",
                "workspace",
                "touch",
                "-c",
                "--",
                "lib.rs"
            ]
        );

        // The executor derives the arguments it runs from these fields, so the
        // executed invocation is pinned here rather than restated.
        let parsed = crate::runner::exec_command::parse_compose_exec_args(&plan.args)
            .expect("the plan parses as the executor reads it");
        assert_eq!(parsed.service, "workspace");
        assert_eq!(
            parsed.working_dir.as_deref(),
            Some(std::ffi::OsStr::new("/workspace"))
        );
        assert!(!parsed.tty);
        assert_eq!(
            parsed.command,
            [
                OsString::from("touch"),
                OsString::from("-c"),
                OsString::from("--"),
                OsString::from("lib.rs"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// A Cargo input is named by its own bytes. Decoding the name lossily would
    /// refresh a different file than the one the container compiles, so the
    /// decoder is the thing under test: macOS volumes refuse to create such a
    /// name, but the bytes git reports must still survive unchanged.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_is_carried_as_its_own_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let bytes = b"odd\xff.rs";
        let path = super::path_from_bytes(bytes).expect("a Unix path carries arbitrary bytes");
        assert_eq!(path.as_os_str().as_bytes(), bytes);
        assert!(super::is_cargo_input(&path));
        assert_ne!(
            path.as_os_str().to_string_lossy(),
            String::from_utf8(bytes.to_vec()).unwrap_or_default(),
            "the name is never round-tripped through text"
        );
    }
}
