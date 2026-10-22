use super::*;
use crate::ctx::*;
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::time::{Duration, Instant};

fn cx(budget: Duration) -> ObserveContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            line_max: 4096,
            log_rotate: 0,
            log_keep: 0,
            queue_bytes: 4096,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .unwrap()
}
fn http_target(address: std::net::SocketAddr, expect: StatusSet) -> ObservationTarget {
    ObservationTarget::Http {
        address,
        host: "localhost".into(),
        path: "/health".into(),
        method: HttpReadMethod::Get,
        expect,
        connect_timeout: Duration::from_millis(200),
        total_timeout: Duration::from_millis(250),
    }
}
#[test]
fn probe_http_status_policy_is_explicit_and_bounded() {
    for (status, expected, ready) in [
        (200, StatusSet::default(), true),
        (204, StatusSet::default(), true),
        (299, StatusSet::default(), true),
        (401, StatusSet::default(), false),
        (404, StatusSet::default(), false),
        (500, StatusSet::default(), false),
        (401, StatusSet::new(vec![401..=401]).unwrap(), true),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            assert!(request[..n].starts_with(b"GET /health HTTP/1.1\r\n"));
            stream
                .write_all(
                    format!("HTTP/1.1 {status} fixture\r\nContent-Length: 0\r\n\r\n").as_bytes(),
                )
                .unwrap();
        });
        assert_eq!(
            cx(Duration::from_secs(2))
                .connect(http_target(address, expected))
                .verified(),
            Some(&ProbeIo::Http { status, ready })
        );
        peer.join().unwrap();
    }
    assert!(StatusSet::new(vec![99..=299]).is_err());
    assert!(StatusSet::new(vec![]).is_err());
}
#[test]
fn probe_hung_http_and_oversized_or_bad_headers_are_unknown() {
    for response in [
        None,
        Some(b"HTTP/1.1 200\r\nBad: ".as_slice()),
        Some(b"NOT HTTP\r\n\r\n"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            if let Some(bytes) = response {
                stream.write_all(bytes).unwrap();
            }
            std::thread::sleep(Duration::from_millis(400));
        });
        let start = Instant::now();
        assert!(matches!(
            cx(Duration::from_millis(100)).connect(http_target(address, StatusSet::default())),
            Observed::Unknown(_)
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        peer.join().unwrap();
    }
}
#[test]
fn probe_tcp_unix_dns_and_untrusted_targets() {
    let cx = cx(Duration::from_secs(2));
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    assert_eq!(
        cx.connect(ObservationTarget::Tcp(tcp.local_addr().unwrap()))
            .verified(),
        Some(&ProbeIo::TcpOpen)
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert_eq!(
        cx.connect(ObservationTarget::UnixSocket(path)).verified(),
        Some(&ProbeIo::UnixOpen)
    );
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 512];
        let (n, client) = server.recv_from(&mut bytes).unwrap();
        bytes[2] = 0x81;
        bytes[3] = 0x80;
        server.send_to(&bytes[..n], client).unwrap();
    });
    assert!(matches!(
        cx.connect(ObservationTarget::Dns {
            name: "gateway.test".into(),
            server: address
        }),
        Observed::Verified(ProbeIo::Dns { .. }, _)
    ));
    peer.join().unwrap();
    assert!(matches!(
        cx.connect(ObservationTarget::Tcp("203.0.113.1:80".parse().unwrap())),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
}
#[test]
fn probe_waiter_rechecks_all_predicates_and_preserves_unknown_order() {
    struct Strict {
        calls: std::cell::Cell<usize>,
        // Verified from this observation round on; the deadline is only a
        // safety net, so the proof never depends on how fast a round runs.
        ready_after: usize,
    }
    impl Probe for Strict {
        fn observe(&self, cx: &ObserveContext) -> Observed<Readiness> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() >= self.ready_after && cx.check("fake").is_ok() {
                Observed::Verified(Readiness::Created, evidence())
            } else {
                Observed::Unknown(UnknownReason::Injected)
            }
        }
    }
    // The second predicate holds two rounds after the first: the waiter must
    // recheck the successful one too rather than accumulate a stale success.
    let first = Strict {
        calls: 0.into(),
        ready_after: 2,
    };
    let second = Strict {
        calls: 0.into(),
        ready_after: 4,
    };
    let results = wait_all(&[&first, &second], &cx(Duration::from_secs(10)));
    assert_eq!(results.len(), 2);
    assert!(matches!(results[0], Observed::Verified(..)));
    assert!(matches!(results[1], Observed::Verified(..)));
    assert_eq!(first.calls.get(), 4);
    assert_eq!(second.calls.get(), 4);
}
#[test]
fn probe_waiter_preserves_unknown_order_after_the_deadline() {
    struct Strict(Observed<Readiness>);
    impl Probe for Strict {
        fn observe(&self, _cx: &ObserveContext) -> Observed<Readiness> {
            self.0.clone()
        }
    }
    // An elapsed budget stops the waiter after its first round, so the
    // permanent unknown cannot turn this into a waiting proof.
    let ready = Strict(Observed::Verified(Readiness::Created, evidence()));
    let unknown = Strict(Observed::Unknown(UnknownReason::Injected));
    let results = wait_all(&[&ready, &unknown], &cx(Duration::ZERO));
    assert_eq!(results.len(), 2);
    assert!(matches!(results[0], Observed::Verified(..)));
    assert!(matches!(
        results[1],
        Observed::Unknown(UnknownReason::Injected)
    ));
}
#[cfg(feature = "fault-injection")]
#[test]
fn probe_unknown_injection_and_fault_points_make_no_transport_or_process_call() {
    crate::fault::set_unknown(true);
    let cx = cx(Duration::from_secs(1));
    assert!(matches!(
        cx.connect(ObservationTarget::Tcp("127.0.0.1:1".parse().unwrap())),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        cx.probe_process(ProbeCommand {
            argv: vec![],
            cwd: "/".into(),
            env: Default::default(),
            deadline: Duration::from_secs(1),
            max_output: 4096
        }),
        Observed::Unknown(UnknownReason::Injected)
    ));
    crate::fault::set_unknown(false);
    for point in [
        crate::fault::Point::ProbeObservation,
        crate::fault::Point::ProbeConnect,
    ] {
        for fault in [crate::fault::Fault::Eperm, crate::fault::Fault::Enospc] {
            crate::fault::arm_point(point, fault);
            assert!(matches!(
                cx.connect(ObservationTarget::Tcp("127.0.0.1:1".parse().unwrap())),
                Observed::Unknown(_)
            ));
            assert!(crate::fault::trace().contains(&point));
        }
    }
    crate::fault::arm(None);
}

#[cfg(feature = "fault-injection")]
#[test]
fn probe_runtime_fault_boundaries_preserve_no_execution_before_ack() {
    let executable = crate::supervise::Executable::capture(&std::path::PathBuf::from(
        std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").unwrap(),
    ))
    .unwrap();
    let runtime = ProbeRuntime::install(executable).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let command = || ProbeCommand {
        argv: vec!["/usr/bin/true".into()],
        cwd: dir.path().into(),
        env: Default::default(),
        // This matrix proves barrier/fault ordering, including cold guard
        // startup. Tight deadline behavior has separate transport and native
        // custody proofs; leave this scenario enough startup headroom.
        deadline: Duration::from_secs(10),
        max_output: 4096,
    };
    let cx = cx(Duration::from_secs(30)).with_probes(runtime);
    crate::fault::arm(None);
    let observed = cx.probe_process(command());
    assert!(matches!(observed, Observed::Verified(..)), "{observed:?}");
    let trace = crate::fault::trace();
    for point in [
        crate::fault::Point::ProbeObservation,
        crate::fault::Point::ProbeFork,
        crate::fault::Point::ProbeRegister,
        crate::fault::Point::ProbeBeforeGo,
        crate::fault::Point::ProbeAfterGo,
    ] {
        assert!(trace.contains(&point), "missing boundary {point:?}");
        for fault in [crate::fault::Fault::Enospc, crate::fault::Fault::Eperm] {
            crate::fault::arm_point(point, fault);
            assert!(matches!(cx.probe_process(command()), Observed::Unknown(_)));
            assert!(crate::fault::trace().contains(&point));
        }
    }
    crate::fault::arm(None);
    let observed = cx.probe_process(command());
    assert!(matches!(observed, Observed::Verified(..)), "{observed:?}");
}

#[test]
fn probe_http_head_has_no_body_and_headers_obey_capture_limit() {
    for oversized in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut bytes = [0; 1024];
                let n = stream.read(&mut bytes).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
            }
            assert!(request.starts_with(b"HEAD /health HTTP/1.1\r\n"));
            let response = if oversized {
                format!("HTTP/1.1 200 OK\r\nX-Large: {}\r\n\r\n", "x".repeat(8192))
            } else {
                "HTTP/1.1 204 OK\r\nContent-Length: 0\r\n\r\n".into()
            };
            // An oversized peer can be disconnected as soon as the bound is
            // crossed; completing its write is outside the probe's obligations.
            let _ = stream.write_all(response.as_bytes());
        });
        let mut target = http_target(address, StatusSet::default());
        if let ObservationTarget::Http { method, .. } = &mut target {
            *method = HttpReadMethod::Head;
        }
        let observed = cx(Duration::from_secs(2)).connect(target);
        if oversized {
            assert!(matches!(
                observed,
                Observed::Unknown(UnknownReason::Unparsable(_))
            ));
        } else {
            assert_eq!(
                observed.verified(),
                Some(&ProbeIo::Http {
                    status: 204,
                    ready: true
                })
            );
        }
        peer.join().unwrap();
    }
}
