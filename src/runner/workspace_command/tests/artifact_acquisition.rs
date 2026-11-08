//! Production artifact acquisition: the host Prepare's choice between a
//! development build and a release download, the producer's single-flight, and
//! the refusals that keep stale or unverified bytes from being published.
//!
//! Every host here is a private shell fixture and every release body comes from
//! `RELEASE_FIXTURES`, so no test builds a real artifact or reaches GitHub.

use super::{linux_artifact_fixture, write_linux_artifact};
use crate::contract_test_support::EnvGuard;
use crate::runner::workspace_client::context::{sha256_text, WorkspaceContainerIdentity};
use crate::runner::workspace_command::provisioning::{
    acquire_linux_workspace_effigy_artifact_with, artifact_plan, check_acquired_artifact,
    ensure_downloaded_linux_workspace_effigy_artifact,
    ensure_local_linux_workspace_effigy_artifact, linux_workspace_artifact_lock_path,
    linux_workspace_effigy_artifact_is_current, linux_workspace_effigy_cache_path,
    linux_workspace_effigy_release_url, prepare_workspace_artifact_with,
    read_trimmed_identity_file, verify_guest_artifact, workspace_artifact_receipt_path,
    workspace_effigy_active_version_file, workspace_effigy_install_identity_file,
    workspace_effigy_local_install_identity, workspace_effigy_release_install_identity,
    ArtifactHost, ArtifactPlan, LinuxWorkspaceArtifactSource, LinuxWorkspaceTarget,
    ARTIFACT_LOCK_WAIT, RELEASE_FETCHES, RELEASE_FETCH_DELAY, RELEASE_FIXTURES,
};
use effigy_containers::EffectiveContainerPolicy;
use std::path::{Path, PathBuf};

const X86: LinuxWorkspaceTarget = LinuxWorkspaceTarget::X86_64Gnu;
const X86_ARTIFACT: &str = "effigy-x86_64-unknown-linux-gnu";

/// Writes an executable host at `path` whose body is the whole shell program.
fn host_at(path: &Path, body: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("host parent")).expect("mkdir host dir");
    std::fs::write(
        path,
        format!(
            "#!/bin/sh\nset -eu\nEFFIGY_FIXTURE_ACTIVE_VERSION={}\n{body}\n",
            effigy_core::shell::shell_quote(&effigy_core::build_info::active_version())
        ),
    )
    .expect("write host");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod host");
    path.to_path_buf()
}

/// A producer that writes a fresh artifact for whichever target the host asks
/// for, with a receipt the digest check can authenticate. Every run appends one
/// line to `builds.log`.
const VERIFIED_BUILD: &str = r#"echo built >> builds.log
[ "$1" = "workspace:linux:artifact" ] || exit 2
triple="$EFFIGY_LINUX_RELEASE_TRIPLE"
mkdir -p .effigy/linux-release/artifacts
out=".effigy/linux-release/artifacts/effigy-$triple"
printf 'fresh-bytes\n' > "$out"
printf '%s\n' "$EFFIGY_FIXTURE_ACTIVE_VERSION" > "$out.active-version"
digest=$( (shasum -a 256 "$out" 2>/dev/null || sha256sum "$out") | cut -d' ' -f1)
printf 'release_triple=%s\nsha256=%s\nbuild_kind=workspace-artifact\n' "$triple" "$digest" > "$out.receipt""#;

fn build_count(root: &Path) -> usize {
    std::fs::read_to_string(root.join("builds.log"))
        .map(|log| log.lines().count())
        .unwrap_or(0)
}

/// A scratch base that holds a consumer app, a release host and a nearby
/// checkout, with no repo marker above the host's own directory.
fn release_world(label: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = effigy_test_support::Fixture::thread_lifetime(&format!("artifact-acq-{label}-"))
        .expect("fresh artifact acquisition fixture");
    let app = base.join("app");
    std::fs::create_dir_all(&app).expect("mkdir app");
    let host = host_at(
        &base.join("app").join("bin").join("effigy"),
        "echo built >> builds.log\nexit 9",
    );
    (base, app, host)
}

fn home_guard(base: &Path) -> EnvGuard {
    let home = base.join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    EnvGuard::set_many(&[("HOME", Some(home.display().to_string()))])
}

fn write_release_cache(cache: &Path, bytes: &str) {
    std::fs::create_dir_all(cache.parent().expect("cache parent")).expect("mkdir cache");
    std::fs::write(cache, bytes).expect("write cache bytes");
    let digest = sha256_text(bytes.as_bytes());
    std::fs::write(
        workspace_artifact_receipt_path(cache),
        format!(
            "release_triple={}\nsha256={}\nbuild_kind=release-download\n",
            X86.release_triple(),
            digest.trim_start_matches("sha256:")
        ),
    )
    .expect("write release receipt");
    std::fs::write(
        workspace_effigy_install_identity_file(cache),
        format!("{}\n", workspace_effigy_release_install_identity()),
    )
    .expect("write release identity");
    std::fs::write(
        workspace_effigy_active_version_file(cache),
        effigy_core::build_info::active_version(),
    )
    .expect("write release version");
}

#[test]
fn a_development_host_builds_a_missing_artifact_once_and_publishes_verified_output() {
    let root = linux_artifact_fixture("dev-missing");
    let host = host_at(&root.join(".local-install/bin/effigy"), VERIFIED_BUILD);
    let _home = home_guard(&root);

    let bound = acquire_linux_workspace_effigy_artifact_with(
        &host,
        &root,
        &root,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect("a development host builds its missing artifact");

    assert_eq!(bound.artifact, root.join(X86.artifact_relative_path()));
    assert_eq!(
        bound.install_identity,
        workspace_effigy_local_install_identity(&host).expect("host identity")
    );
    verify_guest_artifact(&bound.artifact, X86).expect("published bytes match their receipt");
    assert_eq!(build_count(&root), 1);

    let reused = acquire_linux_workspace_effigy_artifact_with(
        &host,
        &root,
        &root,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect("a current artifact is reused");
    assert_eq!(reused, bound);
    assert_eq!(build_count(&root), 1, "a current artifact starts no build");
}

#[test]
fn a_development_build_tree_host_is_provenanced_to_its_checkout() {
    let root = linux_artifact_fixture("dev-build-tree");
    let host = host_at(&root.join("target/debug/effigy"), VERIFIED_BUILD);
    let plan = artifact_plan(&host, &root, &root, LinuxWorkspaceArtifactSource::Auto)
        .expect("a checkout's own build output is a development host");
    assert_eq!(plan, ArtifactPlan::Build { checkout: root });
}

#[test]
fn a_release_host_on_a_cold_cache_downloads_the_exact_target_without_a_builder() {
    let (base, app, host) = release_world("cold");
    let _home = home_guard(&base);
    let bytes = "published-release-bytes";
    let url = linux_workspace_effigy_release_url(X86);
    let digest = sha256_text(bytes.as_bytes());
    RELEASE_FIXTURES.with(|fixtures| {
        let mut fixtures = fixtures.borrow_mut();
        fixtures.insert(url.clone(), bytes.as_bytes().to_vec());
        fixtures.insert(
            format!("{url}.sha256"),
            format!("{}  {X86_ARTIFACT}\n", digest.trim_start_matches("sha256:")).into_bytes(),
        );
    });

    let bound = acquire_linux_workspace_effigy_artifact_with(
        &host,
        &app,
        &app,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect("a release host downloads its matching asset");

    let cache = linux_workspace_effigy_cache_path(X86).expect("cache path");
    assert_eq!(bound.artifact, cache);
    assert_eq!(
        bound.install_identity,
        workspace_effigy_release_install_identity()
    );
    assert_eq!(
        std::fs::read_to_string(&cache).expect("cached bytes"),
        bytes
    );
    verify_guest_artifact(&cache, X86).expect("cache is authenticated by its receipt");
    assert_eq!(
        build_count(&base.join("app")),
        0,
        "a release host never runs a builder"
    );
}

#[test]
fn a_valid_release_cache_is_reused_without_fetching() {
    let (base, app, host) = release_world("cache-hit");
    let _home = home_guard(&base);
    let cache = linux_workspace_effigy_cache_path(X86).expect("cache path");
    write_release_cache(&cache, "cached-release-bytes");
    RELEASE_FIXTURES.with(|fixtures| fixtures.borrow_mut().clear());

    let bound = acquire_linux_workspace_effigy_artifact_with(
        &host,
        &app,
        &app,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect("a verified cache is reused; a fetch would fail with no fixture");

    assert_eq!(bound.artifact, cache);
    assert_eq!(
        std::fs::read_to_string(&cache).expect("cached bytes"),
        "cached-release-bytes"
    );
}

#[test]
fn a_nearby_checkout_cannot_turn_a_release_host_into_a_source_build() {
    let (base, app, host) = release_world("nearby");
    let _home = home_guard(&base);
    let nearby = base.join("effigy");
    std::fs::create_dir_all(nearby.join("config")).expect("mkdir nearby config");
    std::fs::write(nearby.join("effigy.toml"), "").expect("nearby manifest");
    std::fs::write(nearby.join("config/tasks.toml"), "").expect("nearby tasks");
    std::fs::write(nearby.join("config/containers.toml"), "").expect("nearby containers");

    assert_eq!(
        artifact_plan(&host, &app, &app, LinuxWorkspaceArtifactSource::Auto).expect("plan"),
        ArtifactPlan::Download,
        "auto never builds from a checkout the release host merely sits near"
    );
    assert_eq!(
        artifact_plan(&host, &app, &app, LinuxWorkspaceArtifactSource::Download).expect("plan"),
        ArtifactPlan::Download
    );
    assert_eq!(
        artifact_plan(&host, &app, &app, LinuxWorkspaceArtifactSource::Local).expect("plan"),
        ArtifactPlan::Build { checkout: nearby },
        "an explicit `local` names a checkout and builds only there"
    );
}

#[test]
fn explicit_source_policies_follow_the_host_provenance() {
    let root = linux_artifact_fixture("policy-dev");
    let dev = host_at(&root.join(".local-install/bin/effigy"), "exit 9");
    let _home = home_guard(&root);
    assert_eq!(
        artifact_plan(&dev, &root, &root, LinuxWorkspaceArtifactSource::Auto).expect("auto"),
        ArtifactPlan::Build {
            checkout: root.clone()
        }
    );
    let refused = artifact_plan(&dev, &root, &root, LinuxWorkspaceArtifactSource::Download)
        .expect_err("a local build never substitutes the published release");
    assert!(refused.to_string().contains("download"), "{refused}");

    let (base, app, release) = release_world("policy-release");
    let _home = home_guard(&base);
    assert_eq!(
        artifact_plan(&release, &app, &app, LinuxWorkspaceArtifactSource::Download)
            .expect("download"),
        ArtifactPlan::Download
    );
    artifact_plan(&release, &app, &app, LinuxWorkspaceArtifactSource::Local)
        .expect_err("`local` with no checkout anywhere names what is missing");

    // A `+local.` stamp with no checkout we can prove refuses both sources.
    let stamped = host_at(&base.join("loose/bin/effigy"), "exit 9");
    std::fs::write(
        workspace_effigy_active_version_file(&stamped),
        "v0.15.0+local.abc1234\n",
    )
    .expect("write stamp");
    for source in [
        LinuxWorkspaceArtifactSource::Auto,
        LinuxWorkspaceArtifactSource::Download,
    ] {
        artifact_plan(&stamped, &app, &app, source)
            .expect_err("an unproven local build neither builds nor downloads");
    }
}

#[test]
fn a_release_cache_with_a_bad_checksum_publishes_nothing() {
    let (base, app, host) = release_world("bad-checksum");
    let _home = home_guard(&base);
    let url = linux_workspace_effigy_release_url(X86);
    RELEASE_FIXTURES.with(|fixtures| {
        let mut fixtures = fixtures.borrow_mut();
        fixtures.insert(url.clone(), b"tampered-bytes".to_vec());
        fixtures.insert(
            format!("{url}.sha256"),
            format!("{}\n", "0".repeat(64)).into_bytes(),
        );
    });

    acquire_linux_workspace_effigy_artifact_with(
        &host,
        &app,
        &app,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect_err("bytes the release does not name are refused");
    let cache = linux_workspace_effigy_cache_path(X86).expect("cache path");
    assert!(
        !cache.exists(),
        "a refused download never reaches the cache"
    );
    assert!(!workspace_artifact_receipt_path(&cache).exists());
}

#[test]
fn a_release_without_a_fetchable_asset_refuses_and_keeps_the_old_cache() {
    let (base, app, host) = release_world("no-asset");
    let _home = home_guard(&base);
    let cache = linux_workspace_effigy_cache_path(X86).expect("cache path");
    write_release_cache(&cache, "older-release-bytes");
    // Make the cache stale for this release: its receipt names other bytes.
    std::fs::write(
        workspace_artifact_receipt_path(&cache),
        format!(
            "release_triple={}\nsha256={}\n",
            X86.release_triple(),
            "1".repeat(64)
        ),
    )
    .expect("stale receipt");
    RELEASE_FIXTURES.with(|fixtures| fixtures.borrow_mut().clear());

    let error = acquire_linux_workspace_effigy_artifact_with(
        &host,
        &app,
        &app,
        LinuxWorkspaceArtifactSource::Auto,
        X86,
    )
    .expect_err("no fetchable asset is an actionable refusal");
    assert!(
        error.to_string().contains("no test release fixture"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(&cache).expect("old bytes stay"),
        "older-release-bytes"
    );
}

#[test]
fn a_failed_build_publishes_nothing_and_releases_its_lock() {
    let root = linux_artifact_fixture("failed-build");
    let host = host_at(
        &root.join(".local-install/bin/effigy"),
        "echo built >> builds.log\nmkdir -p .effigy/linux-release/artifacts\nprintf 'partial' > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu\nexit 3",
    );
    let artifact = root.join(X86.artifact_relative_path());

    ensure_local_linux_workspace_effigy_artifact(&host, &root, X86)
        .expect_err("a failed producer run is refused");
    assert_eq!(build_count(&root), 1);
    assert!(!workspace_effigy_install_identity_file(&artifact).exists());
    assert!(!workspace_effigy_active_version_file(&artifact).exists());

    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(linux_workspace_artifact_lock_path(&root, X86))
        .expect("lock file survives the failed run");
    lock.try_lock()
        .expect("a failed run releases its single-flight lock");
}

#[test]
fn a_build_that_only_touches_old_bytes_is_refused_and_unchanged() {
    let root = linux_artifact_fixture("touched-old");
    let host = host_at(
        &root.join(".local-install/bin/effigy"),
        r#"touch .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu
printf '%s\n' "$EFFIGY_FIXTURE_ACTIVE_VERSION" > .effigy/linux-release/artifacts/effigy-x86_64-unknown-linux-gnu.active-version
exit 0"#,
    );
    let artifact = root.join(X86.artifact_relative_path());
    write_linux_artifact(&artifact, "unchanged old bytes", Some("local:v0.0.0:1:1"));

    ensure_local_linux_workspace_effigy_artifact(&host, &root, X86)
        .expect_err("touch, version rewrite and success cannot bless unchanged bytes");
    assert_eq!(
        read_trimmed_identity_file(&workspace_effigy_install_identity_file(&artifact)).as_deref(),
        Some("local:v0.0.0:1:1")
    );
    assert_eq!(
        std::fs::read_to_string(&artifact).expect("old bytes"),
        "unchanged old bytes"
    );
}

#[test]
fn concurrent_preparations_share_one_producer_run() {
    let root = linux_artifact_fixture("single-flight");
    let host = host_at(
        &root.join(".local-install/bin/effigy"),
        &format!("sleep 1\n{VERIFIED_BUILD}"),
    );
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let (root, host, barrier) = (root.clone(), host.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                ensure_local_linux_workspace_effigy_artifact(&host, &root, X86)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("preparation thread"))
        .collect::<Vec<_>>();

    for result in &results {
        let bound = result.as_ref().expect("both preparations are satisfied");
        assert_eq!(bound.artifact, root.join(X86.artifact_relative_path()));
    }
    assert_eq!(
        build_count(&root),
        1,
        "the waiter finds the first run's output current instead of building"
    );
    assert!(linux_workspace_effigy_artifact_is_current(
        &root.join(X86.artifact_relative_path()),
        X86,
        &workspace_effigy_local_install_identity(&host).expect("host identity"),
    ));
}

const CONTAINER_MODE_MANIFEST: &str = r#"[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace"

[containers.web.services.app]
catalog = "node"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
"#;

/// The same container with no declared `[systems.*]` workspace: a hybrid
/// environment, which never probes, builds or downloads a guest artifact.
const HYBRID_MANIFEST: &str = r#"[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace"

[containers.web.services.app]
catalog = "node"
"#;

/// A checkout-shaped repo whose `effigy.toml` declares the given containers.
fn repo_with_manifest(label: &str, manifest: &str) -> PathBuf {
    let repo = linux_artifact_fixture(label);
    std::fs::write(repo.join("effigy.toml"), manifest).expect("container manifest");
    repo
}

fn web_policy(repo: &Path) -> EffectiveContainerPolicy {
    let mut policy = crate::runner::test_support::effective_container_policy(
        "web",
        "demo-web",
        "app",
        repo.join("compose.yml"),
    );
    policy.repo_root = repo.to_path_buf();
    policy.workspace_user = Some("dev".to_owned());
    policy.workspace_home = Some("/home/dev".to_owned());
    policy
}

/// What the Docker probe reports for a running container of this image family.
fn probed_image(platform: &str, libc: &str) -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: "a".repeat(64),
        vm_boot_id: "boot-fixture".to_owned(),
        platform: platform.to_owned(),
        libc: libc.to_owned(),
    }
}

fn host_for(binary: PathBuf, cwd: &Path, source: LinuxWorkspaceArtifactSource) -> ArtifactHost {
    ArtifactHost {
        binary,
        cwd: cwd.to_path_buf(),
        source,
    }
}

#[test]
fn prepare_for_a_hybrid_environment_never_probes_or_builds() {
    let repo = repo_with_manifest("prepare-hybrid", HYBRID_MANIFEST);
    let host = host_for(
        host_at(
            &repo.join(".local-install/bin/effigy"),
            "echo built >> builds.log\nexit 9",
        ),
        &repo,
        LinuxWorkspaceArtifactSource::Auto,
    );
    let probed = std::cell::Cell::new(false);

    let prepared = prepare_workspace_artifact_with(&repo, &web_policy(&repo), &host, || {
        probed.set(true);
        Ok(probed_image("x86_64", "gnu"))
    })
    .expect("a hybrid environment prepares nothing");

    assert_eq!(prepared, None);
    assert!(
        !probed.get(),
        "no guest identity is probed without a declared workspace"
    );
    assert_eq!(build_count(&repo), 0);
}

#[test]
fn prepare_builds_the_family_the_probed_image_declares() {
    let repo = repo_with_manifest("prepare-family", CONTAINER_MODE_MANIFEST);
    let host = host_for(
        host_at(&repo.join(".local-install/bin/effigy"), VERIFIED_BUILD),
        &repo,
        LinuxWorkspaceArtifactSource::Auto,
    );
    let _home = home_guard(&repo);
    let policy = web_policy(&repo);

    let x86 = prepare_workspace_artifact_with(&repo, &policy, &host, || {
        Ok(probed_image("x86_64", "gnu"))
    })
    .expect("prepare for a glibc x86_64 image")
    .expect("a container-mode workspace acquires its artifact");
    assert_eq!(x86.target, X86);
    assert_eq!(x86.artifact, repo.join(X86.artifact_relative_path()));
    verify_guest_artifact(&x86.artifact, X86).expect("acquired bytes are authenticated");

    let musl = prepare_workspace_artifact_with(&repo, &policy, &host, || {
        Ok(probed_image("aarch64", "musl"))
    })
    .expect("prepare for a musl aarch64 image")
    .expect("a container-mode workspace acquires its artifact");
    assert_eq!(musl.target, LinuxWorkspaceTarget::Aarch64Musl);
    assert!(musl.artifact.ends_with("effigy-aarch64-unknown-linux-musl"));
    assert_eq!(
        build_count(&repo),
        2,
        "each image family is built once, and the first family is reused"
    );
}

#[test]
fn prepare_refuses_an_image_without_a_provable_libc_before_any_build() {
    let repo = repo_with_manifest("prepare-libc", CONTAINER_MODE_MANIFEST);
    let host = host_for(
        host_at(&repo.join(".local-install/bin/effigy"), VERIFIED_BUILD),
        &repo,
        LinuxWorkspaceArtifactSource::Auto,
    );
    let _home = home_guard(&repo);

    let error = prepare_workspace_artifact_with(&repo, &web_policy(&repo), &host, || {
        Ok(probed_image("x86_64", ""))
    })
    .expect_err("an unprovable libc has no artifact to match");

    assert!(error.to_string().contains("C library"), "{error}");
    assert_eq!(build_count(&repo), 0);
}

#[test]
fn prepare_on_a_release_host_downloads_the_probed_family_without_a_builder() {
    let base = effigy_test_support::Fixture::thread_lifetime("artifact-acq-prepare-release-")
        .expect("fixture");
    let repo = base.join("app");
    std::fs::create_dir_all(&repo).expect("mkdir app");
    std::fs::write(repo.join("effigy.toml"), CONTAINER_MODE_MANIFEST).expect("manifest");
    std::fs::create_dir_all(repo.join("config")).expect("mkdir config");
    std::fs::write(repo.join("config/tasks.toml"), "").expect("tasks");
    std::fs::write(repo.join("config/containers.toml"), "").expect("containers");
    let host = host_for(
        host_at(&repo.join("bin/effigy"), "echo built >> builds.log\nexit 9"),
        &repo,
        LinuxWorkspaceArtifactSource::Auto,
    );
    let _home = home_guard(&base);
    let bytes = "released-musl-bytes";
    let url = linux_workspace_effigy_release_url(LinuxWorkspaceTarget::Aarch64Musl);
    let digest = sha256_text(bytes.as_bytes());
    RELEASE_FIXTURES.with(|fixtures| {
        let mut fixtures = fixtures.borrow_mut();
        fixtures.insert(url.clone(), bytes.as_bytes().to_vec());
        fixtures.insert(
            format!("{url}.sha256"),
            format!(
                "{}  effigy-aarch64-unknown-linux-musl\n",
                digest.trim_start_matches("sha256:")
            )
            .into_bytes(),
        );
    });

    let prepared = prepare_workspace_artifact_with(&repo, &web_policy(&repo), &host, || {
        Ok(probed_image("aarch64", "musl"))
    })
    .expect("a release host prepares its probed family")
    .expect("a container-mode workspace acquires its artifact");

    assert_eq!(prepared.target, LinuxWorkspaceTarget::Aarch64Musl);
    assert_eq!(
        prepared.install_identity,
        workspace_effigy_release_install_identity()
    );
    assert_eq!(build_count(&repo), 0, "a release host never runs a builder");
}

#[test]
fn publication_consumes_only_the_family_and_identity_it_acquired() {
    let repo = repo_with_manifest("publish-check", CONTAINER_MODE_MANIFEST);
    let host = host_for(
        host_at(&repo.join(".local-install/bin/effigy"), VERIFIED_BUILD),
        &repo,
        LinuxWorkspaceArtifactSource::Auto,
    );
    let _home = home_guard(&repo);
    let prepared = prepare_workspace_artifact_with(&repo, &web_policy(&repo), &host, || {
        Ok(probed_image("x86_64", "gnu"))
    })
    .expect("prepare")
    .expect("acquired");

    check_acquired_artifact(&prepared, X86).expect("the acquired family publishes");
    check_acquired_artifact(&prepared, LinuxWorkspaceTarget::Aarch64Musl)
        .expect_err("an image that changed family after preparation is refused");

    std::fs::write(
        workspace_effigy_install_identity_file(&prepared.artifact),
        "local:replaced-after-prepare\n",
    )
    .expect("replace identity");
    let error = check_acquired_artifact(&prepared, X86)
        .expect_err("bytes whose identity moved after preparation are not published");
    assert!(
        error.to_string().contains("changed after it was prepared"),
        "{error}"
    );
}

#[test]
fn a_producer_killed_mid_build_publishes_nothing_and_releases_its_lock() {
    let root = linux_artifact_fixture("producer-killed");
    let host = host_at(
        &root.join(".local-install/bin/effigy"),
        "echo built >> builds.log\nmkdir -p .effigy/linux-release/artifacts\nprintf 'partial' > \".effigy/linux-release/artifacts/effigy-$EFFIGY_LINUX_RELEASE_TRIPLE\"\nkill -9 $$",
    );
    let artifact = root.join(X86.artifact_relative_path());

    ensure_local_linux_workspace_effigy_artifact(&host, &root, X86)
        .expect_err("a producer killed before it finishes is a failed build");

    assert_eq!(build_count(&root), 1);
    assert!(!workspace_effigy_install_identity_file(&artifact).exists());
    assert!(!workspace_effigy_active_version_file(&artifact).exists());
    assert!(!linux_workspace_effigy_artifact_is_current(
        &artifact,
        X86,
        &workspace_effigy_local_install_identity(&host).expect("host identity"),
    ));
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(linux_workspace_artifact_lock_path(&root, X86))
        .expect("lock file survives the killed run");
    lock.try_lock()
        .expect("a killed producer releases the single-flight lock");
}

#[test]
fn a_waiter_that_gives_up_builds_and_publishes_nothing() {
    let root = linux_artifact_fixture("waiter-gives-up");
    let host = host_at(&root.join(".local-install/bin/effigy"), VERIFIED_BUILD);
    let artifact = root.join(X86.artifact_relative_path());
    // Another preparation holds this target's build lock for the whole wait.
    let lock_path = linux_workspace_artifact_lock_path(&root, X86);
    std::fs::create_dir_all(lock_path.parent().expect("lock dir")).expect("mkdir locks");
    let holder = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .expect("holder file");
    holder.try_lock().expect("holder takes the lock");
    ARTIFACT_LOCK_WAIT.with(|wait| wait.set(Some(std::time::Duration::from_millis(50))));

    let error = ensure_local_linux_workspace_effigy_artifact(&host, &root, X86)
        .expect_err("a waiter that cannot get the lock in time gives up");
    ARTIFACT_LOCK_WAIT.with(|wait| wait.set(None));
    drop(holder);

    assert!(error.to_string().contains("timed out waiting"), "{error}");
    assert_eq!(
        build_count(&root),
        0,
        "a waiter that gave up never runs the producer"
    );
    assert!(!workspace_effigy_install_identity_file(&artifact).exists());
    assert!(!artifact.exists());
}

#[test]
fn concurrent_release_downloads_share_one_fetch() {
    let (base, _app, _host) = release_world("download-once");
    let _home = home_guard(&base);
    let url = linux_workspace_effigy_release_url(X86);
    let bytes = "shared-release-bytes";
    let digest = sha256_text(bytes.as_bytes());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let (url, barrier) = (url.clone(), barrier.clone());
            let checksum = format!("{}  {X86_ARTIFACT}\n", digest.trim_start_matches("sha256:"));
            std::thread::spawn(move || {
                RELEASE_FIXTURES.with(|fixtures| {
                    let mut fixtures = fixtures.borrow_mut();
                    fixtures.insert(url.clone(), bytes.as_bytes().to_vec());
                    fixtures.insert(format!("{url}.sha256"), checksum.into_bytes());
                });
                RELEASE_FETCH_DELAY.with(|delay| delay.set(std::time::Duration::from_millis(300)));
                barrier.wait();
                let path = ensure_downloaded_linux_workspace_effigy_artifact(X86);
                (path, RELEASE_FETCHES.with(std::cell::Cell::get))
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("download thread"))
        .collect::<Vec<_>>();

    let cache = linux_workspace_effigy_cache_path(X86).expect("cache path");
    for (path, _) in &results {
        assert_eq!(path.as_ref().expect("both callers are satisfied"), &cache);
    }
    let fetches: usize = results.iter().map(|(_, fetches)| fetches).sum();
    assert_eq!(
        fetches, 1,
        "one caller fetches; the other reuses the verified cache"
    );
    verify_guest_artifact(&cache, X86).expect("the single published cache verifies");
}
