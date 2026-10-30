// Included under the existing provisioning test module: it reuses only its
// fixture builders, never a second implementation of Prepare or its policy.
use super::*;

struct TracedGuest {
    inner: FakeGuest,
    calls: Vec<(String, String)>,
}
impl TracedGuest {
    fn new() -> Self {
        let mut inner = guest();
        inner.put_file(
            "/foreign/container-sentinel",
            "container:foreign:boot:17",
            0o444,
        );
        Self {
            inner,
            calls: vec![],
        }
    }
    fn call(&mut self, method: &str, path: &str) {
        assert!(
            !path.starts_with("/foreign/"),
            "Prepare touched the foreign container"
        );
        assert!(
            !path.contains("/.effigy/"),
            "guest accessed host state or vault"
        );
        self.calls.push((method.into(), path.into()));
    }
    fn foreign_survives(&self) {
        assert_eq!(
            self.inner.text("/foreign/container-sentinel").as_deref(),
            Some("container:foreign:boot:17")
        );
    }
}
impl GuestReader for TracedGuest {
    fn read_file(
        &mut self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, crate::runner::error::RunnerError> {
        self.call("read", path);
        self.inner.read_file(path)
    }
    fn file_sha256(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, crate::runner::error::RunnerError> {
        self.call("digest", path);
        self.inner.file_sha256(path)
    }
}
impl GuestWorkspace for TracedGuest {
    fn read_link(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, crate::runner::error::RunnerError> {
        self.call("read_link", path);
        self.inner.read_link(path)
    }
    fn ensure_dir(
        &mut self,
        path: &str,
        mode: u32,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("mkdir", path);
        self.inner.ensure_dir(path, mode)
    }
    fn write_file(
        &mut self,
        path: &str,
        bytes: &[u8],
        mode: u32,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("write", path);
        self.inner.write_file(path, bytes, mode)
    }
    fn set_owner(
        &mut self,
        path: &str,
        owner: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("owner", path);
        self.inner.set_owner(path, owner)
    }
    fn stage_host_file(
        &mut self,
        host_path: &Path,
        path: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("stage", path);
        self.inner.stage_host_file(host_path, path)
    }
    fn install_executable(
        &mut self,
        staging: &str,
        path: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("install", path);
        self.inner.install_executable(staging, path)
    }
    fn replace_pointer(
        &mut self,
        path: &str,
        target: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("pointer", path);
        self.inner.replace_pointer(path, target)
    }
    fn replace_link(
        &mut self,
        path: &str,
        target: &str,
    ) -> Result<(), crate::runner::error::RunnerError> {
        self.call("link", path);
        self.inner.replace_link(path, target)
    }
    fn probe_container_identity(
        &mut self,
    ) -> Result<WorkspaceContainerIdentity, crate::runner::error::RunnerError> {
        self.call("identity", "container");
        self.inner.probe_container_identity()
    }
}

fn repo() -> Fixture {
    let root = Fixture::new("workspace-provision-matrix-").expect("fresh private fixture");
    std::fs::create_dir(root.path().join(".git")).expect("fake git metadata");
    std::fs::write(root.path().join("effigy.toml"), "[tasks]\n").expect("fixture manifest");
    root
}

#[test]
fn hybrid_prepare_performs_no_guest_io_or_credential_publication() {
    let repo = repo();
    let mut guest = TracedGuest::new();
    let before = guest.inner.tree_digest();
    assert!(prepare(
        repo.path(),
        &mut guest,
        &repo.path().join("missing-artifact"),
        None,
        None,
        None,
        container_identity()
    )
    .is_none());
    assert!(guest.calls.is_empty(), "hybrid touched a guest");
    assert_eq!(guest.inner.tree_digest(), before);
    guest.foreign_survives();
    let path = repo.path().to_path_buf();
    drop(repo);
    assert!(!path.exists(), "exact fixture cleanup");
}

#[test]
fn prepare_is_immutable_and_enrolment_is_private_and_never_in_context_json() {
    let repo = repo();
    let artifact = artifact(repo.path(), "pinned artifact bytes");
    let mut guest = TracedGuest::new();
    let secret = "enrolment-secret-440";
    let prepared = prepare(
        repo.path(),
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        Some(WorkspaceEnrolmentCredential::new(
            "7",
            HOST_CONTAINER_ID,
            secret,
        )),
        container_identity(),
    )
    .expect("prepared");
    let first_digest = guest.inner.tree_digest();
    let first_binary = guest
        .inner
        .node(&binary_path(&prepared.binary.digest))
        .expect("binary")
        .clone();
    assert_eq!(guest.inner.mode(ENROL_ROOT), Some(0o700));
    assert_eq!(guest.inner.mode(ENROL_CREDENTIAL_PATH), Some(0o600));
    let context = guest
        .inner
        .text(&context_path(prepared.context_generation))
        .expect("context JSON");
    assert!(
        !context.contains(secret),
        "enrolment credential leaked into context JSON"
    );
    assert!(
        !format!("{:?}", guest.calls).contains(secret),
        "credential leaked into effect trace"
    );
    guest.calls.clear();
    let again = prepare(
        repo.path(),
        &mut guest,
        &artifact,
        Some(operator_workspace()),
        Some("run-7"),
        None,
        container_identity(),
    )
    .expect("same preparation");
    assert_eq!(again.context_generation, prepared.context_generation);
    assert_eq!(
        guest.inner.node(&binary_path(&prepared.binary.digest)),
        Some(&first_binary)
    );
    assert!(
        !guest
            .calls
            .iter()
            .any(|(op, _)| op == "install" || op == "stage"),
        "Prepare replaced pinned executable"
    );
    assert_eq!(guest.inner.tree_digest(), first_digest);
    guest.foreign_survives();
    let path = repo.path().to_path_buf();
    drop(repo);
    assert!(!path.exists(), "exact fixture cleanup");
}
