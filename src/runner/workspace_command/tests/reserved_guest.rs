//! A strict fake at the external I/O boundary, using the actual owner, journal,
//! session custody, TLS listener, schema constructor and commit/CAS path.
use super::*;
use crate::runner::environment::{self, workspace::*};
use crate::runner::workspace_client::guest_fake::FakeGuest;
use crate::runner::workspace_transport::qualification::{
    Backend, GuestDialEvidence, ResolvedAlias,
};
use crate::runner::workspace_transport::TlsChannelTransport;
use effigy_kernel::ctx::{EffectClass, Placement};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId, Surface, TaskId,
};
use effigy_kernel::run::BeginOpts;
use effigy_kernel::workspace_session::{
    ArtifactEvidence, GuestContextInputs, ImagePlatform, LoaderFamily, PublishInputs,
    TargetEvidence,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

struct Guest(
    Rc<RefCell<FakeGuest>>,
    Option<(usize, bool)>,
    Rc<Cell<usize>>,
    Rc<Cell<bool>>,
);
impl Guest {
    fn effect<T>(
        &mut self,
        action: impl FnOnce(&mut FakeGuest) -> Result<T, RunnerError>,
    ) -> Result<T, RunnerError> {
        let index = self.2.get() + 1;
        self.2.set(index);
        let injected = self.1.filter(|(at, _)| *at == index);
        if injected == Some((index, false)) {
            self.3.set(true);
            return Err(RunnerError::task_invocation("crash before guest I/O"));
        }
        let result = action(&mut self.0.borrow_mut());
        if injected == Some((index, true)) {
            self.3.set(true);
            return Err(RunnerError::task_invocation("crash after guest I/O"));
        }
        result
    }
}
impl GuestReader for Guest {
    fn read_file(&mut self, p: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        self.0.borrow_mut().read_file(p)
    }
    fn file_sha256(&mut self, p: &str) -> Result<Option<String>, RunnerError> {
        self.0.borrow_mut().file_sha256(p)
    }
}
impl GuestWorkspace for Guest {
    fn read_link(&mut self, p: &str) -> Result<Option<String>, RunnerError> {
        self.0.borrow_mut().read_link(p)
    }
    fn ensure_dir(&mut self, p: &str, m: u32) -> Result<(), RunnerError> {
        self.effect(|guest| guest.ensure_dir(p, m))
    }
    fn write_file(&mut self, p: &str, b: &[u8], m: u32) -> Result<(), RunnerError> {
        self.effect(|guest| guest.write_file(p, b, m))
    }
    fn write_no_replace(&mut self, p: &str, b: &[u8], m: u32) -> Result<(), RunnerError> {
        self.effect(|guest| guest.write_no_replace(p, b, m))
    }
    fn install_no_replace(&mut self, s: &str, p: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.install_no_replace(s, p))
    }
    fn set_owner(&mut self, p: &str, o: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.set_owner(p, o))
    }
    fn stage_host_file(&mut self, h: &Path, p: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.stage_host_file(h, p))
    }
    fn install_executable(&mut self, s: &str, p: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.install_executable(s, p))
    }
    fn replace_pointer(&mut self, p: &str, t: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.replace_pointer(p, t))
    }
    fn replace_link(&mut self, p: &str, t: &str) -> Result<(), RunnerError> {
        self.effect(|guest| guest.replace_link(p, t))
    }
    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        self.0.borrow_mut().probe_container_identity()
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    LongObservationBudget,
    ExpiredDeadline,
    /// The gated re-observation sees a replaced target.
    GatedChangedTarget,
    /// The gated re-observation names another valid full id.
    GatedIdMismatch,
    /// The absolute deadline is spent before the gated read can run.
    GatedExpiredDeadline,
    Identity,
    UnknownIdentity,
    Alias,
    Peer,
    Pin,
    Publication,
    ContextProbe,
    ContextPeer,
    ContextPin,
    StaleDocument,
    Io(usize, bool),
    Recovery(usize),
    RecoveryDead,
    RecoveryUnknown,
    Hybrid,
}
#[derive(Clone)]
struct Probe {
    identity: GuestSelfObservation,
    fault: Fault,
    guest: Rc<RefCell<FakeGuest>>,
    context_probed: std::rc::Rc<std::cell::Cell<bool>>,
}
impl WorkspaceGuestProbe for Probe {
    fn context_dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        context: &GuestObject,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        self.context_probed.set(true);
        if matches!(self.fault, Fault::ContextProbe) {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "context probe interrupted".into(),
            ));
        }
        self.identity
            .compare(target)
            .expect("independent fake observation");
        if matches!(self.fault, Fault::StaleDocument) {
            let bytes = self
                .guest
                .borrow_mut()
                .read_file(&context.path)
                .unwrap()
                .unwrap();
            let mut copied: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            copied["session"]["generation"] =
                (copied["session"]["generation"].as_u64().unwrap() + 1).into();
            self.guest.borrow_mut().put_file(
                &context.path,
                serde_json::to_vec(&copied).unwrap(),
                0o444,
            );
        }
        let bytes = self
            .guest
            .borrow_mut()
            .read_file(&context.path)
            .unwrap()
            .expect("complete staged document");
        if Digest::of(&bytes) != context.digest {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "actual guest document digest differs".into(),
            ));
        }
        let document = WorkspaceContextV2::parse(&bytes).expect("guest cross-parser");
        assert_eq!(
            document.artifact_digest,
            format!("sha256:{}", object.digest.hex())
        );
        assert_eq!(
            self.guest.borrow().text(CONTEXT_CURRENT_PATH).as_deref(),
            Some("3"),
            "context probe precedes every selector switch"
        );
        let pin = Digest::parse_hex(document.locator.pin.strip_prefix("sha256:").unwrap()).unwrap();
        match self.dial(target, &document.locator.endpoint, &pin, budget) {
            Observed::Verified(mut reply, _) => {
                if matches!(self.fault, Fault::ContextPeer) {
                    reply.peer.set_ip("127.0.0.2".parse().unwrap());
                }
                if matches!(self.fault, Fault::ContextPin) {
                    reply.pin = Digest::of(b"changed full context pin");
                }
                Observed::Verified(reply, evidence())
            }
            _ => panic!("fixture context dial did not verify"),
        }
    }
    fn identity(
        &mut self,
        _: &WorkspaceContainerIdentity,
        _: &GuestObject,
        _: Duration,
    ) -> Observed<GuestSelfObservation> {
        if matches!(self.fault, Fault::UnknownIdentity) {
            return Observed::Unknown(UnknownReason::Inconsistent("fixture unknown".into()));
        }
        let mut identity = self.identity.clone();
        if matches!(self.fault, Fault::Identity) {
            identity.vm_boot_id = "another-boot".into();
        }
        Observed::Verified(identity, evidence())
    }
}
impl GuestChannelProbe for Probe {
    fn resolve(
        &mut self,
        target: &WorkspaceContainerIdentity,
        alias: &str,
        _: Duration,
    ) -> Observed<ResolvedAlias> {
        assert_eq!(alias, "effigy-host");
        if matches!(self.fault, Fault::Alias) {
            return Observed::Unknown(UnknownReason::Inconsistent("ambiguous alias".into()));
        }
        Observed::Verified(
            ResolvedAlias {
                target: target.clone(),
                address: "127.0.0.1".parse().unwrap(),
            },
            evidence(),
        )
    }
    fn dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        let port = endpoint
            .strip_prefix("effigy-host:")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let peer = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        // A real pinned TLS handshake, with no application frames or credentials.
        let transport =
            TlsChannelTransport::connect(peer, pin.clone(), budget).expect("fixture TLS");
        drop(transport);
        let mut observed = GuestDialEvidence {
            target: target.clone(),
            peer,
            pin: pin.clone(),
        };
        if matches!(self.fault, Fault::Peer) {
            observed.peer.set_ip("127.0.0.2".parse().unwrap());
        }
        if matches!(self.fault, Fault::Pin) {
            observed.pin = Digest::of(b"other pin");
        }
        Observed::Verified(observed, evidence())
    }
}

fn elf() -> Vec<u8> {
    let interpreter = "/lib64/ld-linux-x86-64.so.2";
    let mut b = vec![0; 512];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    b[18..20].copy_from_slice(&62u16.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&1u16.to_le_bytes());
    b[64..68].copy_from_slice(&3u32.to_le_bytes());
    b[72..80].copy_from_slice(&128u64.to_le_bytes());
    b[96..104].copy_from_slice(&((interpreter.len() + 1) as u64).to_le_bytes());
    b[128..128 + interpreter.len()].copy_from_slice(interpreter.as_bytes());
    b
}

fn run(fault: Fault) -> usize {
    let fixture = effigy_test_support::Fixture::new("workspace-reserved-").expect("fixture");
    std::fs::create_dir(fixture.path().join("home")).unwrap();
    effigy_containers::with_test_effigy_home(&fixture.path().join("home"), || {
        let repo = fixture.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        // This producer requires explicit checkout-scoped ownership labels.
        std::fs::write(
            repo.join(".git/config"),
            "[effigy]\n runtimeScope = ephemeral\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("effigy.toml"),
            r#"
[containers]
default = "dev"
[containers.dev]
compose_file = "compose.yml"
primary_service = "app"
working_dir = "/workspace"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "dev"
working_dir = "/workspace"
user = "vscode"
home = "/home/vscode"
"#,
        )
        .unwrap();
        std::fs::write(
            repo.join("compose.yml"),
            "services:\n  app:\n    image: fixture:latest\n",
        )
        .unwrap();
        let bin = fixture.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let inherited_path = std::env::var_os("PATH").unwrap_or_default();
        let mut test_path = bin.as_os_str().to_os_string();
        test_path.push(":");
        test_path.push(inherited_path);
        let _environment = effigy_test_support::env::ScopedEnv::set_many(&[
            ("PATH", test_path),
            ("EFFIGY_COMPOSE_BACKEND", "docker".into()),
            ("DOCKER_CONTEXT", "fixture-context".into()),
        ]);
        let mut policy = effigy_containers::prepare_container_policy(&repo, Some("dev")).unwrap();
        policy.workspace_user = Some("vscode".into());
        policy.workspace_home = Some("/home/vscode".into());
        let mut labels = serde_json::Map::new();
        for file in &policy.compose_files {
            let document: serde_yaml::Value =
                serde_yaml::from_str(&policy.read_compose(file).unwrap()).unwrap();
            if let Some(mapping) = document
                .get("services")
                .and_then(|services| services.get("app"))
                .and_then(|service| service.get("labels"))
                .and_then(|value| value.as_mapping())
            {
                for (key, value) in mapping {
                    let value = match value {
                        serde_yaml::Value::String(value) => value.clone(),
                        serde_yaml::Value::Bool(value) => value.to_string(),
                        _ => continue,
                    };
                    labels.insert(key.as_str().unwrap().into(), value.into());
                }
            }
        }
        labels.insert(
            "com.docker.compose.project".into(),
            policy.project_name.clone().into(),
        );
        labels.insert("com.docker.compose.service".into(), "app".into());
        labels.insert("com.docker.compose.oneoff".into(), "False".into());
        let inspect = fixture.path().join("inspect.json");
        let container_id = "a1".repeat(32);
        std::fs::write(&inspect, serde_json::to_vec(&serde_json::json!([{
            "Id":container_id, "Name":format!("/{}-app-1",policy.project_name), "Image":format!("sha256:{}","c".repeat(64)),
            "State":{"Running":true}, "Config":{"Image":"fixture:latest","Labels":labels}
        }])).unwrap()).unwrap();
        let docker = bin.join("docker");
        std::fs::write(&docker, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\ncase \" $* \" in\n *' image inspect '*) echo '[{{\"Id\":\"sha256:{}\"}}]' ;;\n *' ps '*) echo {} ;;\n *' inspect '*) cat '{}' ;;\nesac\n", fixture.path().join("docker.calls").display(), "c".repeat(64), "a1".repeat(32), inspect.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(docker, std::fs::Permissions::from_mode(0o755)).unwrap();
        if matches!(fault, Fault::Hybrid) {
            let text = std::fs::read_to_string(repo.join("effigy.toml")).unwrap();
            std::fs::write(
                repo.join("effigy.toml"),
                text.split_once("[systems]").unwrap().0,
            )
            .unwrap();
        }
        let env = environment::env_ref(&policy).expect("environment");
        let task = TaskId::new(env.checkout, "", Surface::Published, "workspace", None).unwrap();
        let invocation = ResolvedInvocation::new(InvocationSpec {
            owner: task.resource(),
            task,
            route: Route::Task,
            argv: vec![],
            cwd: String::new(),
            target: ExecutionTarget::Host,
            env: BTreeMap::new(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: BTreeMap::new(),
        })
        .unwrap();
        let version = effigy_core::build_info::active_version();
        let paths = WorkspacePathMap {
            host_checkout: repo.to_string_lossy().into(),
            guest_checkout: "/workspace".into(),
            workdir: "/workspace".into(),
            user: "vscode".into(),
            home: "/home/vscode".into(),
        };
        let identity = WorkspaceContainerIdentity {
            container_id: "a1".repeat(32),
            vm_boot_id: "3a7c86df-0045-472e-b065-c1295816ab22".into(),
            platform: "x86_64".into(),
            libc: "gnu".into(),
        };
        let inputs = PublishInputs {
            artifact: ArtifactEvidence::Unselected {
                image_reference: "fixture".into(),
                backend: "docker".into(),
            },
            selection: Digest::of(b"selection"),
            inputs_digest: Digest::of(b"inputs"),
            // Selection/staging fixtures start from an independently observed
            // container; creation authority is proved by the #510 producer tests.
            target: TargetEvidence::Observed(ContainerIdentity {
                container_id: identity.container_id.clone(),
                vm_boot_id: identity.vm_boot_id.clone(),
                backend: "docker".into(),
            }),
            platform: None,
            guest: Some(GuestContextInputs {
                version: version.clone(),
                protocol: 1,
                host_user: "1000".into(),
                checkout_id: env.checkout.text(),
                environment_resource: env.resource.name().to_string(),
                profile: "default".into(),
                runtime: None,
                vm_type: None,
                routes: vec![],
                paths: paths.clone(),
                sources: vec![],
            }),
        };

        let mut guest = FakeGuest::new().with_container(identity.clone());
        // A predecessor inode and selectors are retained throughout staging failures.
        guest.put_file(BINARY_CURRENT_PATH, b"/predecessor", 0o644);
        guest.put_link(ENTRY_SELECTOR, "/predecessor");
        guest.put_file(CONTEXT_CURRENT_PATH, b"3", 0o644);
        guest.put_file("/predecessor", b"old bytes", 0o555);
        let artifact = repo.join("artifact");
        let bytes = elf();
        std::fs::write(&artifact, &bytes).unwrap();
        std::fs::write(workspace_effigy_active_version_file(&artifact), &version).unwrap();
        std::fs::write(
            workspace_artifact_receipt_path(&artifact),
            format!(
                "release_triple=x86_64-unknown-linux-gnu\nsha256={}\n",
                Digest::of(&bytes).hex()
            ),
        )
        .unwrap();
        let object = GuestObject {
            path: format!("/run/effigy/objects/bin/{}", Digest::of(&bytes)),
            digest: Digest::of(&bytes),
        };
        let alias =
            ChannelAliasPlan::observed(Backend::LinuxDocker, "default", None, None).unwrap();
        let shared = Rc::new(RefCell::new(guest));
        let probed = std::rc::Rc::new(std::cell::Cell::new(false));
        let probe = Probe {
            identity: GuestSelfObservation {
                container_id: identity.container_id.clone(),
                vm_boot_id: identity.vm_boot_id.clone(),
                platform: identity.platform.clone(),
                libc: identity.libc.clone(),
            },
            fault,
            guest: shared.clone(),
            context_probed: probed.clone(),
        };
        let minted = Cell::new(0);
        let io_count = Rc::new(Cell::new(0));
        let fired = Rc::new(Cell::new(false));
        let io_fault = match fault {
            Fault::Io(index, after) => Some((index, after)),
            _ => None,
        };
        let request = WorkspaceUpRequest {
            policy: &policy,
            retention: effigy_containers::environment::Retention::Persistent,
            inputs: &inputs,
            invocation: &invocation,
            options: BeginOpts {
                id: RunId::new(1_760_000_000_000, [1; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                custody: None,
                next: effigy_kernel::outcome::Invocation::validated(
                    vec!["effigy".into(), "container".into(), "status".into()],
                    |_| true,
                )
                .unwrap(),
            },
            supervisor_id: "fixture-supervisor",
        };
        let gated_reads = std::rc::Rc::new(Cell::new(0));
        let gated_reads_for_closure = gated_reads.clone();
        let probe_identity = probe.identity.clone();
        let probe_guest = probe.guest.clone();
        let probe_probed = probe.context_probed.clone();
        let probe_fault = probe.fault;
        let observe = |activation: &mut WorkspaceActivation<'static>| {
            let target = ContainerIdentity {
                container_id: identity.container_id.clone(),
                vm_boot_id: identity.vm_boot_id.clone(),
                backend: "docker".into(),
            };
            let image = ImagePlatform {
                os: "linux".into(),
                architecture: "amd64".into(),
                variant: String::new(),
                loader: LoaderFamily::Gnu,
            };
            let generation = activation.reservation().generation().get().to_string();
            let context = WorkspaceContextInput {
                generation: 0,
                host: WorkspaceHostIdentity {
                    version: version.clone(),
                    protocol: 1,
                },
                binary: GuestBinaryIdentity {
                    version: version.clone(),
                    digest: format!("sha256:{}", object.digest.hex()),
                    protocol: 1,
                },
                activation: WorkspaceActivationIdentity {
                    host_user: "1000".into(),
                    checkout_id: env.checkout.text(),
                    environment: WorkspaceEnvironmentIdentity {
                        resource_id: env.resource.name().to_string(),
                        generation: generation.clone(),
                    },
                    container: identity.clone(),
                },
                session_run_id: None,
                session_generation: None,
                backend: WorkspaceBackendDescriptor {
                    kind: "docker".into(),
                    profile: "default".into(),
                    runtime: None,
                    vm_type: None,
                },
                paths: paths.clone(),
                sources: vec![],
                routes: vec![],
                channel: WorkspaceChannelLocator::default(),
            };
            let snapshots = if matches!(fault, Fault::Publication) {
                vec![(
                    GuestObject {
                        path: "/run/effigy/bad".into(),
                        digest: Digest::of(b"expected"),
                    },
                    b"wrong".to_vec(),
                )]
            } else {
                vec![]
            };
            let reobserved_target = target.clone();
            let reobserved_image = image.clone();
            let reobserved_project = policy.project_name.clone();
            let reobserved_object = object.clone();
            let reobserved_version = version.clone();
            let creating_run = activation.environment_mut().id();
            let original_deadline = activation.environment_mut().observation_deadline(
                std::time::Instant::now() + Duration::from_secs(24 * 60 * 60),
            );
            Ok(ReservedGuestInput {
                guest: Box::new(Guest(
                    shared.clone(),
                    io_fault,
                    io_count.clone(),
                    fired.clone(),
                )),
                artifact: artifact.clone(),
                reobserve: Box::new({
                    let gated_reads = gated_reads_for_closure;
                    move |deadline| {
                        gated_reads.set(gated_reads.get() + 1);
                        assert!(std::time::Instant::now() < deadline);
                        assert!(
                            deadline <= original_deadline,
                            "callback exceeded the activation deadline"
                        );
                        Ok((
                            Observed::Verified(
                                vec![TargetCandidate {
                                    identity: match fault {
                                        // A replaced target keeps the shape and changes
                                        // the boot id.
                                        Fault::GatedChangedTarget => ContainerIdentity {
                                            vm_boot_id: "ff".repeat(32),
                                            ..reobserved_target.clone()
                                        },
                                        // Another valid full id: individually a good
                                        // observation, but not the frozen one.
                                        Fault::GatedIdMismatch => ContainerIdentity {
                                            container_id: "ee".repeat(32),
                                            ..reobserved_target.clone()
                                        },
                                        _ => reobserved_target.clone(),
                                    },
                                    project: reobserved_project,
                                    created_by: Some(creating_run),
                                }],
                                evidence(),
                            ),
                            Observed::Verified(
                                BoundPlatform {
                                    target: reobserved_target,
                                    platform: reobserved_image.clone(),
                                    artifact_platform: reobserved_image,
                                    artifact: reobserved_object,
                                    artifact_version: reobserved_version,
                                    execution_available: true,
                                },
                                evidence(),
                            ),
                        ))
                    }
                }),
                targets: Observed::Verified(
                    vec![TargetCandidate {
                        identity: target.clone(),
                        project: policy.project_name.clone(),
                        created_by: Some(activation.environment_mut().id()),
                    }],
                    evidence(),
                ),
                platform: Observed::Verified(
                    BoundPlatform {
                        target,
                        platform: image.clone(),
                        artifact_platform: image,
                        artifact: object.clone(),
                        artifact_version: version.clone(),
                        execution_available: true,
                    },
                    evidence(),
                ),
                alias: alias.clone(),
                probe: Box::new(Probe {
                    identity: probe_identity.clone(),
                    fault: probe_fault,
                    guest: probe_guest.clone(),
                    context_probed: probe_probed.clone(),
                }),
                context,
                snapshots,
                credential: Box::new(|session, g| {
                    minted.set(minted.get() + 1);
                    assert_eq!(g, 4);
                    assert_eq!(
                        session.environment_generation().get().to_string(),
                        generation
                    );
                    // A canonical channel credential: the serving facts keep
                    // the exact bytes the guest presents, so publication
                    // refuses any other shape.
                    Ok(WorkspaceEnrolmentCredential::new(
                        generation,
                        identity.container_id.clone(),
                        "ab".repeat(32),
                    ))
                }),
                deadline: match fault {
                    // The one absolute deadline is already past: the producer must
                    // refuse rather than measure a fresh budget from now.
                    Fault::ExpiredDeadline | Fault::GatedExpiredDeadline => {
                        std::time::Instant::now() - Duration::from_secs(1)
                    }
                    // The gated re-observation must still see the activation's own
                    // deadline however far out this one is.
                    Fault::LongObservationBudget => {
                        std::time::Instant::now() + Duration::from_secs(24 * 60 * 60)
                    }
                    _ => std::time::Instant::now() + Duration::from_secs(20),
                },
            })
        };
        let result = match fault {
            Fault::Recovery(partial) => {
                recover_fixture(request, observe, shared.clone(), partial, fault)
            }
            Fault::RecoveryDead | Fault::RecoveryUnknown => {
                recover_fixture(request, observe, shared.clone(), 0, fault)
            }
            _ => up_prepared(request, &[], observe),
        };
        if matches!(fault, Fault::None | Fault::LongObservationBudget) {
            let calls = std::fs::read_to_string(fixture.path().join("docker.calls")).unwrap();
            let applies = calls
                .lines()
                .filter(|line| line.contains(" up -d "))
                .collect::<Vec<_>>();
            assert_eq!(applies.len(), 1, "one ordinary Compose apply");
            assert!(applies[0].starts_with("--context fixture-context compose "));
            assert!(applies[0].contains("--build --remove-orphans"));
            for file in &policy.compose_files {
                assert!(applies[0].contains(file.to_str().unwrap()));
            }
        }
        if matches!(
            fault,
            Fault::GatedChangedTarget | Fault::GatedIdMismatch | Fault::GatedExpiredDeadline
        ) {
            match &result {
                Err(error) => {
                    let text = format!("{error:?}");
                    if matches!(fault, Fault::GatedExpiredDeadline) {
                        // A spent deadline refuses before the read, so the read
                        // never happens and the message names the deadline.
                        assert!(
                            text.contains("deadline") || text.contains("expired"),
                            "{text}"
                        );
                    } else {
                        assert!(
                            text.contains("preflight and gated target/image identities"),
                            "{text}"
                        );
                    }
                }
                Ok(_) => panic!("a gated re-observation that differs must refuse"),
            }
            assert_eq!(minted.get(), 0, "no credential is minted");
            if matches!(fault, Fault::GatedExpiredDeadline) {
                assert_eq!(
                    gated_reads.get(),
                    0,
                    "a spent absolute deadline must refuse before the gated read runs"
                );
            } else {
                assert_eq!(gated_reads.get(), 1, "the gated read ran exactly once");
            }
        }
        let mut guest = shared.borrow_mut();
        assert_eq!(guest.text("/predecessor").as_deref(), Some("old bytes"));
        if matches!(fault, Fault::Hybrid) {
            assert!(matches!(result, Ok(WorkspaceUp::Hybrid)));
            assert_eq!(minted.get(), 0);
            assert_eq!(io_count.get(), 0);
            assert!(!probed.get());
            assert_eq!(guest.text(CONTEXT_CURRENT_PATH).as_deref(), Some("3"));
        } else if matches!(
            fault,
            Fault::None
                | Fault::LongObservationBudget
                | Fault::Recovery(_)
                | Fault::RecoveryUnknown
        ) {
            let WorkspaceUp::Selected(mut owner) = result.expect("selected") else {
                panic!("selected owner")
            };
            assert_eq!(minted.get(), 1);
            assert!(probed.get());
            assert_eq!(guest.text(CONTEXT_CURRENT_PATH).as_deref(), Some("4"));
            let doc =
                WorkspaceContextV2::parse(&guest.read_file(&context_path(4)).unwrap().unwrap())
                    .unwrap();
            assert_eq!(doc.session.run_id, owner.session.run().to_string());
            assert_eq!(doc.session.generation, owner.session.generation().get());
            assert_eq!(doc.context_generation, 4);
            assert_eq!(
                doc.locator.endpoint,
                owner.channel.locator.channel().endpoint.clone().unwrap()
            );
            assert_eq!(guest.mode(&doc.credential.path), Some(0o600));
            let live = crate::runner::workspace_client::context::LiveGuestRun {
                binary: GuestBinaryIdentity {
                    version: doc.version.clone(),
                    digest: doc.artifact_digest.clone(),
                    protocol: 1,
                },
                container: identity.clone(),
                environment_generation: owner.session.environment_generation().get().to_string(),
                session_run_id: Some(owner.session.run().to_string()),
                session_generation: Some(owner.session.generation().get()),
                context_generation: 4,
            };
            let verified = doc
                .verify(&mut IndependentIdentity(probe_identity.clone()), &live)
                .expect("actual produced document independently verified");
            assert_eq!(
                verified.session_generation,
                owner.session.generation().get()
            );
            let mut stale = live.clone();
            stale.session_generation = Some(owner.session.generation().get() + 1);
            assert!(doc
                .verify(&mut IndependentIdentity(probe_identity.clone()), &stale)
                .is_err());
            assert_eq!(
                crate::runner::workspace_client::context::v2::read_published_v2(&mut *guest)
                    .unwrap()
                    .unwrap(),
                doc
            );

            owner.channel.launch.stop();
            let cleanup = owner
                .effect
                .operation()
                .cleanup(Duration::from_secs(10))
                .unwrap();
            owner
                .supervisor
                .settle(&cleanup, &mut effigy_containers::environment::EnvSettlement)
                .unwrap();
        } else if matches!(fault, Fault::ExpiredDeadline) {
            let error = match result.expect_err("an expired publication deadline refuses") {
                WorkspaceUpError::Owner { cause, .. } => cause.to_string(),
                WorkspaceUpError::Finish { cause, .. } => cause.to_string(),
                WorkspaceUpError::Preparation(error) => error.to_string(),
            };
            assert!(error.contains("deadline expired"), "{error}");
            assert_eq!(minted.get(), 0, "no credential is minted past the deadline");
            assert_eq!(io_count.get(), 0, "an expired budget performs no guest I/O");
            assert_eq!(guest.text(CONTEXT_CURRENT_PATH).as_deref(), Some("3"));
            assert_eq!(
                guest.text(BINARY_CURRENT_PATH).as_deref(),
                Some("/predecessor")
            );
            assert_eq!(
                guest.link_target(ENTRY_SELECTOR).as_deref(),
                Some("/predecessor")
            );
            // The refused publication is recorded unknown, so it neither
            // publishes nor leaves a selection obligation a successor must settle.
            let ctx = environment::context(&repo).unwrap();
            let observed = effigy_kernel::store::Store::read(&ctx.observe(), ctx.root);
            let journal = observed.verified().expect("journal");
            assert!(
                !matches!(
                    effigy_kernel::workspace_session::unfinished_selection(journal, &env.resource),
                    Observed::Verified(Some(_), _)
                ),
                "a publication that never started leaves no pending selection"
            );
        } else if matches!(fault, Fault::Io(..)) {
            assert!(
                fired.get(),
                "every enumerated I/O boundary was reached: {fault:?}"
            );
            assert!(
                result.is_err(),
                "a partial guest effect cannot return success: {fault:?}"
            );
            let binary = guest.text(BINARY_CURRENT_PATH).unwrap();
            assert!(binary == "/predecessor" || binary == object.path);
            let entry = guest.link_target(ENTRY_SELECTOR).unwrap();
            assert!(entry == "/predecessor" || entry == object.path);
            assert!(matches!(
                guest.text(CONTEXT_CURRENT_PATH).as_deref(),
                Some("3" | "4")
            ));
            // If a committed switch was interrupted, abort settles the exact
            // successor then abandons its obligation. No Pending is waved away.
            let ctx = environment::context(&repo).unwrap();
            let observed = effigy_kernel::store::Store::read(&ctx.observe(), ctx.root);
            let journal = observed.verified().expect("journal");
            assert!(
                !matches!(
                    effigy_kernel::workspace_session::unfinished_selection(journal, &env.resource),
                    Observed::Verified(Some(_), _)
                ),
                "failed selection must complete bounded abandonment: {fault:?}"
            );
        } else {
            assert!(result.is_err(), "{fault:?}");
            assert_eq!(guest.text(CONTEXT_CURRENT_PATH).as_deref(), Some("3"));
            assert_eq!(
                guest.text(BINARY_CURRENT_PATH).as_deref(),
                Some("/predecessor")
            );
            assert_eq!(
                guest.link_target(ENTRY_SELECTOR).as_deref(),
                Some("/predecessor")
            );
            if matches!(
                fault,
                Fault::Identity
                    | Fault::UnknownIdentity
                    | Fault::GatedChangedTarget
                    | Fault::GatedIdMismatch
                    | Fault::GatedExpiredDeadline
            ) {
                assert_eq!(minted.get(), 0);
            } else {
                assert_eq!(
                    minted.get(),
                    1,
                    "fault reached its intended boundary: {fault:?}"
                );
            }
            // The gated re-observation refuses before anything is staged, which is
            // exactly the point: no byte reaches the guest on a changed target.
            assert!(
                guest.file_sha256(&object.path).unwrap().is_none()
                    || !matches!(
                        fault,
                        Fault::GatedChangedTarget
                            | Fault::GatedIdMismatch
                            | Fault::GatedExpiredDeadline
                    ),
                "a gated refusal must not stage the artifact"
            );
            assert!(
                guest.file_sha256(&object.path).unwrap().is_some()
                    || matches!(
                        fault,
                        Fault::GatedChangedTarget
                            | Fault::GatedIdMismatch
                            | Fault::GatedExpiredDeadline
                    ),
                "actual staging ran before injected fault"
            );
        }
        io_count.get()
    })
}

#[test]
fn workspace_reserved_owner_publishes_one_full_v2_through_real_commit_and_selection() {
    run(Fault::None);
}
#[test]
fn workspace_reserved_owner_refuses_identity_alias_peer_pin_and_publication_without_selector_switch(
) {
    for fault in [
        Fault::Identity,
        Fault::UnknownIdentity,
        Fault::Alias,
        Fault::Peer,
        Fault::Pin,
        Fault::Publication,
        Fault::ContextProbe,
        Fault::ContextPeer,
        Fault::ContextPin,
        Fault::StaleDocument,
    ] {
        run(fault);
    }
}

#[test]
fn workspace_reserved_every_guest_write_boundary_crash_before_after_preserves_authority() {
    let count = run(Fault::None);
    assert!(count >= 10, "enumerate real staging and selection effects");
    for index in 1..=count {
        for after in [false, true] {
            run(Fault::Io(index, after));
        }
    }
}

struct TlsProbe;
impl effigy_kernel::workspace_session::ServingProbe for TlsProbe {
    fn pinned_handshake(
        &mut self,
        endpoint: std::net::SocketAddr,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<()> {
        match TlsChannelTransport::connect(endpoint, pin.clone(), budget) {
            Ok(transport) => {
                drop(transport);
                Observed::Verified((), evidence())
            }
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
}
struct ExactTarget(Rc<RefCell<FakeGuest>>);
impl effigy_kernel::workspace_session::TargetProbe for ExactTarget {
    fn verify_target(&mut self, container: &str, boot: &str, _: Duration) -> Observed<()> {
        match self.0.borrow_mut().probe_container_identity() {
            Ok(actual) if actual.container_id == container && actual.vm_boot_id == boot => {
                Observed::Verified((), evidence())
            }
            Ok(_) => Observed::Absent(effigy_kernel::observe::AbsenceProof::ContainerGone {
                id: container.into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
}
fn recover_fixture<'a>(
    request: WorkspaceUpRequest<'_>,
    observe: impl FnOnce(&mut WorkspaceActivation<'static>) -> Result<ReservedGuestInput<'a>, EnvError>,
    shared: Rc<RefCell<FakeGuest>>,
    partial: usize,
    fault: Fault,
) -> Result<WorkspaceUp, WorkspaceUpError> {
    use effigy_containers::environment::{Up, WorkspaceBegin};
    let failure = |cause| WorkspaceUpError::Owner {
        cause: Box::new(cause),
        cleanup: None,
    };
    let ctx = environment::context(&request.policy.repo_root).unwrap();
    let env = environment::env_ref(request.policy).unwrap();
    let backend = environment::backend(&request.policy.repo_root, request.policy);
    let closure = &request.policy.render.as_ref().unwrap().closure;
    let prepare = Up {
        render: Some(closure),
        project: &request.policy.project_name,
        backend: &backend,
        retention: request.retention,
        workdir: &request.policy.repo_root,
        primary_service: &request.policy.primary_service,
        compose_files: &request.policy.compose_files,
    };
    let preflight = effigy_containers::environment::CreationPreflight::observe(
        backend.adapter().as_mut(),
        prepare.creation_request(),
    )
    .map_err(|error| failure(EnvError::Invalid(error.to_string())))?;
    let sealed = effigy_kernel::objects::Objects::seal_for_preflight(
        &ctx.cx,
        ctx.root,
        closure,
        Instant::now() + Duration::from_secs(20),
    )
    .map_err(|error| failure(EnvError::Kernel(error)))?;
    let WorkspaceBegin::Activate(mut activation) = WorkspaceActivation::begin(
        &ctx,
        &env,
        request.inputs,
        request.invocation,
        request.options,
        request.supervisor_id,
    )
    .map_err(failure)?
    else {
        panic!("fresh reservation");
    };
    let before = shared.borrow().tree_digest();
    assert!(
        WorkspaceActivation::begin(
            &ctx,
            &env,
            request.inputs,
            request.invocation,
            BeginOpts {
                id: RunId::new(1_760_000_000_000, [2; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                custody: None,
                next: effigy_kernel::outcome::Invocation::validated(vec!["recover".into()], |_| {
                    true
                })
                .unwrap()
            },
            "competing-supervisor"
        )
        .is_err(),
        "concurrent actor cannot reserve or enter the held Env"
    );
    assert_eq!(before, shared.borrow().tree_digest());
    let backend = environment::backend(&request.policy.repo_root, request.policy);
    activation
        .apply_sealed_with(
            Up {
                render: request.policy.render.as_ref().map(|render| &render.closure),
                project: &request.policy.project_name,
                backend: &backend,
                retention: request.retention,
                workdir: &request.policy.repo_root,
                primary_service: &request.policy.primary_service,
                compose_files: &request.policy.compose_files,
            },
            &sealed,
            &preflight,
            || Ok(()),
        )
        .map_err(failure)?;
    let input = observe(&mut activation).map_err(failure)?;
    let mut publication = prepare_reserved_guest(&mut activation, input).map_err(failure)?;
    let committed = activation
        .commit(&publication.receipt, &publication.channel.launch)
        .map_err(failure)?;
    let obligation = committed.selection_obligation().unwrap();
    for change in obligation
        .receipt()
        .selection()
        .switches
        .iter()
        .take(partial)
    {
        if change.path == ENTRY_SELECTOR {
            shared
                .borrow_mut()
                .replace_link(&change.path, &change.next)
                .unwrap();
        } else {
            shared
                .borrow_mut()
                .replace_pointer(&change.path, &change.next)
                .unwrap();
        }
    }
    // The activation ends; the independently retained supervisor stays alive.
    let (supervisor, session) = match activation.interrupt_for_recovery() {
        Ok(value) => value,
        Err((cause, custody)) => {
            return Err(WorkspaceUpError::Finish {
                cause: Box::new(cause),
                custody,
                channel: Box::new(publication.channel),
                serving: Box::new(publication.serving),
            })
        }
    };
    let mut owner = WorkspaceOwner {
        supervisor,
        session,
        serving: crate::runner::workspace_client::ServingFacts::fixture(
            &publication.channel.locator,
            std::path::PathBuf::from("/workspace"),
        ),
        channel: publication.channel,
        creation: None,
        effect: std::sync::Arc::new(ctx.cx.clone()),
        observe: std::sync::Arc::new(ctx.observe()),
        root: ctx.root,
    };
    if matches!(fault, Fault::RecoveryDead) {
        owner.channel.launch.stop();
    }
    if matches!(fault, Fault::RecoveryUnknown) {
        assert!(recover_selection(
            &mut owner,
            obligation,
            publication.guest.as_mut(),
            &mut UnknownTarget,
            &mut TlsProbe
        )
        .is_err());
        assert_eq!(
            shared.borrow().text(CONTEXT_CURRENT_PATH).as_deref(),
            Some("3"),
            "unknown liveness cannot switch"
        );
    }
    let recovered = recover_selection(
        &mut owner,
        obligation,
        publication.guest.as_mut(),
        &mut ExactTarget(shared.clone()),
        &mut TlsProbe,
    );
    if matches!(fault, Fault::RecoveryDead) {
        assert!(recovered.is_err());
        assert_eq!(
            shared.borrow().text(CONTEXT_CURRENT_PATH).as_deref(),
            Some("3"),
            "dead listener cannot switch"
        );
        let cleanup = owner
            .effect
            .operation()
            .cleanup(Duration::from_secs(10))
            .unwrap();
        owner
            .supervisor
            .settle(&cleanup, &mut effigy_containers::environment::EnvSettlement)
            .unwrap();
        let closed = || Observed::Absent(effigy_kernel::observe::AbsenceProof::GateFreeRunClosed);
        effigy_kernel::workspace_session::selection_abandoned(
            &owner.effect,
            owner.root,
            obligation,
            &effigy_kernel::workspace_session::SelectionLiveness {
                session: closed(),
                target: Observed::Verified((), evidence()),
                listener: closed(),
            },
            "fixture observed dead listener after commit",
            Wait::FailFast,
        )
        .unwrap();
        return Err(failure(refused("fixture listener is established dead")));
    }
    recovered.map_err(failure)?;
    Ok(WorkspaceUp::Selected(Box::new(owner)))
}
#[test]
fn workspace_reserved_crash_after_commit_and_each_partial_switch_resumes_under_real_env_guard() {
    for partial in 0..=3 {
        run(Fault::Recovery(partial));
    }
}

struct UnknownTarget;
impl effigy_kernel::workspace_session::TargetProbe for UnknownTarget {
    fn verify_target(&mut self, _: &str, _: &str, _: Duration) -> Observed<()> {
        Observed::Unknown(UnknownReason::Injected)
    }
}
#[test]
fn workspace_reserved_recovery_unknown_retries_but_dead_listener_abandons_without_switch() {
    run(Fault::RecoveryUnknown);
    run(Fault::RecoveryDead);
}

struct IndependentIdentity(GuestSelfObservation);
impl crate::runner::workspace_client::identity::GuestIdentity for IndependentIdentity {
    fn observe(
        &mut self,
    ) -> Result<GuestSelfObservation, crate::runner::workspace_client::identity::GuestIdentityRefusal>
    {
        Ok(self.0.clone())
    }
}

#[test]
fn workspace_reserved_hybrid_does_not_admit_stage_probe_or_mint() {
    run(Fault::Hybrid);
}

#[test]
fn workspace_reserved_gated_reobservation_of_a_replaced_target_refuses() {
    run(Fault::GatedChangedTarget);
}

#[test]
fn workspace_reserved_gated_reobservation_of_another_valid_id_refuses() {
    run(Fault::GatedIdMismatch);
}

#[test]
fn workspace_reserved_gated_reobservation_past_the_deadline_refuses_without_reading() {
    run(Fault::GatedExpiredDeadline);
}

#[test]
fn workspace_reserved_expired_publication_deadline_refuses_without_guest_io() {
    run(Fault::ExpiredDeadline);
}

#[test]
fn workspace_reserved_callback_keeps_the_original_activation_deadline() {
    run(Fault::LongObservationBudget);
}

// Shared external-I/O doubles for the creator producer/consumer integration.
pub(in crate::runner) fn creator_guest(shared: Rc<RefCell<FakeGuest>>) -> Box<dyn GuestWorkspace> {
    Box::new(Guest(
        shared,
        None,
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(false)),
    ))
}

pub(in crate::runner) fn creator_probe(
    identity: WorkspaceContainerIdentity,
    guest: Rc<RefCell<FakeGuest>>,
) -> Box<dyn WorkspaceGuestProbe> {
    Box::new(Probe {
        identity: GuestSelfObservation {
            container_id: identity.container_id,
            vm_boot_id: identity.vm_boot_id,
            platform: identity.platform,
            libc: identity.libc,
        },
        fault: Fault::None,
        guest,
        context_probed: Rc::new(Cell::new(false)),
    })
}
