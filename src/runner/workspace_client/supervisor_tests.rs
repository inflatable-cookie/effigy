//! #507 proofs: durable serving supervision of one committed session.
//!
//! Every proof here uses the same fixtures #452's serving proofs use: a real
//! private loopback listener and key issued by the transport, a real session run
//! adopted by its supervisor, and disposable journal directories under a
//! temporary path. Nothing starts a container, a live gateway or a second
//! process. What replaces a process boundary is a call boundary: the closure
//! that selects the session and hands its custody over returns, and every
//! assertion after it is about the value that outlived that call.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{EffectContext, ObserveContext};
use effigy_kernel::id::RunId;
use effigy_kernel::id::{Digest, Generation};
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::workspace_channel::{
    self as host, Frame, Hello, Transport, PROTOCOL_V1, SCHEMA,
};
use effigy_kernel::workspace_session::{ServingLease, SessionRef, SessionSupervisor};

use super::super::tests::{
    cleanup, prepare_owned_channel, with_serving_session, NoEffects, GUEST_CHECKOUT,
};
use super::{discover, HandoffRefusal, ServiceOwner, ServingHandoff};
use crate::runner::workspace_client::serving::OwnedWorkspaceChannel;
use crate::runner::workspace_client::ServingFacts;
use crate::runner::workspace_transport::qualification::PreparedHostChannel;
use crate::runner::workspace_transport::TlsChannelTransport;

/// The custody #452's container-up caller hands over: the adopted supervisor,
/// the prepared listener and key, and the validated facts, moved in whole.
fn custody<'a>(
    supervisor: SessionSupervisor<'a>,
    session: &SessionRef,
    prepared: PreparedHostChannel,
    cx: &'a EffectContext,
    observe: &'a ObserveContext,
    root: &'a StateRoot,
) -> OwnedWorkspaceChannel<'a> {
    let serving = ServingFacts::fixture(&prepared.locator, Path::new(GUEST_CHECKOUT).to_path_buf());
    OwnedWorkspaceChannel {
        supervisor,
        session: session.clone(),
        serving,
        channel: prepared,
        effect: Arc::new(cx.clone()),
        observe: Arc::new(observe.clone()),
        root,
    }
}

/// The lease a prepared channel published.
fn published(prepared: &PreparedHostChannel) -> ServingLease {
    prepared.locator.bound().lease().clone()
}

/// The hello a guest sends on the fixture facts: they pin this host artifact,
/// the session run and the first context generation.
fn guest_hello(session: RunId) -> Frame {
    Frame::Hello(Hello {
        schema: SCHEMA.to_owned(),
        protocol_versions: vec![PROTOCOL_V1],
        effigy_version: "fixture".to_owned(),
        binary_digest: Digest([0; 32]),
        context_generation: Generation::new(1),
        session_run_id: session,
    })
}

/// Write one framed host frame.
fn write_frame(transport: &mut TlsChannelTransport, frame: &Frame) -> io::Result<()> {
    let body =
        host::encode_frame(frame).map_err(|refusal| io::Error::other(refusal.to_string()))?;
    let mut framed = Vec::with_capacity(body.len() + 4);
    framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
    framed.extend_from_slice(&body);
    transport.write_all(&framed)
}

/// Read one framed host answer, bounded so a wedged host cannot hold the proof.
fn read_frame(transport: &mut TlsChannelTransport, until: Instant) -> io::Result<Frame> {
    let mut buffer = Vec::new();
    loop {
        if buffer.len() >= 4 {
            let size = u32::from_be_bytes(buffer[..4].try_into().expect("four bytes")) as usize;
            if buffer.len() >= size + 4 {
                return host::decode_frame(&buffer[4..size + 4])
                    .map_err(|refusal| io::Error::other(format!("{refusal}")));
            }
        }
        if Instant::now() > until {
            return Err(io::Error::other("the host did not answer the guest"));
        }
        let mut chunk = [0u8; 4096];
        match transport.read(&mut chunk) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the host closed before answering the guest",
                ))
            }
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// What one guest saw: the host's negotiation answer, and the refusal it wrote
/// after the guest presented the session's enrolment credential (`None` means
/// the credential was accepted and the host is waiting for a request).
fn guest_enrols(
    endpoint: SocketAddr,
    pin: &Digest,
    session: RunId,
    generation: Generation,
) -> (Option<Frame>, Option<Frame>) {
    let mut transport =
        match TlsChannelTransport::connect(endpoint, pin.clone(), Duration::from_secs(5)) {
            Ok(transport) => transport,
            Err(_) => return (None, None),
        };
    let deadline = Instant::now() + Duration::from_secs(10);
    if write_frame(&mut transport, &guest_hello(session)).is_err() {
        return (None, None);
    }
    let hello = read_frame(&mut transport, deadline).ok();
    if !matches!(hello, Some(Frame::HelloOk(_))) {
        return (hello, None);
    }
    // The enrolment credential the retained facts installed, with the session,
    // generations and target they were bound to.
    let enrol = Frame::Enrol(host::EnrolRequest {
        credential: host::Secret::new([0x42; 32]),
        capability: [0x24; 32],
        session_run_id: session,
        session_generation: generation,
        environment_generation: Generation::new(1),
        container_id: "fixture".to_owned(),
        vm_boot_id: "fixture".to_owned(),
    });
    if write_frame(&mut transport, &enrol).is_err() {
        return (hello, None);
    }
    // An accepted enrolment is answered with nothing; a refused one answers once
    // with its refusal before closing.
    let refusal = read_frame(&mut transport, Instant::now() + Duration::from_millis(400)).ok();
    (hello, refusal)
}

/// Select the session, hand its custody over and return: the shape of the
/// container-up call whose return must not end the service.
fn select_and_hand_off<'a>(
    supervisor: SessionSupervisor<'a>,
    session: &SessionRef,
    prepared: PreparedHostChannel,
    cx: &'a EffectContext,
    observe: &'a ObserveContext,
    root: &'a StateRoot,
) -> (ServiceOwner<'a>, ServingLease) {
    let lease = published(&prepared);
    let handoff = ServingHandoff::validated(
        custody(supervisor, session, prepared, cx, observe, root),
        &lease,
    )
    .expect("the committed lease is the custody's own published lease");
    (
        ServiceOwner::adopt(handoff).expect("the committed session is adopted"),
        lease,
    )
}

/// A guest is negotiated and granted by the retained facts after the call that
/// selected the session returned, and settlement closes the listener exactly
/// once.
#[test]
fn a_committed_session_keeps_serving_after_the_selecting_call_returns() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let lease = published(&prepared);
        let endpoint = prepared
            .launch
            .endpoint()
            .expect("a bound session listener");

        let (mut service, committed) =
            select_and_hand_off(supervisor, &session, prepared, cx, observe, root);

        // The selecting call is gone. The session's custody is not: the same
        // session, the same committed lease, the same listener and the key.
        assert_eq!(service.session().run(), session.run());
        assert_eq!(service.lease(), &committed);
        assert_eq!(service.lease(), &lease);
        assert!(service.key_held(), "the key survived the selecting call");
        assert!(
            TcpStream::connect(endpoint).is_ok(),
            "the published listener survived the selecting call"
        );

        // The session run serves the channel only once its owner says so.
        assert!(
            !service.serving(),
            "nothing is served before the first turn"
        );

        let pin = lease.pin().clone();
        let run = session.run();
        let generation = session.generation();
        let (sender, receiver) = std::sync::mpsc::channel();
        let guest = std::thread::spawn(move || {
            let _ = sender.send(guest_enrols(endpoint, &pin, run, generation));
        });
        // The guest's TCP connect is already in flight; accept until its
        // connection is the one dispatched. Only the accept waits here, not the
        // listener's custody.
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            match service.serve_pending(&mut host::NoStreams) {
                Ok(true) => break,
                Ok(false) if Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(false) => panic!("the guest's connection was never accepted"),
                Err(error) => {
                    // The guest closes once its answer is in hand, so the
                    // connection's own turn can end with a read failure. That
                    // ends one connection, never the session's service.
                    assert!(
                        !error.is_terminal(),
                        "an ended connection is not a terminal end: {error}"
                    );
                    break;
                }
            }
        }
        let (answer, refusal) = receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("the guest answered");
        guest.join().expect("the guest thread ended");
        assert!(
            matches!(answer, Some(Frame::HelloOk(_))),
            "the retained facts negotiated with the guest: {answer:?}"
        );
        assert!(
            refusal.is_none(),
            "the retained grant book accepted the session's enrolment credential: {refusal:?}"
        );
        assert!(service.serving(), "the session run serves its channel");
        assert_eq!(service.served(), 1);

        let cleanup = cleanup(cx);
        let retired = service
            .retire(&cleanup, &mut NoEffects)
            .expect("the session settles once");
        assert_eq!(retired.session.run(), session.run());
        assert_eq!(retired.served, 1);
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "no listener outlives the settlement"
        );
    });
}

/// Only the committed lease hands off. A custody that never published it, holds
/// no key, or belongs to another session refuses, and nothing of it survives.
#[test]
fn only_the_committed_lease_hands_off() {
    // A custody labelled with another session's identity: the commit it hands
    // off names a session this custody is not.
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let lease = published(&prepared);
        let endpoint = prepared.launch.endpoint().expect("bound");
        let other = SessionRef::new(
            RunId::new(1, [201; 10]).expect("run id"),
            session.fence().clone(),
            session.environment().clone(),
            session.environment_generation(),
        );
        assert!(matches!(
            ServingHandoff::validated(
                custody(supervisor, &other, prepared, cx, observe, root),
                &lease,
            ),
            Err(HandoffRefusal::WrongSession)
        ));
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "a mislabelled custody keeps nothing listening"
        );
    });

    // A lease another session published: this custody published its own, so the
    // commit it is handed is not one it can serve.
    let foreign_lease = with_serving_session(|mut supervisor, session, _cx, _root, _observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        published(&prepared)
    });
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = prepared.launch.endpoint().expect("bound");
        assert!(matches!(
            ServingHandoff::validated(
                custody(supervisor, &session, prepared, cx, observe, root),
                &foreign_lease,
            ),
            Err(HandoffRefusal::Unpublished)
        ));
        assert!(TcpStream::connect(endpoint).is_err());
    });

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        // Two prepares on one session publish two leases. The custody holds the
        // first; a commit that recorded the second is not this custody's.
        let first = prepare_owned_channel(&mut supervisor, &session);
        let second = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = first.launch.endpoint().expect("bound");
        let other = published(&second);
        assert!(matches!(
            ServingHandoff::validated(
                custody(supervisor, &session, first, cx, observe, root),
                &other,
            ),
            Err(HandoffRefusal::Unpublished)
        ));
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "an uncommitted custody keeps nothing listening"
        );
    });

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let mut prepared = prepare_owned_channel(&mut supervisor, &session);
        let lease = published(&prepared);
        // A launch whose key is gone cannot serve under it.
        prepared.launch.stop();
        assert!(matches!(
            ServingHandoff::validated(
                custody(supervisor, &session, prepared, cx, observe, root),
                &lease,
            ),
            Err(HandoffRefusal::KeyReleased)
        ));
    });
}

/// An interrupted handoff closes its prepared candidate and reports exactly
/// that. It is not custody: nothing outlives it.
#[test]
fn an_interrupted_handoff_serves_nothing() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let lease = published(&prepared);
        let endpoint = prepared.launch.endpoint().expect("bound");
        let handoff = ServingHandoff::validated(
            custody(supervisor, &session, prepared, cx, observe, root),
            &lease,
        )
        .expect("committed custody");

        let interrupted = handoff.interrupted();
        assert_eq!(interrupted.session.run(), session.run());
        assert!(
            interrupted.socket_closed,
            "the prepared listener is stopped"
        );
        assert!(
            interrupted.key_released,
            "the prepared session's key is wiped"
        );
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "an interrupted handoff leaves no listener"
        );
    });
}

/// Serving ends on the session's own stop, on an owner loss, or on a custody
/// the kernel refuses — and it never ends by retrying into service.
#[test]
fn serving_ends_on_a_stop_an_owner_loss_or_a_refused_custody() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let (mut service, _) =
            select_and_hand_off(supervisor, &session, prepared, cx, observe, root);

        let stop = AtomicBool::new(true);
        let end = service.serve(
            &mut host::NoStreams,
            Duration::from_millis(1),
            &stop,
            &mut || false,
        );
        assert!(matches!(end, super::ServingEnd::Stopped), "{end}");

        let stop = AtomicBool::new(false);
        let end = service.serve(
            &mut host::NoStreams,
            Duration::from_millis(1),
            &stop,
            &mut || true,
        );
        assert!(matches!(end, super::ServingEnd::OwnerLost), "{end}");

        // The session's own stop request stops serving now; the next turn is a
        // terminal refusal, not a silent re-serve.
        service.stop();
        assert!(!service.key_held(), "stopping wipes the session key");
        let end = service.serve(
            &mut host::NoStreams,
            Duration::from_millis(1),
            &stop,
            &mut || false,
        );
        match end {
            super::ServingEnd::Failed(error) => assert!(
                error.is_terminal(),
                "a stopped custody is a terminal end: {error}"
            ),
            other => panic!("a stopped session served again: {other}"),
        }

        let cleanup = cleanup(cx);
        let retired = service
            .retire(&cleanup, &mut NoEffects)
            .expect("the stopped session still settles once");
        assert_eq!(retired.served, 0);
    });
}

/// A serving owner that published another lease is not retained under this
/// one: adoption refuses and the candidate it was handed stops.
#[test]
fn a_server_that_published_another_lease_is_not_retained() {
    use crate::runner::workspace_client::serving::{AdoptError, ServingOwner};

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let first = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = first.launch.endpoint().expect("bound");
        let second = prepare_owned_channel(&mut supervisor, &session);
        let other = published(&second);
        assert_ne!(other, published(&first), "two prepares publish two leases");

        let adopted =
            ServingOwner::adopt_production(custody(supervisor, &session, first, cx, observe, root))
                .expect("the first channel adopts under its own lease");
        let refused = ServiceOwner::retained(adopted, session, other);
        assert!(
            matches!(refused, Err(AdoptError::WrongSession)),
            "a server that published another lease is not retained"
        );
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "refusing to retain a lease stops the candidate"
        );
    });
}

/// The session is discoverable from durable identity alone, and a crash between
/// stopping and settling leaves the recorded custody for recovery instead of
/// resurrecting a listener.
#[test]
fn the_session_is_discovered_from_durable_identity_alone() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let (mut service, _) =
            select_and_hand_off(supervisor, &session, prepared, cx, observe, root);

        // Discovery asks the checkout journal, not this module: the same answer
        // a later command in another process would read.
        let facility = match discover(observe, root, session.environment()) {
            Observed::Verified(Some(facility), _) => facility,
            other => panic!("the committed session is discoverable: {other:?}"),
        };
        assert_eq!(facility.session.run(), session.run());
        assert_eq!(facility.session.generation(), session.generation());
        assert!(facility.live(), "the session run is live");

        // A crash window: the listener is stopped and the key wiped, but the run
        // has not settled. Discovery still names the session, and nothing
        // serves: a shutdown is not a resurrection.
        service.stop();
        let facility = match discover(observe, root, session.environment()) {
            Observed::Verified(Some(facility), _) => facility,
            other => panic!("a stopped session keeps its custody record: {other:?}"),
        };
        assert!(
            facility.live(),
            "an unsettled session is still the recoverable one"
        );

        let cleanup = cleanup(cx);
        service
            .retire(&cleanup, &mut NoEffects)
            .expect("the session settles once");
        // Settlement removes the environment's session mapping (or leaves its
        // closed run); either way nothing is served or resurrected from it.
        match discover(observe, root, session.environment()) {
            Observed::Verified(Some(facility), _) => assert!(
                !facility.live(),
                "a settled session is not the live one: {:?}",
                facility.phase
            ),
            Observed::Verified(None, _) | Observed::Absent(_) => {}
            Observed::Unknown(reason) => panic!("settlement leaves a readable journal: {reason}"),
        }
    });
}
