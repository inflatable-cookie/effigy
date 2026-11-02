//! Direct-child VM Compose paths refuse secret delivery without retained custody.
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

#[test]
fn secret_bearing_vm_spawn_requires_retained_kernel_custody() {
    let root = tempfile::tempdir().expect("fixture root");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let program = fake_colima(root.path(), "unretained", "exit 0");
    let env = vec![("DATABASE_URL".to_owned(), OsString::from(SENTINEL))];
    let result = effigy_containers::with_test_effigy_home(&home, || {
        spawn_compose_plan_inherit_with_env(&plan(root.path(), &program), &env)
    });
    if let Ok((mut child, owner)) = result {
        let _ = child.wait();
        drop(owner);
        panic!("direct child ran without retained kernel custody");
    }
    assert!(
        !root.path().join("unretained.observed").exists(),
        "no child starts"
    );
}
