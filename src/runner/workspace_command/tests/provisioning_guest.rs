//! Container-mode Prepare and the guest's version lock, over a fake guest.
//!
//! Each test drives the same publication and verification code the Compose
//! backend runs; only the container filesystem and the pinned artifact are
//! fixtures.

use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;
use effigy_test_support::Fixture;

use crate::contract_test_support::EnvGuard;
use crate::runner::workspace_client::context::{
    binary_path, context_path, read_published_context, read_published_generation, GuestReader,
    LiveGuestRun, WorkspaceChannelLocator, WorkspaceContainerIdentity,
    WorkspaceEnrolmentCredential, BINARY_CURRENT_PATH, CHANNEL_PROTOCOL_VERSION,
    CONTEXT_CURRENT_PATH, ENROL_CREDENTIAL_PATH, ENROL_ROOT, HOST_REPREPARE_COMMAND,
};
use crate::runner::workspace_client::guest_fake::FakeGuest;
use crate::runner::workspace_command::provisioning::{
    prepare_workspace_guest, verify_pinned_guest_binary, workspace_effigy_active_version_file,
    OperatorWorkspace, PreparedWorkspaceGuest, WorkspacePrepareFacts,
    EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
};

const ENTRY_PATH: &str = "/usr/local/bin/effigy";

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

fn container_identity() -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: "9d1c0b6a7f5e".to_owned(),
        vm_boot_id: "3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41".to_owned(),
        platform: "x86_64".to_owned(),
    }
}

fn guest() -> FakeGuest {
    FakeGuest::new().with_container(container_identity())
}

/// A pinned artifact fixture: the exact bytes, plus the host version receipt
/// Prepare writes when it binds an artifact.
fn artifact(repo: &Path, bytes: &str) -> PathBuf {
    let path = repo.join("artifacts/effigy-x86_64-unknown-linux-gnu");
    std::fs::create_dir_all(path.parent().expect("artifact parent")).expect("artifact directory");
    std::fs::write(&path, bytes).expect("artifact bytes");
    std::fs::write(
        workspace_effigy_active_version_file(&path),
        format!("{}\n", effigy_core::build_info::active_version()),
    )
    .expect("active version");
    path
}

fn prepare(
    repo: &Path,
    guest: &mut FakeGuest,
    artifact: &Path,
    workspace: Option<OperatorWorkspace>,
    session_run_id: Option<&str>,
    enrolment: Option<WorkspaceEnrolmentCredential>,
) -> Option<PreparedWorkspaceGuest> {
    let policy = policy(repo, workspace.is_some());
    prepare_workspace_guest(
        guest,
        artifact,
        container_identity(),
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
        },
    )
    .expect("prepare")
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
    )
    .expect("container mode prepares a guest")
}

fn live(generation: u64) -> LiveGuestRun {
    LiveGuestRun {
        binary: crate::runner::workspace_client::context::GuestBinaryIdentity {
            version: effigy_core::build_info::active_version(),
            digest: String::new(),
            protocol: CHANNEL_PROTOCOL_VERSION,
        },
        container: container_identity(),
        environment_generation: String::new(),
        session_run_id: Some("run-7".to_owned()),
        context_generation: generation,
    }
}

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
    let directory = context_path(1);
    assert_eq!(
        guest.mode(&directory),
        Some(0o444),
        "a generation is read-only"
    );
    assert_eq!(
        guest.mode("/run/effigy/context/1"),
        Some(0o555),
        "a generation directory is sealed"
    );
    let document = guest.text(&directory).expect("context document");
    assert!(document.contains("\"schema\": \"effigy.workspace-context.v1\""));
    assert!(document.contains(&format!(
        "\"version\": \"{}\"",
        effigy_core::build_info::active_version()
    )));
    assert!(document.contains("\"protocol\": 1"));
    assert!(document.contains("9d1c0b6a7f5e"));
    assert!(document.contains("3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41"));
    assert!(document.contains("\"session_run_id\": \"run-7\""));
    assert!(document.contains("\"host_checkout\""));
    assert!(document.contains("\"user\": \"vscode\""));

    let executable = binary_path(&prepared.binary.digest);
    assert_eq!(guest.mode(&executable), Some(0o555));
    assert_eq!(
        guest.text(&executable),
        Some("pinned guest bytes".to_owned())
    );
    assert_eq!(
        guest.text(BINARY_CURRENT_PATH),
        Some(prepared.binary.digest)
    );
    assert_eq!(guest.link_target(ENTRY_PATH), Some(executable));
    assert_eq!(
        guest.mode(ENTRY_PATH),
        Some(0o777),
        "the entry path is a symlink, not a copied executable"
    );
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
    )
    .expect("first prepare");
    let second = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
    )
    .expect("second prepare");

    assert_eq!(first.context_generation, second.context_generation);
    assert_eq!(guest.text(CONTEXT_CURRENT_PATH), Some("1".to_owned()));
    assert!(
        !guest.paths().contains(&"/run/effigy/context/2"),
        "an unchanged publication writes no second generation: {:?}",
        guest.paths()
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
    )
    .expect("first prepare");
    let first_generation = guest.text(&context_path(1)).expect("first document");

    std::fs::write(repo.join("effigy.toml"), "[tasks]\ndev = {}\n").expect("manifest change");
    let second = prepare(
        &repo,
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-8"),
        None,
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

    let prepared = prepare(&repo, &mut guest, &artifact, None, None, None);

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
    )
    .expect("linked prepare");

    assert_ne!(
        primary_prepared.activation.checkout_id, linked_prepared.activation.checkout_id,
        "a linked worktree is its own checkout"
    );
    assert_eq!(
        linked_prepared.paths.host_checkout,
        linked
            .canonicalize()
            .expect("canonical linked root")
            .display()
            .to_string()
    );
    assert!(linked_guest
        .text(&context_path(1))
        .expect("linked context")
        .contains(
            &linked
                .canonicalize()
                .expect("canonical")
                .display()
                .to_string()
        ));
}

#[test]
fn a_prepared_workspace_verifies_and_an_unprepared_one_names_the_reprepare() {
    let repo = fixture("verify");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    verify_pinned_guest_binary(&mut guest).expect_err("unprepared refuses");

    let prepared = published(&repo, &mut guest);

    verify_pinned_guest_binary(&mut guest).expect("prepared verifies");
    let error = verify_pinned_guest_binary(&mut guest)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        !error.contains(HOST_REPREPARE_COMMAND),
        "a prepared guest verifies"
    );
    assert_eq!(prepared.context_generation, 1);
}

#[test]
fn an_unprepared_workspace_refusal_names_the_host_reprepare() {
    let mut guest = guest();

    let error = verify_pinned_guest_binary(&mut guest)
        .expect_err("unprepared refuses")
        .to_string();

    assert!(error.contains("no prepared guest context"));
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_context_pinning_another_version_refuses_and_names_the_reprepare() {
    let repo = fixture("wrong-version");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    let prepared = published(&repo, &mut guest);
    let document = guest
        .text(&context_path(prepared.context_generation))
        .expect("context document")
        .replace(
            &format!(
                "\"version\": \"{}\"",
                effigy_core::build_info::active_version()
            ),
            "\"version\": \"9.9.9\"",
        );
    guest.put_file(&context_path(prepared.context_generation), document, 0o444);

    let error = verify_pinned_guest_binary(&mut guest)
        .expect_err("version")
        .to_string();

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

    let error = verify_pinned_guest_binary(&mut replacement)
        .expect_err("missing executable")
        .to_string();

    assert!(error.contains(&executable));
    assert!(error.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn a_replaced_container_refuses_the_context_and_names_the_reprepare() {
    let repo = fixture("replaced-container");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let published = read_published_context(&mut guest)
        .expect("read")
        .expect("prepared");

    let mut replaced = live(published.generation);
    replaced.binary = published.context.binary.clone();
    replaced.environment_generation = published.context.activation.environment.generation.clone();
    replaced.container.container_id = "replacement".to_owned();
    let refusal = published.context.verify(&replaced).expect_err("replaced");

    assert_eq!(
        refusal.code,
        crate::runner::workspace_client::context::ContextRefusalCode::IdentityMismatch
    );
    assert!(refusal.message.contains(HOST_REPREPARE_COMMAND));
}

#[test]
fn local_reads_leave_the_guest_filesystem_unchanged() {
    let repo = fixture("pure-reads");
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let mut guest = guest();
    published(&repo, &mut guest);
    let before = guest.tree_digest();

    let generation = read_published_generation(&mut guest).expect("generation");
    let published = read_published_context(&mut guest)
        .expect("read")
        .expect("prepared");
    let mut live = live(published.generation);
    live.binary = published.context.binary.clone();
    live.environment_generation = published.context.activation.environment.generation.clone();
    published.context.verify(&live).expect("verified");
    verify_pinned_guest_binary(&mut guest).expect("verified");

    assert_eq!(generation, Some(1));
    assert_eq!(before, guest.tree_digest(), "a read writes nothing");
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
    )
    .expect("second prepare");

    assert_ne!(first.binary.digest, second.binary.digest);
    assert_eq!(
        guest.text(&first_executable),
        Some("first pinned bytes".to_owned()),
        "the executable a live run started keeps its exact bytes"
    );
    assert_eq!(
        guest.text(&binary_path(&second.binary.digest)),
        Some("second pinned bytes".to_owned())
    );
    assert_eq!(
        guest.text(BINARY_CURRENT_PATH),
        Some(second.binary.digest.clone())
    );
    assert_eq!(
        guest.link_target(ENTRY_PATH),
        Some(binary_path(&second.binary.digest)),
        "only the entry pointer moves"
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
            "sha256:aa11",
            "9d1c0b6a7f5e",
            "enrolment-token",
        )),
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
