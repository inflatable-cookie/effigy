//! A service that really changed is replaced reliably (effigy#362).
//!
//! `nerdctl compose` keeps its own name store. When it still holds a container
//! this project owns and the recreate loses on that name, Effigy removes it and
//! retries the create once. Another project's container is reported, never
//! removed. The runtime is a strict fake that records every call.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::implementation::{run_compose_invocation_streaming_with_env, ContainerExecError};

const PROJECT: &str = "app-dev-wt-abcdef012345";
const ID: &str = "c0ffee1234567890c0ffee1234567890c0ffee1234567890c0ffee1234567890";

/// `up` fails on the name store while `conflict` exists, `inspect` reports the
/// container's project label and `rm -f` frees the name.
const FAKE_RUNTIME: &str = r###"#!/usr/bin/env python3
import json, sys
from pathlib import Path
fixture = Path("__FIXTURE__")
args = sys.argv[1:]
if "--" in args:
    args = args[args.index("--") + 1:]
with (fixture / "calls.jsonl").open("a") as log:
    log.write(json.dumps(args) + "\n")
if "inspect" in args:
    label = (fixture / "label").read_text().strip()
    print(json.dumps([{"Id": args[-1], "Config": {"Labels": {"com.docker.compose.project": label}}}]))
elif "rm" in args:
    (fixture / "conflict").unlink(missing_ok=True)
elif "up" in args:
    if (fixture / "conflict").exists():
        sys.stderr.write('msg="name-store error: name \\"__PROJECT__-app-1\\" is already used by ID __ID__"\n')
        sys.exit(1)
else:
    sys.exit(90)
"###;

/// A private checkout whose `docker` is the fake, labelled `project`.
fn fixture(project: &str) -> PathBuf {
    let root = tempfile::tempdir().expect("fixture").keep();
    std::fs::create_dir(root.join("bin")).unwrap();
    let runtime = root.join("bin/docker");
    std::fs::write(
        &runtime,
        FAKE_RUNTIME
            .replace("__FIXTURE__", &root.display().to_string())
            .replace("__PROJECT__", PROJECT)
            .replace("__ID__", ID),
    )
    .unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(root.join("label"), project).unwrap();
    std::fs::write(root.join("conflict"), "").unwrap();
    std::fs::write(root.join("compose.yml"), "services: {}\n").unwrap();
    root
}

/// The `up` calls the fake recorded, or how many mention a container removal.
fn calls(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("calls.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn count(root: &Path, needle: &str) -> usize {
    calls(root)
        .iter()
        .filter(|call| call.contains(needle))
        .count()
}

fn up(root: &Path) -> Result<std::process::Output, ContainerExecError> {
    let _lock = crate::test_env_lock();
    let previous = (
        std::env::var_os("PATH"),
        std::env::var_os("EFFIGY_COMPOSE_BACKEND"),
    );
    unsafe {
        std::env::set_var(
            "PATH",
            format!(
                "{}:{}",
                root.join("bin").display(),
                std::env::var("PATH").unwrap()
            ),
        );
        std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");
    }
    let mut policy = crate::policy::model::test_policy(root);
    policy.project_name = PROJECT.to_owned();
    policy.compose_files = vec![root.join("compose.yml")];
    let result = run_compose_invocation_streaming_with_env(
        root,
        &policy,
        std::ffi::OsStr::new("docker"),
        &[
            OsString::from("compose"),
            OsString::from("-p"),
            OsString::from(PROJECT),
            OsString::from("up"),
            OsString::from("-d"),
        ],
        "docker compose up",
        &[],
        None,
        &mut |_| {},
    );
    unsafe {
        match previous.0 {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        match previous.1 {
            Some(value) => std::env::set_var("EFFIGY_COMPOSE_BACKEND", value),
            None => std::env::remove_var("EFFIGY_COMPOSE_BACKEND"),
        }
    }
    result
}

#[test]
fn a_name_store_conflict_on_this_project_clears_the_container_and_retries_once() {
    let root = fixture(PROJECT);
    assert!(up(&root)
        .expect("the retried create succeeds")
        .status
        .success());
    assert_eq!(
        count(&root, &format!("\"rm\", \"-f\", \"{ID}")),
        1,
        "the owned container is removed once, by the id the runtime named"
    );
    assert_eq!(count(&root, "\"up\""), 2, "the create is retried once");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_name_store_conflict_from_another_project_is_reported_and_never_removed() {
    let root = fixture("someone-elses-project");
    let error = up(&root).expect_err("a foreign conflict is reported");
    assert!(error.to_string().contains(ID), "{error}");
    assert_eq!(
        count(&root, "\"rm\""),
        0,
        "another project's container is kept"
    );
    assert_eq!(
        count(&root, "\"up\""),
        1,
        "a foreign conflict is not retried"
    );
    std::fs::remove_dir_all(&root).ok();
}
