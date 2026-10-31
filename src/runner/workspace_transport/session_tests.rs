use super::listener::SessionTlsListener;
use super::qualification::*;
use super::*;
use crate::runner::workspace_client::context::WorkspaceContainerIdentity;
use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OperationContext,
    Placement,
};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, Generation, InvocationSpec, ResolvedInvocation,
    ResourceId, Route, RunId, Surface, TaskId,
};
use effigy_kernel::observe::{Evidence, Observed};
use effigy_kernel::outcome::Invocation;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, Run, SettlementAdapter};
use effigy_kernel::workspace_session::{ChannelListener, ListenerPhase, SessionLaunch, SessionRef};
use std::collections::BTreeMap;

fn evidence() -> Evidence {
    Evidence::new("private workspace TLS proof".into(), None).unwrap()
}
struct NoEffects;
impl SettlementAdapter for NoEffects {
    fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Verified(effect.spec.target.clone(), evidence())
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Verified((), evidence())
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Verified(Reconciled::Released, evidence())
    }
}
fn with_session(test: impl FnOnce(&mut Run<'_>, &SessionRef)) {
    with_session_at(None, test);
}
fn with_session_at(
    directory: Option<&std::path::Path>,
    test: impl FnOnce(&mut Run<'_>, &SessionRef),
) {
    let cx: EffectContext = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap();
    let home = directory
        .is_none()
        .then(|| effigy_test_support::Fixture::short_home("workspace-tls-").unwrap());
    let root = StateRoot::effects(
        &cx,
        directory.unwrap_or_else(|| home.as_ref().unwrap().path()),
    )
    .unwrap();
    let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [37; 16]);
    let environment = ResourceId::environment(checkout, "stack").unwrap();
    let task = TaskId::new(checkout, "", Surface::Published, "workspace", None).unwrap();
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap();
    let opts = BeginOpts {
        id: RunId::new(1, [37; 10]).unwrap(),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: Invocation::validated(vec!["recover".into()], |args| args == ["recover"]).unwrap(),
        custody: None,
    };
    let cleanup = cx.operation().cleanup(Duration::from_secs(5)).unwrap();
    // Create the environment row through admission. Only its fixture engine
    // observation is seeded directly; production activation owns this state.
    let mut env_spec = invocation.spec().clone();
    env_spec.owner = environment.clone();
    let env_invocation = ResolvedInvocation::new(env_spec).unwrap();
    let env_opts = BeginOpts {
        id: RunId::new(1, [36; 10]).unwrap(),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: Invocation::validated(vec!["recover".into()], |args| args == ["recover"]).unwrap(),
        custody: None,
    };
    Run::begin(
        &cx,
        &root,
        &env_invocation,
        env_opts,
        &cleanup,
        &mut NoEffects,
    )
    .unwrap()
    .settle(&cleanup, &mut NoEffects)
    .unwrap();
    let connection =
        rusqlite::Connection::open(root.path().join(effigy_kernel::store::DB_NAME)).unwrap();
    connection
        .execute(
            "UPDATE resources SET state='active',last_applied='1' WHERE name=?1",
            [environment.name().to_string()],
        )
        .unwrap();
    drop(connection);
    let (run, session) = SessionRef::admit(
        &cx,
        &root,
        &invocation,
        opts,
        &cleanup,
        &mut NoEffects,
        &Fence {
            resource: environment,
            generation: Generation::new(1),
        },
        effigy_kernel::supervise::Placement::Detached,
    )
    .unwrap();
    let mut run =
        effigy_kernel::workspace_session::SessionSupervisor::adopt(run, "tls-supervisor").unwrap();
    test(&mut run, &session);
}
fn target() -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: "a".repeat(64),
        vm_boot_id: "9b10e808-091a-433b-9ec9-e2624a1362ef".into(),
        platform: "linux/arm64".into(),
        libc: "gnu".into(),
    }
}
struct PrivateGuest {
    target: WorkspaceContainerIdentity,
    peer: SocketAddr,
    wrong_pin: bool,
    calls: usize,
}
impl GuestChannelProbe for PrivateGuest {
    fn resolve(
        &mut self,
        _: &WorkspaceContainerIdentity,
        alias: &str,
        _: Duration,
    ) -> Observed<ResolvedAlias> {
        assert_eq!(alias, "effigy-host");
        Observed::Verified(
            ResolvedAlias {
                target: self.target.clone(),
                address: self.peer.ip(),
            },
            evidence(),
        )
    }
    fn dial(
        &mut self,
        _: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        if self.peer.port() == 0 {
            self.peer.set_port(
                endpoint
                    .strip_prefix("effigy-host:")
                    .unwrap()
                    .parse()
                    .unwrap(),
            );
        }
        assert_eq!(endpoint, format!("effigy-host:{}", self.peer.port()));
        self.calls += 1;
        // The private fixture is the guest's network namespace. The TLS dial
        // sends no application frame and reports the address actually dialled.
        match TlsChannelTransport::connect(self.peer, pin.clone(), budget) {
            Ok(transport) => {
                let verified = transport.verified_peer().unwrap().digest;
                drop(transport);
                Observed::Verified(
                    GuestDialEvidence {
                        target: self.target.clone(),
                        peer: self.peer,
                        pin: if self.wrong_pin {
                            Digest::of(b"wrong")
                        } else {
                            verified
                        },
                    },
                    evidence(),
                )
            }
            Err(_) => Observed::Unknown(effigy_kernel::observe::UnknownReason::Unparsable(
                "private guest dial failed".into(),
            )),
        }
    }
}
// A private TCP proxy gives the guest an IPv6 endpoint while the host binds
// IPv4. No machine routing, DNS, gateway or container is touched.
struct PrivateProxy {
    stop: Arc<std::sync::atomic::AtomicBool>,
    sockets: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl PrivateProxy {
    fn start(host: SocketAddr) -> Self {
        use std::sync::atomic::Ordering;
        let listener =
            std::net::TcpListener::bind(SocketAddr::new("::1".parse().unwrap(), host.port()))
                .unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sockets = Arc::new(std::sync::Mutex::new(Vec::<std::net::TcpStream>::new()));
        let worker_stop = stop.clone();
        let worker_sockets = sockets.clone();
        let worker = std::thread::spawn(move || {
            let mut connections = Vec::new();
            while !worker_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut guest, _)) => {
                        guest.set_nonblocking(false).unwrap();
                        guest
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        guest
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let Ok(mut upstream) =
                            std::net::TcpStream::connect_timeout(&host, Duration::from_millis(200))
                        else {
                            continue;
                        };
                        upstream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        upstream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        {
                            let mut sockets = worker_sockets.lock().unwrap();
                            sockets.push(guest.try_clone().unwrap());
                            sockets.push(upstream.try_clone().unwrap());
                        }
                        connections.push(std::thread::spawn(move || {
                            let mut guest_read = guest.try_clone().unwrap();
                            let mut upstream_write = upstream.try_clone().unwrap();
                            let forward = std::thread::spawn(move || {
                                let _ = std::io::copy(&mut guest_read, &mut upstream_write);
                                let _ = upstream_write.shutdown(std::net::Shutdown::Write);
                            });
                            let _ = std::io::copy(&mut upstream, &mut guest);
                            let _ = guest.shutdown(std::net::Shutdown::Write);
                            forward.join().unwrap();
                        }));
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("private proxy accept: {error}"),
                }
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            stop,
            sockets,
            worker: Some(worker),
        }
    }
}
impl Drop for PrivateProxy {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        for socket in self.sockets.lock().unwrap().iter() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn backend_qualification(backend: Backend) {
    with_session(|run, session| {
        let target = target();
        let ip = "127.0.0.1".parse().unwrap();
        let guest_ip = if backend == Backend::LinuxDocker {
            ip
        } else {
            "::1".parse().unwrap()
        };
        let plan = ChannelAliasPlan::observed(backend, "private-profile", Some(ip), Some(guest_ip))
            .unwrap();
        assert_eq!(
            plan.render_value,
            if backend == Backend::Colima {
                "::1"
            } else {
                "host-gateway"
            }
        );
        let resolved = ResolvedAlias {
            target: target.clone(),
            address: guest_ip,
        };
        let bind = channel_bind(&plan, &target, &resolved).unwrap();
        let (listener, key, mut acceptor) =
            SessionTlsListener::bind(bind.listen, Duration::from_secs(1)).unwrap();
        let endpoint = listener.endpoint().unwrap();
        let _proxy = (backend != Backend::LinuxDocker).then(|| PrivateProxy::start(endpoint));
        let guest_endpoint = SocketAddr::new(guest_ip, endpoint.port());
        let pin = key.pin();
        let mut launch =
            SessionLaunch::launch(run, session, "tls-supervisor", key, Box::new(listener)).unwrap();
        assert!(acceptor.accept(&launch, run).is_err());
        assert!(launch.publish(run).is_err());
        let mut guest = PrivateGuest {
            target: target.clone(),
            peer: guest_endpoint,
            wrong_pin: false,
            calls: 0,
        };
        let mut probe = GuestServingProbe {
            guest: &mut guest,
            target: &target,
            bind: &bind,
            qualified_pin: None,
        };
        let locator = probe
            .qualify(&mut launch, run, Duration::from_secs(2))
            .unwrap();
        assert_eq!(locator.session().run, session.run());
        assert_eq!(locator.bound().lease().pin(), &pin);
        assert_eq!(locator.bound().lease().socket(), Some(endpoint));
        assert_eq!(
            locator.channel().endpoint.as_deref(),
            Some(locator.bound().lease().endpoint())
        );
        assert_eq!(
            locator.bound().lease().endpoint(),
            format!("effigy-host:{}", endpoint.port())
        );
        assert_eq!(locator.target(), &target);
        assert_eq!(
            locator.channel().server_identity,
            Some(format!("sha256:{}", pin.hex()))
        );
        assert_eq!(
            launch.publish(run).unwrap().lease(),
            locator.bound().lease()
        );
        let mut numeric = locator.channel().clone();
        numeric.endpoint = Some(endpoint.to_string());
        assert!(super::opener::open_pinned_channel(
            Duration::from_secs(1),
            1,
            &numeric,
            || panic!("numeric locator must refuse before resolution")
        )
        .is_err());
        launch.serve(run).unwrap();
        assert!(acceptor.accept(&launch, run).unwrap().is_none());
        // Transport is accepted only after the actual session allows serving.
        let mut client = super::opener::open_pinned_channel(
            Duration::from_secs(2),
            1,
            locator.channel(),
            || Ok(format!("{guest_ip} effigy-host\n").into_bytes()),
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        let mut retained = loop {
            if let Some(transport) = acceptor.accept(&launch, run).unwrap() {
                break transport;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        };
        retained
            .write_all(
                &((effigy_kernel::workspace_channel::MAX_FRAME_BYTES + 1) as u32).to_be_bytes(),
            )
            .unwrap();
        assert!(effigy_kernel::workspace_channel::read_frame(&mut client).is_err());
        client.operation_timeout = Duration::from_millis(100);
        let started = Instant::now();
        assert!(client.write_all(&vec![0; 8 << 20]).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        launch.abandon_owner();
        assert_eq!(launch.phase(), ListenerPhase::Stopped);
        assert!(!launch.key_held());
        assert!(launch.lease().is_none());
        assert!(retained.write_all(b"no surviving capability").is_err());
        assert!(retained.verified_peer().is_none());
        assert!(TlsChannelTransport::connect(endpoint, pin, Duration::from_millis(100)).is_err());
        assert!(launch.publish(run).is_err());
        drop(client);
    });
}
#[test]
fn colima_private_channel() {
    backend_qualification(Backend::Colima);
}
#[test]
fn docker_desktop_private_channel() {
    backend_qualification(Backend::DockerDesktop);
}
#[test]
fn linux_docker_fresh_channel() {
    backend_qualification(Backend::LinuxDocker);
}
#[test]
fn unknown_proxy_and_wrong_target_refuse_before_bind() {
    assert!(ChannelAliasPlan::observed(Backend::Colima, "private-profile", None, None).is_err());
    assert!(
        ChannelAliasPlan::observed(Backend::DockerDesktop, "private-profile", None, None).is_err()
    );
    let plan =
        ChannelAliasPlan::observed(Backend::LinuxDocker, "private-profile", None, None).unwrap();
    assert_eq!(plan.render_value, "host-gateway"); // no pre-apply network lookup
    let mut wrong = target();
    wrong.vm_boot_id = "changed".into();
    assert!(channel_bind(
        &plan,
        &target(),
        &ResolvedAlias {
            target: wrong,
            address: "127.0.0.1".parse().unwrap()
        }
    )
    .is_err());
    let proxy = ChannelAliasPlan::observed(
        Backend::DockerDesktop,
        "private-profile",
        Some("127.0.0.1".parse().unwrap()),
        None,
    )
    .unwrap();
    let bind = channel_bind(
        &proxy,
        &target(),
        &ResolvedAlias {
            target: target(),
            address: "192.0.2.15".parse().unwrap(),
        },
    )
    .unwrap();
    assert_ne!(bind.listen.ip(), bind.expected_peer); // guest proxy address is not assumed host-bindable
}
#[test]
fn missing_or_ambiguous_hosts_never_trigger_dns_fallback() {
    use super::opener::resolve_hosts;
    assert!(resolve_hosts(b"127.0.0.1 localhost\n").is_err());
    assert!(resolve_hosts(b"127.0.0.1 effigy-host\n::1 effigy-host\n").is_err());
    assert!(resolve_hosts(b"0.0.0.0 effigy-host\n").is_err());
    assert_eq!(
        resolve_hosts(b"127.0.0.1 localhost effigy-host # pinned alias\n").unwrap(),
        "127.0.0.1".parse::<std::net::IpAddr>().unwrap()
    );
}

#[test]
fn owning_session_drop_revokes_retained_listener_and_key() {
    let mut retained = None;
    with_session(|run, session| {
        let (listener, key, acceptor) =
            SessionTlsListener::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(1))
                .unwrap();
        let endpoint = listener.endpoint().unwrap();
        let pin = key.pin();
        let launch =
            SessionLaunch::launch(run, session, "tls-supervisor", key, Box::new(listener)).unwrap();
        retained = Some((launch, acceptor, endpoint, pin));
    });
    let (launch, _acceptor, endpoint, pin) = retained.unwrap();
    assert!(!launch.key_held());
    assert_eq!(launch.phase(), ListenerPhase::Stopped);
    assert!(TlsChannelTransport::connect(endpoint, pin, Duration::from_millis(100)).is_err());
}

#[test]
fn hybrid_preparation_performs_no_guest_probe_or_listener_launch() {
    with_session(|run, session| {
        let plan = ChannelAliasPlan::observed(Backend::LinuxDocker, "private-profile", None, None)
            .unwrap();
        struct NoGuest;
        impl GuestChannelProbe for NoGuest {
            fn resolve(
                &mut self,
                _: &WorkspaceContainerIdentity,
                _: &str,
                _: Duration,
            ) -> Observed<ResolvedAlias> {
                panic!("hybrid must not resolve")
            }
            fn dial(
                &mut self,
                _: &WorkspaceContainerIdentity,
                _: &str,
                _: &Digest,
                _: Duration,
            ) -> Observed<GuestDialEvidence> {
                panic!("hybrid must not dial")
            }
        }
        assert!(prepare_channel(
            ChannelPreparation {
                container_mode: false,
                plan: &plan,
                target: &target(),
                session,
                supervisor: "tls-supervisor"
            },
            run,
            &mut NoGuest,
            Duration::from_secs(1)
        )
        .unwrap()
        .is_none());
    });
}

#[test]
fn fresh_target_resolution_precedes_binding_and_failure_closes_everything() {
    with_session(|run, session| {
        let plan = ChannelAliasPlan::observed(Backend::LinuxDocker, "private-profile", None, None)
            .unwrap();
        let target = target();
        let mut guest = PrivateGuest {
            target: target.clone(),
            peer: "127.0.0.1:0".parse().unwrap(),
            wrong_pin: true,
            calls: 0,
        };
        let result = prepare_channel(
            ChannelPreparation {
                container_mode: true,
                plan: &plan,
                target: &target,
                session,
                supervisor: "tls-supervisor",
            },
            run,
            &mut guest,
            Duration::from_secs(2),
        );
        assert!(result.is_err());
        assert_eq!(guest.calls, 1);
        assert!(
            std::net::TcpStream::connect_timeout(&guest.peer, Duration::from_millis(100)).is_err()
        );
        // A failed activation cannot reuse its listener or key. A retry creates
        // and qualifies fresh custody for the same still-owned session.
        guest.peer.set_port(0);
        guest.wrong_pin = false;
        let PreparedHostChannel {
            mut launch,
            acceptor: _acceptor,
            locator,
        } = prepare_channel(
            ChannelPreparation {
                container_mode: true,
                plan: &plan,
                target: &target,
                session,
                supervisor: "tls-supervisor",
            },
            run,
            &mut guest,
            Duration::from_secs(2),
        )
        .unwrap()
        .unwrap();
        assert_eq!(locator.session().run, session.run());
        assert!(locator.channel().is_ready());
        launch.stop();
    });
}

#[test]
fn session_owner_crash_child() {
    let Some(directory) = std::env::var_os("EFFIGY_TLS_CRASH_ROOT") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let journal = directory.join("journal");
    std::fs::create_dir(&journal).unwrap();
    with_session_at(Some(&journal), |run, session| {
        let (listener, key, _acceptor) =
            SessionTlsListener::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(1))
                .unwrap();
        let endpoint = listener.endpoint().unwrap();
        let pin = key.pin();
        let _launch =
            SessionLaunch::launch(run, session, "tls-supervisor", key, Box::new(listener)).unwrap();
        std::fs::write(
            directory.join("bound.tmp"),
            format!("{endpoint}\n{}", pin.hex()),
        )
        .unwrap();
        std::fs::rename(directory.join("bound.tmp"), directory.join("bound")).unwrap();
        std::thread::park_timeout(Duration::from_secs(15));
    });
}
#[test]
fn owner_process_crash_releases_socket_and_cannot_reuse_old_pin() {
    use effigy_test_support::process::FixtureCommandExt;
    let fixture = effigy_test_support::Fixture::short_home("tls-crash-").unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "runner::workspace_transport::session_tests::session_owner_crash_child",
            "--nocapture",
        ])
        .env("EFFIGY_TLS_CRASH_ROOT", fixture.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn_owned().unwrap();
    let pid = child.id();
    let until = Instant::now() + Duration::from_secs(10);
    let record = loop {
        if let Ok(record) = std::fs::read_to_string(fixture.path().join("bound")) {
            break record;
        }
        assert!(Instant::now() < until, "listener child {pid} did not bind");
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut fields = record.lines();
    let endpoint = fields.next().unwrap().parse().unwrap();
    let old_pin = Digest::parse_hex(fields.next().unwrap()).unwrap();
    TlsChannelTransport::connect(endpoint, old_pin.clone(), Duration::from_secs(2))
        .unwrap()
        .close()
        .unwrap();
    child.kill().unwrap(); // exact owned PID, never a process-name sweep
    let exit = child.wait().unwrap();
    assert!(!exit.success(), "expected owned crash of PID {pid}: {exit}");
    assert!(child.group_closed());
    assert!(
        TlsChannelTransport::connect(endpoint, old_pin.clone(), Duration::from_millis(100))
            .is_err()
    );
    let (mut successor, key, _acceptor) =
        SessionTlsListener::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(1)).unwrap();
    assert_ne!(key.pin(), old_pin);
    successor.stop();
}

#[test]
fn guest_peer_port_pin_and_boot_mismatches_never_qualify() {
    use effigy_kernel::workspace_session::ServingProbe;
    struct Reply(GuestDialEvidence);
    impl GuestChannelProbe for Reply {
        fn resolve(
            &mut self,
            _: &WorkspaceContainerIdentity,
            _: &str,
            _: Duration,
        ) -> Observed<ResolvedAlias> {
            panic!("already resolved")
        }
        fn dial(
            &mut self,
            _: &WorkspaceContainerIdentity,
            _: &str,
            _: &Digest,
            _: Duration,
        ) -> Observed<GuestDialEvidence> {
            Observed::Verified(self.0.clone(), evidence())
        }
    }
    let target = target();
    let pin = Digest::of(b"qualified key");
    let endpoint: SocketAddr = "127.0.0.1:42000".parse().unwrap();
    let bind = ChannelBind {
        listen: "127.0.0.1:0".parse().unwrap(),
        alias: "effigy-host".into(),
        expected_peer: "192.0.2.11".parse().unwrap(),
    };
    let correct = GuestDialEvidence {
        target: target.clone(),
        peer: SocketAddr::new(bind.expected_peer, endpoint.port()),
        pin: pin.clone(),
    };
    let mut cases = vec![correct.clone(); 4];
    cases[0].pin = Digest::of(b"changed key");
    cases[1].peer.set_port(42001);
    cases[2].target.vm_boot_id = "another boot".into();
    cases[3].peer.set_ip("192.0.2.12".parse().unwrap());
    for reply in cases {
        let mut guest = Reply(reply);
        let mut probe = GuestServingProbe {
            guest: &mut guest,
            target: &target,
            bind: &bind,
            qualified_pin: None,
        };
        assert!(matches!(
            probe.pinned_handshake(endpoint, &pin, Duration::from_secs(1)),
            Observed::Unknown(_)
        ));
        assert!(probe.qualified_pin.is_none());
    }
}
