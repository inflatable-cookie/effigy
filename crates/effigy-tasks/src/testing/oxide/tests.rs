use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::{route_rust_plan, ESCAPE_COMMAND, ESCAPE_ENV};
use crate::testing::{TestRunner, TestRunnerPlan};

/// Detected suites read process-wide state (PATH and the escape knob); every
/// test that changes it holds this lock.
fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Restores the environment the test found.
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

fn fixture(name: &str) -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::new(&format!("effigy-oxide-route-{name}-"))
        .expect("create fixture")
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat").permissions();
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

/// A stub `oxide` whose `pool describe --json` behaves as `pool_body` says.
fn fake_oxide(root: &Path, pool_body: &str) -> PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("mkdir bin");
    write_executable(
        &bin.join("oxide"),
        &format!(
            "#!/bin/sh\nif [ \"$1 $2\" = \"pool describe\" ]; then\n{pool_body}\nfi\necho \"oxide: unexpected arguments $*\" >&2\nexit 64\n"
        ),
    );
    bin
}

fn with_path(bin: &Path, extra: &[(&'static str, Option<&'static str>)]) -> EnvGuard {
    let prior = std::env::var("PATH").unwrap_or_default();
    let mut entries = vec![("PATH", Some(format!("{}:{prior}", bin.display())))];
    entries.extend(
        extra
            .iter()
            .map(|(key, value)| (*key, value.map(str::to_owned))),
    );
    EnvGuard::set_many(&entries)
}

fn plan(runner: TestRunner, command: &str) -> TestRunnerPlan {
    TestRunnerPlan {
        runner,
        command: command.to_owned(),
        evidence: vec!["found `Cargo.toml`".to_owned()],
    }
}

const SCHEMA_OK: &str = "  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/1\"}'\n  exit 0";

#[test]
fn oxide_route_leases_nextest_when_pool_describe_reports_the_schema() {
    let workspace = fixture("nextest");
    let bin = fake_oxide(workspace.path(), SCHEMA_OK);
    let _env = with_path(&bin, &[(ESCAPE_ENV, None)]);

    let routed = route_rust_plan(plan(
        TestRunner::CargoNextest,
        "cargo nextest run --workspace",
    ))
    .expect("route");

    assert_eq!(routed.command, "oxide nextest run --workspace");
    assert!(
        routed.evidence[0].contains("oxide.pool.describe/1"),
        "{:?}",
        routed.evidence
    );
}

#[test]
fn oxide_route_maps_the_cargo_test_fallback_to_oxide_test() {
    let workspace = fixture("cargo-test");
    let bin = fake_oxide(workspace.path(), SCHEMA_OK);
    let _env = with_path(&bin, &[(ESCAPE_ENV, None)]);

    let routed = route_rust_plan(plan(TestRunner::CargoTest, "cargo test")).expect("route");

    assert_eq!(routed.command, "oxide test");
}

#[test]
fn oxide_route_keeps_plain_cargo_without_oxide_on_path() {
    let workspace = fixture("no-oxide");
    let empty = workspace.path().join("empty-bin");
    fs::create_dir_all(&empty).expect("mkdir empty bin");
    // Only the empty fixture directory: the host's real `oxide` must not leak
    // into this route.
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(empty.display().to_string())),
        (ESCAPE_ENV, None),
    ]);

    let routed =
        route_rust_plan(plan(TestRunner::CargoNextest, "cargo nextest run")).expect("route");

    assert_eq!(routed.command, "cargo nextest run");
    assert!(
        routed.evidence[0].contains("no `oxide` on PATH"),
        "{:?}",
        routed.evidence
    );
}

#[test]
fn oxide_route_honours_the_plain_cargo_escape() {
    let workspace = fixture("escape");
    let bin = fake_oxide(workspace.path(), SCHEMA_OK);
    let _env = with_path(&bin, &[(ESCAPE_ENV, Some("cargo"))]);

    let routed =
        route_rust_plan(plan(TestRunner::CargoNextest, "cargo nextest run")).expect("route");

    assert_eq!(routed.command, "cargo nextest run");
    assert!(
        routed.evidence[0].contains(ESCAPE_COMMAND),
        "{:?}",
        routed.evidence
    );
}

#[test]
fn oxide_route_fails_loudly_when_pool_describe_fails() {
    let workspace = fixture("pool-failure");
    let bin = fake_oxide(
        workspace.path(),
        "  printf '%s\\n' 'oxide: unknown_accounting: the pool ledger is unreadable' >&2\n  exit 1",
    );
    let _env = with_path(&bin, &[(ESCAPE_ENV, None)]);

    let error = route_rust_plan(plan(TestRunner::CargoNextest, "cargo nextest run"))
        .expect_err("an unusable oxide must not fall back silently");

    let message = error.to_string();
    assert!(message.contains("unknown_accounting"), "{message}");
    assert!(message.contains(ESCAPE_COMMAND), "{message}");
}

#[test]
fn oxide_route_fails_loudly_on_an_unknown_pool_schema() {
    let workspace = fixture("unknown-schema");
    let bin = fake_oxide(
        workspace.path(),
        "  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/9\"}'\n  exit 0",
    );
    let _env = with_path(&bin, &[(ESCAPE_ENV, None)]);

    let error = route_rust_plan(plan(TestRunner::CargoNextest, "cargo nextest run"))
        .expect_err("an unknown schema must not fall back silently");

    let message = error.to_string();
    assert!(message.contains("oxide.pool.describe/9"), "{message}");
    assert!(message.contains(ESCAPE_COMMAND), "{message}");
}

#[test]
fn oxide_route_leaves_vitest_alone() {
    let workspace = fixture("vitest");
    let bin = fake_oxide(workspace.path(), SCHEMA_OK);
    let _env = with_path(&bin, &[(ESCAPE_ENV, None)]);

    let routed = route_rust_plan(plan(TestRunner::Vitest, "vitest run")).expect("route");

    assert_eq!(routed.command, "vitest run");
    assert_eq!(routed.evidence, vec!["found `Cargo.toml`".to_owned()]);
}
