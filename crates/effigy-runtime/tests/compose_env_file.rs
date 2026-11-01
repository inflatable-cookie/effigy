//! Workspace-route proof for the VM-hosted Compose interpolation file: the
//! inherited Compose spawn names a per-invocation file, the child sees 0700/0600
//! while it runs, and nothing with the fixture sentinel persists after the child
//! is reaped and the held file dropped. Fixture secrets only.
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use effigy_containers::{BackendId, ContainerAction, ContainerComposeInvocationPlan};
use effigy_runtime::signals::spawn_compose_plan_inherit_with_env;

const SENTINEL: &str = "effigy-fixture-sentinel-workspace-41c2";

/// A fake `colima` that records the modes of the env file's directory and file,
/// then runs `tail`. The log sits beside the home, never inside it.
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

fn plan(root: &Path, program: &Path) -> ContainerComposeInvocationPlan {
    ContainerComposeInvocationPlan {
        backend_id: BackendId::colima_nerdctl(),
        repo_root: root.to_path_buf(),
        profile: "effigy".to_owned(),
        action: ContainerAction::Activate,
        program: program.as_os_str().to_owned(),
        args: ["compose", "-f", "compose.yml", "up", "-d"]
            .into_iter()
            .map(OsString::from)
            .collect(),
        label: "workspace route".to_owned(),
    }
}

fn run_once(root: &Path, home: &Path, program: &Path) -> bool {
    let env = vec![(
        "DATABASE_URL".to_owned(),
        OsString::from(format!("postgres://fixture/{SENTINEL}")),
    )];
    effigy_containers::with_test_effigy_home(home, || {
        let (mut child, interpolation) =
            spawn_compose_plan_inherit_with_env(&plan(root, program), &env)
                .expect("the workspace route spawns");
        let status = child.wait().expect("the child is reaped");
        drop(interpolation);
        status.success()
    })
}

#[test]
fn workspace_route_removes_its_file_on_success_and_on_failure() {
    for (name, tail, expect_success) in [("ws-ok", "exit 0", true), ("ws-fail", "exit 3", false)] {
        let root = tempfile::tempdir().expect("fixture root");
        let home = root.path().join("home");
        fs::create_dir_all(&home).expect("home");
        let program = fake_colima(root.path(), name, tail);

        let success = run_once(root.path(), &home, &program);
        assert_eq!(
            success, expect_success,
            "{name}: the child exit is preserved"
        );

        let observed = fs::read_to_string(root.path().join(format!("{name}.observed")))
            .expect("the child observed its file");
        assert!(
            observed.contains("drwx------"),
            "{name}: directory 0700:\n{observed}"
        );
        assert!(
            observed.contains("-rw-------"),
            "{name}: file 0600:\n{observed}"
        );

        let remaining = fs::read_dir(home.join("compose-env"))
            .map(|entries| entries.flatten().count())
            .unwrap_or(0);
        assert_eq!(
            remaining, 0,
            "{name}: no interpolation file survives its child"
        );
        for file in files_under(&home) {
            let bytes = fs::read(&file).unwrap_or_default();
            assert!(
                !bytes
                    .windows(SENTINEL.len())
                    .any(|window| window == SENTINEL.as_bytes()),
                "{name}: sentinel bytes persist in {}",
                file.display()
            );
        }
    }
}

#[test]
fn workspace_route_spawn_failure_removes_its_file() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let program = root.path().join("missing").join("colima");
    let env = vec![(
        "DATABASE_URL".to_owned(),
        OsString::from(format!("postgres://fixture/{SENTINEL}")),
    )];
    let spawned = effigy_containers::with_test_effigy_home(&home, || {
        spawn_compose_plan_inherit_with_env(&plan(root.path(), &program), &env).is_ok()
    });
    assert!(!spawned, "a program that cannot spawn is reported");
    let remaining = fs::read_dir(home.join("compose-env"))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(
        remaining, 0,
        "the file is removed before the spawn error returns"
    );
}
