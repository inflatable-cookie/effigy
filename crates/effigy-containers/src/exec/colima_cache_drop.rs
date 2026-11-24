//! Best-effort release of Colima guest VirtioFS dentry and inode references.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::compose::{resolve_compose_backend_for_repo, ComposeBackend};
use crate::EffectiveContainerPolicy;

use super::implementation::ContainerExecError;
use super::process::run_command_capture_allow_failure_with_deadline;

const CACHE_DROP_TIMEOUT: Duration = Duration::from_secs(5);
const DROP_CACHES_SCRIPT: &str = "cd / && sync && echo 2 > /proc/sys/vm/drop_caches";

/// Ask a Colima guest to drop its dentry and inode caches within `deadline`.
/// A non-zero Colima exit remains an observed output for the caller to handle.
pub fn drop_colima_virtiofs_dentry_caches(
    profile: &str,
    deadline: Instant,
) -> Result<std::process::Output, ContainerExecError> {
    let args = [
        "ssh",
        "--profile",
        profile,
        "--",
        "sudo",
        "-n",
        "sh",
        "-c",
        DROP_CACHES_SCRIPT,
    ];
    // Colima passes its host cwd to Lima's guest shell. The helper itself
    // must leave the shared worktree before asking Linux to reclaim it.
    run_command_capture_allow_failure_with_deadline(
        Path::new("/"),
        "colima",
        &args,
        "drop Colima guest VirtioFS caches",
        Some(deadline),
    )
}

/// Try to release Colima guest VirtioFS references and report observed macOS
/// VM handles on stderr. This maintenance does not change the teardown outcome.
pub fn release_colima_virtiofs_references_after_teardown(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) {
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return;
    }
    let deadline = Instant::now() + CACHE_DROP_TIMEOUT;
    let _ = drop_colima_virtiofs_dentry_caches(&policy.profile, deadline);
    #[cfg(target_os = "macos")]
    super::colima_virtiofs_handles::warn_about_surviving_handles(repo_root, &policy.profile);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{exec::shutdown_container, policy::model::test_policy, test_env_lock};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct EnvRestore(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl EnvRestore {
        fn set(values: &[(&'static str, std::ffi::OsString)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            for (key, value) in values {
                std::env::set_var(key, value);
            }
            Self(previous)
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn fake_colima(root: &Path, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("fake bin");
        let log = root.join("colima-args.log");
        let path = bin.join("colima");
        fs::write(&path, body.replace("{log}", &log.display().to_string()))
            .expect("write fake colima");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("fake colima mode");
        (bin, log)
    }

    fn fixture_env(body: &str) -> (tempfile::TempDir, EnvRestore, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("fixture dir");
        let root = dir.path().canonicalize().expect("canonical fixture");
        let (bin, _) = fake_colima(&root, body);
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let env = EnvRestore::set(&[
            ("PATH", path.into()),
            ("LIMA_HOME", root.join("lima").into_os_string()),
            ("EFFIGY_COMPOSE_BACKEND", "colima".into()),
        ]);
        (dir, env, root)
    }

    #[test]
    fn drop_helper_invokes_exact_colima_ssh_command() {
        let _lock = test_env_lock();
        let (_dir, _env, root) =
            fixture_env("#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$@\" > '{log}'\nexit 0\n");
        let mut policy = test_policy(&root);
        policy.profile = "worktree-profile".to_owned();

        release_colima_virtiofs_references_after_teardown(&root, &policy);

        let args = fs::read_to_string(root.join("colima-args.log")).expect("fake arguments");
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            [
                "/",
                "ssh",
                "--profile",
                "worktree-profile",
                "--",
                "sudo",
                "-n",
                "sh",
                "-c",
                "cd / && sync && echo 2 > /proc/sys/vm/drop_caches",
            ]
        );
    }

    #[test]
    fn docker_backend_is_a_strict_noop() {
        let _lock = test_env_lock();
        let (_dir, _env, root) = fixture_env("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}'\n");
        std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");
        let policy = test_policy(&root);

        release_colima_virtiofs_references_after_teardown(&root, &policy);

        assert!(!root.join("colima-args.log").exists());
    }

    #[test]
    fn failing_cache_drop_does_not_fail_container_shutdown() {
        let _lock = test_env_lock();
        let (_dir, _env, root) = fixture_env(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{log}'\nif [ \"$1\" = ssh ]; then exit 7; fi\nexit 0\n",
        );
        let policy = test_policy(&root);

        shutdown_container(&root, &policy).expect("compose teardown succeeds");

        let args = fs::read_to_string(root.join("colima-args.log")).expect("fake arguments");
        assert!(args.contains(
            "ssh\n--profile\neffigy\n--\nsudo\n-n\nsh\n-c\ncd / && sync && echo 2 > /proc/sys/vm/drop_caches\n"
        ));
    }
}
