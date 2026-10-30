use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::resolve_target_test_plans;

/// Detection reads process-wide state (PATH and the plain-Cargo escape), so
/// every test that changes it holds this lock.
fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EnvGuard {
    original: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set_many(entries: &[(&'static str, Option<String>)]) -> Self {
        let lock = test_lock();
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push((*key, std::env::var_os(key)));
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self {
            original,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

/// A Cargo workspace root plus a stub `oxide` and `cargo-nextest`.
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    bin: PathBuf,
}

const POOL_OK: &str = "  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/1\"}'\n  exit 0";

fn fixture(name: &str, oxide_body: &str) -> Fixture {
    let dir = tempfile::Builder::new()
        .prefix(&format!("effigy-builtin-oxide-{name}-"))
        .tempdir()
        .expect("create fixture");
    let root = dir.path().join("repo");
    fs::create_dir_all(&root).expect("mkdir repo");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crate\"]\n",
    )
    .expect("write cargo toml");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("mkdir bin");
    write_executable(&bin.join("cargo-nextest"), "#!/bin/sh\nexit 0\n");
    write_executable(
        &bin.join("oxide"),
        &format!(
            "#!/bin/sh\nif [ \"$1 $2\" = \"pool describe\" ]; then\n{oxide_body}\nfi\nexit 64\n"
        ),
    );
    Fixture {
        _dir: dir,
        root,
        bin,
    }
}

fn with_fixture_path(fixture: &Fixture) -> EnvGuard {
    let prior = std::env::var("PATH").unwrap_or_default();
    EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{prior}", fixture.bin.display()))),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ])
}

#[test]
fn host_target_routes_the_detected_rust_suite_through_oxide() {
    let fixture = fixture("host", POOL_OK);
    let _env = with_fixture_path(&fixture);

    let (plans, _, _, _) =
        resolve_target_test_plans(&[], &fixture.root, true, None).expect("test plans");

    assert_eq!(plans[0].command, "oxide nextest run --workspace");
    assert!(
        plans[0]
            .evidence
            .iter()
            .any(|line| line.contains("oxide.pool.describe/1")),
        "{:?}",
        plans[0].evidence
    );
}

#[test]
fn container_target_keeps_the_plain_cargo_route() {
    let fixture = fixture("container", POOL_OK);
    let _env = with_fixture_path(&fixture);

    let (plans, _, _, _) =
        resolve_target_test_plans(&[], &fixture.root, false, None).expect("test plans");

    assert_eq!(plans[0].command, "cargo nextest run --workspace");
}
