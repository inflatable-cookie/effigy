use crate::runner::tests::prelude::{
    assert_deferred_task_case_table, assert_deferred_task_failure_case_table, fs, lock_test,
    run_task_expect_empty_output, temp_workspace, workspace_with_optional_defer_manifest,
    write_executable, write_root_manifest, DeferredTaskCase, DeferredTaskFailureCase, EnvGuard,
};
use effigy_cli::{DeferArgs, TaskInvocation};
use effigy_context::{CapturedEnv, EffigyRuntimeContext};
use effigy_execution::{ExecutionEnvironmentPlan, ExecutionSurface, TaskExecutionRequestBuilder};

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
    let temp_home = root.join("fake-home");
    let gateway_home = temp_home.join(".effigy/gateway");
    fs::create_dir_all(&lease_home).expect("mkdir lease home");
    fs::create_dir_all(&gateway_home).expect("mkdir fake gateway home");
    let env = EnvGuard::set_many(&[
        ("PATH", Some(format!("{}:{old_path}", bin_dir.display()))),
        ("HOME", Some(temp_home.display().to_string())),
        ("EFFIGY_EXECUTABLE", Some(fake_effigy.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        (
            "EFFIGY_GATEWAY_MKCERT_BIN",
            Some(bin_dir.join("mkcert").display().to_string()),
        ),
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

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn php_library_fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/php-library-bundle")
}

fn setup_php_library_path_bundle(root: &std::path::Path) {
    let bundle_dir = root.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle_dir).expect("copy fixture bundle");
}

fn write_container_handoff_external_mount_defer_manifest(root: &std::path::Path) {
    write_root_manifest(
        root,
        r#"[containers]
default = "web"

[containers.web]
driver = "colima"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"

[[containers.web.host.mounts]]
host = "../../libraries"
container = "/var/www/libraries"
external = true

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[defer]
run = "printf deferred > handoff-marker.txt"
run_in = "container"
"#,
    );
}

fn container_handoff_context(root: &std::path::Path) -> EffigyRuntimeContext {
    EffigyRuntimeContext::builder()
        .cwd_override(Some(root.to_path_buf()))
        .captured_env(CapturedEnv {
            container_handoff: Some("1".into()),
            ..Default::default()
        })
        .capture()
        .expect("capture context")
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
fn run_manifest_task_php_app_bundle_defers_inside_container() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);

    run_task_expect_empty_output(
        &root,
        "missing-task",
        &["--watch"],
        "php_app bundle container deferral should succeed",
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
fn run_manifest_task_php_app_bundle_defers_locally_inside_handoff_container() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral-handoff");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    std::fs::create_dir_all(root.join("app")).expect("app dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    write_executable(
        &composer_bin.join("effigy"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            root.join("fake-legacy-effigy.log").display(),
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
        "php_app bundle handoff-local deferral should succeed",
    );

    let docker = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        docker.contains("\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" missing-task --watch"),
        "expected handoff-local deferral to invoke composer-home effigy inside container, got {docker}"
    );
}

#[test]
fn run_manifest_task_container_handoff_deferral_skips_host_mount_validation() {
    let _guard = lock_test();
    let root = temp_workspace("container-deferral-handoff-external-mount");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(&root);
    let context = container_handoff_context(&root);
    let output = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::run_defer(DeferArgs {
            task: TaskInvocation {
                name: "true".to_owned(),
                args: Vec::new(),
            },
            repo_override: None,
            output_json: false,
        })
    })
    .expect("container handoff deferral should skip host mount validation");
    assert_eq!(output, "");

    let marker = fs::read_to_string(root.join("handoff-marker.txt")).expect("read marker");
    assert_eq!(marker, "deferred");
}

#[test]
fn run_manifest_task_container_handoff_implicit_deferral_skips_host_mount_validation() {
    let _guard = lock_test();
    let root = temp_workspace("container-implicit-deferral-handoff-external-mount");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    write_container_handoff_external_mount_defer_manifest(&root);
    let context = container_handoff_context(&root);
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(context.clone())
        .task("missing-task", Vec::new())
        .surface(ExecutionSurface::DirectCli)
        .environment(ExecutionEnvironmentPlan::default().cwd(root.clone()))
        .build()
        .expect("build request");

    let output = crate::runner::command_context::with_runtime_context(&context, || {
        crate::runner::execute::api::run_manifest_task_request(request)
    })
    .expect("implicit container handoff deferral should skip host mount validation");
    assert_eq!(output, "");

    let marker = fs::read_to_string(root.join("handoff-marker.txt")).expect("read marker");
    assert_eq!(marker, "deferred");
}

#[test]
fn run_manifest_task_php_app_handoff_local_deferral_prefers_composer_global_bin() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral-handoff-composer-bin");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    std::fs::create_dir_all(root.join("app")).expect("app dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
"#,
    );
    let (docker_log, _env) = setup_fake_docker_deferral_runtime(&root, false);
    let composer_home = root.join("composer-home");
    let composer_bin = composer_home.join("vendor/bin");
    fs::create_dir_all(&composer_bin).expect("mkdir composer bin");
    write_executable(
        &composer_bin.join("effigy"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            root.join("fake-legacy-effigy.log").display(),
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
        "php_app bundle handoff-local deferral should prefer composer global bin",
    );

    let docker = fs::read_to_string(&docker_log).expect("read fake docker log");
    assert!(
        docker.contains(
            "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" missing-task --watch"
        ),
        "expected composer-home effigy path in handoff-local deferral command, got {docker}"
    );
}

#[test]
fn run_manifest_task_php_app_container_lease_reaper_shuts_down_expired_env() {
    let _guard = lock_test();
    let root = temp_workspace("php_app-container-deferral-reaper");
    std::fs::create_dir(root.join(".git")).expect("git dir");
    setup_php_library_path_bundle(&root);
    write_root_manifest(
        &root,
        r#"[bundle]
base = { type = "path", dir = "bundles/php-library" }
project_name = "legacy-dev"
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
        "php_app bundle container deferral should succeed",
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
fn run_manifest_task_php_app_library_bundle_isolates_leases_across_repos_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("php_library-shared-lease");
    let shared_root = root.join("libraries/php-app");
    let repo_a = shared_root.join("clockwork");
    let repo_b = shared_root.join("clocksmith");
    std::fs::create_dir_all(&repo_a).expect("mkdir repo a");
    std::fs::create_dir_all(&repo_b).expect("mkdir repo b");
    std::fs::create_dir(repo_a.join(".git")).expect("git dir a");
    std::fs::create_dir(repo_b.join(".git")).expect("git dir b");
    let bundle_a = repo_a.join("bundles/php-library");
    let bundle_b = repo_b.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle_a).expect("copy fixture bundle a");
    copy_dir_all(&php_library_fixture_dir(), &bundle_b).expect("copy fixture bundle b");
    write_root_manifest(
        &repo_a,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
shared_root = "{}"
"#,
            shared_root.display()
        ),
    );
    write_root_manifest(
        &repo_b,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
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
        "php_library bundle container deferral should succeed",
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
        "second php_library bundle container deferral should also succeed",
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
fn run_manifest_task_php_app_library_bundle_prepares_workspace_permissions_before_exec() {
    let _guard = lock_test();
    let root = temp_workspace("php_library-permission-prep");
    let shared_root = root.join("libraries/php-app");
    let repo = shared_root.join("zest");
    std::fs::create_dir_all(&repo).expect("mkdir repo");
    std::fs::create_dir(repo.join(".git")).expect("git dir");
    let bundle = repo.join("bundles/php-library");
    copy_dir_all(&php_library_fixture_dir(), &bundle).expect("copy fixture bundle");
    write_root_manifest(
        &repo,
        &format!(
            r#"[bundle]
base = {{ type = "path", dir = "bundles/php-library" }}
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
        "php_library bundle container deferral should prepare permissions",
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
