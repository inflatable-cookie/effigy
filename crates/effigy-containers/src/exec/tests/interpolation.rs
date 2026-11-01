//! Child-route proofs for the VM-hosted Compose interpolation file. A fake
//! `colima` records the directory and file modes it was handed, then runs its
//! route-specific tail. The fixture secret is a sentinel; after each route
//! returns, no file under the Effigy home may hold its bytes, and the next
//! invocation names a fresh file.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::process::{
    run_command_capture_os_with_env, run_command_capture_with_timeout_and_env,
    run_detached_command_with_env,
};

/// A fixture secret. It exists only in these tests and in the fake child.
const SENTINEL: &str = "effigy-fixture-sentinel-7f3a9c";

/// A `colima` fake that records `ls -ld` of the env file's directory and `ls -l`
/// of the file, then runs `tail`. The log lives beside the home, never inside it.
fn fake_colima(root: &Path, name: &str, tail: &str) -> PathBuf {
    let bin = root.join(name);
    fs::create_dir_all(&bin).expect("fake bin dir");
    let log = root.join(format!("{name}.observed"));
    let script = format!(
        "#!/bin/sh\n\
         f=\"\"\n\
         prev=\"\"\n\
         for a in \"$@\"; do\n\
           if [ \"$prev\" = \"--env-file\" ]; then f=\"$a\"; fi\n\
           prev=\"$a\"\n\
         done\n\
         {{ ls -ld \"$(dirname \"$f\")\"; ls -l \"$f\"; }} >> '{log}'\n\
         {tail}\n",
        log = log.display()
    );
    let path = bin.join("colima");
    fs::write(&path, script).expect("fake colima");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("fake colima mode");
    path
}

fn observed(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(format!("{name}.observed"))).unwrap_or_default()
}

fn compose_args() -> Vec<OsString> {
    ["compose", "-f", "compose.yml", "up", "-d"]
        .into_iter()
        .map(OsString::from)
        .collect()
}

fn secret_env() -> Vec<(String, OsString)> {
    vec![(
        "DATABASE_URL".to_owned(),
        OsString::from(format!("postgres://fixture/{SENTINEL}")),
    )]
}

/// Every file under the home, so a persisted sentinel is found wherever it lands.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// After a route returns: the compose-env directory is empty of files and no
/// file under the home holds the sentinel bytes.
fn assert_no_sentinel_persists(home: &Path, route: &str) {
    let compose_env = home.join("compose-env");
    let names = fs::read_dir(&compose_env)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(
        names, 0,
        "{route}: no interpolation file survives its child"
    );
    for file in files_under(home) {
        let bytes = fs::read(&file).unwrap_or_default();
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|window| window == SENTINEL.as_bytes()),
            "{route}: sentinel bytes persist in {}",
            file.display()
        );
    }
}

/// The child saw a 0700 directory and a 0600 single-link file while it ran.
fn assert_child_saw_private_modes(root: &Path, name: &str, route: &str) {
    let log = observed(root, name);
    assert!(
        log.contains("drwx------"),
        "{route}: directory 0700 while the child ran:\n{log}"
    );
    assert!(
        log.contains("-rw-------"),
        "{route}: file 0600 while the child ran:\n{log}"
    );
}

fn home_with_fake_fixture(root: &Path) -> PathBuf {
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home");
    home
}

#[test]
fn private_route_refuses_before_interpolation_or_child() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "private", "exit 0");
    let error = crate::with_test_effigy_home(&home, || {
        super::process::run_private_detached_command(
            root.path(),
            &program.to_string_lossy(),
            &compose_args(),
            "workspace apply",
            &secret_env(),
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .expect_err("private path must refuse")
    });
    assert!(error.to_string().contains(
        "private workspace Compose is unavailable until the reviewed same-Env capability lands"
    ));
    assert!(observed(root.path(), "private").is_empty());
    assert!(files_under(&home).is_empty());
}

#[test]
fn ordinary_route_success_removes_the_file_and_regenerates_for_the_next_run() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "ok", "exit 0");
    let program = program.to_string_lossy().into_owned();
    let env = secret_env();
    let args = compose_args();
    crate::with_test_effigy_home(&home, || {
        run_command_capture_os_with_env(root.path(), &program, &args, "ordinary success", &env)
            .expect("the ordinary route succeeds");
        run_command_capture_os_with_env(root.path(), &program, &args, "ordinary success", &env)
            .expect("the next invocation succeeds");
    });
    assert_child_saw_private_modes(root.path(), "ok", "ordinary success");
    assert_eq!(
        observed(root.path(), "ok").matches("-rw-------").count(),
        2,
        "each invocation names its own file"
    );
    assert_no_sentinel_persists(&home, "ordinary success");
}

#[test]
fn ordinary_route_failure_removes_the_file() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "fail", "exit 3")
        .to_string_lossy()
        .into_owned();
    let env = secret_env();
    let result = crate::with_test_effigy_home(&home, || {
        run_command_capture_os_with_env(
            root.path(),
            &program,
            &compose_args(),
            "ordinary failure",
            &env,
        )
    });
    assert!(result.is_err(), "the failing child is reported");
    assert_child_saw_private_modes(root.path(), "fail", "ordinary failure");
    assert_no_sentinel_persists(&home, "ordinary failure");
}

#[test]
fn ordinary_route_timeout_removes_the_file_after_the_child_is_stopped() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "hang", "sleep 30")
        .to_string_lossy()
        .into_owned();
    let env = secret_env();
    let result = crate::with_test_effigy_home(&home, || {
        run_command_capture_with_timeout_and_env(
            root.path(),
            &program,
            &["compose", "-f", "compose.yml", "up"],
            "ordinary timeout",
            Duration::from_secs(1),
            &env,
        )
    });
    assert!(result.is_err(), "the timed-out child is reported");
    assert_child_saw_private_modes(root.path(), "hang", "ordinary timeout");
    assert_no_sentinel_persists(&home, "ordinary timeout");
}

#[test]
fn detached_route_success_removes_the_file() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "detached", "exit 0")
        .to_string_lossy()
        .into_owned();
    let env = secret_env();
    crate::with_test_effigy_home(&home, || {
        run_detached_command_with_env(root.path(), &program, &compose_args(), "detached", &env)
            .expect("the detached route succeeds");
    });
    assert_child_saw_private_modes(root.path(), "detached", "detached");
    assert_no_sentinel_persists(&home, "detached");
}

#[test]
fn spawn_failure_removes_the_file_before_the_error_returns() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = root
        .path()
        .join("missing")
        .join("colima")
        .to_string_lossy()
        .into_owned();
    let env = secret_env();
    let result = crate::with_test_effigy_home(&home, || {
        run_command_capture_os_with_env(
            root.path(),
            &program,
            &compose_args(),
            "spawn failure",
            &env,
        )
    });
    assert!(result.is_err(), "a program that cannot spawn is reported");
    assert_no_sentinel_persists(&home, "spawn failure");
}
