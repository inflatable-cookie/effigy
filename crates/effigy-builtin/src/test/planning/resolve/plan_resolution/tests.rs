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

/// A Cargo workspace root with the runner stubs the plan reads.
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    bin: PathBuf,
}

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

const POOL_OK: &str = "  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/1\"}'\n  exit 0";

fn with_fixture_path(fixture: &Fixture, escape: Option<&str>) -> EnvGuard {
    let prior = std::env::var("PATH").unwrap_or_default();
    EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{prior}", fixture.bin.display()))),
        ("EFFIGY_OXIDE_ESCAPE", escape.map(str::to_owned)),
    ])
}

#[test]
fn host_target_routes_the_detected_rust_suite_through_oxide() {
    let fixture = fixture("host", POOL_OK);
    let _env = with_fixture_path(&fixture, None);

    let (plans, _, _, _) = resolve_target_test_plans(&[], &fixture.root, true).expect("test plans");

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
    let _env = with_fixture_path(&fixture, None);

    let (plans, _, _, _) =
        resolve_target_test_plans(&[], &fixture.root, false).expect("test plans");

    assert_eq!(plans[0].command, "cargo nextest run --workspace");
}

#[test]
fn plain_cargo_escape_keeps_the_cargo_route() {
    let fixture = fixture("escape", POOL_OK);
    let _env = with_fixture_path(&fixture, Some("cargo"));

    let (plans, _, _, _) = resolve_target_test_plans(&[], &fixture.root, true).expect("test plans");

    assert_eq!(plans[0].command, "cargo nextest run --workspace");
}

#[test]
fn unusable_oxide_fails_and_names_the_escape() {
    let fixture = fixture(
        "unusable",
        "  printf '%s\\n' 'oxide: unknown_accounting: pool ledger unreadable' >&2\n  exit 1",
    );
    let _env = with_fixture_path(&fixture, None);

    let error = resolve_target_test_plans(&[], &fixture.root, true)
        .expect_err("an unusable oxide must not fall back silently");

    let message = error.to_string();
    assert!(message.contains("unknown_accounting"), "{message}");
    assert!(message.contains("EFFIGY_OXIDE_ESCAPE=cargo"), "{message}");
}
