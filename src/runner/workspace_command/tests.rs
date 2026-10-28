mod builder_recovery;
mod provisioning_guest;

use super::*;
use crate::contract_test_support::EnvGuard;
use crate::runner::exec_command::{BoundedCommandBudget, BoundedCommandPhase};
use crate::runner::interactive_session::{
    classify_interactive_session_ownership, InteractiveSessionIntent,
};
use crate::runner::runtime_session_context::{
    with_runtime_session_context, PublicWorkspaceCleanupOverride, RuntimeSessionContext,
};
use crate::runner::test_support::effective_container_policy;
use crate::runner::workspace_command::provisioning::persist_effigy_source_repo_root;
use crate::runner::workspace_command::provisioning::{
    configured_effigy_repo_root, configured_linux_workspace_artifact_source,
    describe_linux_workspace_artifact_refusal, discover_effigy_repo_root,
    ensure_linux_workspace_effigy_artifact, ensure_local_linux_workspace_effigy_artifact,
    ensure_local_linux_workspace_effigy_artifact_bounded, linux_workspace_artifact_lock_path,
    linux_workspace_artifact_offline_hint, linux_workspace_effigy_artifact_is_current,
    linux_workspace_effigy_cache_path, linux_workspace_effigy_release_url,
    linux_workspace_release_cache_is_current, plan_workspace_permission_prep,
    produced_linux_workspace_artifact_is_current, read_trimmed_identity_file,
    read_trimmed_workspace_effigy_active_version, render_workspace_effigy_staging_path,
    resolve_local_effigy_repo_root_from_paths, resolve_local_workspace_effigy_freshness_anchor,
    run_linux_workspace_effigy_artifact_build, sibling_effigy_repo_root,
    wait_for_in_progress_linux_workspace_artifact_build, workspace_effigy_active_version_file,
    workspace_effigy_install_identity_file, workspace_effigy_local_install_identity,
    workspace_effigy_release_install_identity, LinuxWorkspaceArtifactSource, LinuxWorkspaceTarget,
    WorkspacePermissionMode, EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
};
use crate::runner::workspace_command::session::classify_workspace_session_ownership;
use std::sync::atomic::{AtomicU64, Ordering};

fn temp_repo(manifest: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-system-tests-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
    std::fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  workspace:\n    image: example\n",
    )
    .expect("write compose");
    root
}

#[test]
fn permission_plan_keeps_root_bun_tree_and_shallows_child_package_trees() {
    let plan = plan_workspace_permission_prep(&[
        "/home/dev".to_owned(),
        "/workspace-root/acowtancy/node_modules".to_owned(),
        "/workspace-root/acowtancy/dairy/node_modules".to_owned(),
        "/workspace-root/acowtancy/cream/node_modules".to_owned(),
        "/workspace-root/underlay/node_modules".to_owned(),
        "/cache".to_owned(),
    ]);

    assert_eq!(plan.skipped_nested, 0);
    assert_eq!(plan.shallow_dependency_trees, 2);
    assert!(plan.targets.iter().any(|target| {
        target.path == "/workspace-root/acowtancy/node_modules"
            && target.mode == WorkspacePermissionMode::Recursive
    }));
    assert!(plan.targets.iter().any(|target| {
        target.path == "/workspace-root/acowtancy/dairy/node_modules"
            && target.mode == WorkspacePermissionMode::Shallow
    }));
    assert!(plan.targets.iter().any(|target| {
        target.path == "/workspace-root/acowtancy/cream/node_modules"
            && target.mode == WorkspacePermissionMode::Shallow
    }));
    assert!(plan.targets.iter().any(|target| {
        target.path == "/workspace-root/underlay/node_modules"
            && target.mode == WorkspacePermissionMode::Recursive
    }));
}

#[test]
fn permission_plan_skips_targets_nested_under_a_recursive_parent() {
    let plan = plan_workspace_permission_prep(&[
        "/workspace-root/vendor".to_owned(),
        "/workspace-root/vendor/bin".to_owned(),
        "/cache".to_owned(),
    ]);

    assert_eq!(plan.skipped_nested, 1);
    assert!(!plan
        .targets
        .iter()
        .any(|target| target.path == "/workspace-root/vendor/bin"));
    assert!(plan.targets.iter().any(|target| {
        target.path == "/workspace-root/vendor" && target.mode == WorkspacePermissionMode::Recursive
    }));
}

fn test_policy() -> effigy_containers::EffectiveContainerPolicy {
    effective_container_policy("stack", "demo-stack", "workspace", "docker-compose.yml")
}

#[test]
fn resolve_public_workspace_container_uses_implied_default_workspace() {
    let root = temp_repo(
        r#"
[catalog]
alias = "probe"

[containers]
default = "app"

[containers.app]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev]
"#,
    );

    let container = resolve_public_workspace_container(&root, None, None, "workspace")
        .expect("resolve workspace container");

    assert_eq!(container.as_deref(), Some("app"));
}

#[test]
fn resolve_public_workspace_container_rejects_inline_workspace_binding() {
    let root = temp_repo(
        r#"
[catalog]
alias = "probe"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace" }
"#,
    );

    let error = resolve_public_workspace_container(&root, None, None, "workspace")
        .expect_err("inline workspace container should be rejected");

    assert_eq!(
        error.to_string(),
        "`effigy workspace` does not support inline workspace containers yet"
    );
}

use std::path::Path;

/// Fresh repo-shaped fixture, exclusively owned by this test invocation.
fn linux_artifact_fixture(label: &str) -> std::path::PathBuf {
    let root = effigy_test_support::Fixture::thread_lifetime(&format!("effigy-artifact-{label}-"))
        .expect("fresh private artifact fixture");
    std::fs::create_dir_all(root.join("config")).expect("mkdir config");
    std::fs::write(root.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(root.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(root.join("config/containers.toml"), "").expect("write containers");
    root
}

/// Writes an artifact with an optional identity sidecar. The receipt names the
/// x86_64 target and the version sidecar is a deliberately old value.
fn write_linux_artifact(artifact: &Path, bytes: &str, identity: Option<&str>) {
    std::fs::create_dir_all(artifact.parent().expect("artifacts dir")).expect("mkdir artifacts");
    std::fs::write(artifact, bytes).expect("write artifact");
    write_linux_artifact_receipt(artifact, LinuxWorkspaceTarget::X86_64Gnu.release_triple());
    std::fs::write(
        workspace_effigy_active_version_file(artifact),
        "v0.0.0-stale\n",
    )
    .expect("write version");
    if let Some(identity) = identity {
        std::fs::write(
            workspace_effigy_install_identity_file(artifact),
            format!("{identity}\n"),
        )
        .expect("write identity");
    }
}

fn write_current_linux_artifact(artifact: &Path, bytes: &str, identity: &str) {
    write_linux_artifact(artifact, bytes, Some(identity));
    std::fs::write(
        workspace_effigy_active_version_file(artifact),
        effigy_core::build_info::active_version(),
    )
    .unwrap();
}

fn write_linux_artifact_receipt(artifact: &Path, triple: &str) {
    std::fs::write(
        artifact
            .parent()
            .expect("artifacts dir")
            .join("rehearsal.txt"),
        format!("release_triple={triple}\ncompleted_at=2026-10-15T00:00:00Z\n"),
    )
    .expect("write receipt");
}

/// Private fake host. Its body is the whole shell program, so each test chooses
/// exactly what the supported build does: fresh output, stale output, or failure.
fn fake_effigy_host(root: &Path, body: &str) -> std::path::PathBuf {
    let host = root.join("effigy-host");
    std::fs::write(
        &host,
        format!(
            "#!/bin/sh\nset -eu\nEFFIGY_FIXTURE_ACTIVE_VERSION={}\n{body}\n",
            effigy_core::shell::shell_quote(&effigy_core::build_info::active_version())
        ),
    )
    .expect("write host");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&host).expect("stat host").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&host, perms).expect("chmod host");
    }
    host
}

const FRESH_X86_BUILD: &str = r#"touch built.marker
[ "$1" = "workspace:linux:artifact" ] || exit 2
mkdir -p .effigy/linux-release/artifacts
printf 'fresh-bytes\n' > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu
printf '%s\n' "$EFFIGY_FIXTURE_ACTIVE_VERSION" > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu.active-version
printf 'release_triple=x86_64-unknown-linux-gnu\ncompleted_at=2026-10-15T00:00:00Z\n' > .effigy/linux-release/artifacts/rehearsal.txt"#;

const STALE_IDENTITY: &str = "local:v0.0.0:1:1";

#[test]
fn linux_artifact_current_identity_reuses_without_build() {
    let root = linux_artifact_fixture("reuse");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 9");
    let expected = workspace_effigy_local_install_identity(&host).expect("host identity");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_current_linux_artifact(&artifact, "current-bytes", &expected);

    let resolved =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect("reuse current artifact");

    assert_eq!(resolved, artifact);
    assert!(
        !root.join("built.marker").exists(),
        "a current artifact must not start a build"
    );
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("read artifact"),
        "current-bytes"
    );
}

#[test]
fn linux_artifact_matching_identity_cannot_overwrite_stale_or_missing_version() {
    for version in [Some("v0.0.0-stale"), None] {
        let root = linux_artifact_fixture("version-mismatch");
        let host = fake_effigy_host(&root, "exit 9");
        let expected = workspace_effigy_local_install_identity(&host).unwrap();
        let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "old bytes", Some(&expected));
        let sidecar = workspace_effigy_active_version_file(&artifact);
        if version.is_none() {
            std::fs::remove_file(&sidecar).unwrap();
        }
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect_err("matching identity cannot bless a conflicting/unknown version");
        assert_eq!(read_trimmed_identity_file(&sidecar).as_deref(), version);
        assert_eq!(std::fs::read_to_string(&artifact).unwrap(), "old bytes");
    }
}

#[test]
fn linux_artifact_newer_host_rebuilds_and_binds_fresh_identity() {
    let root = linux_artifact_fixture("newer-host");
    let host = fake_effigy_host(&root, "exit 9");
    let old_identity = workspace_effigy_local_install_identity(&host).expect("old identity");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(&old_identity));

    std::thread::sleep(std::time::Duration::from_millis(10));
    let host = fake_effigy_host(&root, FRESH_X86_BUILD);
    let new_identity = workspace_effigy_local_install_identity(&host).expect("new identity");
    assert_ne!(old_identity, new_identity);

    let resolved =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect("rebuild stale artifact");

    assert_eq!(resolved, artifact);
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("read artifact"),
        "fresh-bytes\n"
    );
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(new_identity.as_str())
    );
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact)).as_deref(),
        Some(effigy_core::build_info::active_version().as_str())
    );
}

#[test]
fn linux_artifact_successful_build_without_fresh_output_is_refused() {
    let root = linux_artifact_fixture("exit-zero");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 0");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    std::thread::sleep(std::time::Duration::from_millis(10));

    let error =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect_err("exit zero without fresh output must not hand off");

    assert!(root.join("built.marker").exists(), "the build must run");
    let message = error.to_string();
    assert!(
        message.contains("did not produce a current artifact"),
        "{message}"
    );
    assert!(message.contains("rebuild with `EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact`"), "{message}");
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY)
    );
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact)).as_deref(),
        Some("v0.0.0-stale")
    );
}

#[test]
fn linux_artifact_no_op_rebuild_of_current_version_bytes_is_bound() {
    // A direct `workspace:linux:artifact` leaves current-version bytes with a
    // stale install identity; the no-op rebuild reproduces identical bytes.
    let root = linux_artifact_fixture("noop-current");
    let host = fake_effigy_host(
        &root,
        r#"touch .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu
printf '%s\n' "$EFFIGY_FIXTURE_ACTIVE_VERSION" > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu.active-version
exit 0"#,
    );
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "current-bytes", Some(STALE_IDENTITY));
    std::fs::write(
        workspace_effigy_active_version_file(&artifact),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));

    let bound =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect("identical current-version bytes from a successful rebuild are bound");
    assert_eq!(bound, artifact);
    assert_ne!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY),
        "the install identity is rebound to this host"
    );
}

#[test]
fn linux_artifact_touched_old_bytes_are_refused_in_ordinary_and_bounded_preparation() {
    for bounded in [false, true] {
        let root = linux_artifact_fixture("touched-old-output");
        let host = fake_effigy_host(
            &root,
            r#"touch .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu
printf '%s\n' "$EFFIGY_FIXTURE_ACTIVE_VERSION" > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu.active-version
exit 0"#,
        );
        let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_linux_artifact(&artifact, "unchanged old bytes", Some(STALE_IDENTITY));
        let policy = test_policy();
        let origin = std::time::Instant::now();
        let mut budget = BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(60),
            4096,
            4096,
        );
        let result = if bounded {
            ensure_local_linux_workspace_effigy_artifact_bounded(
                &host,
                &root,
                LinuxWorkspaceTarget::X86_64Gnu,
                &policy,
                &mut budget,
            )
        } else {
            ensure_local_linux_workspace_effigy_artifact(
                &host,
                &root,
                LinuxWorkspaceTarget::X86_64Gnu,
            )
        };
        result
            .expect_err("touch, version rewrite and successful exit cannot bless unchanged bytes");
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                .as_deref(),
            Some(STALE_IDENTITY)
        );
        assert_eq!(
            std::fs::read_to_string(&artifact).unwrap(),
            "unchanged old bytes"
        );
    }
}

#[test]
fn linux_artifact_in_progress_build_rechecks_final_metadata_in_both_paths() {
    for bounded in [false, true] {
        let root = linux_artifact_fixture("in-progress-final");
        let host = fake_effigy_host(&root, "exit 9");
        let expected = workspace_effigy_local_install_identity(&host).unwrap();
        let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        write_current_linux_artifact(&artifact, "transient bytes", &expected);
        let lock = linux_workspace_artifact_lock_path(&root);
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "owned writer").unwrap();
        let writer_artifact = artifact.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::fs::write(
                workspace_effigy_install_identity_file(&writer_artifact),
                STALE_IDENTITY,
            )
            .unwrap();
            std::fs::remove_file(lock).unwrap();
        });
        let policy = test_policy();
        let origin = std::time::Instant::now();
        let mut budget = BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(10),
            4096,
            4096,
        );
        let result = if bounded {
            ensure_local_linux_workspace_effigy_artifact_bounded(
                &host,
                &root,
                LinuxWorkspaceTarget::X86_64Gnu,
                &policy,
                &mut budget,
            )
        } else {
            ensure_local_linux_workspace_effigy_artifact(
                &host,
                &root,
                LinuxWorkspaceTarget::X86_64Gnu,
            )
        };
        writer.join().expect("settle owned build writer");
        result.expect_err("transient current metadata while locked cannot authorize handoff");
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact))
                .as_deref(),
            Some(STALE_IDENTITY)
        );
    }
}

#[test]
fn bounded_linux_artifact_success_without_valid_output_refuses_without_rebinding() {
    for (case, body, expected_bytes, receipt_target) in [
        ("unchanged", ":", Some("old bytes"), "x86_64-unknown-linux-gnu"),
        (
            "removed",
            "rm .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu",
            None,
            "x86_64-unknown-linux-gnu",
        ),
        (
            "wrong-target",
            "printf 'new bytes' > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu\nprintf 'release_triple=aarch64-unknown-linux-gnu\\n' > .effigy/linux-release/artifacts/rehearsal.txt",
            Some("new bytes"),
            "aarch64-unknown-linux-gnu",
        ),
    ] {
        let fixture = effigy_test_support::Fixture::new("effigy-bounded-invalid-output-")
            .expect("owned private artifact fixture");
        let root = fixture.path();
        let host = fake_effigy_host(
            root,
            &format!(
                r#"if [ "$1" = --fixture-ready ]; then
  printf 'fixture-ready'
  exit 0
fi
[ "$1" = workspace:linux:artifact ]
[ "$EFFIGY_LINUX_RELEASE_TRIPLE" = x86_64-unknown-linux-gnu ]
printf 'started\n' >> build.trace
{body}
printf 'completed\n' >> build.trace"#,
            ),
        );
        let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
        // A matching version does not authorize stale identity or wrong-target
        // output. Neither readiness nor the build may rewrite either sidecar.
        write_current_linux_artifact(&artifact, "old bytes", STALE_IDENTITY);

        // Separate cold fixture startup from the refusal oracle. The retained
        // original failure expired at 10.679s before proving any matrix branch.
        // Like the discovery fixtures, establish readiness through the same
        // bounded transport first; the actual build still has its original 10s
        // budget, starts a new process and must finish its exact branch.
        let readiness_origin = std::time::Instant::now();
        let mut readiness = BoundedCommandBudget::new(
            readiness_origin,
            readiness_origin + std::time::Duration::from_secs(30),
            4096,
            4096,
        );
        let ready = readiness
            .run_command(
                root,
                host.as_os_str(),
                &["--fixture-ready".into()],
                None,
                &[],
                BoundedCommandPhase::WorkspacePreparation,
            )
            .expect("private fake build host must become ready");
        assert!(ready.status.success(), "{case}: readiness failed");
        assert_eq!(ready.stdout, b"fixture-ready");
        assert!(ready.stderr.is_empty());
        assert!(!root.join("build.trace").exists());
        eprintln!("{case}: cold fixture readiness {:?}", readiness_origin.elapsed());

        let origin = std::time::Instant::now();
        let mut budget = BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(10),
            4096,
            4096,
        );
        let error = ensure_local_linux_workspace_effigy_artifact_bounded(
            &host,
            root,
            LinuxWorkspaceTarget::X86_64Gnu,
            &test_policy(),
            &mut budget,
        )
        .expect_err("exit zero is insufficient without current target output");
        assert_eq!(
            std::fs::read_to_string(root.join("build.trace")).expect("build progress"),
            "started\ncompleted\n",
            "{case}: the real bounded build must complete exactly once: {error}"
        );
        let message = error.to_string();
        let RunnerError::BoundedExecFailure { json_details, .. } = error else {
            panic!("{case}: expected structured bounded refusal: {message}");
        };
        let details: serde_json::Value = serde_json::from_str(&json_details).expect("refusal JSON");
        assert_eq!(details["failure"], "workspace_preparation_failed", "{case}");
        assert_eq!(details["phase"], "workspace_preparation", "{case}");
        assert_eq!(details["preparation"]["captures"], 1, "{case}");
        assert!(message.contains("build left no current workspace artifact"), "{case}: {message}");
        assert!(message.contains("EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact"), "{case}: {message}");
        assert!(!message.contains(&root.display().to_string()), "{message}");
        assert_eq!(std::fs::read_to_string(&artifact).ok().as_deref(), expected_bytes, "{case}");
        assert!(std::fs::read_to_string(artifact.parent().unwrap().join("rehearsal.txt"))
            .expect("receipt").contains(&format!("release_triple={receipt_target}\n")), "{case}");
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
            Some(STALE_IDENTITY),
            "{case}"
        );
        assert_eq!(
            read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact)).as_deref(),
            Some(effigy_core::build_info::active_version().as_str()),
            "{case}"
        );

        // Genuine expiry remains a separate refusal: no second build can run
        // with an already expired original budget, even after fixture readiness.
        let expired_origin = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let mut expired = BoundedCommandBudget::new(expired_origin, expired_origin, 4096, 4096);
        let error = ensure_local_linux_workspace_effigy_artifact_bounded(
            &host, root, LinuxWorkspaceTarget::X86_64Gnu, &test_policy(), &mut expired,
        ).expect_err("expired preparation must refuse before another build");
        let RunnerError::BoundedExecFailure { json_details, .. } = error else {
            panic!("{case}: expected structured deadline refusal: {error}");
        };
        let details: serde_json::Value = serde_json::from_str(&json_details).expect("deadline JSON");
        assert_eq!(details["failure"], "deadline", "{case}");
        assert_eq!(details["phase"], "workspace_preparation", "{case}");
        assert_eq!(expired.preparation_captures(), 0, "{case}");
        assert_eq!(std::fs::read_to_string(root.join("build.trace")).unwrap(), "started\ncompleted\n");
    }
}

#[test]
fn linux_artifact_wrong_target_build_output_is_refused() {
    let root = linux_artifact_fixture("wrong-target");
    let host = fake_effigy_host(
        &root,
        r#"touch built.marker
mkdir -p .effigy/linux-release/artifacts
printf 'fresh-bytes\n' > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu
printf 'release_triple=aarch64-unknown-linux-gnu\ncompleted_at=2026-10-15T00:00:00Z\n' > .effigy/linux-release/artifacts/rehearsal.txt"#,
    );
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    std::thread::sleep(std::time::Duration::from_millis(10));

    let error =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect_err("wrong-target receipt must not hand off");

    let message = error.to_string();
    assert!(
        message.contains("did not produce a current artifact"),
        "{message}"
    );
    assert!(message.contains("names another target"), "{message}");
}

#[test]
fn linux_artifact_failed_build_never_reuses_stale_bytes() {
    let root = linux_artifact_fixture("failed-build");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 3");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));

    let error =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect_err("failed build must refuse stale bytes");

    let message = error.to_string();
    assert!(message.contains("is not current"), "{message}");
    assert!(
        message.contains(&format!("artifact identity `{STALE_IDENTITY}`")),
        "{message}"
    );
    assert!(
        message.contains(&format!(
            "host version `{}`",
            effigy_core::build_info::active_version()
        )),
        "{message}"
    );
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("read artifact"),
        "old-bytes"
    );
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some(STALE_IDENTITY)
    );
}

#[test]
fn linux_artifact_unlabelled_bytes_are_never_stamped_current() {
    let root = linux_artifact_fixture("unlabelled");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 3");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "unknown-bytes", None);

    let error =
        ensure_local_linux_workspace_effigy_artifact(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
            .expect_err("unlabelled bytes must not hand off");

    assert!(error.to_string().contains("artifact identity `unknown`"));
    assert!(!workspace_effigy_install_identity_file(&artifact).exists());
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_active_version_file(&artifact)).as_deref(),
        Some("v0.0.0-stale")
    );
}

#[test]
fn linux_artifact_is_current_only_by_identity_and_receipt_not_timestamps() {
    let root = linux_artifact_fixture("identity-only");
    let host = fake_effigy_host(&root, "exit 9");
    let expected = workspace_effigy_local_install_identity(&host).expect("host identity");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    let target = LinuxWorkspaceTarget::X86_64Gnu;

    write_linux_artifact(&artifact, "bytes", Some(STALE_IDENTITY));
    std::thread::sleep(std::time::Duration::from_millis(10));
    // A freshly rewritten file with an unmatched identity is still stale.
    write_linux_artifact(&artifact, "bytes", Some(STALE_IDENTITY));
    assert!(!linux_workspace_effigy_artifact_is_current(
        &artifact, target, &expected
    ));

    write_current_linux_artifact(&artifact, "bytes", &expected);
    assert!(linux_workspace_effigy_artifact_is_current(
        &artifact, target, &expected
    ));

    write_linux_artifact_receipt(&artifact, LinuxWorkspaceTarget::Aarch64Gnu.release_triple());
    assert!(!linux_workspace_effigy_artifact_is_current(
        &artifact, target, &expected
    ));
}

#[test]
fn linux_artifact_refusal_reports_versions_identities_and_rebuild_command() {
    let root = linux_artifact_fixture("refusal");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "bytes", None);

    let detail = describe_linux_workspace_artifact_refusal(
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
        "local:v9.9.9:2:3",
        &root,
    );

    assert!(
        detail.contains("host identity `local:v9.9.9:2:3`"),
        "{detail}"
    );
    assert!(detail.contains("artifact identity `unknown`"), "{detail}");
    assert!(
        detail.contains("artifact version `v0.0.0-stale`"),
        "{detail}"
    );
    assert!(
        detail.contains(&format!(
            "host version `{}`",
            effigy_core::build_info::active_version()
        )),
        "{detail}"
    );
    assert!(
        detail.contains("rehearsal receipt matches target"),
        "{detail}"
    );
    assert!(
        detail.contains(
            "EFFIGY_LINUX_RELEASE_TRIPLE=x86_64-unknown-linux-gnu effigy workspace:linux:artifact"
        ),
        "{detail}"
    );
}

#[test]
fn linux_artifact_offline_hint_recommends_only_supported_recovery() {
    let refusal =
        "linux workspace artifact `/x/effigy` is not current for `x86_64-unknown-linux-gnu`";
    let hinted = linux_workspace_artifact_offline_hint(
        crate::runner::error::RunnerError::task_invocation("build failed"),
        refusal,
    );
    let message = hinted.to_string();
    assert!(message.contains("build failed"));
    assert!(message.contains(refusal));
    assert!(message.contains("ubuntu:22.04"));
    assert!(message.contains("effigy-linux-release-builder"));
    assert!(message.contains(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV));
    assert!(message.contains("download"));
    assert!(!message.contains("Reuse a cached artifact"));
}

#[test]
fn linux_release_cache_is_current_only_when_identity_names_this_release() {
    let root = linux_artifact_fixture("release-cache");
    let cache = root.join("effigy-x86_64-unknown-linux-gnu");
    std::fs::write(&cache, "downloaded-bytes").expect("write cache");
    let release = workspace_effigy_release_install_identity();

    assert!(!linux_workspace_release_cache_is_current(&cache, &release));
    std::fs::write(
        workspace_effigy_install_identity_file(&cache),
        "release:v0.0.0\n",
    )
    .expect("write other identity");
    assert!(!linux_workspace_release_cache_is_current(&cache, &release));
    std::fs::write(
        workspace_effigy_install_identity_file(&cache),
        format!("{release}\n"),
    )
    .expect("write release identity");
    assert!(!linux_workspace_release_cache_is_current(&cache, &release));
    std::fs::write(
        workspace_effigy_active_version_file(&cache),
        effigy_core::build_info::active_version(),
    )
    .unwrap();
    assert!(linux_workspace_release_cache_is_current(&cache, &release));
}

#[test]
fn bounded_linux_artifact_current_identity_reuses_without_build() {
    let root = linux_artifact_fixture("bounded-reuse");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 9");
    let expected = workspace_effigy_local_install_identity(&host).expect("host identity");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_current_linux_artifact(&artifact, "current-bytes", &expected);
    let policy = test_policy();
    let origin = std::time::Instant::now();
    let mut budget = BoundedCommandBudget::new(
        origin,
        origin + std::time::Duration::from_secs(60),
        1024 * 1024,
        1024 * 1024,
    );

    let resolved = ensure_local_linux_workspace_effigy_artifact_bounded(
        &host,
        &root,
        LinuxWorkspaceTarget::X86_64Gnu,
        &policy,
        &mut budget,
    )
    .expect("reuse current artifact");

    assert_eq!(resolved, artifact);
    assert!(!root.join("built.marker").exists());
}

#[test]
fn bounded_linux_artifact_rebuilds_stale_identity_with_fresh_output() {
    let root = linux_artifact_fixture("bounded-rebuild");
    let host = fake_effigy_host(&root, FRESH_X86_BUILD);
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    std::thread::sleep(std::time::Duration::from_millis(10));
    let policy = test_policy();
    let origin = std::time::Instant::now();
    let mut budget = BoundedCommandBudget::new(
        origin,
        origin + std::time::Duration::from_secs(60),
        1024 * 1024,
        1024 * 1024,
    );

    let resolved = ensure_local_linux_workspace_effigy_artifact_bounded(
        &host,
        &root,
        LinuxWorkspaceTarget::X86_64Gnu,
        &policy,
        &mut budget,
    )
    .expect("bounded rebuild");

    assert_eq!(resolved, artifact);
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("read artifact"),
        "fresh-bytes\n"
    );
}

#[test]
fn bounded_linux_artifact_refuses_stale_bytes_with_static_recovery() {
    let root = linux_artifact_fixture("bounded-stale");
    let host = fake_effigy_host(&root, "touch built.marker\nexit 3");
    let artifact = root.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    write_linux_artifact(&artifact, "old-bytes", Some(STALE_IDENTITY));
    let policy = test_policy();
    let origin = std::time::Instant::now();
    let mut budget = BoundedCommandBudget::new(
        origin,
        origin + std::time::Duration::from_secs(60),
        1024 * 1024,
        1024 * 1024,
    );

    let error = ensure_local_linux_workspace_effigy_artifact_bounded(
        &host,
        &root,
        LinuxWorkspaceTarget::X86_64Gnu,
        &policy,
        &mut budget,
    )
    .expect_err("stale bytes must be refused");

    let message = error.to_string();
    assert!(
        message.contains("could not refresh the stale workspace artifact"),
        "{message}"
    );
    assert!(!message.contains(&root.display().to_string()), "{message}");
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("read artifact"),
        "old-bytes"
    );
}

#[test]
fn discover_effigy_repo_root_walks_up_to_repo_markers() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-root-discovery-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("config")).expect("mkdir config");
    std::fs::create_dir_all(root.join("target/debug")).expect("mkdir debug");
    std::fs::write(root.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(root.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(root.join("config/containers.toml"), "").expect("write containers");

    let discovered = discover_effigy_repo_root(Some(root.join("target/debug").as_path()))
        .expect("discover repo root");
    assert_eq!(discovered, root);
}

#[test]
fn linux_workspace_release_url_matches_published_artifact_shape() {
    assert_eq!(
        linux_workspace_effigy_release_url(LinuxWorkspaceTarget::X86_64Gnu),
        format!(
            "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-x86_64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert_eq!(
        linux_workspace_effigy_release_url(LinuxWorkspaceTarget::Aarch64Gnu),
        format!(
            "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-aarch64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn linux_workspace_artifact_build_runs_built_in_task_with_target_env() {
    let root = std::env::temp_dir().join(format!(
        "effigy-linux-rehearsal-dispatch-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir root");

    let args_path = root.join("args.txt");
    let env_path = root.join("env.txt");
    let host = root.join("effigy-host");
    std::fs::write(
        &host,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$EFFIGY_LINUX_RELEASE_TRIPLE\" > '{}'\n",
            args_path.display(),
            env_path.display()
        ),
    )
    .expect("write host");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&host).expect("stat host").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&host, perms).expect("chmod host");
    }

    run_linux_workspace_effigy_artifact_build(&host, &root, LinuxWorkspaceTarget::X86_64Gnu)
        .expect("run artifact build");

    let args = std::fs::read_to_string(args_path).expect("read args");
    let env = std::fs::read_to_string(env_path).expect("read env");

    assert_eq!(args.trim(), "workspace:linux:artifact");
    assert_eq!(env.trim(), "x86_64-unknown-linux-gnu");
}

#[test]
fn wait_for_in_progress_linux_workspace_artifact_build_reuses_completed_artifact() {
    let root = std::env::temp_dir().join(format!(
        "effigy-linux-artifact-wait-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let artifacts_dir = root.join(".effigy/linux-release/artifacts");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");
    let freshness_anchor = root.join("target/debug/effigy");
    std::fs::create_dir_all(freshness_anchor.parent().expect("anchor parent"))
        .expect("mkdir target");
    std::fs::write(&freshness_anchor, "host").expect("write host");
    let expected_identity =
        workspace_effigy_local_install_identity(&freshness_anchor).expect("anchor identity");
    let artifact = artifacts_dir.join("effigy-x86_64-unknown-linux-gnu");
    let receipt = artifacts_dir.join("rehearsal.txt");
    let lock_path = linux_workspace_artifact_lock_path(&root);
    std::fs::create_dir_all(lock_path.parent().expect("lock parent")).expect("mkdir locks");
    std::fs::write(&lock_path, "{}").expect("write lock");

    let lock_path_clone = lock_path.clone();
    let artifact_clone = artifact.clone();
    let receipt_clone = receipt.clone();
    let expected_identity_clone = expected_identity.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&artifact_clone, "artifact").expect("write artifact");
        std::fs::write(
            workspace_effigy_install_identity_file(&artifact_clone),
            format!("{expected_identity_clone}\n"),
        )
        .expect("write identity");
        std::fs::write(
            workspace_effigy_active_version_file(&artifact_clone),
            effigy_core::build_info::active_version(),
        )
        .unwrap();
        std::fs::write(
            &receipt_clone,
            "release_triple=x86_64-unknown-linux-gnu\ncompleted_at=2026-06-09T00:00:00Z\n",
        )
        .expect("write receipt");
        let _ = std::fs::remove_file(lock_path_clone);
    });

    let reused = wait_for_in_progress_linux_workspace_artifact_build(
        &root,
        &expected_identity,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    )
    .expect("wait succeeds");
    writer.join().expect("settle owned writer");
    assert!(reused);
}

#[test]
fn wait_for_in_progress_linux_workspace_artifact_build_returns_false_without_lock() {
    let root = std::env::temp_dir().join(format!(
        "effigy-linux-artifact-no-wait-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let artifacts_dir = root.join(".effigy/linux-release/artifacts");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");
    let freshness_anchor = root.join("target/debug/effigy");
    std::fs::create_dir_all(freshness_anchor.parent().expect("anchor parent"))
        .expect("mkdir target");
    std::fs::write(&freshness_anchor, "host").expect("write host");
    let expected_identity =
        workspace_effigy_local_install_identity(&freshness_anchor).expect("anchor identity");
    let artifact = artifacts_dir.join("effigy-x86_64-unknown-linux-gnu");

    let reused = wait_for_in_progress_linux_workspace_artifact_build(
        &root,
        &expected_identity,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    )
    .expect("wait succeeds");

    assert!(!reused);
}

#[test]
fn local_workspace_effigy_freshness_anchor_prefers_newest_repo_local_build() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-anchor-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let host = root.join("outside-effigy");
    let target_debug = root.join("target/debug/effigy");
    let local_install = root.join(".local-install/bin/effigy");
    for path in [&target_debug, &local_install] {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir parent");
    }
    std::fs::write(&host, "host").expect("write host");
    std::fs::write(&target_debug, "debug").expect("write target debug");
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&local_install, "local-install").expect("write local install");

    let anchor = resolve_local_workspace_effigy_freshness_anchor(&host, &root);
    assert_eq!(anchor, local_install);
}

#[test]
fn workspace_artifact_source_defaults_to_auto() {
    let _env = EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, None)]);
    assert_eq!(
        configured_linux_workspace_artifact_source().expect("artifact source"),
        LinuxWorkspaceArtifactSource::Auto
    );
}

#[test]
fn workspace_artifact_source_accepts_download_aliases() {
    for value in ["download", "github", "release"] {
        let _env =
            EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, Some(value.to_owned()))]);
        assert_eq!(
            configured_linux_workspace_artifact_source().expect("artifact source"),
            LinuxWorkspaceArtifactSource::Download
        );
    }
}

#[test]
fn workspace_artifact_source_rejects_unknown_values() {
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("weird".to_owned()),
    )]);
    let error = configured_linux_workspace_artifact_source().expect_err("unknown artifact source");
    let rendered = error.to_string();
    assert!(rendered.contains(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV));
    assert!(rendered.contains("auto"));
    assert!(rendered.contains("download"));
}

#[test]
fn workspace_artifact_source_download_bypasses_discoverable_local_repo() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-source-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("consumer");
    let local_effigy = root.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(local_effigy.join("config")).expect("mkdir config");
    std::fs::write(local_effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(local_effigy.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(local_effigy.join("config/containers.toml"), "").expect("write containers");
    let local_artifact =
        local_effigy.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    std::fs::create_dir_all(local_artifact.parent().expect("artifact parent"))
        .expect("mkdir artifact parent");
    std::fs::write(&local_artifact, "local-artifact").expect("write local artifact");

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");
    std::fs::create_dir_all(cache_path.parent().expect("cache parent"))
        .expect("mkdir cache parent");
    std::fs::write(&cache_path, "downloaded-artifact").expect("write cache artifact");
    std::fs::write(
        workspace_effigy_install_identity_file(&cache_path),
        format!("{}\n", workspace_effigy_release_install_identity()),
    )
    .expect("write cache identity");
    std::fs::write(
        workspace_effigy_active_version_file(&cache_path),
        effigy_core::build_info::active_version(),
    )
    .unwrap();

    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let artifact =
        ensure_linux_workspace_effigy_artifact(&workspace, LinuxWorkspaceTarget::X86_64Gnu)
            .expect("resolve artifact");

    assert_eq!(artifact, cache_path);
    let active_version = workspace_effigy_active_version_file(&artifact);
    assert!(active_version.is_file());
    let contents = std::fs::read_to_string(active_version).expect("read active version");
    assert!(!contents.trim().is_empty());
    let install_identity = workspace_effigy_install_identity_file(&artifact);
    assert!(install_identity.is_file());
    let identity_contents =
        std::fs::read_to_string(install_identity).expect("read install identity");
    assert_eq!(
        identity_contents.trim(),
        workspace_effigy_release_install_identity()
    );
}

#[test]
fn resolve_local_effigy_repo_root_ignores_cached_download_artifact_in_auto_mode() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-auto-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-auto-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("workspace-app-reference");
    let local_effigy = root.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(local_effigy.join("config")).expect("mkdir config");
    std::fs::write(local_effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(local_effigy.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(local_effigy.join("config/containers.toml"), "").expect("write containers");
    let local_artifact =
        local_effigy.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    std::fs::create_dir_all(local_artifact.parent().expect("artifact parent"))
        .expect("mkdir artifact parent");
    std::fs::write(&local_artifact, "local-artifact").expect("write local artifact");

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");
    std::fs::create_dir_all(cache_path.parent().expect("cache parent"))
        .expect("mkdir cache parent");
    std::fs::write(&cache_path, "downloaded-artifact").expect("write cache artifact");
    std::fs::write(
        workspace_effigy_install_identity_file(&cache_path),
        format!("{}\n", workspace_effigy_release_install_identity()),
    )
    .expect("write cache identity");

    let _env = EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, None)]);
    let resolved = resolve_local_effigy_repo_root_from_paths(&workspace, None, None);

    assert_eq!(resolved, Some(local_effigy));
}

#[test]
fn workspace_linux_cache_path_is_versioned() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-cache-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");

    assert!(cache_path.starts_with(&temp_home));
    assert!(cache_path
        .display()
        .to_string()
        .contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    assert!(cache_path
        .display()
        .to_string()
        .ends_with("effigy-x86_64-unknown-linux-gnu"));
}

#[test]
fn linux_workspace_target_maps_machine_names() {
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("x86_64"),
        Some(LinuxWorkspaceTarget::X86_64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("amd64"),
        Some(LinuxWorkspaceTarget::X86_64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("aarch64"),
        Some(LinuxWorkspaceTarget::Aarch64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("arm64"),
        Some(LinuxWorkspaceTarget::Aarch64Gnu)
    );
    assert_eq!(LinuxWorkspaceTarget::from_machine("ppc64le"), None);
}

#[test]
fn sibling_effigy_repo_root_prefers_adjacent_effigy_checkout() {
    let parent = std::env::temp_dir().join(format!(
        "effigy-sibling-root-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = parent.join("workspace-app-reference");
    let effigy = parent.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(effigy.join("config")).expect("mkdir config");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("config/containers.toml"), "").expect("write containers");

    let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
    assert_eq!(discovered, effigy);
}

#[test]
fn sibling_effigy_repo_root_discovers_projects_effigy_from_ancestor() {
    let root = std::env::temp_dir().join(format!(
        "effigy-projects-sibling-root-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("legacy/sites/contactpatch");
    let effigy = root.join("projects/effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(effigy.join("config")).expect("mkdir config");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("config/containers.toml"), "").expect("write containers");

    let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
    assert_eq!(discovered, effigy);
}

#[test]
fn configured_effigy_repo_root_reads_host_pointer_file() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-configured-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let effigy = temp_home.join("projects/effigy");
    std::fs::create_dir_all(effigy.join("config")).expect("mkdir config");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("config/tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("config/containers.toml"), "").expect("write containers");
    std::fs::create_dir_all(temp_home.join(".effigy")).expect("mkdir .effigy");
    std::fs::write(
        temp_home.join(".effigy/source.toml"),
        format!("repo_root = \"{}\"\n", effigy.display()),
    )
    .expect("write source config");

    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let discovered = configured_effigy_repo_root().expect("configured repo root");

    assert_eq!(discovered, effigy);
}

#[test]
fn resolve_local_effigy_repo_root_prefers_live_sibling_checkout_over_configured_pointer() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-source-priority-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(temp_home.join(".effigy")).expect("mkdir .effigy");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-source-priority-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("workspace-app-reference");
    let sibling_effigy = root.join("effigy");
    let configured_effigy = temp_home.join("old-effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    for repo in [&sibling_effigy, &configured_effigy] {
        std::fs::create_dir_all(repo.join("config")).expect("mkdir config");
        std::fs::write(repo.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(repo.join("config/tasks.toml"), "").expect("write tasks");
        std::fs::write(repo.join("config/containers.toml"), "").expect("write containers");
    }
    std::fs::write(
        temp_home.join(".effigy/source.toml"),
        format!("repo_root = \"{}\"\n", configured_effigy.display()),
    )
    .expect("write source config");

    let resolved = resolve_local_effigy_repo_root_from_paths(&workspace, None, None);

    assert_eq!(resolved, Some(sibling_effigy));
}

#[test]
fn persist_effigy_source_repo_root_writes_host_pointer_file() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-persisted-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let effigy = temp_home.join("projects/effigy");
    std::fs::create_dir_all(&effigy).expect("mkdir effigy");

    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    persist_effigy_source_repo_root(&effigy).expect("persist source config");
    let raw =
        std::fs::read_to_string(temp_home.join(".effigy/source.toml")).expect("read source config");

    assert!(raw.contains(&format!("repo_root = \"{}\"", effigy.display())));
}

#[test]
fn workspace_session_cleanup_matrix_stays_stable_for_direct_seeded_and_bootstrap_handoff_cases() {
    let standard_cases = [
        (
            "public-started",
            classify_interactive_session_ownership(
                InteractiveSessionIntent::PublicWorkspace,
                false,
                true,
            ),
            true,
        ),
        (
            "public-ready-adopted",
            classify_interactive_session_ownership(
                InteractiveSessionIntent::PublicWorkspace,
                true,
                true,
            ),
            false,
        ),
        (
            "public-routes-completed-here",
            classify_interactive_session_ownership(
                InteractiveSessionIntent::PublicWorkspace,
                true,
                false,
            ),
            true,
        ),
        (
            "seeded-started",
            classify_interactive_session_ownership(
                InteractiveSessionIntent::SeededTask,
                false,
                true,
            ),
            true,
        ),
        (
            "seeded-adopted",
            classify_interactive_session_ownership(
                InteractiveSessionIntent::SeededTask,
                true,
                false,
            ),
            false,
        ),
    ];

    for (label, ownership, expected_cleanup) in standard_cases {
        assert_eq!(
            should_shutdown_started_system(ownership, true),
            expected_cleanup,
            "unexpected cleanup policy for {label}"
        );
    }

    with_runtime_session_context(
        RuntimeSessionContext {
            public_workspace_cleanup: PublicWorkspaceCleanupOverride::ForceStopOnExit,
            ..RuntimeSessionContext::default()
        },
        || {
            let ownership = classify_workspace_session_ownership(
                InteractiveSessionIntent::PublicWorkspace,
                true,
                true,
                None,
            );
            assert!(
                should_shutdown_started_system(ownership, true),
                "bootstrap handoff should force stop-on-exit even for a ready adopted runtime"
            );
        },
    );
}

#[test]
fn explicit_workspace_cleanup_override_forces_stop_on_exit_without_thread_local_context() {
    let ownership = classify_workspace_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        true,
        true,
        Some(PublicWorkspaceCleanupOverride::ForceStopOnExit),
    );

    assert!(
        should_shutdown_started_system(ownership, true),
        "explicit bootstrap handoff override should force stop-on-exit"
    );
}

#[test]
fn explicit_seeded_cleanup_override_forces_stop_on_exit_without_thread_local_context() {
    let ownership = classify_workspace_session_ownership(
        InteractiveSessionIntent::SeededTask,
        true,
        true,
        Some(PublicWorkspaceCleanupOverride::ForceStopOnExit),
    );

    assert!(
        should_shutdown_started_system(ownership, true),
        "explicit bootstrap seeded override should force stop-on-exit"
    );
}

#[test]
fn workspace_shell_exit_prompt_can_preserve_a_runtime_even_when_ownership_would_stop_it() {
    let ownership = classify_workspace_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        false,
        true,
        None,
    );

    assert!(!super::resolve_workspace_shell_exit_cleanup(
        ownership,
        true,
        Some(false),
    ));
}

#[test]
fn workspace_shell_exit_prompt_can_stop_a_runtime_even_when_ownership_would_preserve_it() {
    let ownership = classify_workspace_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        true,
        true,
        None,
    );

    assert!(super::resolve_workspace_shell_exit_cleanup(
        ownership,
        true,
        Some(true),
    ));
}

#[test]
fn workspace_shell_exit_cleanup_falls_back_to_ownership_when_no_prompt_is_available() {
    let ownership = classify_workspace_session_ownership(
        InteractiveSessionIntent::PublicWorkspace,
        true,
        true,
        None,
    );

    assert!(!super::resolve_workspace_shell_exit_cleanup(
        ownership, true, None,
    ));
}

#[test]
fn effective_workspace_repo_override_falls_back_to_repo_root() {
    let repo_root = Path::new("/tmp/demo-repo");
    assert_eq!(
        effective_workspace_repo_override(repo_root, None),
        Some(repo_root.to_path_buf())
    );
    assert_eq!(
        effective_workspace_repo_override(repo_root, Some(PathBuf::from("/tmp/explicit"))),
        Some(PathBuf::from("/tmp/explicit"))
    );
}

#[test]
fn workspace_handoff_preparation_runs_gateway_and_permissions_in_shared_order() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy();
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let routes_were_ready_before_handoff = prepare_workspace_handoff_using(
        WorkspaceHandoffRequest {
            repo_root,
            policy: &policy,
            container_name: Some("web"),
            repo_override: Some(PathBuf::from("/tmp/demo-repo")),
            initial_command: Some("effigy dev"),
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |repo_root, policy| {
                events.lock().expect("events lock").push(format!(
                    "routes-ready:{}:{}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(false)
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |policy| {
                events
                    .lock()
                    .expect("events lock")
                    .push(format!("start-gateway:{}", policy.name));
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |repo_root, policy| {
                events.lock().expect("events lock").push(format!(
                    "register-routes:{}:{}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |repo_root, policy, container_name, repo_override| {
                events.lock().expect("events lock").push(format!(
                    "ensure-provisioning:{}:{}:{container_name:?}:{repo_override:?}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |policy, initial_command| {
                events.lock().expect("events lock").push(format!(
                    "render-transition:{}:{initial_command:?}",
                    policy.name
                ));
                Ok(())
            }
        },
    )
    .expect("prepare workspace handoff");

    assert!(!routes_were_ready_before_handoff);
    assert_eq!(
        *events.lock().expect("events lock"),
        vec![
            "routes-ready:/tmp/demo-repo:stack".to_owned(),
            "start-gateway:stack".to_owned(),
            "ensure-provisioning:/tmp/demo-repo:stack:Some(\"web\"):Some(\"/tmp/demo-repo\")"
                .to_owned(),
            "render-transition:stack:Some(\"effigy dev\")".to_owned(),
        ]
    );
}

#[test]
fn workspace_handoff_preparation_preserves_explicit_repo_override() {
    let repo_root = std::path::Path::new("/tmp/demo-repo");
    let policy = test_policy();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured_clone = std::sync::Arc::clone(&captured);

    prepare_workspace_handoff_using(
        WorkspaceHandoffRequest {
            repo_root,
            policy: &policy,
            container_name: Some("web"),
            repo_override: Some(std::path::PathBuf::from("/tmp/cloned-target")),
            initial_command: Some("effigy dev"),
        },
        |_repo_root, _policy| Ok(true),
        |_policy| Ok(()),
        |_repo_root, _policy| Ok(()),
        move |repo_root, policy, container_name, repo_override| {
            *captured_clone.lock().expect("capture lock") = Some((
                repo_root.to_path_buf(),
                policy.name.clone(),
                container_name.map(str::to_owned),
                repo_override,
            ));
            Ok(())
        },
        |_policy, _initial_command| Ok(()),
    )
    .expect("prepare workspace handoff");

    assert_eq!(
        *captured.lock().expect("capture lock"),
        Some((
            std::path::PathBuf::from("/tmp/demo-repo"),
            "stack".to_owned(),
            Some("web".to_owned()),
            Some(std::path::PathBuf::from("/tmp/cloned-target")),
        ))
    );
}

#[test]
fn workspace_handoff_preparation_registers_routes_when_gateway_surface_is_active() {
    let repo_root = Path::new("/tmp/demo-repo");
    let mut policy = test_policy();
    policy.dns_routes = vec![effigy_containers::EffectiveDnsRoute {
        domain: "clientname.test".to_owned(),
        declared_domain: "clientname.test".to_owned(),
        tls: false,
        port: None,
        service: None,
        target_host: None,
    }];
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));

    prepare_workspace_handoff_using(
        WorkspaceHandoffRequest {
            repo_root,
            policy: &policy,
            container_name: Some("web"),
            repo_override: Some(PathBuf::from("/tmp/demo-repo")),
            initial_command: None,
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |_, _| {
                events.lock().expect("events lock").push("routes-ready");
                Ok(true)
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |_| {
                events.lock().expect("events lock").push("start-gateway");
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |_, _| {
                events.lock().expect("events lock").push("register-routes");
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |_, _, _, _| {
                events
                    .lock()
                    .expect("events lock")
                    .push("ensure-provisioning");
                Ok(())
            }
        },
        {
            let events = std::sync::Arc::clone(&events);
            move |_, _| {
                events
                    .lock()
                    .expect("events lock")
                    .push("render-transition");
                Ok(())
            }
        },
    )
    .expect("prepare workspace handoff");

    assert_eq!(
        *events.lock().expect("events lock"),
        vec![
            "routes-ready",
            "start-gateway",
            "register-routes",
            "ensure-provisioning",
            "render-transition",
        ]
    );
}

#[test]
fn workspace_effigy_staging_path_is_unique() {
    let first = render_workspace_effigy_staging_path();
    let second = render_workspace_effigy_staging_path();
    assert_ne!(first, second);
    assert!(first.starts_with("/tmp/effigy-host-"));
    assert!(second.starts_with("/tmp/effigy-host-"));
}

#[test]
fn workspace_effigy_active_version_reader_trims_and_ignores_missing_files() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-active-version-reader-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir root");
    let missing = root.join("missing.active-version");
    assert_eq!(
        read_trimmed_workspace_effigy_active_version(&missing).expect("missing version"),
        None
    );

    let present = root.join("effigy.active-version");
    std::fs::write(&present, "0.7.1\n").expect("write active version");
    assert_eq!(
        read_trimmed_workspace_effigy_active_version(&present).expect("present version"),
        Some("0.7.1".to_owned())
    );
}

#[test]
fn workspace_effigy_local_install_identity_tracks_anchor_metadata() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-install-identity-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir root");
    let anchor = root.join("effigy");
    std::fs::write(&anchor, "v1").expect("write anchor");

    let first = workspace_effigy_local_install_identity(&anchor).expect("first identity");
    assert!(first.starts_with("local:v"));
    assert_eq!(first.split(':').count(), 4);

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&anchor, "v2-with-different-size").expect("rewrite anchor");
    let second = workspace_effigy_local_install_identity(&anchor).expect("second identity");

    assert_ne!(first, second);
}

/// Prepare consumes an already produced artifact: a same-version artifact with
/// the producer receipt for this target is current, and another version's
/// bytes are not (contract 052 §8.2, §8.9).
#[test]
fn a_produced_local_artifact_is_current_only_for_this_host_version() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-produced-artifact-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir artifact root");
    let artifact = root.join("effigy-x86_64-unknown-linux-gnu");
    std::fs::write(&artifact, "produced bytes").expect("write artifact");
    std::fs::write(
        workspace_effigy_active_version_file(&artifact),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("write version");
    std::fs::write(
        root.join("rehearsal.txt"),
        "release_triple=x86_64-unknown-linux-gnu\nsha256=00\n",
    )
    .expect("write receipt");

    assert!(produced_linux_workspace_artifact_is_current(
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu
    ));

    std::fs::write(workspace_effigy_active_version_file(&artifact), "9.9.9\n")
        .expect("write stale version");
    assert!(
        !produced_linux_workspace_artifact_is_current(&artifact, LinuxWorkspaceTarget::X86_64Gnu),
        "another version's bytes are not this host's artifact"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_failed_shutdown_line_names_the_system_and_the_first_cause_line() {
    let error = RunnerError::task_invocation(
        "docker compose down exited with code 1\nno such service: app",
    );
    assert_eq!(
        workspace_shutdown_failure_line("app", &error),
        "workspace system app shutdown failed: docker compose down exited with code 1"
    );
}
