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
use super::{
    discover, HandoffRefusal, RecoveryOutcome, RetirementCause, ServiceOwner, ServingHandoff,
};
use crate::runner::workspace_client::serving::OwnedWorkspaceChannel;
use crate::runner::workspace_client::ServingFacts;
use crate::runner::workspace_transport::qualification::PreparedHostChannel;
use crate::runner::workspace_transport::TlsChannelTransport;

/// The custody #452's activation emits (`WorkspaceUp::Selected`): the adopted
/// supervisor, the prepared listener and key, and the validated facts, moved in
/// whole. Production hands the activation's own owner; the proofs build the same
/// value from the same fields.
fn owner<'a>(
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
        owner(supervisor, session, prepared, cx, observe, root),
        &lease,
    )
    .expect("the committed lease is the custody's own published lease");
    (
        ServiceOwner::adopt(handoff).expect("the committed session is adopted"),
        lease,
    )
}

/// The committed custody's own lease, adopted and ready to serve.
fn serving<'a>(
    supervisor: SessionSupervisor<'a>,
    session: &SessionRef,
    prepared: PreparedHostChannel,
    cx: &'a EffectContext,
    observe: &'a ObserveContext,
    root: &'a StateRoot,
) -> ServiceOwner<'a> {
    let lease = published(&prepared);
    let handoff = ServingHandoff::validated(
        owner(supervisor, session, prepared, cx, observe, root),
        &lease,
    )
    .expect("the committed lease is the custody's own");
    ServiceOwner::adopt(handoff).expect("the committed session is adopted")
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
                owner(supervisor, &other, prepared, cx, observe, root),
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
                owner(supervisor, &session, prepared, cx, observe, root),
                &foreign_lease,
            ),
            Err(HandoffRefusal::Unpublished)
        ));
        assert!(TcpStream::connect(endpoint).is_err());
    });

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let mut prepared = prepare_owned_channel(&mut supervisor, &session);
        let lease = published(&prepared);
        // A launch whose key is gone cannot serve under it.
        prepared.launch.stop();
        assert!(matches!(
            ServingHandoff::validated(
                owner(supervisor, &session, prepared, cx, observe, root),
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
            owner(supervisor, &session, prepared, cx, observe, root),
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
/// A custody is retained under its own published lease only: another session's
/// lease refuses, and the candidate it came with stops.
#[test]
fn a_custody_is_not_retained_under_another_sessions_lease() {
    use crate::runner::workspace_client::serving::{AdoptError, ServingOwner};

    let foreign = with_serving_session(|mut supervisor, session, _cx, _root, _observe| {
        published(&prepare_owned_channel(&mut supervisor, &session))
    });

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let first = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = first.launch.endpoint().expect("bound");
        let adopted =
            ServingOwner::adopt_production(owner(supervisor, &session, first, cx, observe, root))
                .expect("the channel adopts under its own lease");
        let refused = ServiceOwner::retained(adopted, session, foreign);
        assert!(
            matches!(refused, Err(AdoptError::WrongSession)),
            "a custody is not retained under another session's lease"
        );
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "refusing to retain the custody stops the candidate"
        );
    });
}

/// One session serves through exactly one owner: a second launch for the same
/// session refuses at the kernel boundary instead of opening a second listener.
#[test]
fn a_session_serves_through_one_owner_only() {
    with_serving_session(|mut supervisor, session, _cx, _root, _observe| {
        let mut first = prepare_owned_channel(&mut supervisor, &session);
        first
            .launch
            .serve(&supervisor)
            .expect("the session's first owner serves it");
        let mut second = prepare_owned_channel(&mut supervisor, &session);
        let refused = second.launch.serve(&supervisor);
        assert!(
            matches!(
                refused,
                Err(effigy_kernel::workspace_session::ServingRefusal::AlreadyPublished)
            ),
            "a second owner for one serving session refuses: {refused:?}"
        );
        // The first owner keeps serving: the refusal took nothing away.
        assert!(first.launch.serving(&supervisor));
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
            _ => panic!("the committed session is not discoverable from its own journal"),
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
            _ => panic!("a stopped but unsettled session loses its custody record"),
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
            Observed::Verified(Some(facility), _) => {
                assert!(!facility.live(), "a settled session is not the live one")
            }
            Observed::Verified(None, _) | Observed::Absent(_) => {}
            Observed::Unknown(reason) => panic!("settlement leaves a readable journal: {reason}"),
        }
    });
}

/// A Pending selection is completed only by positive evidence: an unproven fact
/// leaves it pending, and only a well-formed refusal disproves it. Nothing here
/// serves or settles from a partial answer.
#[test]
fn a_pending_selection_is_only_completed_by_positive_evidence() {
    use effigy_containers::environment::EnvError;
    use effigy_kernel::run::RunError;

    let refused = RecoveryOutcome::classify(Err(EnvError::Kernel(RunError::refused(
        "the published target is gone",
    ))));
    assert!(
        matches!(refused, RecoveryOutcome::Disproven(_)),
        "a refusal disproves the selection: {refused}"
    );

    for pending in [
        EnvError::Kernel(RunError::unavailable("the listener did not answer")),
        EnvError::Kernel(RunError::broken("the obligation row is unreadable")),
        EnvError::Busy("another transition holds Gate(Env)".to_owned()),
    ] {
        let outcome = RecoveryOutcome::classify(Err(pending));
        assert!(
            matches!(outcome, RecoveryOutcome::Pending(_)),
            "an unproven fact leaves the selection pending: {outcome}"
        );
    }

    assert!(
        matches!(
            RecoveryOutcome::classify(Ok(())),
            RecoveryOutcome::Completed
        ),
        "proven liveness and readback complete the selection"
    );
}

/// Retirement names why the session ended, and every cause takes the same one
/// release: the listener is closed and the run settles once.
#[test]
fn retirement_names_its_cause_and_releases_once() {
    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let endpoint = prepared.launch.endpoint().expect("bound");
        let service = serving(supervisor, &session, prepared, cx, observe, root);

        let cleanup = cleanup(cx);
        let retired = service
            .retire_for(RetirementCause::Down, &cleanup, &mut NoEffects)
            .expect("the session settles");
        assert_eq!(retired.cause, RetirementCause::Down);
        assert_eq!(retired.cause.as_str(), "down");
        assert_eq!(retired.session.run(), session.run());
        assert!(
            TcpStream::connect(endpoint).is_err(),
            "no listener outlives a down's retirement"
        );
    });
}

/// A down is the kernel's own coordination: this session may go down only when
/// nothing else holds its environment, and the answer names only its children.
#[test]
fn a_down_is_answered_by_the_kernel_coordination() {
    use effigy_kernel::workspace_session::DownStart;

    with_serving_session(|mut supervisor, session, cx, root, observe| {
        let prepared = prepare_owned_channel(&mut supervisor, &session);
        let service = serving(supervisor, &session, prepared, cx, observe, root);
        match service.down(observe, root) {
            Observed::Verified(DownStart::Ready(children), _) => {
                assert!(
                    children.children().is_empty(),
                    "the fixture session started no child run of its own"
                );
            }
            Observed::Verified(DownStart::Refused { .. }, _) => {
                panic!("the session is the environment's own holder here, so nothing refuses it")
            }
            _ => panic!("a down of this session is not readable from its journal"),
        }
    });
}

/// Only a selection that switched and was read back may be served. A pending
/// commit published nothing a guest can rely on, and an abandoned one was
/// revoked: both refuse before anything is adopted.
#[test]
fn handoff_requires_a_switched_selection() {
    use effigy_kernel::workspace_session::SelectionState;

    assert!(
        super::committed_state(SelectionState::Switched).is_ok(),
        "a switched and read-back selection is servable"
    );
    for state in [SelectionState::Pending, SelectionState::Abandoned] {
        let refused = super::committed_state(state)
            .expect_err("a selection that has not switched is never served");
        assert!(
            matches!(refused, HandoffRefusal::Unswitched(named) if named == state.as_str()),
            "the refusal names the durable state it found: {refused}"
        );
    }
}

/// The entry that opens the first terminal hands it its own capability, 0600 in
/// guest-private storage, and that terminal's later commands authenticate from
/// it: no shared file, and no second enrolment.
#[test]
fn a_first_terminal_keeps_the_capability_the_entry_handed_it() {
    use crate::runner::workspace_client::guest_fake::FakeGuest;
    use crate::runner::workspace_client::supervisor::stage_first_terminal_capability;

    let context =
        super::super::tests::workspace_document(Some(super::super::tests::session_run()), None);
    let mut guest = FakeGuest::new();
    super::super::tests::publish(&mut guest, &context, None);
    let secret = host::Secret::new([0x5a; 32]);
    let generation = Generation::new(context.session.generation);

    let handed =
        stage_first_terminal_capability(&mut guest, "terminal-1", generation, &secret, "app")
            .expect("the entry hands the terminal its capability");
    assert_eq!(
        handed.path,
        format!(
            "{}/capabilities/terminal-1",
            super::super::context::CLIENT_ROOT
        )
    );
    assert_eq!(
        guest.mode(&handed.path),
        Some(0o600),
        "the record is owner-only"
    );
    assert_eq!(guest.owner(&handed.path), Some("app"));

    // The terminal's later command presents it, with no enrolment credential
    // anywhere: the handoff is the terminal's own authority.
    match super::super::load_auth(&mut guest, &context, "terminal-1")
        .expect("the handed-over capability is presented")
    {
        super::super::GuestAuth::Capability(presented) => {
            assert!(
                presented.matches(&secret),
                "the capability the entry handed over"
            )
        }
        _ => panic!("a handed-over capability is presented, not re-enrolled"),
    }

    // Another terminal does not see it.
    assert!(
        super::super::load_auth(&mut guest, &context, "terminal-2").is_err(),
        "a capability belongs to one client"
    );
}
