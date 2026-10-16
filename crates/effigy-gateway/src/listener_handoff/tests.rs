//! Private fixture proofs for the launchd listener handoff.
//!
//! Every test runs on ephemeral loopback endpoints under the same UID with
//! owned descriptors only. Nothing here registers a launchd job, touches the
//! production endpoints `80`/`443`, elevates, or reads live gateway state.
//! The production adoption refusal (outside launchd) is genuine evidence that
//! the daemon cannot silently start this way without the administrator setup.

use super::{
    close_owned_fd, parse_launchd_definition, production_specs, validate_and_adopt,
    validate_launchd_definition, ActivationFailure, DescriptorRefusal, HandedSocketSpec,
    HandoffIdentity, ListenerHandoffError, ListenerHandoffSetup, PlistValue,
    HANDOFF_HTTPS_SOCKET_NAME, HANDOFF_HTTP_SOCKET_NAME, HANDOFF_RUN_FLAG, HANDOFF_RUN_SUBCOMMAND,
    LAUNCHD_JOB_LABEL,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::path::PathBuf;

fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

/// Bind a real listening loopback socket on an ephemeral port.
fn listening_socket() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind(loopback_ephemeral()).expect("ephemeral loopback bind");
    let addr = listener.local_addr().expect("local address");
    (listener, addr)
}

fn spec(name: &'static str, endpoint: SocketAddr) -> HandedSocketSpec {
    HandedSocketSpec { name, endpoint }
}

fn operator_identity() -> HandoffIdentity {
    HandoffIdentity {
        effective_uid: 501,
        sudo_environment_present: false,
    }
}

/// Take exclusive ownership of a fixture listener's descriptor.
fn into_owned_fd(listener: TcpListener) -> RawFd {
    listener.into_raw_fd()
}

/// Retain the original socket object while its handed numeric slot can be
/// closed and reused by another test. The duplicate prevents object-handle ABA.
struct DescriptorCloseWitness {
    slot: RawFd,
    original: OwnedFd,
}

impl DescriptorCloseWitness {
    fn new(socket: &impl AsFd) -> Self {
        Self {
            slot: socket.as_fd().as_raw_fd(),
            original: socket
                .as_fd()
                .try_clone_to_owned()
                .expect("retain fixture socket"),
        }
    }
}

fn assert_descriptor_closed(witness: &DescriptorCloseWitness) {
    // SAFETY: F_DUPFD_CLOEXEC accepts any numeric slot and validates it in the
    // kernel. Success creates a new descriptor owned exclusively by this call;
    // it neither adopts nor closes the possibly unrelated original slot.
    let snapshot = unsafe { libc::fcntl(witness.slot, libc::F_DUPFD_CLOEXEC, 0) };
    if snapshot < 0 {
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF),
            "only an absent descriptor proves closure without an object snapshot"
        );
        return;
    }
    // SAFETY: successful F_DUPFD_CLOEXEC returned a new, open descriptor with
    // no other owner and no cleanup requirement beyond close. Only this copy
    // is consumed; the observed numeric slot remains untouched.
    let snapshot = unsafe { OwnedFd::from_raw_fd(snapshot) };
    let stat = nix::sys::stat::fstat(&snapshot).expect("inspect owned descriptor snapshot");
    if stat.st_mode & libc::S_IFMT == libc::S_IFSOCK {
        assert_ne!(
            super::fd_socket_identity(snapshot.as_raw_fd()).expect("snapshot socket identity"),
            super::fd_socket_identity(witness.original.as_raw_fd())
                .expect("retained original socket identity"),
            "refused handed slot still refers to the original socket"
        );
    }
}

/// Closure can reuse the same test-owned slot without making its successor
/// the handed socket. Keep both objects alive and preserve the successor.
#[test]
fn listener_handoff_close_oracle_distinguishes_atomic_owned_slot_reuse() {
    let (original, _) = listening_socket();
    let slot: OwnedFd = original.into();
    let witness = DescriptorCloseWitness::new(&slot);
    let (successor, successor_addr) = listening_socket();
    // SAFETY: both descriptors are open and exclusively owned by this test.
    // dup2 atomically replaces the still-owned slot; there is no close/reopen
    // gap in which another thread could acquire the destination descriptor.
    // `slot` remains its sole owner and now closes the duplicated successor.
    assert_eq!(
        unsafe { libc::dup2(successor.as_raw_fd(), slot.as_raw_fd()) },
        slot.as_raw_fd()
    );
    let old_oracle = std::panic::catch_unwind(|| {
        // SAFETY: the destination is still an open fixture-owned descriptor.
        assert_eq!(unsafe { libc::fcntl(slot.as_raw_fd(), libc::F_GETFD) }, -1);
    });
    assert!(
        old_oracle.is_err(),
        "old numeric-only oracle must reject reuse"
    );
    assert_descriptor_closed(&witness);
    let stream = std::net::TcpStream::connect(successor_addr).expect("connect successor");
    let (accepted, _) = successor.accept().expect("successor remains usable");
    drop(accepted);
    drop(stream);
}

/// A live original descriptor is a real leak and must still fail the oracle.
#[test]
fn listener_handoff_close_oracle_refuses_an_unclosed_original() {
    let (original, _) = listening_socket();
    let witness = DescriptorCloseWitness::new(&original);
    assert!(std::panic::catch_unwind(|| assert_descriptor_closed(&witness)).is_err());
    assert_eq!(
        original.local_addr().expect("original remains owned").ip(),
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    );
}

/// The production boundary adopts real listening loopback sockets, keeps
/// them accepting, and marks them close-on-exec.
#[test]
fn listener_handoff_adopts_real_listening_loopback_sockets() {
    let (http_listener, http_addr) = listening_socket();
    let (https_listener, https_addr) = listening_socket();
    let specs = [
        spec(HANDOFF_HTTP_SOCKET_NAME, http_addr),
        spec(HANDOFF_HTTPS_SOCKET_NAME, https_addr),
    ];
    let activated = [
        (HANDOFF_HTTP_SOCKET_NAME, vec![into_owned_fd(http_listener)]),
        (
            HANDOFF_HTTPS_SOCKET_NAME,
            vec![into_owned_fd(https_listener)],
        ),
    ];
    let listeners = validate_and_adopt(&specs, &activated, &operator_identity()).expect("adoption");
    assert_eq!(listeners.http.local_addr().expect("addr"), http_addr);
    assert_eq!(listeners.https.local_addr().expect("addr"), https_addr);

    // The adopted listener is the same live socket: a real connection round trip.
    let stream = std::net::TcpStream::connect(http_addr).expect("connect");
    let (accepted, _) = listeners.http.accept().expect("accept");
    drop(accepted);
    drop(stream);

    for listener in [&listeners.http, &listeners.https] {
        let flags = unsafe { libc::fcntl(listener.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0, "adopted descriptor stays open");
        assert!(
            flags & libc::FD_CLOEXEC != 0,
            "adopted descriptor must be close-on-exec"
        );
    }
}

/// Adopted listeners convert to tokio listeners without losing acceptance.
#[test]
fn listener_handoff_adopted_listeners_convert_to_tokio() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move {
        let (http_listener, http_addr) = listening_socket();
        let (https_listener, https_addr) = listening_socket();
        let specs = [
            spec(HANDOFF_HTTP_SOCKET_NAME, http_addr),
            spec(HANDOFF_HTTPS_SOCKET_NAME, https_addr),
        ];
        let activated = [
            (HANDOFF_HTTP_SOCKET_NAME, vec![into_owned_fd(http_listener)]),
            (
                HANDOFF_HTTPS_SOCKET_NAME,
                vec![into_owned_fd(https_listener)],
            ),
        ];
        let listeners = validate_and_adopt(&specs, &activated, &operator_identity())
            .expect("adoption")
            .into_tokio()
            .expect("tokio conversion");
        let connect = tokio::net::TcpStream::connect(http_addr)
            .await
            .expect("connect");
        let (_accepted, _) = listeners.0.accept().await.expect("accept");
        drop(connect);
    });
}

/// A descriptor presenting the wrong endpoint refuses and closes.
#[test]
fn listener_handoff_refuses_wrong_endpoint_and_closes_handed_descriptors() {
    let (actual_listener, actual_addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&actual_listener);
    let fd = into_owned_fd(actual_listener);
    let specs = [spec(
        HANDOFF_HTTP_SOCKET_NAME,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), actual_addr.port() + 1),
    )];
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &operator_identity(),
    )
    .expect_err("wrong endpoint must refuse");
    let ListenerHandoffError::DescriptorRefused { socket, refusal } = &error else {
        panic!("expected a descriptor refusal, got {error:?}");
    };
    assert_eq!(*socket, HANDOFF_HTTP_SOCKET_NAME);
    match refusal {
        DescriptorRefusal::WrongEndpoint { expected, actual } => {
            assert_eq!(*actual, actual_addr);
            assert_eq!(actual.port(), expected.port() - 1);
        }
        other => panic!("expected WrongEndpoint, got {other:?}"),
    }
    assert_descriptor_closed(&fd_witness);
}

/// An IPv6 socket refuses against the IPv4 loopback contract.
#[test]
fn listener_handoff_refuses_wrong_family() {
    let listener = TcpListener::bind(SocketAddr::new(
        IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        0,
    ))
    .expect("ipv6 loopback bind");
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let specs = [spec(HANDOFF_HTTP_SOCKET_NAME, loopback_ephemeral())];
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &operator_identity(),
    )
    .expect_err("wrong family must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::DescriptorRefused {
                refusal: DescriptorRefusal::WrongFamily,
                ..
            }
        ),
        "expected WrongFamily, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
}

/// A connected stream socket (not listening) refuses.
#[test]
fn listener_handoff_refuses_non_listening_stream_socket() {
    let (listener, addr) = listening_socket();
    let stream = std::net::TcpStream::connect(addr).expect("connect");
    let accepted = listener.accept().expect("accept").0;
    let fd_witness = DescriptorCloseWitness::new(&accepted);
    let fd = accepted.into_raw_fd();
    drop(stream);
    let specs = [spec(HANDOFF_HTTP_SOCKET_NAME, addr)];
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &operator_identity(),
    )
    .expect_err("non-listening socket must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::DescriptorRefused {
                refusal: DescriptorRefusal::NotListening,
                ..
            }
        ),
        "expected NotListening, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
    close_owned_fd(listener.into_raw_fd());
}

/// A datagram socket refuses the stream-only contract.
#[test]
fn listener_handoff_refuses_datagram_socket() {
    let socket = UdpSocket::bind(loopback_ephemeral()).expect("udp bind");
    let fd_witness = DescriptorCloseWitness::new(&socket);
    let fd = socket.into_raw_fd();
    let specs = [spec(HANDOFF_HTTP_SOCKET_NAME, loopback_ephemeral())];
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &operator_identity(),
    )
    .expect_err("datagram socket must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::DescriptorRefused {
                refusal: DescriptorRefusal::WrongSocketType,
                ..
            }
        ),
        "expected WrongSocketType, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
}

/// The same socket object handed under both names refuses and closes once.
#[test]
fn listener_handoff_refuses_duplicate_socket_object() {
    let (listener, addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let duplicate_owned = fd_witness.original.try_clone().expect("duplicate socket");
    let duplicate_witness = DescriptorCloseWitness::new(&duplicate_owned);
    let duplicate = duplicate_owned.into_raw_fd();
    let specs = [
        spec(HANDOFF_HTTP_SOCKET_NAME, addr),
        spec(HANDOFF_HTTPS_SOCKET_NAME, addr),
    ];
    let error = validate_and_adopt(
        &specs,
        &[
            (HANDOFF_HTTP_SOCKET_NAME, vec![fd]),
            (HANDOFF_HTTPS_SOCKET_NAME, vec![duplicate]),
        ],
        &operator_identity(),
    )
    .expect_err("duplicate socket object must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::DescriptorRefused {
                refusal: DescriptorRefusal::DuplicateSocketObject,
                ..
            }
        ),
        "expected DuplicateSocketObject, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
    assert_descriptor_closed(&duplicate_witness);
}

/// A missing or duplicated descriptor count for one socket name refuses.
#[test]
fn listener_handoff_refuses_missing_or_extra_socket_descriptors() {
    let (listener, addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let extra = fd_witness.original.try_clone().expect("extra socket");
    let specs = [
        spec(HANDOFF_HTTP_SOCKET_NAME, addr),
        spec(HANDOFF_HTTPS_SOCKET_NAME, addr),
    ];
    // Missing https entry: zero descriptors for the name.
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &operator_identity(),
    )
    .expect_err("missing socket must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::UnexpectedDescriptorCount { count: 0, .. }
        ),
        "expected count 0 refusal, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
    drop(extra);

    // Two descriptors for one name refuse before any validation.
    let (listener, addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let extra_owned = fd_witness.original.try_clone().expect("extra socket");
    let extra_witness = DescriptorCloseWitness::new(&extra_owned);
    let extra = extra_owned.into_raw_fd();
    let specs = [spec(HANDOFF_HTTP_SOCKET_NAME, addr)];
    let error = validate_and_adopt(
        &specs,
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd, extra])],
        &operator_identity(),
    )
    .expect_err("two descriptors for one name must refuse");
    assert!(
        matches!(
            error,
            ListenerHandoffError::UnexpectedDescriptorCount { count: 2, .. }
        ),
        "expected count 2 refusal, got {error:?}"
    );
    assert_descriptor_closed(&fd_witness);
    assert_descriptor_closed(&extra_witness);
}

/// Root and sudo-environment identities refuse before any descriptor work.
#[test]
fn listener_handoff_refuses_root_and_sudo_identities() {
    let (listener, addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let specs = [spec(HANDOFF_HTTP_SOCKET_NAME, addr)];
    let activated = [(HANDOFF_HTTP_SOCKET_NAME, vec![fd])];
    let root = HandoffIdentity {
        effective_uid: 0,
        sudo_environment_present: false,
    };
    let error =
        validate_and_adopt(&specs, &activated, &root).expect_err("root identity must refuse");
    assert!(matches!(error, ListenerHandoffError::RootIdentity));
    // The refusal is fail-closed: the handed descriptor is still closed.
    assert_descriptor_closed(&fd_witness);

    let (listener, addr) = listening_socket();
    let fd_witness = DescriptorCloseWitness::new(&listener);
    let fd = into_owned_fd(listener);
    let sudo = HandoffIdentity {
        effective_uid: 501,
        sudo_environment_present: true,
    };
    let error = validate_and_adopt(
        &[spec(HANDOFF_HTTP_SOCKET_NAME, addr)],
        &[(HANDOFF_HTTP_SOCKET_NAME, vec![fd])],
        &sudo,
    )
    .expect_err("sudo environment must refuse");
    assert!(matches!(error, ListenerHandoffError::ElevatedInvocation));
    assert_descriptor_closed(&fd_witness);
}

/// Production adoption refuses outside launchd without binding anything.
#[test]
fn listener_handoff_production_adoption_refuses_outside_launchd() {
    let error = super::adopt_handed_listeners().expect_err("no launchd activation in tests");
    #[cfg(target_os = "macos")]
    match &error {
        // Both refusals are correct here: the test harness is not a launchd
        // job (ESRCH), and a sudo-wrapped harness is an elevated invocation.
        ListenerHandoffError::ActivationFailed {
            failure: ActivationFailure::NotLaunchedByLaunchd,
            ..
        }
        | ListenerHandoffError::ElevatedInvocation => {}
        other => panic!("expected NotLaunchedByLaunchd refusal, got {other:?}"),
    }
    #[cfg(not(target_os = "macos"))]
    assert!(
        matches!(error, ListenerHandoffError::UnsupportedPlatform),
        "expected UnsupportedPlatform, got {error:?}"
    );
    assert_eq!(production_specs().len(), 2);
    let endpoints: Vec<u16> = production_specs()
        .iter()
        .map(|spec| spec.endpoint.port())
        .collect();
    assert_eq!(endpoints, vec![80, 443]);
}

fn definition_fixture() -> ListenerHandoffSetup {
    ListenerHandoffSetup {
        label: LAUNCHD_JOB_LABEL,
        program_path: PathBuf::from("/usr/local/bin/effigy"),
        user_name: "tom".to_owned(),
        group_name: "staff".to_owned(),
        home: PathBuf::from("/Users/tom"),
    }
}

fn definition_with_replacement(haystack: &str, needle: &str, replacement: &str) -> String {
    let position = haystack
        .find(needle)
        .unwrap_or_else(|| panic!("fixture must contain {needle:?}"));
    let mut replaced = String::with_capacity(haystack.len());
    replaced.push_str(&haystack[..position]);
    replaced.push_str(replacement);
    replaced.push_str(&haystack[position + needle.len()..]);
    replaced
}

/// The generated definition round-trips through the strict parser and
/// validates as the known-good loopback shape.
#[test]
fn listener_handoff_definition_roundtrips_and_validates() {
    let setup = ListenerHandoffSetup {
        program_path: PathBuf::from("/opt/weird&path/effigy"),
        ..definition_fixture()
    };
    let plist = setup.render_launchd_plist();
    let parsed = parse_launchd_definition(&plist).expect("generated bytes parse");
    validate_launchd_definition(&parsed).expect("generated bytes validate");
    // The escaped ampersand round-trips exactly.
    let arguments = match parsed.get("ProgramArguments") {
        Some(PlistValue::Array(arguments)) => arguments.clone(),
        other => panic!("ProgramArguments must be an array, got {other:?}"),
    };
    assert_eq!(
        arguments[0],
        PlistValue::String("/opt/weird&path/effigy".to_owned())
    );
    assert_eq!(
        arguments[1],
        PlistValue::String(HANDOFF_RUN_SUBCOMMAND.to_owned())
    );
    assert_eq!(
        arguments[2],
        PlistValue::String(HANDOFF_RUN_FLAG.to_owned())
    );
    let sockets = match parsed.get("Sockets") {
        Some(PlistValue::Dict(sockets)) => sockets.clone(),
        other => panic!("Sockets must be a dict, got {other:?}"),
    };
    assert_eq!(sockets.len(), 2);
}

/// Mutated definitions parse but refuse: unknown config never validates.
#[test]
fn listener_handoff_definition_refuses_unknown_shapes() {
    let base = definition_fixture().render_launchd_plist();

    let wildcard = definition_with_replacement(
        &base,
        "<string>127.0.0.1</string>",
        "<string>0.0.0.0</string>",
    );
    validate_launchd_definition(&parse_launchd_definition(&wildcard).expect("parses"))
        .expect_err("wildcard node must refuse");

    let moved_port =
        definition_with_replacement(&base, "<integer>80</integer>", "<integer>8080</integer>");
    validate_launchd_definition(&parse_launchd_definition(&moved_port).expect("parses"))
        .expect_err("moved http port must refuse");

    let renamed = definition_with_replacement(
        &base,
        &format!("<string>{LAUNCHD_JOB_LABEL}</string>"),
        "<string>other.label</string>",
    );
    validate_launchd_definition(&parse_launchd_definition(&renamed).expect("parses"))
        .expect_err("renamed label must refuse");

    let no_keepalive = definition_with_replacement(
        &base,
        "<key>KeepAlive</key>\n  <true/>",
        "<key>KeepAlive</key>\n  <false/>",
    );
    validate_launchd_definition(&parse_launchd_definition(&no_keepalive).expect("parses"))
        .expect_err("disabled KeepAlive must refuse");

    let swapped_flag = definition_with_replacement(&base, HANDOFF_RUN_FLAG, "--something-else");
    validate_launchd_definition(&parse_launchd_definition(&swapped_flag).expect("parses"))
        .expect_err("swapped run flag must refuse");

    let extra_socket_key = definition_with_replacement(
        &base,
        "<key>SockType</key>\n      <string>TCP</string>\n    </dict>",
        "<key>SockType</key>\n      <string>TCP</string>\n      <key>SockPassive</key>\n      <true/>\n    </dict>",
    );
    validate_launchd_definition(&parse_launchd_definition(&extra_socket_key).expect("parses"))
        .expect_err("extra socket key must refuse");
}

/// The strict parser refuses structures outside the generator's subset.
#[test]
fn listener_handoff_parser_refuses_unsupported_structures() {
    assert_eq!(
        parse_launchd_definition("<plist version=\"1.0\"></plist>").expect_err("missing prolog"),
        super::DefinitionParseError::MissingProlog,
    );
    let plist = definition_fixture().render_launchd_plist();
    let commented = definition_with_replacement(&plist, "<dict>", "<!-- note --><dict>");
    assert!(
        parse_launchd_definition(&commented).is_err(),
        "comments refuse"
    );
    let trailing = format!("{plist}<extra/>");
    assert!(
        parse_launchd_definition(&trailing).is_err(),
        "trailing content refuses"
    );
    let entity = definition_with_replacement(&plist, "/usr/local/bin/effigy", "/a&#12;b");
    assert!(
        parse_launchd_definition(&entity).is_err(),
        "numeric entities refuse"
    );
    let broken = plist.replace("</dict>\n</plist>", "");
    assert!(
        parse_launchd_definition(&broken).is_err(),
        "incomplete document refuses"
    );
}

/// Setup validation refuses a non-canonical or missing executable.
#[test]
fn listener_handoff_setup_validation_refuses_bad_program_path() {
    let mut setup = definition_fixture();
    setup.program_path = PathBuf::from("relative/effigy");
    assert!(setup.validate().is_err(), "relative program path refuses");
    setup.program_path = std::env::temp_dir().join(format!(
        "effigy-listener-handoff-missing-{}",
        std::process::id()
    ));
    assert!(setup.validate().is_err(), "missing program path refuses");
}

/// The daemon refuses handoff startup outside launchd and publishes no record.
#[test]
fn listener_handoff_daemon_startup_refuses_without_launchd_and_writes_no_record() {
    let fixture = tempfile::tempdir().expect("fresh listener-handoff fixture");
    let config = crate::server::GatewayConfig::standard(fixture.path().to_path_buf())
        .with_listener_handoff(true);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let error = runtime
        .block_on(crate::server::run_gateway(config))
        .expect_err("handoff adoption must refuse outside launchd");
    let crate::error::GatewayError::ListenerHandoff(_) = &error else {
        panic!("expected a listener handoff refusal, got {error:?}");
    };
    assert!(
        !fixture.path().join("gateway.pid").exists(),
        "a refused handoff start must not publish a generation record"
    );
}
