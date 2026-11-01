//! Direct-child routes refuse VM Compose secret delivery before spawn.

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

fn home_with_fake_fixture(root: &Path) -> PathBuf {
    let home = root.join("home");
    fs::create_dir_all(&home).expect("home");
    home
}

#[test]
fn public_private_route_refuses_before_interpolation_or_child() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let program = fake_colima(root.path(), "private", "exit 0");
    let error = crate::with_test_effigy_home(&home, || {
        super::implementation::run_backend_invocation_private_until(
            root.path(),
            program.as_os_str(),
            &compose_args(),
            "workspace apply",
            &secret_env(),
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .expect_err("private path must refuse")
    });
    match error {
        super::implementation::ContainerExecError::Launch { error, .. } => {
            assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
            assert_eq!(
                error.to_string(),
                "private workspace Compose is unavailable until the reviewed same-Env capability lands"
            );
        }
        other => panic!("unexpected private refusal: {other}"),
    }
    assert!(!root.path().join("private.observed").exists());
    assert!(fs::read_dir(&home).unwrap().next().is_none());
}

#[test]
fn direct_routes_refuse_secret_bearing_vm_compose_before_spawn() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = home_with_fake_fixture(root.path());
    let env = secret_env();
    for route in ["capture", "timeout", "detached"] {
        let program = fake_colima(root.path(), route, "exit 0")
            .to_string_lossy()
            .into_owned();
        let result = crate::with_test_effigy_home(&home, || match route {
            "capture" => {
                run_command_capture_os_with_env(root.path(), &program, &compose_args(), route, &env)
            }
            "timeout" => run_command_capture_with_timeout_and_env(
                root.path(),
                &program,
                &["compose", "up"],
                route,
                Duration::from_secs(2),
                &env,
            ),
            _ => run_detached_command_with_env(root.path(), &program, &compose_args(), route, &env),
        });
        assert!(result.is_err(), "{route} must refuse");
        assert!(!root.path().join(format!("{route}.observed")).exists());
        assert_no_sentinel_persists(&home, route);
    }
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
