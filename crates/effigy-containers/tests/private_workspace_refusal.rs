//! Exercise the public entrypoint from outside the library: an unavailable
//! private capability must not prepare interpolation or execute a backend.
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use effigy_containers::exec::{run_backend_invocation_private_until, ContainerExecError};

const REFUSAL: &str =
    "private workspace Compose is unavailable until the reviewed same-Env capability lands";
const SENTINEL: &str = "private-workspace-refusal-sentinel-c194";
const CHILD_ROOT: &str = "EFFIGY_PRIVATE_REFUSAL_TEST_ROOT";

#[test]
fn public_private_workspace_refuses_before_file_or_backend_child() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        assert_refusal_without_effects(Path::new(&root));
        return;
    }

    // The library uses HOME in this external test binary. Isolate it in a
    // child test process instead of changing the test runner's environment.
    let fixture = tempfile::tempdir().expect("private refusal fixture");
    let home = fixture.path().join("home");
    fs::create_dir(&home).expect("private home");
    let status = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "public_private_workspace_refuses_before_file_or_backend_child",
            "--nocapture",
        ])
        .env(CHILD_ROOT, fixture.path())
        .env("HOME", &home)
        .status()
        .expect("run isolated public refusal proof");
    assert!(status.success(), "public refusal proof failed: {status}");
}

fn assert_refusal_without_effects(root: &Path) {
    let home = root.join("home");
    assert_eq!(effigy_containers::effigy_home(), Some(home.join(".effigy")));
    let env = [("DATABASE_URL".to_owned(), OsString::from(SENTINEL))];
    let cases: &[(&str, &[&str], bool)] = &[
        ("colima", &["compose", "up", "-d"], false),
        ("docker", &["compose", "up", "--detach"], false),
        ("colima", &["compose", "up", "-d"], true),
        ("colima", &["down", "up", "-d"], false),
        ("colima", &[], false),
    ];
    for (index, (name, args, expired)) in cases.iter().enumerate() {
        let bin = root.join(format!("bin-{index}"));
        fs::create_dir(&bin).expect("private backend directory");
        let program = bin.join(name);
        fs::write(&program, "#!/bin/sh\nprintf executed > backend-started\n")
            .expect("backend marker program");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700))
            .expect("backend executable mode");
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let deadline = if *expired {
            Instant::now() - Duration::from_secs(1)
        } else {
            Instant::now() + Duration::from_secs(5)
        };
        let error = run_backend_invocation_private_until(
            root,
            program.as_os_str(),
            &args,
            "workspace apply",
            &env,
            deadline,
        )
        .expect_err("private execution must refuse without a capability");
        assert_eq!(
            error.to_string(),
            format!("failed to launch `workspace apply`: {REFUSAL}")
        );
        assert!(!format!("{error:?}").contains(SENTINEL));
        match error {
            ContainerExecError::Launch { command, error } => {
                assert_eq!(command, "workspace apply");
                assert_eq!(error.kind(), ErrorKind::Unsupported);
                assert_eq!(error.to_string(), REFUSAL);
            }
            other => panic!("expected fixed unavailable classification, got {other:?}"),
        }
        // A Colima interpolation attempt would create this directory even if
        // its file were immediately dropped. The backend itself writes the
        // other marker if any fallback executes it.
        assert!(!home.join(".effigy").exists(), "interpolation was prepared");
        assert!(!root.join("backend-started").exists(), "backend executed");
        assert!(!root.join(".effigy").exists(), "checkout state was created");
    }
}
