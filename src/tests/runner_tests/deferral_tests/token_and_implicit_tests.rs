use crate::runner::tests::prelude::{
    assert_deferred_task_case_table, assert_deferred_task_failure_case_table, fs, lock_test,
    run_task_expect_empty_output, temp_workspace, workspace_with_optional_defer_manifest,
    write_executable, write_root_manifest, DeferredTaskCase, DeferredTaskFailureCase, EnvGuard,
};

fn setup_fake_docker_deferral_runtime(
    root: &std::path::Path,
    service_running: bool,
) -> (std::path::PathBuf, EnvGuard) {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir fake runtime bin");
    let docker_log = root.join("fake-docker.log");
    let composer_log = root.join("fake-composer.log");
    let nested_effigy_log = root.join("fake-effigy.log");
    let mkcert_caroot = root.join("fake-mkcert-caroot");
    fs::write(&docker_log, "").expect("seed fake docker log");
    fs::write(&composer_log, "").expect("seed fake composer log");
    fs::write(&nested_effigy_log, "").expect("seed fake effigy log");
    fs::create_dir_all(&mkcert_caroot).expect("mkdir fake mkcert caroot");
    fs::write(mkcert_caroot.join("rootCA.pem"), "fake-root-ca\n").expect("write fake root ca");
    let runtime_state = root.join("fake-docker-running");
    if service_running {
        fs::write(&runtime_state, "running\n").expect("seed fake runtime state");
    }
    write_executable(
        &bin_dir.join("docker"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = ps ]; then\n  if [ -f '{}' ]; then\n    printf 'legacy-dev-app-1\\tUp 2 minutes\\t\\tlegacy-dev\\t\\tapp\\n'\n    printf 'legacy-dev-web-1\\tUp 2 minutes\\t0.0.0.0:8201->80/tcp\\tlegacy-dev\\t\\tweb\\n'\n    printf 'legacy-dev-pma-1\\tUp 2 minutes\\t0.0.0.0:8202->80/tcp\\tlegacy-dev\\t\\tpma\\n'\n    printf 'legacy-dev-db-1\\tUp 2 minutes\\t0.0.0.0:3306->3306/tcp\\tlegacy-dev\\t\\tdb\\n'\n    printf 'legacy-dev-redis-1\\tUp 2 minutes\\t0.0.0.0:6379->6379/tcp\\tlegacy-dev\\t\\tredis\\n'\n    printf 'legacy-dev-memcache-1\\tUp 2 minutes\\t0.0.0.0:11211->11211/tcp\\tlegacy-dev\\t\\tmemcache\\n'\n    printf 'legacy-dev-mail-1\\tUp 2 minutes\\t0.0.0.0:23625->8025/tcp\\tlegacy-dev\\t\\tmail\\n'\n  fi\n  exit 0\nfi\nif [ \"$1\" = compose ]; then\n  for arg in \"$@\"; do\n    case \"$arg\" in\n      up)\n        printf 'running\\n' > '{}'\n        exit 0\n        ;;\n      down)\n        rm -f '{}'\n        exit 0\n        ;;\n    esac\n  done\nfi\nexit 0\n",
            docker_log.display(),
            runtime_state.display(),
            runtime_state.display(),
            runtime_state.display(),
        ),
    );
    write_executable(
        &bin_dir.join("colima"),
        "#!/bin/sh\ncase \"$1\" in\n  status)\n    printf 'INFO[0000] status: Running\\n'\n    exit 0\n    ;;\n  start)\n    printf 'started\\n'\n    exit 0\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
    );
    write_executable(
        &bin_dir.join("composer"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            composer_log.display(),
        ),
    );
    let fake_effigy = bin_dir.join("effigy");
    write_executable(
        &fake_effigy,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            nested_effigy_log.display(),
        ),
    );
    write_executable(
        &bin_dir.join("mkcert"),
        &format!(
            "#!/bin/sh\nset -eu\ncase \"${{1:-}}\" in\n  -help)\n    exit 0\n    ;;\n  -check|-install)\n    exit 0\n    ;;\n  -CAROOT)\n    printf '%s\\n' '{}'\n    exit 0\n    ;;\n  -cert-file)\n    cert=\"$2\"\n    key=\"$4\"\n    mkdir -p \"$(dirname \"$cert\")\" \"$(dirname \"$key\")\"\n    printf 'fake-cert\\n' > \"$cert\"\n    printf 'fake-key\\n' > \"$key\"\n    exit 0\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
            mkcert_caroot.display()
        ),
    );
    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    let lease_home = root.join("lease-home");
    fs::create_dir_all(&lease_home).expect("mkdir lease home");
    let env = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{old_path}", bin_dir.display()))),
        ("EFFIGY_EXECUTABLE", Some(fake_effigy.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
            Some("1".to_owned()),
        ),
        (
            "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
            Some(lease_home.display().to_string()),
        ),
    ]);
    (docker_log, env)
}

#[test]
fn run_manifest_task_defers_when_task_missing_with_token_support() {
    let _guard = lock_test();
    let cases = [
        DeferredTaskCase {
            workspace: "defer-missing",
            defer_run: "printf deferred",
            defer_run_in: None,
            request: "unknown-task",
            args: &[],
        },
        DeferredTaskCase {
            workspace: "defer-tokens",
            defer_run: "test {request} = 'unknown-task' && test {args} = '--dry-run'",
            defer_run_in: None,
            request: "unknown-task",
            args: &["--dry-run"],
        },
        DeferredTaskCase {
            workspace: "defer-path-like-request",
            defer_run: "test {request} = 'services/api/dev' && test {args} = '--watch'",
            defer_run_in: None,
            request: "services/api/dev",
            args: &["--watch"],
        },
        DeferredTaskCase {
            workspace: "defer-either-falls-back-to-host",
            defer_run: "test {request} = 'unknown-task' && test {args} = '--watch'",
            defer_run_in: Some("either"),
            request: "unknown-task",
            args: &["--watch"],
        },
    ];

    assert_deferred_task_case_table(&cases);
}

#[test]
fn run_manifest_task_container_deferral_requires_resolved_container_binding() {
    let _guard = lock_test();
    let cases = [DeferredTaskFailureCase {
        workspace: "defer-container-without-container-target",
        defer_run: "printf deferred",
        defer_run_in: "container",
        request: "unknown-task",
        args: &[],
        expected_message: "no default workspace container binding could be resolved",
    }];

    assert_deferred_task_failure_case_table(&cases);
}

#[test]
fn run_manifest_task_legacy_root_markers_no_longer_enable_deferral() {
    let _guard = lock_test();
    let root = workspace_with_optional_defer_manifest("legacy-markers-no-deferral", None, None);
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("effigy.json"), "{}\n").expect("write legacy marker");
    fs::write(root.join("composer.json"), "{}\n").expect("write composer marker");

    let err =
        crate::runner::tests::prelude::run_task_in_workspace(&root, "version", &["--dry-run"])
            .expect_err("legacy markers should not defer");
    crate::runner::tests::prelude::assert_task_not_found_any(err);
}

#[test]
fn run_manifest_task_decodelabs_bundle_defers_inside_container() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-container-deferral");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_root_manifest(
        &root,
        r#"[bundle]
base = "decodelabs"
host = "legacy.test"
project_name = "legacy-dev"
databases = ["legacy"]
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "decodelabs bundle container deferral should succeed",
    );

    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        log.contains("compose"),
        "expected docker compose invocation, got {log}"
    );
    assert!(log.contains("up"), "expected docker compose up, got {log}");
    assert!(
        log.contains("exec"),
        "expected docker compose exec, got {log}"
    );
    assert!(
        log.contains("app sh -lc"),
        "expected workspace service exec, got {log}"
    );
    assert!(
        log.contains("EFFIGY_COLOR=always"),
        "expected forced color env, got {log}"
    );
    assert!(
        log.contains("FORCE_COLOR=3"),
        "expected force-color env, got {log}"
    );
    assert!(
        log.contains("unset NO_COLOR; export EFFIGY_COLOR=always CLICOLOR_FORCE=1 FORCE_COLOR=3"),
        "expected deferred shell command to clear NO_COLOR and force color, got {log}"
    );
    assert!(
        log.contains(
            "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" missing-task --watch"
        ),
        "expected deferred command in container exec, got {log}"
    );
    let lease_token =
        crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
            &root, "web",
        )
        .expect("read lease token");
    assert!(
        lease_token.is_some(),
        "expected active host-container lease"
    );
}

#[test]
fn run_manifest_task_decodelabs_bundle_defers_locally_inside_handoff_container() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-container-deferral-handoff");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    std::fs::create_dir_all(root.join("app")).expect("app dir");
    write_root_manifest(
        &root,
        r#"[bundle]
base = "decodelabs"
host = "legacy.test"
project_name = "legacy-dev"
databases = ["legacy"]
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    let legacy_log = root.join("fake-legacy-effigy.log");
    write_executable(
        &composer_bin.join("effigy"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            legacy_log.display(),
        ),
    );
    let _handoff = EnvGuard::set_many(&[
        ("EFFIGY_INTERNAL_CONTAINER_HANDOFF", Some("1".to_owned())),
        ("COMPOSER_HOME", Some(composer_home.display().to_string())),
    ]);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "decodelabs bundle handoff-local deferral should succeed",
    );

    let docker = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        !docker.contains("compose"),
        "expected no docker compose usage inside handoff container, got {docker}"
    );
    let composer = fs::read_to_string(&legacy_log).expect("read legacy effigy log");
    assert!(
        composer.contains("missing-task") && composer.contains("--watch"),
        "expected local legacy effigy deferral inside handoff container, got {composer}"
    );
}

#[test]
fn run_manifest_task_decodelabs_handoff_local_deferral_prefers_composer_global_bin() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-container-deferral-handoff-composer-bin");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    std::fs::create_dir_all(root.join("app")).expect("app dir");
    write_root_manifest(
        &root,
        r#"[bundle]
base = "decodelabs"
host = "legacy.test"
project_name = "legacy-dev"
databases = ["legacy"]
"#,
    );
    let (_docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    let legacy_log = root.join("fake-legacy-effigy.log");
    let path_log = root.join("fake-path-effigy.log");
    write_executable(
        &composer_bin.join("effigy"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            legacy_log.display(),
        ),
    );
    write_executable(
        &root.join("bin/effigy"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 97\n",
            path_log.display(),
        ),
    );
    let _handoff = EnvGuard::set_many(&[
        ("EFFIGY_INTERNAL_CONTAINER_HANDOFF", Some("1".to_owned())),
        ("COMPOSER_HOME", Some(composer_home.display().to_string())),
    ]);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "decodelabs bundle handoff-local deferral should prefer composer global bin",
    );

    let legacy = fs::read_to_string(&legacy_log).expect("read fake legacy effigy log");
    assert!(
        legacy.contains("missing-task") && legacy.contains("--watch"),
        "expected composer-home effigy to run, got {legacy}"
    );
    let path = fs::read_to_string(&path_log).unwrap_or_default();
    assert!(
        path.trim().is_empty(),
        "expected PATH effigy not to run, got {path}"
    );
}

#[test]
fn run_manifest_task_decodelabs_container_lease_reaper_shuts_down_expired_env() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-container-deferral-reaper");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_root_manifest(
        &root,
        r#"[bundle]
base = "decodelabs"
host = "legacy.test"
project_name = "legacy-dev"
databases = ["legacy"]
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);
    let _timeout = EnvGuard::set_many(&[(
        "EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS",
        Some("0".to_owned()),
    )]);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "decodelabs bundle container deferral should succeed",
    );

    let token = crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
        &root, "web",
    )
    .expect("read lease token")
    .expect("lease token");
    crate::runner::host_container_lease::run_host_container_lease_reaper_for_tests(
        &root, "web", &token,
    )
    .expect("run reaper");

    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        log.contains("down"),
        "expected docker compose down, got {log}"
    );
    let lease_token =
        crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
            &root, "web",
        )
        .expect("read lease token after reaper");
    assert!(
        lease_token.is_none(),
        "expected cleared host-container lease"
    );
}

#[test]
fn run_manifest_task_decodelabs_library_bundle_isolates_leases_across_repos_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-library-shared-lease");
    let shared_root = root.join("libraries/decodelabs");
    let repo_a = shared_root.join("clockwork");
    let repo_b = shared_root.join("clocksmith");
    std::fs::create_dir_all(&repo_a).expect("mkdir repo a");
    std::fs::create_dir_all(&repo_b).expect("mkdir repo b");
    std::fs::create_dir(repo_a.join(".git")).expect("git dir a");
    std::fs::create_dir(repo_b.join(".git")).expect("git dir b");
    write_root_manifest(
        &repo_a,
        &format!(
            r#"[bundle]
base = "decodelabs-library"
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    write_root_manifest(
        &repo_b,
        &format!(
            r#"[bundle]
base = "decodelabs-library"
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    let (_docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);

    run_task_expect_empty_output(
        &repo_a,
        "missing-task",
        &["--watch"],
        "decodelabs-library bundle container deferral should succeed",
    );

    let token_a = crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
        &repo_a, "web",
    )
    .expect("read lease token for repo a");
    let token_b = crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
        &repo_b, "web",
    )
    .expect("read lease token for repo b");
    let token_a = token_a.expect("expected lease token for repo a");
    assert!(
        token_b.is_none(),
        "expected repo b to have no lease until it starts its own container runtime"
    );
    run_task_expect_empty_output(
        &repo_b,
        "missing-task",
        &["--watch"],
        "second decodelabs-library bundle container deferral should also succeed",
    );
    let token_b = crate::runner::host_container_lease::read_host_container_lease_token_for_tests(
        &repo_b, "web",
    )
    .expect("read lease token for repo b after startup")
    .expect("lease token for repo b after startup");
    assert_ne!(
        token_a, token_b,
        "expected library repos to get separate host-container leases by default"
    );
}

#[test]
fn run_manifest_task_decodelabs_library_bundle_prepares_workspace_permissions_before_exec() {
    let _guard = lock_test();
    let root = temp_workspace("decodelabs-library-permission-prep");
    let shared_root = root.join("libraries/decodelabs");
    let repo = shared_root.join("zest");
    std::fs::create_dir_all(&repo).expect("mkdir repo");
    std::fs::create_dir(repo.join(".git")).expect("git dir");
    write_root_manifest(
        &repo,
        &format!(
            r#"[bundle]
base = "decodelabs-library"
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);

    run_task_expect_empty_output(
        &repo,
        "missing-task",
        &["--watch"],
        "decodelabs-library bundle container deferral should prepare permissions",
    );

    let log = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        log.contains("exec -T -u 0 app sh -lc"),
        "expected root-owned permission prep exec before deferred command, got {log}"
    );
    assert!(
        log.contains("/workspace-root/zest/vendor"),
        "expected permission prep to include isolated vendor target, got {log}"
    );
    assert!(
        log.contains("chown -fR"),
        "expected permission prep chown command, got {log}"
    );
    assert!(
        log.contains("exec -T -w /workspace-root/zest -u dev"),
        "expected deferred workspace exec as dev user after prep, got {log}"
    );
}
