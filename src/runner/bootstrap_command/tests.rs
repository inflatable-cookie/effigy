//! Runner-path integration tests.
//!
//! These exercise the full shell (`run_bootstrap_with_cwd`) end-to-end,
//! driving through the real runner callbacks (`load_task_manifest` +
//! `run_manifest_task_with_cwd`). Crate-domain behavior is covered by
//! integration tests in `crates/effigy-bootstrap/tests/integration.rs`.

use super::{
    bootstrap_runtime_session_context, render_bootstrap_progress_message, run_bootstrap_with_cwd,
};
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, PublicWorkspaceCleanupOverride};
use effigy_cli::{BootstrapArgs, BootstrapDbSeedInput, BootstrapDepsSyncMode, BootstrapSubcommand};
#[allow(dead_code)]
#[path = "../../../crates/effigy-bootstrap/tests/support.rs"]
mod support;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use support::{
    attach_remote_and_push, bare_remote_path, commit_all, init_bare_remote, init_git_repo, temp_dir,
};

/// RAII guard that prepends `bin_dir` to `PATH` for the duration of a test
/// and restores the prior value on drop. Always pair with the shared
/// `crate::contract_test_support::lock_test()` mutex so no other test
/// in the binary mutates `PATH` (or `cwd`) while this guard is live.
struct PathPrepend {
    original: String,
}

impl PathPrepend {
    fn new(bin_dir: &Path) -> Self {
        let original = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{original}", bin_dir.display()));
        }
        Self { original }
    }
}

impl Drop for PathPrepend {
    fn drop(&mut self) {
        unsafe {
            std::env::set_var("PATH", &self.original);
        }
    }
}

#[test]
fn bootstrap_run_context_skips_lease_refresh_without_forcing_workspace_cleanup() {
    let context = bootstrap_runtime_session_context("bootstrap run");
    assert_eq!(
        context.lease_refresh_policy,
        LeaseRefreshPolicy::SkipRefresh
    );
    assert_eq!(
        context.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::Default
    );
}

#[test]
fn bootstrap_start_context_skips_lease_refresh_and_forces_stop_on_exit() {
    let context = bootstrap_runtime_session_context("bootstrap start");
    assert_eq!(
        context.lease_refresh_policy,
        LeaseRefreshPolicy::SkipRefresh
    );
    assert_eq!(
        context.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::ForceStopOnExit
    );
}

#[test]
fn bootstrap_progress_message_preserves_plain_text_without_color() {
    let rendered = render_bootstrap_progress_message(
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n",
        false,
    );
    assert_eq!(
        rendered,
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n"
    );
}

#[test]
fn bootstrap_progress_message_colors_known_prefixes() {
    let rendered = render_bootstrap_progress_message(
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n",
        true,
    );
    assert!(rendered.contains("\u{1b}["));
    assert!(rendered.contains("[ok]"));
    assert!(rendered.contains("[gateway]"));
    assert!(rendered.contains("[bootstrap]"));
}

fn create_child_remote(name: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir child scripts");
    fs::write(worktree.join("README.md"), format!("# {name}\n")).expect("write child readme");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[tasks."bootstrap:child"]
run = "sh ./scripts/child-setup.sh"
"#,
    )
    .expect("write child manifest");
    fs::write(
        worktree.join("scripts/child-setup.sh"),
        "#!/bin/sh\nset -eu\nprintf child-setup > child-setup.txt\n",
    )
    .expect("write child setup");
    let mut perms = fs::metadata(worktree.join("scripts/child-setup.sh"))
        .expect("child setup metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(worktree.join("scripts/child-setup.sh"), perms).expect("chmod child setup");
    init_git_repo(&worktree);
    commit_all(&worktree, "init child");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_js_child_remote(name: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::write(
        worktree.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");
    fs::write(worktree.join("package.json"), "{}\n").expect("write package");
    init_git_repo(&worktree);
    commit_all(&worktree, "init js child");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_plain_remote(name: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::write(worktree.join("README.md"), format!("# {name}\n")).expect("write readme");
    fs::write(worktree.join("effigy.toml"), "").expect("write manifest");
    init_git_repo(&worktree);
    commit_all(&worktree, "init plain");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_bootstrap(child_remote: &Path) -> PathBuf {
    let worktree = temp_dir("root-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        format!(
            r#"[bootstrap]
run = "sh ./scripts/root-setup.sh"
start = "bootstrap:start"

[[bootstrap.children]]
path = "child-app"
repo = "{}"
run = "sh ./scripts/child-setup.sh"
required = true

[tasks."bootstrap:start"]
run = "sh ./scripts/start.sh"
"#,
            child_remote.display()
        ),
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/root-setup.sh"),
        "#!/bin/sh\nset -eu\nprintf root-setup > root-setup.txt\n",
    )
    .expect("write root setup");
    fs::write(
        worktree.join("scripts/start.sh"),
        "#!/bin/sh\nset -eu\nprintf started > start.txt\n",
    )
    .expect("write start");
    for name in ["root-setup.sh", "start.sh"] {
        let script = worktree.join("scripts").join(name);
        let mut perms = fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root");
    let remote = bare_remote_path("root-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_sibling_bootstrap_deps(child_remote: &Path) -> PathBuf {
    let worktree = temp_dir("root-sibling-deps-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        format!(
            r#"[package_manager]
js = "bun"

[bootstrap]
run = [
  {{ task = "bootstrap deps sync ../underlay" }}
]

[[bootstrap.children]]
path = "../underlay"
repo = "{}"
required = true
"#,
            child_remote.display()
        ),
    )
    .expect("write manifest");
    init_git_repo(&worktree);
    commit_all(&worktree, "init root sibling deps");
    let remote = bare_remote_path("root-sibling-deps-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_optional_missing_child() -> PathBuf {
    let worktree = temp_dir("root-optional-child-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[bootstrap]
run = "sh ./scripts/root-setup.sh"

[[bootstrap.children]]
path = "missing-child"
repo = "/definitely/not/a/real/repo.git"
run = "sh ./scripts/child-setup.sh"
required = false
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/root-setup.sh"),
        "#!/bin/sh\nset -eu\nprintf root-setup > root-setup.txt\n",
    )
    .expect("write root setup");
    let script = worktree.join("scripts/root-setup.sh");
    let mut perms = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");
    init_git_repo(&worktree);
    commit_all(&worktree, "init root optional child");
    let remote = bare_remote_path("root-optional-child-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_bootstrap_db_seed_task() -> PathBuf {
    let worktree = temp_dir("root-db-seed-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[bootstrap]
start = "bootstrap:start"

[tasks."bootstrap:db-seed"]
run = "sh ./scripts/db-seed.sh"

[tasks."bootstrap:start"]
run = "sh ./scripts/start.sh"
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/db-seed.sh"),
        r#"#!/bin/sh
set -eu
test -n "${EFFIGY_BOOTSTRAP_DB_SEEDS_DIR:-}"
test -n "${EFFIGY_BOOTSTRAP_DB_SEED_FILE:-}"
test "${EFFIGY_BOOTSTRAP_DB_SEED_COUNT:-}" = "1"
test -f "$EFFIGY_BOOTSTRAP_DB_SEED_FILE"
test -f ".effigy/local/db-seeds/latest.sql"
cmp "$EFFIGY_BOOTSTRAP_DB_SEED_FILE" ".effigy/local/db-seeds/latest.sql"
printf seeded > db-seed.txt
"#,
    )
    .expect("write db seed task");
    fs::write(
        worktree.join("scripts/start.sh"),
        r#"#!/bin/sh
set -eu
test -f db-seed.txt
printf started > start.txt
"#,
    )
    .expect("write start");
    for name in ["db-seed.sh", "start.sh"] {
        let script = worktree.join("scripts").join(name);
        let mut perms = fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root db seed");
    let remote = bare_remote_path("root-db-seed-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_single_database_bundle_bootstrap_db_seed_task() -> PathBuf {
    let worktree = temp_dir("root-single-db-seed-worktree");
    let bundle_dir = worktree.join("bundles/plain");
    fs::create_dir_all(&bundle_dir).expect("mkdir bundle dir");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        bundle_dir.join("bundle.toml"),
        r#"[bundle]
name = "plain"
description = "Minimal local bundle for bootstrap db seed tests."

[[inputs]]
name = "databases"
type = "list"
required = true
description = "Database names used for bootstrap db seed target validation."
"#,
    )
    .expect("write bundle descriptor");
    fs::write(bundle_dir.join("effigy.toml"), "").expect("write bundle manifest");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[bundle]
base_path = "bundles/plain"
databases = ["contactpatch"]

[tasks."bootstrap:db-seed"]
run = "sh ./scripts/db-seed.sh"
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/db-seed.sh"),
        r#"#!/bin/sh
set -eu
test "${EFFIGY_BOOTSTRAP_DB_SEED_COUNT:-}" = "1"
test -n "${EFFIGY_BOOTSTRAP_DB_SEED_FILE:-}"
test "${EFFIGY_BOOTSTRAP_DB_SEED_TARGET:-}" = "contactpatch"
test -n "${EFFIGY_BOOTSTRAP_DB_SEEDS_JSON:-}"
test -f ".effigy/local/db-seeds/contactpatch--latest.sql"
cmp "$EFFIGY_BOOTSTRAP_DB_SEED_FILE" ".effigy/local/db-seeds/contactpatch--latest.sql"
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"target":"contactpatch"'
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"staged_path":".effigy/local/db-seeds/contactpatch--latest.sql"'
cmp ".effigy/local/db-seeds/contactpatch--latest.sql" expected-contactpatch.sql
printf seeded > db-seed.txt
"#,
    )
    .expect("write db seed task");
    fs::write(
        worktree.join("expected-contactpatch.sql"),
        "seed contactpatch;\n",
    )
    .expect("write expected dump");
    let script = worktree.join("scripts/db-seed.sh");
    let mut perms = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");
    init_git_repo(&worktree);
    commit_all(&worktree, "init root single db seed");
    let remote = bare_remote_path("root-single-db-seed-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_multi_database_bootstrap_db_seed_task() -> PathBuf {
    let worktree = temp_dir("root-multi-db-seed-worktree");
    let bundle_dir = worktree.join("bundles/plain");
    fs::create_dir_all(&bundle_dir).expect("mkdir bundle dir");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        bundle_dir.join("bundle.toml"),
        r#"[bundle]
name = "plain"
description = "Minimal local bundle for bootstrap db seed tests."

[[inputs]]
name = "databases"
type = "list"
required = true
description = "Database names used for bootstrap db seed target validation."
"#,
    )
    .expect("write bundle descriptor");
    fs::write(bundle_dir.join("effigy.toml"), "").expect("write bundle manifest");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[bundle]
base_path = "bundles/plain"
databases = ["cbs", "cbs-mortcalc"]

[bootstrap]
start = "bootstrap:start"

[tasks."bootstrap:db-seed"]
run = "sh ./scripts/db-seed.sh"

[tasks."bootstrap:start"]
run = "sh ./scripts/start.sh"
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/db-seed.sh"),
        r#"#!/bin/sh
set -eu
test "${EFFIGY_BOOTSTRAP_DB_SEED_COUNT:-}" = "2"
test -z "${EFFIGY_BOOTSTRAP_DB_SEED_FILE:-}"
test -z "${EFFIGY_BOOTSTRAP_DB_SEED_TARGET:-}"
test -n "${EFFIGY_BOOTSTRAP_DB_SEEDS_DIR:-}"
test -n "${EFFIGY_BOOTSTRAP_DB_SEED_FILES:-}"
test -n "${EFFIGY_BOOTSTRAP_DB_SEEDS_JSON:-}"
test -f ".effigy/local/db-seeds/cbs--latest.sql"
test -f ".effigy/local/db-seeds/cbs-mortcalc--latest.sql"
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEED_FILES" | grep -F ".effigy/local/db-seeds/cbs--latest.sql"
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEED_FILES" | grep -F ".effigy/local/db-seeds/cbs-mortcalc--latest.sql"
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"target":"cbs"'
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"target":"cbs-mortcalc"'
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"staged_path":".effigy/local/db-seeds/cbs--latest.sql"'
printf '%s' "$EFFIGY_BOOTSTRAP_DB_SEEDS_JSON" | grep -F '"staged_path":".effigy/local/db-seeds/cbs-mortcalc--latest.sql"'
cmp ".effigy/local/db-seeds/cbs--latest.sql" expected-cbs.sql
cmp ".effigy/local/db-seeds/cbs-mortcalc--latest.sql" expected-cbs-mortcalc.sql
printf seeded > db-seed.txt
"#,
    )
    .expect("write db seed task");
    fs::write(
        worktree.join("scripts/start.sh"),
        r#"#!/bin/sh
set -eu
test -f db-seed.txt
printf started > start.txt
"#,
    )
    .expect("write start");
    fs::write(worktree.join("expected-cbs.sql"), "seed cbs;\n").expect("write cbs expected");
    fs::write(
        worktree.join("expected-cbs-mortcalc.sql"),
        "seed mortcalc;\n",
    )
    .expect("write mortcalc expected");
    for name in ["db-seed.sh", "start.sh"] {
        let script = worktree.join("scripts").join(name);
        let mut perms = fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root multi db seed");
    let remote = bare_remote_path("root-multi-db-seed-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_bootstrap_run_db_seed_visibility() -> PathBuf {
    let worktree = temp_dir("root-db-seed-run-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[bootstrap]
run = "sh ./scripts/root-setup.sh"

[tasks."bootstrap:db-seed"]
run = "sh ./scripts/db-seed.sh"
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/root-setup.sh"),
        r#"#!/bin/sh
set -eu
test -n "${EFFIGY_BOOTSTRAP_DB_SEEDS_DIR:-}"
test -f ".effigy/local/db-seeds/latest.sql"
cmp "${EFFIGY_BOOTSTRAP_DB_SEED_FILE}" ".effigy/local/db-seeds/latest.sql"
printf visible > root-seed-check.txt
"#,
    )
    .expect("write root setup");
    fs::write(
        worktree.join("scripts/db-seed.sh"),
        "#!/bin/sh\nset -eu\nprintf seeded > db-seed.txt\n",
    )
    .expect("write db seed task");
    for name in ["root-setup.sh", "db-seed.sh"] {
        let script = worktree.join("scripts").join(name);
        let mut perms = fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root db seed visibility");
    let remote = bare_remote_path("root-db-seed-run-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

#[test]
fn run_bootstrap_with_cwd_starts_when_requested() {
    let child_remote = create_child_remote("child-app-start");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-start");
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");
    assert!(out.contains("[ok] bootstrap completed"));
    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("start.txt")).expect("start marker"),
        "started"
    );
}

#[test]
fn run_bootstrap_with_cwd_stages_db_seed_before_root_run() {
    let root_remote = create_root_remote_with_bootstrap_run_db_seed_visibility();
    let cwd = temp_dir("bootstrap-db-seed-root-run");
    let dump = cwd.join("latest.sql");
    fs::write(&dump, "create table ok;\n").expect("write dump");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: dump.clone(),
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");

    assert!(out.contains("[ok] bootstrap completed"));
    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("root-seed-check.txt")).expect("seed marker"),
        "visible"
    );
    assert_eq!(
        fs::read_to_string(destination.join(".effigy/local/db-seeds/latest.sql"))
            .expect("staged dump"),
        "create table ok;\n"
    );
}

#[test]
fn run_bootstrap_with_cwd_runs_standard_db_seed_task_before_start() {
    let root_remote = create_root_remote_with_bootstrap_db_seed_task();
    let cwd = temp_dir("bootstrap-db-seed-task");
    let dump = cwd.join("latest.sql");
    fs::write(&dump, "seed payload;\n").expect("write dump");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: dump,
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: true,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");

    assert!(out.contains("[ok] bootstrap completed"));
    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("db-seed.txt")).expect("seed marker"),
        "seeded"
    );
    assert_eq!(
        fs::read_to_string(destination.join("start.txt")).expect("start marker"),
        "started"
    );
}

#[test]
fn run_bootstrap_with_cwd_auto_targets_single_database_bundle_seed_input() {
    let root_remote = create_root_remote_with_single_database_bundle_bootstrap_db_seed_task();
    let cwd = temp_dir("bootstrap-single-db-seed-task");
    let dump = cwd.join("latest.sql");
    fs::write(&dump, "seed contactpatch;\n").expect("write dump");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: dump,
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");

    assert!(out.contains("[ok] bootstrap completed"));
    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("db-seed.txt")).expect("seed marker"),
        "seeded"
    );
    assert_eq!(
        fs::read_to_string(destination.join(".effigy/local/db-seeds/contactpatch--latest.sql"))
            .expect("staged dump"),
        "seed contactpatch;\n"
    );
}

#[test]
fn run_bootstrap_with_cwd_runs_multi_target_db_seed_task_for_bundle_databases() {
    let root_remote = create_root_remote_with_multi_database_bootstrap_db_seed_task();
    let cwd = temp_dir("bootstrap-multi-db-seed-task");
    let cbs_dir = cwd.join("cbs-dumps");
    let mortcalc_dir = cwd.join("mortcalc-dumps");
    fs::create_dir_all(&cbs_dir).expect("mkdir cbs dumps");
    fs::create_dir_all(&mortcalc_dir).expect("mkdir mortcalc dumps");
    let cbs_dump = cbs_dir.join("latest.sql");
    let mortcalc_dump = mortcalc_dir.join("latest.sql");
    fs::write(&cbs_dump, "seed cbs;\n").expect("write cbs dump");
    fs::write(&mortcalc_dump, "seed mortcalc;\n").expect("write mortcalc dump");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![
                    BootstrapDbSeedInput {
                        target: Some("cbs".into()),
                        path: cbs_dump,
                    },
                    BootstrapDbSeedInput {
                        target: Some("cbs-mortcalc".into()),
                        path: mortcalc_dump,
                    },
                ],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");

    assert!(out.contains("[ok] bootstrap completed"));
    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("db-seed.txt")).expect("seed marker"),
        "seeded"
    );
    assert_eq!(
        fs::read_to_string(destination.join(".effigy/local/db-seeds/cbs--latest.sql"))
            .expect("staged cbs dump"),
        "seed cbs;\n"
    );
    assert_eq!(
        fs::read_to_string(destination.join(".effigy/local/db-seeds/cbs-mortcalc--latest.sql"))
            .expect("staged mortcalc dump"),
        "seed mortcalc;\n"
    );
}

#[test]
fn run_bootstrap_with_cwd_rejects_unnamed_db_seed_for_multi_database_bundle() {
    let root_remote = create_root_remote_with_multi_database_bootstrap_db_seed_task();
    let cwd = temp_dir("bootstrap-multi-db-seed-unnamed");
    let dump = cwd.join("latest.sql");
    fs::write(&dump, "seed payload;\n").expect("write dump");

    let err = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: dump,
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect_err("bootstrap should reject unnamed db seed for multi-db bundle");

    assert!(
        err.to_string().contains("must name a target because multiple database targets are declared in `[bundle].databases` and `[data.targets]`: cbs, cbs-mortcalc"),
        "unexpected error: {err}"
    );
}

#[test]
fn run_bootstrap_with_cwd_requires_container_registry_for_builtin_db_seed_fallback() {
    let child_remote = create_child_remote("child-app-db-seed-missing");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-db-seed-missing-task");
    let dump = cwd.join("latest.sql");
    fs::write(&dump, "seed payload;\n").expect("write dump");

    let err = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: vec![BootstrapDbSeedInput {
                    target: None,
                    path: dump,
                }],
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect_err("bootstrap should reject missing container registry for builtin db seed fallback");

    assert!(
        err.to_string()
            .contains("manifest does not define a `[containers]` registry"),
        "unexpected error: {err}"
    );
}

#[test]
fn run_bootstrap_with_cwd_reports_optional_child_warning_in_text_output() {
    let root_remote = create_root_remote_with_optional_missing_child();
    let cwd = temp_dir("bootstrap-optional-child-text");
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect("run bootstrap");
    assert!(out.contains("[ok] bootstrap completed"));
    assert!(out.contains("child missing-child: failed"));
    assert!(out.contains("[warn] optional child `missing-child` failed"));
}

#[test]
fn run_bootstrap_with_cwd_syncs_js_and_rust_dependencies() {
    let root = temp_dir("bootstrap-deps-sync");
    fs::write(
        root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");

    let ui = root.join("ui");
    fs::create_dir_all(&ui).expect("mkdir ui");
    fs::write(ui.join("package.json"), "{}\n").expect("write ui package");

    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    fs::write(
        api.join("Cargo.toml"),
        "[package]\nname = \"api\"\nversion = \"0.1.0\"\n",
    )
    .expect("write api cargo");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    fs::write(
        bin_dir.join("cargo"),
        "#!/bin/sh\nprintf '%s ' \"$@\" > cargo.args\nprintf cargo > cargo.marker\n",
    )
    .expect("write cargo");
    for name in ["bun", "cargo"] {
        let script = bin_dir.join(name);
        let mut perms = fs::metadata(&script).expect("stat script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }

    let _process_lock = crate::contract_test_support::lock_test();
    let _path = PathPrepend::new(&bin_dir);
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::DepsSync {
                mode: BootstrapDepsSyncMode::Both,
                paths: vec!["ui".to_owned(), "api".to_owned()],
            },
            output_json: false,
        },
        root.clone(),
    )
    .expect("run bootstrap deps sync");

    assert!(out.contains("bootstrap deps sync completed (2)"));
    assert!(out.contains("ui [js]: bun install"));
    assert!(out.contains("api [rust]: cargo fetch --manifest-path Cargo.toml"));
    assert!(ui.join("bun.marker").is_file(), "bun marker should exist");
    assert!(
        api.join("cargo.marker").is_file(),
        "cargo marker should exist"
    );
    assert_eq!(
        fs::read_to_string(api.join("cargo.args")).expect("read cargo args"),
        "fetch --manifest-path Cargo.toml "
    );
}

#[test]
fn run_bootstrap_with_cwd_resolves_bootstrap_deps_sync_relative_to_cloned_repo_root() {
    let child_remote = create_js_child_remote("underlay-sibling");
    let root_remote = create_root_remote_with_sibling_bootstrap_deps(&child_remote);
    let cwd = temp_dir("bootstrap-sibling-deps");
    let bin_dir = cwd.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    let mut perms = fs::metadata(bin_dir.join("bun"))
        .expect("stat bun")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bin_dir.join("bun"), perms).expect("chmod bun");

    let _process_lock = crate::contract_test_support::lock_test();
    let _path = PathPrepend::new(&bin_dir);
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(PathBuf::from("underlay-reference")),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");

    assert!(out.contains("[ok] bootstrap completed"));
    assert!(
        cwd.join("underlay/bun.marker").is_file(),
        "bun marker should be written under the cloned repo sibling, not the bootstrap parent"
    );
}

#[test]
fn prompt_bootstrap_db_seeds_collects_named_paths() {
    let root = temp_dir("bootstrap-db-seed-prompt");
    let cbs = root.join("cbs.sql");
    let mortcalc = root.join("mortcalc.sql");
    fs::write(&cbs, "cbs").expect("write cbs");
    fs::write(&mortcalc, "mortcalc").expect("write mortcalc");

    let input = format!("{}\n{}\n\n", cbs.display(), mortcalc.display());
    let mut output = Vec::new();
    let seeds = super::collect_bootstrap_db_seed_prompts_from_io(
        &root,
        &["cbs".to_owned(), "cbs-mortcalc".to_owned()],
        &mut Cursor::new(input.into_bytes()),
        &mut output,
    )
    .expect("prompt should succeed");

    assert_eq!(
        seeds,
        vec![
            super::BootstrapDbSeedInput {
                target: Some("cbs".to_owned()),
                path: cbs,
            },
            super::BootstrapDbSeedInput {
                target: Some("cbs-mortcalc".to_owned()),
                path: mortcalc,
            },
        ]
    );
    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("No --db-seed inputs were supplied."));
    assert!(rendered.contains("Continue with 2 database seed file(s)? [Y/n]: "));
}

#[test]
fn prompt_bootstrap_db_seeds_reprompts_invalid_paths_and_allows_skip() {
    let root = temp_dir("bootstrap-db-seed-prompt-invalid");
    let valid = root.join("cbs.sql");
    fs::write(&valid, "cbs").expect("write cbs");

    let input = format!("/definitely/not/real.sql\n{}\n\n\n", valid.display());
    let mut output = Vec::new();
    let seeds = super::collect_bootstrap_db_seed_prompts_from_io(
        &root,
        &["cbs".to_owned(), "cbs-mortcalc".to_owned()],
        &mut Cursor::new(input.into_bytes()),
        &mut output,
    )
    .expect("prompt should succeed");

    assert_eq!(
        seeds,
        vec![super::BootstrapDbSeedInput {
            target: Some("cbs".to_owned()),
            path: valid,
        }]
    );
    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("Path does not exist or is not a readable file"));
}

#[test]
fn prompt_bootstrap_path_reuse_confirms_existing_destination() {
    let root = temp_dir("bootstrap-path-reuse-prompt");
    let destination = root.join("existing");
    fs::create_dir_all(&destination).expect("mkdir destination");
    fs::write(destination.join("README.md"), "existing\n").expect("write marker");

    let mut output = Vec::new();
    super::confirm_bootstrap_path_reuse_from_io(
        &destination,
        &mut Cursor::new(b"y\n".to_vec()),
        &mut output,
    )
    .expect("reuse confirmation should pass");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("Bootstrap destination already exists and is non-empty"));
    assert!(rendered.contains(&destination.display().to_string()));
    assert!(rendered.contains("Reuse this destination and continue? [y/N]: "));
}

#[test]
fn prompt_bootstrap_path_reuse_empty_response_cancels_by_default() {
    let root = temp_dir("bootstrap-path-reuse-prompt-default");
    let destination = root.join("existing");
    fs::create_dir_all(&destination).expect("mkdir destination");

    let err = super::confirm_bootstrap_path_reuse_from_io(
        &destination,
        &mut Cursor::new(b"\n".to_vec()),
        &mut Vec::new(),
    )
    .expect_err("empty confirmation should cancel");

    assert!(
        err.to_string()
            .contains("bootstrap cancelled during destination reuse confirmation"),
        "unexpected error: {err}"
    );
}

#[test]
fn run_bootstrap_with_cwd_rejects_existing_non_empty_destination_without_tty_prompt() {
    let child_remote = create_child_remote("child-app-path-reuse-non-tty");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-path-reuse-non-tty");
    let destination = cwd.join("reuse-target");
    fs::create_dir_all(&destination).expect("mkdir destination");
    fs::write(destination.join("README.md"), "existing\n").expect("write marker");

    let err = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination.clone()),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect_err("non-tty path reuse should fail");

    assert!(
        err.to_string()
            .contains("bootstrap destination already exists and is non-empty"),
        "unexpected error: {err}"
    );
    assert!(err.to_string().contains("--reuse-path"));
}

#[test]
fn run_bootstrap_with_cwd_rejects_existing_non_empty_destination_in_json_mode() {
    let child_remote = create_child_remote("child-app-path-reuse-json");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-path-reuse-json");
    let destination = cwd.join("reuse-target");
    fs::create_dir_all(&destination).expect("mkdir destination");
    fs::write(destination.join("README.md"), "existing\n").expect("write marker");

    let err = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: true,
        },
        cwd,
    )
    .expect_err("json path reuse should fail instead of prompting");

    assert!(
        err.to_string()
            .contains("bootstrap destination already exists and is non-empty"),
        "unexpected error: {err}"
    );
}

#[test]
fn run_bootstrap_with_cwd_plan_skips_existing_destination_prompt() {
    let child_remote = create_child_remote("child-app-path-reuse-plan");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-path-reuse-plan");
    let destination = cwd.join("reuse-target");
    fs::create_dir_all(&destination).expect("mkdir destination");
    fs::write(destination.join("README.md"), "existing\n").expect("write marker");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination.clone()),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: true,
            },
            output_json: false,
        },
        cwd,
    )
    .expect("plan should not prompt or fail");

    assert!(out.contains("[planned] bootstrap request resolved"));
    assert!(out.contains(&destination.display().to_string()));
}

#[test]
fn run_bootstrap_with_cwd_no_prompt_rejects_existing_checkout_without_reuse_path() {
    let root_remote = create_plain_remote("root-path-reuse-bypass");
    let cwd = temp_dir("bootstrap-path-reuse-bypass");
    let destination = cwd.join("reuse-target");

    run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination.clone()),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("initial clone");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: true,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect_err("--no-prompt alone should not bypass path reuse confirmation");

    assert!(out.to_string().contains("--reuse-path"));
}

#[test]
fn run_bootstrap_with_cwd_reuse_path_bypasses_existing_checkout_confirmation() {
    let root_remote = create_plain_remote("root-path-reuse-flag");
    let cwd = temp_dir("bootstrap-path-reuse-flag");
    let destination = cwd.join("reuse-target");

    run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination.clone()),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: false,
                reuse_path: false,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("initial clone");

    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(destination),
                branch: None,
                db_seeds: Vec::new(),
                fresh: false,
                no_prompt: true,
                reuse_path: true,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect("--reuse-path should bypass path reuse confirmation");

    assert!(out.contains("[ok] bootstrap completed"));
}

#[test]
fn run_bootstrap_with_cwd_skips_missing_bootstrap_deps_sync_paths() {
    let root = temp_dir("bootstrap-deps-sync-missing");
    fs::write(
        root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");

    let ui = root.join("ui");
    fs::create_dir_all(&ui).expect("mkdir ui");
    fs::write(ui.join("package.json"), "{}\n").expect("write ui package");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    let script = bin_dir.join("bun");
    let mut perms = fs::metadata(&script).expect("stat script").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");

    let _process_lock = crate::contract_test_support::lock_test();
    let _path = PathPrepend::new(&bin_dir);
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::DepsSync {
                mode: BootstrapDepsSyncMode::Both,
                paths: vec!["ui".to_owned(), "missing-ui".to_owned()],
            },
            output_json: false,
        },
        root.clone(),
    )
    .expect("run bootstrap deps sync");

    assert!(out.contains("bootstrap deps sync completed (1)"));
    assert!(out.contains("ui [js]: bun install"));
    assert!(out.contains("missing-ui [skip]: missing directory"));
    assert!(ui.join("bun.marker").is_file(), "bun marker should exist");
}
