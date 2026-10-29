//! Container-mode Prepare and the guest's version lock, over a fake guest.
//!
//! Each test drives the same publication, verification and local-read code the
//! Compose backend runs; only the container filesystem, the pinned artifact and
//! the mounted checkout are fixtures.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use effigy_containers::EffectiveContainerPolicy;
use effigy_test_support::{process::FixtureCommandExt, Fixture};

use crate::contract_test_support::EnvGuard;
use crate::runner::workspace_client::context::{
    binary_path, context_path, read_published_context, sha256_text, snapshot_path, CheckoutReader,
    ContextRefusalCode, GuestLocalReads, GuestReader, GuestWorkspace, LocalRead, LocalReadError,
    LocalReadOutcome, WorkspaceChannelLocator, WorkspaceContainerIdentity, WorkspaceContext,
    WorkspaceEnrolmentCredential, WorkspaceEnvironmentIdentity, BINARY_CURRENT_PATH,
    CHANNEL_PROTOCOL_VERSION, COMPOSED_MANIFEST_KIND, CONTEXT_CURRENT_PATH, ENROL_CREDENTIAL_PATH,
    ENROL_ROOT, HOST_REPREPARE_COMMAND,
};
use crate::runner::workspace_client::guest_fake::{hex_sha256, DiskGuest, FakeGuest};
use crate::runner::workspace_command::provisioning::{
    environment_generation, manifest_fragment_digest, operator_workspace_for_policy,
    parse_container_identity, prepare_workspace_guest, stage_host_bytes,
    verify_pinned_guest_binary, workspace_effigy_active_version_file, LiveWorkspaceEntry,
    OperatorWorkspace, PreparedWorkspaceGuest, WorkspacePrepareFacts,
    EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, WORKSPACE_ARTIFACT_RECEIPT_FILE,
};

const ENTRY_PATH: &str = "/usr/local/bin/effigy";
const HOST_CONTAINER_ID: &str = "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4";

fn fixture(label: &str) -> PathBuf {
    let root = Fixture::process_lifetime(&format!("effigy-workspace-guest-{label}-"))
        .expect("fixture directory");
    std::fs::create_dir_all(root.join(".git")).expect("git directory");
    std::fs::write(root.join("effigy.toml"), "[tasks]\n").expect("manifest");
    root
}

fn policy(repo: &Path, workspace: bool) -> EffectiveContainerPolicy {
    let mut policy = crate::runner::test_support::effective_container_policy(
        "dev",
        "dev",
        "app",
        repo.join("compose.yml"),
    );
    policy.repo_root = repo.to_path_buf();
    if workspace {
        policy.workspace_user = Some("vscode".to_owned());
        policy.workspace_home = Some("/home/vscode".to_owned());
    }
    policy
}

fn operator_workspace() -> OperatorWorkspace {
    OperatorWorkspace {
        user: "vscode".to_owned(),
        home: "/home/vscode".to_owned(),
        guest_checkout: "/workspace".to_owned(),
    }
}

/// The guest's own view of a container with the given id. Publication records
/// the host-verified full id; a hostname or prefix cannot replace it.
fn observed_identity(container_id: &str) -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: container_id.to_owned(),
        vm_boot_id: "3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41".to_owned(),
        platform: "x86_64".to_owned(),
        libc: "gnu".to_owned(),
    }
}

fn container_identity() -> WorkspaceContainerIdentity {
    observed_identity(HOST_CONTAINER_ID)
}

fn local_identity() -> WorkspaceEnvironmentIdentity {
    WorkspaceEnvironmentIdentity {
        resource_id: "env:dev".to_owned(),
        generation: "7".to_owned(),
    }
}

/// An isolated fake activation fence for tests that do not run the engine.
fn local_fence(environment: &str) -> effigy_kernel::run::Fence {
    use effigy_kernel::id::{CheckoutId, CheckoutKind, Generation, ResourceId};
    effigy_kernel::run::Fence {
        resource: ResourceId::environment(
            CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]),
            environment,
        )
        .unwrap(),
        generation: Generation::new(7),
    }
}

fn guest() -> FakeGuest {
    FakeGuest::new().with_container(container_identity())
}

/// A pinned artifact fixture: the exact bytes, the version receipt and the
/// producer receipt that authenticates them.
fn artifact(repo: &Path, bytes: &str) -> PathBuf {
    artifact_with_receipt(repo, bytes, Some(&sha256_text(bytes.as_bytes())))
}

fn artifact_with_receipt(repo: &Path, bytes: &str, receipt_digest: Option<&str>) -> PathBuf {
    let path = repo.join("artifacts/effigy-x86_64-unknown-linux-gnu");
    std::fs::create_dir_all(path.parent().expect("artifact parent")).expect("artifact directory");
    std::fs::write(&path, bytes).expect("artifact bytes");
    std::fs::write(
        workspace_effigy_active_version_file(&path),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("active version");
    let mut receipt = "release_triple=x86_64-unknown-linux-gnu\n".to_string();
    if let Some(digest) = receipt_digest {
        receipt.push_str(&format!(
            "sha256={}\n",
            digest.strip_prefix("sha256:").unwrap_or(digest)
        ));
    }
    std::fs::write(
        path.parent().unwrap().join(WORKSPACE_ARTIFACT_RECEIPT_FILE),
        receipt,
    )
    .expect("artifact receipt");
    path
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    repo: &Path,
    guest: &mut dyn crate::runner::workspace_client::context::GuestWorkspace,
    artifact: &Path,
    workspace: Option<OperatorWorkspace>,
    session_run_id: Option<&str>,
    enrolment: Option<WorkspaceEnrolmentCredential>,
    container: WorkspaceContainerIdentity,
) -> Option<PreparedWorkspaceGuest> {
    let policy = policy(repo, workspace.is_some());
    prepare_workspace_guest(
        guest,
        artifact,
        crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Gnu,
        WorkspacePrepareFacts {
            repo_root: repo,
            policy: &policy,
            operator_workspace: workspace,
            session_run_id: session_run_id.map(str::to_owned),
            enrolment,
            channel: WorkspaceChannelLocator {
                endpoint: Some("host.docker.internal:47121".to_owned()),
                server_identity: Some("sha256:server".to_owned()),
            },
            backend: crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                kind: "Docker Desktop".to_owned(),
                profile: "default".to_owned(),
                runtime: None,
                vm_type: None,
            },
            container,
            environment: local_fence("dev"),
            artifact_install_identity: None,
        },
    )
    .expect("prepare")
}

/// Prepare and expect a refusal, returning its message.
fn prepare_refusal(repo: &Path, guest: &mut FakeGuest, artifact: &Path) -> String {
    let policy = policy(repo, true);
    prepare_workspace_guest(
        guest,
        artifact,
        crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Gnu,
        WorkspacePrepareFacts {
            repo_root: repo,
            policy: &policy,
            operator_workspace: Some(operator_workspace()),
            session_run_id: Some("run-7".to_owned()),
            enrolment: None,
            channel: WorkspaceChannelLocator::default(),
            backend: crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                kind: "Docker Desktop".to_owned(),
                profile: "default".to_owned(),
                runtime: None,
                vm_type: None,
            },
            container: container_identity(),
            environment: local_fence("dev"),
            artifact_install_identity: None,
        },
    )
    .err()
    .map(|error| error.to_string())
    .unwrap_or_else(|| panic!("prepare must refuse"))
}

fn published(repo: &Path, guest: &mut FakeGuest) -> PreparedWorkspaceGuest {
    let artifact = artifact(repo, "pinned guest bytes");
    prepare(
        repo,
        guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("container mode prepares a guest")
}

/// The live facts the entry check compares a context to.
fn entry() -> LiveWorkspaceEntry {
    LiveWorkspaceEntry {
        environment: local_identity(),
    }
}

fn live(generation: u64) -> crate::runner::workspace_client::context::LiveGuestRun {
    crate::runner::workspace_client::context::LiveGuestRun {
        binary: crate::runner::workspace_client::context::GuestBinaryIdentity {
            version: effigy_core::build_info::active_version(),
            digest: String::new(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        container: container_identity(),
        environment_generation: local_identity().generation,
        session_run_id: Some("run-7".to_owned()),
        context_generation: generation,
    }
}

/// A reader that counts the bytes a bounded caller is charged: everything
/// `read_file` returns plus every digest the probe prints, never the bytes of
/// the file being authenticated.
struct CountingGuest<'a> {
    inner: &'a mut DiskGuest,
    bytes: u64,
}

impl GuestWorkspace for CountingGuest<'_> {
    fn read_link(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, crate::runner::error::RunnerError> {
        let target = self.inner.read_link(path)?;
        self.bytes += target.as_ref().map_or(0, |target| target.len() as u64);
        Ok(target)
    }

    fn ensure_dir(
        &mut self,
        path: &str,
        mode: u32,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.ensure_dir(path, mode)
    }

    fn write_file(
        &mut self,
        path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.write_file(path, bytes, mode)
    }

    fn set_owner(
        &mut self,
        path: &str,
        owner: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.set_owner(path, owner)
    }

    fn stage_host_file(
        &mut self,
        host_path: &Path,
        guest_path: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.stage_host_file(host_path, guest_path)
    }

    fn install_executable(
        &mut self,
        staging: &str,
        path: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.install_executable(staging, path)
    }

    fn replace_pointer(
        &mut self,
        path: &str,
        target: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.replace_pointer(path, target)
    }

    fn replace_link(
        &mut self,
        path: &str,
        target: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.inner.replace_link(path, target)
    }

    fn probe_container_identity(
        &mut self,
    ) -> Result<WorkspaceContainerIdentity, crate::runner::error::RunnerError> {
        self.inner.probe_container_identity()
    }
}

impl crate::runner::workspace_client::context::GuestReader for CountingGuest<'_> {
    fn read_file(
        &mut self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, crate::runner::error::RunnerError> {
        let bytes = self.inner.read_file(path)?;
        self.bytes += bytes.as_ref().map_or(0, Vec::len) as u64;
        Ok(bytes)
    }

    fn file_sha256(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, crate::runner::error::RunnerError> {
        let digest = self.inner.file_sha256(path)?;
        self.bytes += digest.as_ref().map_or(0, String::len) as u64;
        Ok(digest)
    }
}

/// A mounted checkout over a real directory, refusing anything outside it.
struct FixtureCheckout {
    root: PathBuf,
    reads: Vec<String>,
}

impl FixtureCheckout {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            reads: Vec::new(),
        }
    }
}

impl CheckoutReader for FixtureCheckout {
    fn read_checkout_file(
        &mut self,
        relative: &str,
    ) -> Result<Option<Vec<u8>>, crate::runner::error::RunnerError> {
        if relative.starts_with('/') || relative.split('/').any(|part| part == "..") {
            return Err(crate::runner::error::RunnerError::task_invocation(format!(
                "`{relative}` is not a checkout-relative path"
            )));
        }
        self.reads.push(relative.to_owned());
        match std::fs::read(self.root.join(relative)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(crate::runner::error::RunnerError::task_invocation(
                error.to_string(),
            )),
        }
    }
}

fn published_context(guest: &mut FakeGuest) -> WorkspaceContext {
    read_published_context(guest)
        .expect("read")
        .expect("prepared")
        .context
}

// ---------------------------------------------------------------- publication

#[test]
fn prepare_publishes_a_version_locked_binary_and_an_immutable_context() {
    let repo = fixture("publish");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();

    let prepared = published(&repo, &mut guest);

    assert_eq!(prepared.context_generation, 1);
    assert_eq!(guest.text(CONTEXT_CURRENT_PATH), Some("1".to_owned()));
    let document = context_path(1);
    assert_eq!(
        guest.mode(&document),
        Some(0o444),
        "a generation is read-only"
    );
    assert_eq!(
        guest.mode("/run/effigy/context/1"),
        Some(0o555),
        "a generation directory is sealed"
    );
    let rendered = guest.text(&document).expect("context document");
    assert!(rendered.contains("\"schema\":\"effigy.workspace-context.v1\""));
    assert!(rendered.contains(&format!(
        "\"version\":\"{}\"",
        effigy_core::build_info::active_version()
    )));
    assert!(rendered.contains("\"protocol\":1"));
    // The host-verified full container id, not the container's hostname.
    assert!(rendered.contains(HOST_CONTAINER_ID));
    assert!(rendered.contains("3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41"));
    assert!(rendered.contains("\"session_run_id\":\"run-7\""));
    assert!(rendered.contains("\"generation\":\"7\""));
    assert!(rendered.contains("\"kind\":\"manifest\""));
    assert!(rendered.contains("\"/workspace/effigy.toml\""));

    let executable = binary_path(&prepared.binary.digest);
    assert_eq!(guest.mode(&executable), Some(0o555));
    assert_eq!(
        guest.text(&executable),
        Some("pinned guest bytes".to_owned())
    );
    assert_eq!(
        guest.file_sha256(&executable).unwrap(),
        Some(hex_sha256(b"pinned guest bytes"))
    );
    assert_eq!(
        guest.text(BINARY_CURRENT_PATH),
        Some(prepared.binary.digest)
    );
    assert_eq!(guest.link_target(ENTRY_PATH), Some(executable));
}

#[test]
fn an_unchanged_publication_reuses_its_generation() {
    let repo = fixture("unchanged");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let artifact = artifact(&repo, "pinned guest bytes");

    let first = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("first prepare");
    let second = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("second prepare");

    assert_eq!(first.context_generation, second.context_generation);
    assert_eq!(guest.text(CONTEXT_CURRENT_PATH), Some("1".to_owned()));
    assert!(
        !guest.paths().contains(&"/run/effigy/context/2"),
        "an unchanged publication writes no second generation"
    );
}

#[test]
fn a_changed_fact_publishes_a_new_generation_without_editing_the_old_one() {
    let repo = fixture("regeneration");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let artifact = artifact(&repo, "pinned guest bytes");
    prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("first prepare");
    let first_generation = guest.text(&context_path(1)).expect("first document");

    std::fs::write(
        repo.join("effigy.toml"),
        "[tasks]\ndev = { run = \"true\" }\n",
    )
    .expect("manifest change");
    let second = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-8"),
        None,
        container_identity(),
    )
    .expect("second prepare");

    assert_eq!(second.context_generation, 2);
    assert_eq!(guest.text(CONTEXT_CURRENT_PATH), Some("2".to_owned()));
    assert_eq!(
        guest.text(&context_path(1)),
        Some(first_generation),
        "a published generation is never edited"
    );
    assert!(guest
        .text(&context_path(2))
        .expect("second document")
        .contains("run-8"));
}

#[test]
fn hybrid_preparation_publishes_no_guest_at_all() {
    let repo = fixture("hybrid");
    let mut guest = guest();
    let artifact = artifact(&repo, "pinned guest bytes");

    let prepared = prepare(
        &repo,
        &mut guest,
        &artifact,
        None,
        None,
        None,
        container_identity(),
    );

    assert_eq!(prepared, None);
    assert!(
        guest.paths().is_empty(),
        "hybrid mode writes no binary, context, listener or enrolment: {:?}",
        guest.paths()
    );
}

#[test]
fn a_linked_worktree_keeps_its_own_checkout_identity_and_path_map() {
    let repo = fixture("linked-worktree");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let primary = repo.join("primary");
    std::fs::create_dir_all(primary.join(".git")).expect("primary git");
    std::fs::write(primary.join("effigy.toml"), "[tasks]\n").expect("manifest");
    let linked = repo.join("linked");
    std::fs::create_dir_all(&linked).expect("linked worktree directory");
    let worktree_git_dir = primary.join(".git/worktrees/linked");
    std::fs::create_dir_all(&worktree_git_dir).expect("worktree git dir");
    std::fs::write(worktree_git_dir.join("commondir"), "../..\n").expect("commondir");
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", worktree_git_dir.display()),
    )
    .expect("worktree git file");
    std::fs::write(linked.join("effigy.toml"), "[tasks]\n").expect("manifest");
    let artifact = artifact(&repo, "pinned guest bytes");

    let mut primary_guest = guest();
    let primary_prepared = prepare(
        &primary,
        &mut primary_guest,
        &artifact,
        Some(operator_workspace()),
        None,
        None,
        container_identity(),
    )
    .expect("primary prepare");
    let mut linked_guest = guest();
    let linked_prepared = prepare(
        &linked,
        &mut linked_guest,
        &artifact,
        Some(operator_workspace()),
        None,
        None,
        container_identity(),
    )
    .expect("linked prepare");

    assert_ne!(
        primary_prepared.activation.checkout_id, linked_prepared.activation.checkout_id,
        "a linked worktree is its own checkout"
    );
    let canonical = linked.canonicalize().expect("canonical linked root");
    assert_eq!(
        linked_prepared.paths.host_checkout,
        canonical.display().to_string()
    );
    assert!(linked_guest
        .text(&context_path(1))
        .expect("linked context")
        .contains(&canonical.display().to_string()));
}

#[test]
fn a_second_prepare_never_rewrites_an_installed_generation() {
    let repo = fixture("live-executable");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let first = prepare(
        &repo,
        &mut guest,
        &artifact(&repo, "first pinned bytes"),
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("first prepare");
    let first_executable = binary_path(&first.binary.digest);

    let second = prepare(
        &repo,
        &mut guest,
        &artifact(&repo, "second pinned bytes"),
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("second prepare");

    assert_ne!(first.binary.digest, second.binary.digest);
    assert_eq!(
        guest.text(&first_executable),
        Some("first pinned bytes".to_owned()),
        "the executable a live run started keeps its exact bytes"
    );
    assert_eq!(
        guest.text(BINARY_CURRENT_PATH),
        Some(second.binary.digest.clone())
    );
    assert_eq!(
        guest.link_target(ENTRY_PATH),
        Some(binary_path(&second.binary.digest))
    );
}

#[test]
fn enrolment_is_kept_private_to_the_workspace_user() {
    let repo = fixture("enrolment");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();

    prepare(
        &repo,
        &mut guest,
        &artifact(&repo, "pinned guest bytes"),
        Some(operator_workspace()),
        Some("run-7"),
        Some(WorkspaceEnrolmentCredential::new(
            "7",
            "9d1c0b6a7f5e",
            "enrolment-token",
        )),
        container_identity(),
    )
    .expect("prepare");

    assert_eq!(guest.mode(ENROL_ROOT), Some(0o700));
    assert_eq!(guest.mode(ENROL_CREDENTIAL_PATH), Some(0o600));
    assert_eq!(guest.owner(ENROL_CREDENTIAL_PATH), Some("vscode"));
    assert!(guest
        .text(ENROL_CREDENTIAL_PATH)
        .expect("credential")
        .contains("enrolment-token"));
    assert!(
        !guest
            .text(&context_path(1))
            .expect("context")
            .contains("enrolment-token"),
        "the credential never enters the context"
    );
}

#[test]
fn a_prepare_without_a_credential_publishes_no_enrolment() {
    let repo = fixture("no-enrolment");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();

    published(&repo, &mut guest);

    assert_eq!(guest.node(ENROL_ROOT), None);
    assert_eq!(guest.node(ENROL_CREDENTIAL_PATH), None);
}

// ------------------------------------------------------------- authentication

#[test]
fn an_artifact_without_a_digest_receipt_is_refused() {
    let repo = fixture("no-receipt");
    let mut guest = guest();
    let artifact = artifact_with_receipt(&repo, "pinned guest bytes", None);

    let error = prepare_refusal(&repo, &mut guest, &artifact);

    assert!(error.contains("names no digest"), "{error}");
    assert!(
        guest.paths().is_empty(),
        "nothing is published unauthenticated"
    );
}

#[test]
fn a_receipt_that_names_other_bytes_refuses_the_artifact() {
    let repo = fixture("wrong-digest");
    let mut guest = guest();
    let artifact = artifact_with_receipt(&repo, "pinned guest bytes", Some(&"00".repeat(32)));

    let error = prepare_refusal(&repo, &mut guest, &artifact);

    assert!(error.contains("receipt records"), "{error}");
    assert!(
        guest.paths().is_empty(),
        "nothing is published for wrong bytes"
    );
}

#[test]
fn wrong_installed_bytes_are_refused_at_entry() {
    let repo = fixture("wrong-installed");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let prepared = published(&repo, &mut guest);
    let executable = binary_path(&prepared.binary.digest);
    guest.put_file(&executable, "tampered bytes", 0o555);

    let error = verify_pinned_guest_binary(&mut guest, &entry())
        .expect_err("tampered bytes")
        .to_string();

    assert!(error.contains("is sha256:"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn missing_wrong_and_regular_entry_paths_refuse_before_handoff() {
    let repo = fixture("entry-path");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let guest_root = Fixture::new("effigy-entry-path-").expect("guest root");
    let mut guest = DiskGuest::new(guest_root.path().to_owned(), container_identity());
    let artifact = artifact_bytes(&repo, b"\x7fELF\x02\x01\x01pinned entry");
    let prepared = prepare_disk(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
    )
    .expect("prepare entry");
    let executable = binary_path(&prepared.binary.digest);
    verify_pinned_guest_binary(&mut guest, &entry()).expect("intact entry");
    let before = std::fs::read(guest.host_path(&executable).unwrap()).unwrap();
    let entry_path = guest.host_path(ENTRY_PATH).unwrap();
    std::fs::remove_file(&entry_path).unwrap();
    for state in ["missing", "wrong", "regular"] {
        if state == "wrong" {
            // Matching bytes at a different object do not authorize retargeting.
            guest
                .write_file("/run/effigy/other", &before, 0o555)
                .unwrap();
            guest.replace_link(ENTRY_PATH, "/run/effigy/other").unwrap();
        } else if state == "regular" {
            std::fs::remove_file(&entry_path).unwrap();
            guest.write_file(ENTRY_PATH, &before, 0o555).unwrap();
        }
        let error = verify_pinned_guest_binary(&mut guest, &entry())
            .expect_err(state)
            .to_string();
        assert!(error.contains(ENTRY_PATH), "{state}: {error}");
        assert!(error.contains(HOST_REPREPARE_COMMAND), "{error}");
        assert_eq!(
            std::fs::read(guest.host_path(&executable).unwrap()).unwrap(),
            before
        );
    }
}

#[test]
fn an_unprepared_workspace_refusal_names_the_host_reprepare() {
    let mut guest = guest();

    let error = verify_pinned_guest_binary(&mut guest, &entry())
        .expect_err("unprepared refuses")
        .to_string();

    assert!(error.contains("no prepared guest context"));
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_prepared_workspace_verifies() {
    let repo = fixture("verify");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();

    let prepared = published(&repo, &mut guest);

    verify_pinned_guest_binary(&mut guest, &entry()).expect("prepared verifies");
    assert_eq!(prepared.context_generation, 1);
}

#[test]
fn a_wrong_version_artifact_is_refused() {
    let repo = fixture("wrong-version");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let artifact = artifact(&repo, "pinned guest bytes");
    std::fs::write(workspace_effigy_active_version_file(&artifact), "9.9.9\n")
        .expect("wrong version");

    let error = prepare_refusal(&repo, &mut guest, &artifact);

    assert!(error.contains("9.9.9"));
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_missing_pinned_executable_refuses_and_names_the_reprepare() {
    let repo = fixture("missing-executable");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let prepared = published(&repo, &mut guest);
    let executable = binary_path(&prepared.binary.digest);
    let mut replacement = FakeGuest::new().with_container(container_identity());
    let kept = guest
        .paths()
        .into_iter()
        .filter(|path| *path != executable)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let copied = kept
        .into_iter()
        .map(|path| {
            let bytes = guest.read_file(&path).expect("read");
            (path.clone(), bytes, guest.mode(&path).unwrap_or(0o644))
        })
        .collect::<Vec<_>>();
    for (path, bytes, mode) in copied {
        if let Some(bytes) = bytes {
            replacement.put_file(&path, bytes, mode);
        }
    }

    let error = verify_pinned_guest_binary(&mut replacement, &entry())
        .expect_err("missing executable")
        .to_string();

    assert!(error.contains(&executable));
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

// ------------------------------------------------------------------- fences

#[test]
fn a_replaced_container_refuses_the_context_and_names_the_reprepare() {
    let repo = fixture("replaced-container");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let published = published_context(&mut guest);

    let mut replaced = live(published.generation);
    replaced.binary = published.binary.clone();
    replaced.container.container_id = "replacement".to_owned();
    let refusal = published.verify(&replaced).expect_err("replaced");

    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_short_id_never_proves_the_full_backend_identity() {
    let repo = fixture("short-id");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let published = published_context(&mut guest);

    let mut short = live(published.generation);
    short.binary = published.binary.clone();
    short.container.container_id = HOST_CONTAINER_ID[..12].to_owned();

    let refusal = published
        .verify(&short)
        .expect_err("a prefix is not placement proof");
    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);

    let mut unrelated = short.clone();
    unrelated.container.container_id = "0123456789ab".to_owned();
    let refusal = published.verify(&unrelated).expect_err("another container");
    assert_eq!(refusal.code, ContextRefusalCode::IdentityMismatch);
}

#[test]
fn a_stale_context_generation_refuses_before_any_read() {
    let repo = fixture("stale");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let published = published_context(&mut guest);
    let mut checkout = FixtureCheckout::new(repo.clone());
    let mut reads = GuestLocalReads {
        context: &published,
        guest: &mut guest,
        checkout: &mut checkout,
    };

    let error = reads.read(&LocalRead::Tasks, published.generation + 1);

    assert!(
        matches!(error, Err(LocalReadError::Refused(ref refusal)) if refusal.code == ContextRefusalCode::StaleContext)
    );
    assert!(
        checkout.reads.is_empty(),
        "a stale generation reads nothing"
    );
}

// --------------------------------------------------------------- local reads

#[test]
fn local_static_and_observe_reads_serve_the_pinned_snapshot_only() {
    let repo = fixture("local-reads");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    std::fs::write(
        repo.join("effigy.toml"),
        "[tasks]\nzeta = { run = \"true\" }\nalpha = { run = \"true\" }\n",
    )
    .expect("manifest with tasks");
    std::fs::write(repo.join("NOTES.md"), "checkout notes").expect("checkout file");
    let mut guest = guest();
    published(&repo, &mut guest);
    let context = published_context(&mut guest);
    let before = guest.tree_digest();
    let mut checkout = FixtureCheckout::new(repo.clone());

    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };

    // Static: version and pinned identity come from the context.
    let version = reads
        .read(&LocalRead::Version, context.generation)
        .expect("version");
    match version {
        LocalReadOutcome::Version {
            guest_version,
            digest,
            protocol,
            ..
        } => {
            assert_eq!(guest_version, effigy_core::build_info::active_version());
            assert_eq!(digest, context.binary.digest);
            assert_eq!(protocol, CHANNEL_PROTOCOL_VERSION);
        }
        other => panic!("unexpected version read: {other:?}"),
    }

    // Static: task names from the digest-verified manifest snapshot.
    let tasks = reads
        .read(&LocalRead::Tasks, context.generation)
        .expect("tasks");
    assert_eq!(
        tasks,
        LocalReadOutcome::Tasks(vec!["alpha".to_owned(), "zeta".to_owned()])
    );

    // Static: the pinned source identities the context carries.
    let sources = reads
        .read(&LocalRead::Sources, context.generation)
        .expect("sources");
    match sources {
        LocalReadOutcome::Sources(pins) => {
            let manifest = pins
                .iter()
                .find(|pin| pin.kind == "manifest")
                .expect("manifest pin");
            assert_eq!(manifest.name, "effigy.toml");
            assert_eq!(manifest.snapshot.as_deref(), Some("/workspace/effigy.toml"));
        }
        other => panic!("unexpected sources read: {other:?}"),
    }

    // Observe: a file inside the mounted checkout.
    let file = reads
        .read(
            &LocalRead::CheckoutFile("NOTES.md".to_owned()),
            context.generation,
        )
        .expect("checkout file");
    assert_eq!(
        file,
        LocalReadOutcome::CheckoutFile(b"checkout notes".to_vec())
    );

    // Static and Observe are the classes that never enrol (§8.8).
    assert_eq!(
        LocalRead::Version.class(),
        crate::runner::workspace_client::context::GuestOperationClass::Static
    );
    assert_eq!(
        LocalRead::Tasks.class(),
        crate::runner::workspace_client::context::GuestOperationClass::Static
    );
    assert_eq!(
        LocalRead::CheckoutFile("NOTES.md".to_owned()).class(),
        crate::runner::workspace_client::context::GuestOperationClass::Observe
    );
    assert!(!LocalRead::CheckoutFile("NOTES.md".to_owned())
        .class()
        .requires_enrolment());

    assert_eq!(before, guest.tree_digest(), "a local read writes nothing");
    assert_eq!(
        checkout.reads,
        vec!["effigy.toml".to_owned(), "NOTES.md".to_owned()],
        "Sources come from the context; only the task read and the checkout read touch the mount"
    );
}

#[test]
fn a_manifest_that_changed_since_prepare_is_refused_instead_of_reread() {
    let repo = fixture("changed-manifest");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let context = published_context(&mut guest);
    std::fs::write(repo.join("effigy.toml"), "[tasks]\nchanged = {}\n").expect("edit");
    let mut checkout = FixtureCheckout::new(repo.clone());
    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };

    let error = reads.read(&LocalRead::Tasks, context.generation);

    match error {
        Err(LocalReadError::Refused(refusal)) => {
            assert_eq!(refusal.code, ContextRefusalCode::Unavailable);
            assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
        }
        other => panic!("a changed snapshot must refuse: {other:?}"),
    }
}

#[test]
fn a_checkout_read_cannot_escape_the_mounted_checkout() {
    let repo = fixture("traversal");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let context = published_context(&mut guest);
    let mut checkout = FixtureCheckout::new(repo.clone());
    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };

    for path in ["../effigy.toml", "/etc/passwd"] {
        let error = reads.read(
            &LocalRead::CheckoutFile(path.to_owned()),
            context.generation,
        );
        assert!(
            matches!(error, Err(LocalReadError::Guest(_))),
            "`{path}` must be refused: {error:?}"
        );
    }
}

#[test]
fn an_authority_bearing_command_refuses_without_enrolling() {
    let repo = fixture("authority");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let context = published_context(&mut guest);
    let mut checkout = FixtureCheckout::new(repo.clone());
    let before = guest.tree_digest();
    let error = crate::runner::workspace_client::context::refuse_host_authority("task admission");

    assert!(matches!(
        error,
        LocalReadError::HostAuthorityRequired("task admission")
    ));
    assert!(error.to_string().contains("needs host authority"));
    assert!(error.to_string().contains(HOST_REPREPARE_COMMAND));
    // A local read surface holds no credential, so the refusal enrols nothing.
    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };
    reads
        .read(&LocalRead::Version, context.generation)
        .expect("a local read still works after an authority refusal");
    let unenrolled = guest.node(ENROL_ROOT).is_none();
    assert!(unenrolled);
    assert_eq!(before, guest.tree_digest());
}

// --------------------------------------------------- environment generation

#[test]
fn a_workspace_without_an_activated_environment_has_no_fence_to_publish() {
    let repo = fixture("no-activation");
    let policy = policy(&repo, true);

    let error = environment_generation(&policy)
        .expect_err("no activation record")
        .to_string();

    assert!(error.contains("no activation record"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

// ------------------------------------------------------- the mode boundary

/// A manifest whose primary service carries a catalog default workspace
/// identity but declares no `[systems.*]` workspace.
fn hybrid_manifest(repo: &Path) {
    std::fs::write(
        repo.join("effigy.toml"),
        r#"[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace"

[containers.web.services.app]
catalog = "node"
"#,
    )
    .expect("hybrid manifest");
}

/// A policy for the `web` container the boundary fixtures declare.
fn web_policy(repo: &Path, user: &str, home: &str) -> EffectiveContainerPolicy {
    let mut policy = crate::runner::test_support::effective_container_policy(
        "web",
        "demo-web",
        "app",
        repo.join("compose.yml"),
    );
    policy.repo_root = repo.to_path_buf();
    policy.workspace_user = Some(user.to_owned());
    policy.workspace_home = Some(home.to_owned());
    policy
}

fn container_mode_manifest(repo: &Path) {
    std::fs::write(
        repo.join("effigy.toml"),
        r#"[containers]
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
"#,
    )
    .expect("container-mode manifest");
}

#[test]
fn a_catalog_default_workspace_identity_does_not_promote_hybrid_to_container_mode() {
    let repo = fixture("hybrid-boundary");
    hybrid_manifest(&repo);
    // A catalog service's default identity resolves a user but declares no
    // operator workspace: the environment stays hybrid.
    let policy = web_policy(&repo, "node", "/home/node");

    assert_eq!(
        operator_workspace_for_policy(&repo, &policy).expect("resolve"),
        None,
        "hybrid mode provisions no guest even with a resolved user"
    );
}

#[test]
fn a_declared_workspace_is_the_container_mode_boundary() {
    let repo = fixture("container-boundary");
    container_mode_manifest(&repo);
    let policy = web_policy(&repo, "dev", "/home/dev");

    let workspace = operator_workspace_for_policy(&repo, &policy)
        .expect("resolve")
        .expect("a declared workspace is container mode");

    assert_eq!(workspace.user, "dev");
    assert_eq!(workspace.guest_checkout, "/workspace");
}

#[test]
fn conflicting_workspace_bindings_refuse_policy_prepare_and_both_entry_adapters() {
    let _env = EnvGuard::set_many(&[("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", None)]);
    let repo = fixture("conflicting-workspaces");
    container_mode_manifest(&repo);
    let mut manifest = std::fs::read_to_string(repo.join("effigy.toml")).unwrap();
    manifest.push_str(
        "\n[systems.dev.workspaces.alt]\ncontainer = 'web'\nworking_dir = '/alternate'\n",
    );
    std::fs::write(repo.join("effigy.toml"), manifest).unwrap();
    let policy = web_policy(&repo, "dev", "/home/dev");
    let selected = crate::runner::workspace_command::resolve_public_workspace_container(
        &repo,
        Some("dev"),
        Some("alt"),
        "workspace",
    )
    .unwrap();
    assert_eq!(selected.as_deref(), Some("web"));
    let override_config = effigy_manifest::ManifestWorkspaceConfig {
        container: Some(effigy_manifest::ManifestWorkspaceContainerRef::Named(
            "web".to_owned(),
        )),
        working_dir: Some("/alternate".to_owned()),
        ..Default::default()
    };
    for override_config in [None, Some(&override_config)] {
        let error = effigy_containers::load_container_policy_with_workspace(
            &repo,
            Some("web"),
            override_config,
        )
        .expect_err("a selected override must not bypass conflicting declarations")
        .to_string();
        assert!(error.contains("conflicting operator workspaces"), "{error}");
    }
    let prepare = super::super::publish_workspace_guest(&repo, &policy)
        .unwrap_err()
        .to_string();
    let entry = super::super::ensure_workspace_effigy_available_for_policy(&repo, &policy, None)
        .unwrap_err()
        .to_string();
    let origin = std::time::Instant::now();
    let mut budget = crate::runner::exec_command::BoundedCommandBudget::new(
        origin,
        origin + std::time::Duration::from_secs(60),
        4096,
        4096,
    );
    let bounded = super::super::ensure_workspace_effigy_available_for_policy_bounded(
        &repo,
        &policy,
        HOST_CONTAINER_ID,
        &mut budget,
    )
    .unwrap_err()
    .to_string();
    for error in [prepare, entry, bounded] {
        assert!(
            error.contains("dev/app") && error.contains("dev/alt"),
            "{error}"
        );
        assert!(error.contains(HOST_REPREPARE_COMMAND), "{error}");
    }
    assert!(
        !repo.join(".effigy").exists(),
        "a refused selection writes no host state or rendered output"
    );
}

#[test]
fn a_declared_custom_workspace_without_identity_refuses_instead_of_becoming_hybrid() {
    let _env = EnvGuard::set_many(&[("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", None)]);
    let repo = fixture("missing-workspace-identity");
    std::fs::write(
        repo.join("compose.yml"),
        "services:\n  app:\n    image: custom:fixture\n",
    )
    .unwrap();
    std::fs::write(repo.join("effigy.toml"), "[containers]\ndefault = 'web'\n[containers.web]\ncompose_file = 'compose.yml'\nprimary_service = 'app'\nworking_dir = '/workspace'\n[systems]\ndefault = 'dev'\n[systems.dev]\ndefault_workspace = 'app'\n[systems.dev.workspaces.app]\ncontainer = 'web'\nworking_dir = '/workspace'\n").unwrap();
    let mut policy = effigy_containers::load_container_policy(&repo, Some("web")).unwrap();
    assert!(policy.workspace_user.is_none() && policy.workspace_home.is_none());
    let rendered = policy.read_compose(&policy.compose_files[0]).unwrap();
    assert!(
        rendered.contains("/workspace/.effigy/state:ro"),
        "{rendered}"
    );
    for (user, home) in [
        (None, None),
        (Some("dev"), None),
        (None, Some("/home/dev")),
        (Some(""), Some("/home/dev")),
        (Some("dev"), Some("")),
        (Some("dev"), Some("relative/home")),
    ] {
        policy.workspace_user = user.map(str::to_owned);
        policy.workspace_home = home.map(str::to_owned);
        let prepare = super::super::publish_workspace_guest(&repo, &policy)
            .unwrap_err()
            .to_string();
        let entry =
            super::super::ensure_workspace_effigy_available_for_policy(&repo, &policy, None)
                .unwrap_err()
                .to_string();
        let origin = std::time::Instant::now();
        let mut budget = crate::runner::exec_command::BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(60),
            4096,
            4096,
        );
        let bounded = super::super::ensure_workspace_effigy_available_for_policy_bounded(
            &repo,
            &policy,
            HOST_CONTAINER_ID,
            &mut budget,
        )
        .unwrap_err()
        .to_string();
        for error in [prepare, entry, bounded] {
            assert!(error.contains("no usable workspace identity"), "{error}");
            assert!(
                error.contains("`user`") && error.contains("`home`"),
                "{error}"
            );
            assert!(error.contains(HOST_REPREPARE_COMMAND), "{error}");
        }
    }
    assert!(
        !repo.join(".effigy").exists(),
        "an identity refusal writes no host state or rendered output"
    );
}

#[test]
fn explicitly_selected_non_default_workspaces_prepare_and_verify_the_guest() {
    for (system, default_workspace) in [("dev", true), ("audit", true), ("dev", false)] {
        let repo = fixture("selected-workspace");
        container_mode_manifest(&repo);
        let mut manifest = std::fs::read_to_string(repo.join("effigy.toml")).unwrap();
        if !default_workspace {
            manifest = manifest.replace("default_workspace = \"app\"\n", "");
        }
        manifest.push_str("\n[containers.other]\nprimary_service = 'app'\nworking_dir = '/alternate'\n[containers.other.services.app]\ncatalog = 'node'\n[containers.service_only]\nprimary_service = 'app'\nworking_dir = '/service'\n[containers.service_only.services.app]\ncatalog = 'node'\n");
        if system == "audit" {
            manifest.push_str("\n[systems.audit]\ndefault_workspace = 'app'\n[systems.audit.workspaces.app]\ncontainer = 'web'\n");
        }
        manifest.push_str(&format!("\n[systems.{system}.workspaces.alt]\ncontainer = 'other'\nworking_dir = '/alternate'\nuser = 'selected'\nhome = '/home/selected'\n"));
        std::fs::write(repo.join("effigy.toml"), manifest).unwrap();
        let selected = crate::runner::workspace_command::resolve_public_workspace_container(
            &repo,
            Some(system),
            Some("alt"),
            "workspace",
        )
        .unwrap()
        .expect("selected container");
        assert_eq!(selected, "other");
        let mut policy = effigy_containers::load_container_policy(&repo, Some(&selected))
            .expect("selected environment resolves its declared workspace policy");
        assert_eq!(policy.workspace_user.as_deref(), Some("selected"));
        assert_eq!(policy.workspace_home.as_deref(), Some("/home/selected"));
        let rendered = policy.read_compose(&policy.compose_files[0]).unwrap();
        assert!(
            rendered.contains("/alternate/.effigy/state:ro"),
            "{rendered}"
        );
        assert!(
            rendered.contains("/alternate/.effigy/secrets:ro"),
            "{rendered}"
        );
        let workspace = operator_workspace_for_policy(&repo, &policy).unwrap();
        assert_eq!(workspace.as_ref().unwrap().guest_checkout, "/alternate");

        // Both entry adapters must reach the activation fence instead of
        // treating this selected container as a hybrid service.
        let error =
            super::super::ensure_workspace_effigy_available_for_policy(&repo, &policy, None)
                .expect_err("an unprepared non-default workspace must refuse")
                .to_string();
        assert!(error.contains("no activation record"), "{system}: {error}");
        let origin = std::time::Instant::now();
        let mut budget = crate::runner::exec_command::BoundedCommandBudget::new(
            origin,
            origin + std::time::Duration::from_secs(60),
            4096,
            4096,
        );
        let error = super::super::ensure_workspace_effigy_available_for_policy_bounded(
            &repo,
            &policy,
            HOST_CONTAINER_ID,
            &mut budget,
        )
        .expect_err("bounded entry also needs a prepared activation")
        .to_string();
        assert!(error.contains("no activation record"), "{system}: {error}");

        let artifact = artifact(&repo, "pinned selected workspace guest");
        let mut guest = guest();
        let prepared = prepare_workspace_guest(
            &mut guest,
            &artifact,
            crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Gnu,
            WorkspacePrepareFacts {
                repo_root: &repo,
                policy: &policy,
                operator_workspace: workspace,
                session_run_id: Some("run-7".to_owned()),
                enrolment: None,
                channel: WorkspaceChannelLocator::default(),
                backend: crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                    kind: "fake".to_owned(),
                    profile: "default".to_owned(),
                    runtime: None,
                    vm_type: None,
                },
                container: container_identity(),
                environment: local_fence(&policy.name),
                artifact_install_identity: None,
            },
        )
        .unwrap()
        .expect("selected workspace prepares a guest and immutable context");
        assert_eq!(prepared.paths.guest_checkout, "/alternate");
        let mut selected_entry = entry();
        selected_entry.environment.resource_id = format!("env:{}", policy.name);
        verify_pinned_guest_binary(&mut guest, &selected_entry)
            .expect("prepared selected workspace verifies");
        guest = guest.with_container(observed_identity(&"a1".repeat(32)));
        let error = verify_pinned_guest_binary(&mut guest, &selected_entry)
            .unwrap_err()
            .to_string();
        assert!(error.contains(HOST_REPREPARE_COMMAND), "{error}");

        policy.name = "service_only".to_owned();
        assert!(
            operator_workspace_for_policy(&repo, &policy)
                .unwrap()
                .is_none(),
            "an unrelated service with a catalog user remains hybrid"
        );
    }
}

#[test]
fn hybrid_entry_needs_no_prepared_guest_and_no_activation() {
    let repo = fixture("hybrid-entry");
    hybrid_manifest(&repo);
    // A catalog service's default identity resolves a user, but no operator
    // workspace is declared: the entry path must not require a guest, a
    // published context or an environment record.
    let policy = web_policy(&repo, "node", "/home/node");

    crate::runner::workspace_command::ensure_workspace_effigy_available_for_policy(
        &repo, &policy, None,
    )
    .expect("hybrid entry has nothing to verify");
}

#[test]
fn container_mode_entry_refuses_without_an_activation_before_touching_the_guest() {
    let repo = fixture("entry-no-activation");
    container_mode_manifest(&repo);
    let policy = web_policy(&repo, "dev", "/home/dev");

    let error = crate::runner::workspace_command::ensure_workspace_effigy_available_for_policy(
        &repo, &policy, None,
    )
    .expect_err("a workspace with no activated environment has no fence")
    .to_string();

    assert!(error.contains("no activation record"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

// ------------------------------------------------- identity probe parsing

#[test]
fn the_identity_probe_uses_only_the_verified_backend_container_id() {
    // The backend binds this exact exec target even when a cgroup namespace
    // hides its id or a configured hostname claims another container's id.
    let rendered = format!(
        "cgroup=0::/\nboot=boot-a\nhostname={}\ncontainer={}\nplatform=x86_64\nlibc=gnu\n",
        &HOST_CONTAINER_ID[..12],
        HOST_CONTAINER_ID,
    );
    let replacement = "a1".repeat(32);
    let parsed = parse_container_identity(&rendered, &replacement).expect("verified binding");
    assert_eq!(parsed.container_id, replacement);
    assert!(!container_identity().container_matches(&parsed));
    for unverified in ["", &HOST_CONTAINER_ID[..12], "app"] {
        assert!(parse_container_identity(&rendered, unverified).is_err());
    }
}

#[test]
fn a_guest_that_reports_nothing_is_not_a_container_identity() {
    let error = parse_container_identity("boot=\nhostname=\nplatform=\n", HOST_CONTAINER_ID)
        .expect_err("no identity")
        .to_string();
    assert!(error.contains("identity probe"), "{error}");
}

#[test]
fn a_multi_megabyte_executable_is_authenticated_without_reading_its_bytes() {
    let repo = fixture("large-artifact");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let guest_root =
        Fixture::process_lifetime("effigy-workspace-guest-large-").expect("guest root");
    let mut guest = DiskGuest::new(guest_root.clone(), container_identity());
    let mut bytes = vec![0u8; 4 * 1024 * 1024];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    let artifact = artifact_bytes(&repo, &bytes);

    let prepared = prepare_disk(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
    )
    .expect("prepare a multi-megabyte guest");
    assert_eq!(
        guest
            .file_sha256(&binary_path(&prepared.binary.digest))
            .unwrap(),
        Some(hex_sha256(&bytes)),
        "the installed bytes match the pinned digest"
    );

    let mut counting = CountingGuest {
        inner: &mut guest,
        bytes: 0,
    };
    verify_pinned_guest_binary(&mut counting, &entry()).expect("the large guest authenticates");
    assert!(
        counting.bytes < 2048,
        "an ordinary bounded command pays for the context and one digest line, not the \
         executable's {} bytes: it read {}",
        bytes.len(),
        counting.bytes
    );
    let _ = std::fs::remove_dir_all(guest_root);
}

// ------------------------------------------------------ live executable proof

/// The brief's required proof: publishing a new artifact never replaces a live
/// guest executable. This holds a *running* process across publication, on a
/// real filesystem, with no container or VM.
#[test]
fn publishing_a_new_artifact_never_replaces_a_live_guest_executable() {
    let repo = fixture("live-proof");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let guest_root =
        Fixture::process_lifetime("effigy-workspace-guest-live-root-").expect("guest root");
    let mut guest = DiskGuest::new(guest_root.clone(), container_identity());

    // Artifact A is a real program: it prints the digest of the bytes it is
    // running from and then waits, so the test can publish while it runs.
    let artifact_a = script_artifact(&repo, "generation-a");
    let first = prepare_disk(
        &repo,
        &mut guest,
        &artifact_a,
        Some(operator_workspace()),
        Some("run-7"),
    )
    .expect("first prepare");
    let first_executable = binary_path(&first.binary.digest);
    let entered = guest.host_path(ENTRY_PATH).expect("entry path");

    let mut running = Command::new(&entered)
        .arg("hold")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn_owned()
        .expect("the published executable starts");
    let mut reported = String::new();
    {
        use std::io::BufRead;
        let stdout = running.stdout.take().expect("child stdout");
        let mut reader = std::io::BufReader::new(stdout);
        reader
            .read_line(&mut reported)
            .expect("child reports itself");
    }
    assert_eq!(
        reported.trim(),
        "running generation-a",
        "the running process started from A's bytes"
    );

    // The live executable's identity before publication: its bytes and the
    // moment they were last written.
    let live_path = guest
        .host_path(&first_executable)
        .expect("live executable path");
    let before_bytes = std::fs::read(&live_path).expect("old bytes");
    let before_digest = sha256_text(&before_bytes);
    let before_modified = std::fs::metadata(&live_path)
        .and_then(|metadata| metadata.modified())
        .expect("live executable mtime");
    let live_pid = running.id();
    eprintln!(
        "live proof: head={} pid={} executable={} sha256={} mtime={:?}",
        std::env::var("EFFIGY_LIVE_PROOF_HEAD").unwrap_or_else(|_| "head-unset".to_owned()),
        live_pid,
        live_path.display(),
        before_digest,
        before_modified
            .duration_since(std::time::UNIX_EPOCH)
            .expect("mtime after epoch"),
    );

    // Publish artifact B while A runs.
    let before = before_bytes.clone();
    let artifact_b = script_artifact(&repo, "generation-b");
    let second = prepare_disk(
        &repo,
        &mut guest,
        &artifact_b,
        Some(operator_workspace()),
        Some("run-7"),
    )
    .expect("second prepare");
    assert_ne!(first.binary.digest, second.binary.digest);

    // The running process is untouched: it is still alive, and the file it
    // resolved is byte-identical.
    assert!(
        running.try_wait().expect("poll").is_none(),
        "publication must not disturb the running executable"
    );
    let after = std::fs::read(&live_path).expect("old bytes");
    assert_eq!(before, after, "the live executable's bytes never change");
    let after_digest = sha256_text(&after);
    let after_modified = std::fs::metadata(&live_path)
        .and_then(|metadata| metadata.modified())
        .expect("live executable mtime");
    assert_eq!(
        (before_digest.as_str(), before_modified),
        (after_digest.as_str(), after_modified),
        "the live executable's digest and mtime never change"
    );
    assert!(
        running
            .try_wait()
            .expect("poll after publication")
            .is_none(),
        "the process that started from those bytes is still running"
    );
    eprintln!(
        "live proof: after publication sha256={} mtime={:?} still-running=true",
        after_digest,
        after_modified
            .duration_since(std::time::UNIX_EPOCH)
            .expect("mtime after epoch"),
    );

    // A new invocation resolves the new generation through the entry path.
    let fresh = Command::new(&entered)
        .arg("report")
        .output_owned()
        .expect("a fresh invocation starts");
    assert!(fresh.status.success());
    assert_eq!(
        String::from_utf8_lossy(&fresh.stdout).trim(),
        "running generation-b",
        "a new invocation runs the newly published generation"
    );
    assert_eq!(
        std::fs::read_link(&entered).expect("entry link"),
        guest
            .host_path(&binary_path(&second.binary.digest))
            .unwrap()
    );
    assert!(
        guest
            .host_path(&first_executable)
            .map(|path| path.is_file())
            .unwrap_or(false),
        "the superseded generation stays on disk for the run that holds it"
    );

    eprintln!(
        "live proof: fresh invocation ran {} and entry link names {}",
        String::from_utf8_lossy(&fresh.stdout).trim(),
        std::fs::read_link(&entered).expect("entry link").display()
    );

    // EOF releases the shell builtin without leaving a sleeping child. The
    // owned group guard also stops this exact fixture on an assertion unwind.
    drop(running.stdin.take());
    let exit = running
        .wait()
        .expect("the live guest exits after stdin EOF");
    assert!(exit.success(), "live guest exit: {exit}");
    assert!(running.group_closed(), "the live guest group is closed");
    eprintln!("live proof: stopped pid={live_pid} exit={exit} group-closed=true");
    let _ = std::fs::remove_dir_all(guest_root);
}

/// A pinned artifact fixture with arbitrary bytes.
fn artifact_bytes(repo: &Path, bytes: &[u8]) -> PathBuf {
    let path = repo.join("artifacts/effigy-x86_64-unknown-linux-gnu");
    std::fs::create_dir_all(path.parent().expect("artifact parent")).expect("artifact directory");
    std::fs::write(&path, bytes).expect("artifact bytes");
    std::fs::write(
        workspace_effigy_active_version_file(&path),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("active version");
    std::fs::write(
        path.parent().unwrap().join(WORKSPACE_ARTIFACT_RECEIPT_FILE),
        format!(
            "release_triple=x86_64-unknown-linux-gnu\nsha256={}\n",
            sha256_text(bytes)
                .strip_prefix("sha256:")
                .unwrap_or_default()
        ),
    )
    .expect("artifact receipt");
    path
}

/// An executable artifact whose bytes name their generation, so a running
/// process proves which bytes it started from.
fn script_artifact(repo: &Path, label: &str) -> PathBuf {
    let body = format!(
        "#!/bin/sh\n\
         printf 'running {label}\\n'\n\
         if [ \"$1\" = hold ]; then read -r release || :; fi\n"
    );
    let path = repo
        .join("artifacts")
        .join(format!("effigy-x86_64-unknown-linux-gnu-{label}"));
    std::fs::create_dir_all(path.parent().expect("artifact parent")).expect("artifact directory");
    std::fs::write(&path, &body).expect("artifact bytes");
    std::fs::write(
        workspace_effigy_active_version_file(&path),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("active version");
    std::fs::write(
        path.parent().unwrap().join(WORKSPACE_ARTIFACT_RECEIPT_FILE),
        format!(
            "release_triple=x86_64-unknown-linux-gnu\nsha256={}\n",
            sha256_text(body.as_bytes())
                .strip_prefix("sha256:")
                .unwrap_or_default()
        ),
    )
    .expect("artifact receipt");
    path
}

fn prepare_disk(
    repo: &Path,
    guest: &mut DiskGuest,
    artifact: &Path,
    workspace: Option<OperatorWorkspace>,
    session_run_id: Option<&str>,
) -> Option<PreparedWorkspaceGuest> {
    let policy = policy(repo, workspace.is_some());
    prepare_workspace_guest(
        guest,
        artifact,
        crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Gnu,
        WorkspacePrepareFacts {
            repo_root: repo,
            policy: &policy,
            operator_workspace: workspace,
            session_run_id: session_run_id.map(str::to_owned),
            enrolment: None,
            channel: WorkspaceChannelLocator {
                endpoint: Some("host.docker.internal:47121".to_owned()),
                server_identity: Some("sha256:server".to_owned()),
            },
            backend: crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                kind: "Docker Desktop".to_owned(),
                profile: "default".to_owned(),
                runtime: None,
                vm_type: None,
            },
            container: container_identity(),
            environment: local_fence("dev"),
            artifact_install_identity: None,
        },
    )
    .expect("prepare")
}

// --------------------------------------------------------- entry fences

#[test]
fn entry_refuses_a_replaced_container_a_new_vm_boot_and_a_new_activation() {
    let repo = fixture("entry-fences");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    // A prepared workspace on the identity it was prepared for passes.
    verify_pinned_guest_binary(&mut guest, &entry()).expect("same instance verifies");

    // A replacement container: the hostname could be configured identically,
    // so only the id differs.
    let mut replaced = guest
        .clone()
        .with_container(observed_identity(&"a".repeat(64)));
    let error = verify_pinned_guest_binary(&mut replaced, &entry())
        .expect_err("a replaced container refuses at entry")
        .to_string();
    assert!(error.contains("prepared for container"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));

    // Another VM boot that retained the container filesystem.
    let mut booted_elsewhere = observed_identity(HOST_CONTAINER_ID);
    booted_elsewhere.vm_boot_id = "boot-after-reboot".to_owned();
    let mut rebooted = guest.clone().with_container(booted_elsewhere);
    let error = verify_pinned_guest_binary(&mut rebooted, &entry())
        .expect_err("a new VM boot refuses at entry")
        .to_string();
    assert!(error.contains("VM boot"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));

    // A re-activation with unchanged files still advances the environment
    // generation, so the published context no longer matches.
    let reactivated = LiveWorkspaceEntry {
        environment: WorkspaceEnvironmentIdentity {
            resource_id: "env:dev".to_owned(),
            generation: "8".to_owned(),
        },
    };
    let error = verify_pinned_guest_binary(&mut guest, &reactivated)
        .expect_err("a re-activated environment refuses at entry")
        .to_string();
    assert!(error.contains("generation `7`"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn entry_refuses_a_replacement_that_spoofs_the_old_hostname_and_cgroup() {
    let repo = fixture("spoofed-identity");
    let mut guest = guest();
    published(&repo, &mut guest);
    let rendered = format!(
        "cgroup=0::/docker/{HOST_CONTAINER_ID}\nboot={}\nhostname={}\nplatform=x86_64\nlibc=gnu\n",
        container_identity().vm_boot_id,
        &HOST_CONTAINER_ID[..12],
    );
    let replacement = parse_container_identity(&rendered, &"a1".repeat(32)).unwrap();
    guest = guest.with_container(replacement);
    let before = guest.tree_digest();
    let error = verify_pinned_guest_binary(&mut guest, &entry()).expect_err("replaced backend id");
    assert!(error.to_string().contains(HOST_REPREPARE_COMMAND));
    assert_eq!(before, guest.tree_digest(), "verification is read-only");
}

// ------------------------------------------------------- pinned closure

#[test]
fn the_context_pins_the_composed_task_closure_and_its_sources() {
    let repo = fixture("closure");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    std::fs::create_dir_all(repo.join("fragments")).expect("fragment directory");
    std::fs::write(
        repo.join("effigy.toml"),
        "[manifest]\ninclude = [\"fragments/extra.toml\"]\n\n[tasks]\nroot = { run = \"true\" }\n",
    )
    .expect("manifest with an include");
    std::fs::write(
        repo.join("fragments/extra.toml"),
        "[tasks]\nincluded = { run = \"true\" }\n",
    )
    .expect("included fragment");
    std::fs::create_dir_all(repo.join(".effigy")).expect(".effigy directory");
    std::fs::write(
        repo.join(".effigy/sources.lock.json"),
        "{\"version\":1,\"sources\":{}}",
    )
    .expect("source lock");
    let mut guest = guest();
    published(&repo, &mut guest);
    let context = published_context(&mut guest);

    let kinds = context
        .sources
        .iter()
        .map(|pin| pin.kind.as_str())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"manifest"), "{kinds:?}");
    assert!(kinds.contains(&"source-lock"), "{kinds:?}");
    assert!(kinds.contains(&COMPOSED_MANIFEST_KIND), "{kinds:?}");
    let included = context
        .sources
        .iter()
        .find(|pin| pin.name == "fragments/extra.toml")
        .expect("the included fragment is pinned");
    assert_eq!(
        included.snapshot.as_deref(),
        Some("/workspace/fragments/extra.toml")
    );

    let composed = context
        .sources
        .iter()
        .find(|pin| pin.kind == COMPOSED_MANIFEST_KIND)
        .expect("composed manifest pin");
    assert_eq!(
        composed.snapshot.as_deref(),
        Some(snapshot_path(&composed.digest).as_str())
    );
    let rendered = guest
        .text(&snapshot_path(&composed.digest))
        .expect("the rendered snapshot is guest-readable");
    assert!(rendered.contains("included"), "{rendered}");
    assert_eq!(sha256_text(rendered.as_bytes()), composed.digest);
    assert_eq!(
        guest.mode(&snapshot_path(&composed.digest)),
        Some(0o444),
        "a rendered snapshot is read-only"
    );

    // The task list comes from the pinned closure, so a bundle- or
    // catalog-provided task is visible without refreshing a source.
    let before = guest.tree_digest();
    let mut checkout = FixtureCheckout::new(repo.clone());
    let mut reads = GuestLocalReads {
        context: &context,
        guest: &mut guest,
        checkout: &mut checkout,
    };
    assert_eq!(
        reads
            .read(&LocalRead::Tasks, context.generation)
            .expect("tasks"),
        LocalReadOutcome::Tasks(vec!["included".to_owned(), "root".to_owned()])
    );
    assert_eq!(before, guest.tree_digest(), "a local read writes nothing");
}

#[test]
fn a_pinned_source_closure_names_its_fragment_identity() {
    let root = Fixture::process_lifetime("effigy-workspace-fragment-identity-")
        .expect("fixture directory");
    std::fs::create_dir_all(root.join("manifest")).expect("fragment directory");
    std::fs::write(root.join("bundle.toml"), "[bundle]\nname = \"x\"\n").expect("descriptor");
    std::fs::write(root.join("manifest/tasks.toml"), "[tasks]\na = {}\n").expect("fragment");
    let first = manifest_fragment_digest(&root).expect("digest");
    assert!(first.starts_with("sha256:"));

    std::fs::write(root.join("manifest/tasks.toml"), "[tasks]\nb = {}\n").expect("fragment");
    let second = manifest_fragment_digest(&root).expect("digest");
    assert_ne!(first, second, "fragment bytes are part of the identity");
    let _ = std::fs::remove_dir_all(root);
}

// --------------------------------------------------- private host staging

#[test]
fn staged_host_bytes_are_private_exclusive_and_removed() {
    use std::os::unix::fs::PermissionsExt;

    let staged = stage_host_bytes(b"enrolment-secret").expect("stage");
    let path = staged.path().to_path_buf();
    let metadata = std::fs::metadata(&path).expect("staged file");
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        0o600,
        "a staged file is owner-only from creation"
    );
    let directory = path.parent().expect("staging directory");
    assert_eq!(
        std::fs::metadata(directory)
            .expect("staging directory")
            .permissions()
            .mode()
            & 0o077,
        0,
        "the staging directory is private to this run"
    );
    assert_eq!(
        std::fs::read(&path).expect("staged bytes"),
        b"enrolment-secret"
    );

    drop(staged);
    assert!(!path.exists(), "the guard removes the staged bytes");
}

// ------------------------------------------------------- image libc family

/// An image that needs a musl artifact refuses Prepare when only the glibc
/// artifact exists: the bytes are authenticated, but for the wrong C library.
#[test]
fn a_musl_only_image_refuses_a_glibc_artifact() {
    let repo = fixture("musl-only");
    let mut guest = guest();
    let artifact = artifact(&repo, "pinned guest bytes");

    let policy = policy(&repo, true);
    let error = prepare_workspace_guest(
        &mut guest,
        &artifact,
        crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Musl,
        WorkspacePrepareFacts {
            repo_root: &repo,
            policy: &policy,
            operator_workspace: Some(operator_workspace()),
            session_run_id: Some("run-7".to_owned()),
            enrolment: None,
            channel: WorkspaceChannelLocator::default(),
            backend: crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                kind: "Docker Desktop".to_owned(),
                profile: "default".to_owned(),
                runtime: None,
                vm_type: None,
            },
            container: container_identity(),
            environment: local_fence("dev"),
            artifact_install_identity: None,
        },
    )
    .expect_err("a glibc artifact cannot be published to a musl image")
    .to_string();

    assert!(error.contains("x86_64-unknown-linux-musl"), "{error}");
    assert!(error.contains("has no receipt naming target"), "{error}");
    assert!(
        guest.paths().is_empty(),
        "nothing is published for the wrong C library"
    );
}

#[test]
fn the_identity_probe_reports_the_image_c_library() {
    let musl = format!(
        "cgroup=0::/ \nboot=boot-a\nhostname=app\nplatform=x86_64\n\
         libc_musl=/lib/ld-musl-x86_64.so.1\nlibc_glibc=\n"
    );
    assert_eq!(
        parse_container_identity(&musl, HOST_CONTAINER_ID)
            .expect("musl identity")
            .libc,
        "musl"
    );

    let glibc = "cgroup=0::/\nboot=boot-a\nhostname=app\nplatform=aarch64\n\
                 libc_musl=\nlibc_glibc=/lib/aarch64-linux-gnu/libc.so.6\n";
    assert_eq!(
        parse_container_identity(glibc, HOST_CONTAINER_ID)
            .expect("glibc identity")
            .libc,
        "gnu"
    );

    // Neither marker: the image's C library is unproven, and Prepare refuses
    // rather than guessing glibc.
    let unknown = "cgroup=0::/\nboot=boot-a\nhostname=app\nplatform=x86_64\n\
                   libc_musl=\nlibc_glibc=\n";
    let identity = parse_container_identity(unknown, HOST_CONTAINER_ID).expect("identity");
    assert_eq!(identity.libc, "");
    let error = crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::from_image(
        &identity.platform,
        &identity.libc,
    )
    .expect_err("an unproven libc refuses")
    .to_string();
    assert!(error.contains("no provable C library"), "{error}");
}

#[test]
fn entry_refuses_a_context_prepared_for_another_image_c_library() {
    let repo = fixture("libc-fence");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);

    let mut musl_image = observed_identity(HOST_CONTAINER_ID);
    musl_image.libc = "musl".to_owned();
    let mut replaced_image = guest.clone().with_container(musl_image);

    let error = verify_pinned_guest_binary(&mut replaced_image, &entry())
        .expect_err("an image whose libc changed refuses at entry")
        .to_string();

    assert!(error.contains("prepared for container"), "{error}");
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn interrupted_publication_skips_existing_context_documents() {
    let repo = fixture("orphan-context");
    let mut guest = guest();
    let artifact = artifact(&repo, "immutable executable");
    let first = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        None,
        None,
        container_identity(),
    )
    .unwrap();
    let orphan = first.context_generation + 1;
    let path = crate::runner::workspace_client::context::context_path(orphan);
    use crate::runner::workspace_client::context::GuestWorkspace;
    guest
        .write_file(
            &path,
            b"retained orphan from interrupted publication",
            0o444,
        )
        .unwrap();
    let next = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("changed-session"),
        None,
        container_identity(),
    )
    .unwrap();
    assert_eq!(next.context_generation, orphan + 1);
    assert_eq!(
        guest.text(&path).unwrap(),
        "retained orphan from interrupted publication"
    );
}

#[test]
fn prepare_refuses_to_overwrite_a_corrupt_immutable_executable() {
    let repo = fixture("corrupt-immutable");
    let mut guest = guest();
    let artifact = artifact(&repo, "immutable executable");
    let first = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        None,
        None,
        container_identity(),
    )
    .unwrap();
    let path = binary_path(&first.binary.digest);
    use crate::runner::workspace_client::context::GuestWorkspace;
    guest.write_file(&path, b"changed bytes", 0o555).unwrap();
    let error = prepare_refusal(&repo, &mut guest, &artifact);
    assert!(error.contains("immutable workspace executable"), "{error}");
    assert_eq!(guest.text(&path).unwrap(), "changed bytes");
}

#[test]
fn concurrent_activation_publications_pin_distinct_immutable_fake_guest_contexts() {
    use effigy_containers::environment::{activate_with, BackendError, EnvContext};
    use effigy_kernel::ctx::CancelToken;
    use std::sync::{mpsc, Arc, Mutex};
    let repo = fixture("gated-concurrent-contexts");
    let home = Fixture::new("gated-publication-home-").unwrap();
    let _env = EnvGuard::set_many(&[
        ("HOME", Some(home.path().display().to_string())),
        (
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(home.path().join(".effigy").display().to_string()),
        ),
    ]);
    let policy = policy(&repo, true);
    let context = crate::runner::environment::context(&repo).unwrap();
    let environment = crate::runner::environment::env_ref(&policy).unwrap();
    let artifact = artifact(&repo, "shared immutable executable");
    let shared_guest = Arc::new(Mutex::new(guest()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let publish = |ctx: &EnvContext<'_>, first: bool| {
        let mut published = None;
        activate_with(ctx, &environment, &[], |run| {
            run.publish_guest_with(|fence| {
                // Pause before locking the fake: only Gate(Env) serializes Prepare.
                if first {
                    entered_tx.send(()).unwrap();
                    release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                }
                let mut guest = shared_guest.lock().unwrap();
                let prepared = prepare_workspace_guest(
                    &mut *guest,
                    &artifact,
                    crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget::X86_64Gnu,
                    WorkspacePrepareFacts {
                        repo_root: &repo,
                        policy: &policy,
                        operator_workspace: Some(operator_workspace()),
                        session_run_id: None,
                        enrolment: None,
                        channel: WorkspaceChannelLocator::default(),
                        backend:
                            crate::runner::workspace_client::context::WorkspaceBackendDescriptor {
                                kind: "fake".into(),
                                profile: "test".into(),
                                runtime: None,
                                vm_type: None,
                            },
                        container: container_identity(),
                        environment: fence.clone(),
                        artifact_install_identity: None,
                    },
                )
                .map_err(|error| BackendError::Uncertain(error.to_string()))?
                .unwrap();
                let path = context_path(prepared.context_generation);
                published = Some((
                    path.clone(),
                    guest.text(&path).unwrap(),
                    prepared.context_generation,
                ));
                Ok(effigy_kernel::run::GuestPublicationReceipt {
                    environment: fence.clone(),
                    container_id: prepared.activation.container.container_id,
                    vm_boot_id: prepared.activation.container.vm_boot_id,
                    artifact_digest: prepared.binary.digest,
                    context_generation: prepared.context_generation,
                })
            })
        })
        .unwrap();
        published.unwrap()
    };
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let ctx = EnvContext::new(context.root, CancelToken::default()).unwrap();
            publish(&ctx, true)
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let second = scope.spawn(|| {
            let ctx = EnvContext::new(context.root, CancelToken::default()).unwrap();
            publish(&ctx, false)
        });
        release_tx.send(()).unwrap();
        let original = first.join().unwrap();
        let next = second.join().unwrap();
        assert_eq!((original.2, next.2), (1, 2));
        let mut guest = shared_guest.lock().unwrap();
        assert_eq!(
            guest.text(&original.0).unwrap(),
            original.1,
            "the first published document is immutable"
        );
        let current = read_published_context(&mut *guest).unwrap().unwrap();
        assert_eq!(current.generation, next.2);
        assert_eq!(
            current.context.activation.environment.generation,
            environment_generation(&policy).unwrap().generation
        );
    });
}
