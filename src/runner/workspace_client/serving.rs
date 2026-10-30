//! The host's detached serving owner for one prepared workspace channel
//! (contract 052 §8.7 and §8.11, the E10 handoff).
//!
//! A prepared channel is a session's listener and key held by
//! [`SessionLaunch`], plus the [`HostChannelAcceptor`] that receives TLS
//! connections. Nothing here creates either: the launch exists only for a run
//! that already carries the session's custody (`SessionSupervisor::adopt`), and
//! it can only publish a locator after a pinned handshake qualified its
//! endpoint. This module is the consumer those facts were waiting for: it marks
//! the launch serving *before* it accepts anything, then feeds each accepted
//! connection to the kernel's channel dispatcher.
//!
//! It owns no admission, credential, resolution or effect: the dispatcher is
//! the caller's, with its owning engines' seams, and every request it answers
//! passes the kernel's own pin check, session fence and grants.

use std::io;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use effigy_kernel::ctx::{EffectContext, ObserveContext};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Run, RunError};
use effigy_kernel::workspace_channel::{self as channel, Refusal, Served, Streams, Transport};
use effigy_kernel::workspace_session::{ServingRefusal, SessionLaunch, SessionSupervisor};

use crate::runner::workspace_transport::listener::{HostChannelAcceptor, ServerTlsTransport};

/// Where accepted connections come from.
///
/// Production accepts through the session's bound TLS listener
/// ([`HostChannelAcceptor`]); a proof supplies scripted transports with the same
/// `Transport` contract. Accepting is bounded and never blocks the owner.
pub(crate) trait ChannelAccept {
    type Transport: Transport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<Self::Transport>>;
}

impl ChannelAccept for HostChannelAcceptor {
    type Transport = ServerTlsTransport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<ServerTlsTransport>> {
        HostChannelAcceptor::accept(self, launch, run)
    }
}

/// Why the serving owner did not take or finish a connection.
#[derive(Debug)]
pub(crate) enum ServeError {
    /// The session's own custody refused — unqualified, not adopted, stopped or
    /// another session's — so nothing was accepted.
    Refused(ServingRefusal),
    /// The transport could not accept or complete the connection.
    Transport(io::Error),
    /// The channel refused this connection (pin, negotiation, credential or a
    /// request). That connection is over; the session keeps serving.
    Channel(Refusal),
    /// This owner no longer holds the launch.
    Stopped,
}

impl ServeError {
    /// Whether the owner can serve another connection after this failure. A
    /// refused custody or a released launch is terminal; a refused connection
    /// is not.
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::Refused(_) | Self::Stopped)
    }
}

impl From<ServingRefusal> for ServeError {
    fn from(refusal: ServingRefusal) -> Self {
        Self::Refused(refusal)
    }
}
impl From<io::Error> for ServeError {
    fn from(error: io::Error) -> Self {
        Self::Transport(error)
    }
}
impl From<Refusal> for ServeError {
    fn from(refusal: Refusal) -> Self {
        Self::Channel(refusal)
    }
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => {
                write!(formatter, "{}", RunError::from(*refusal))
            }
            Self::Transport(error) => write!(formatter, "workspace channel accept failed: {error}"),
            Self::Channel(refusal) => write!(formatter, "workspace channel refused: {refusal}"),
            Self::Stopped => formatter.write_str("the workspace serving owner was released"),
        }
    }
}

/// One detached session's serving owner.
///
/// The adopted supervisor is the session's custody. The launch is the listener
/// and key it published. Dropping or [`ServingOwner::stop`]ping this owner
/// stops the listener and wipes the key, so neither outlives it, and a
/// connection is only accepted after [`SessionLaunch::serve`] accepted the
/// adopted run.
pub(crate) struct ServingOwner<'a, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    supervisor: SessionSupervisor<'a>,
    launch: Option<SessionLaunch>,
    acceptor: A,
    server: channel::Server<Ad, Rs, Ob, Id>,
    cx: &'a EffectContext,
    observe: &'a ObserveContext,
    root: &'a StateRoot,
    served: usize,
    _transport: PhantomData<fn() -> T>,
}

impl<'a, T, A, Ad, Rs, Ob, Id> ServingOwner<'a, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    /// Take the session's serving custody. The supervisor must be the adopted
    /// owner of the run the launch belongs to; the kernel refuses anything else
    /// at the first [`SessionLaunch::serve`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        supervisor: SessionSupervisor<'a>,
        launch: SessionLaunch,
        acceptor: A,
        server: channel::Server<Ad, Rs, Ob, Id>,
        cx: &'a EffectContext,
        observe: &'a ObserveContext,
        root: &'a StateRoot,
    ) -> Self {
        Self {
            supervisor,
            launch: Some(launch),
            acceptor,
            server,
            cx,
            observe,
            root,
            served: 0,
            _transport: PhantomData,
        }
    }

    /// The listener this owner still holds, while it holds one.
    pub(crate) fn launch(&self) -> Option<&SessionLaunch> {
        self.launch.as_ref()
    }

    /// Whether the session's key is still in custody.
    pub(crate) fn key_held(&self) -> bool {
        self.launch.as_ref().is_some_and(|launch| launch.key_held())
    }

    /// Whether the launch may dispatch on its listener for this run.
    pub(crate) fn serving(&self) -> bool {
        self.launch
            .as_ref()
            .is_some_and(|launch| launch.serving(&self.supervisor))
    }

    /// Connections this owner has dispatched.
    pub(crate) fn served(&self) -> usize {
        self.served
    }

    /// Serve one pending connection, if one is waiting.
    ///
    /// The launch is marked serving for the adopted run *before* the accept, so
    /// a connection is never taken for a session nothing owns or a locator that
    /// was never published. `Ok(None)` means nothing was waiting; the caller's
    /// cadence decides when to ask again.
    pub(crate) fn serve_pending(
        &mut self,
        streams: &mut impl Streams,
    ) -> Result<Option<Served>, ServeError> {
        {
            let launch = self.launch.as_mut().ok_or(ServeError::Stopped)?;
            launch.serve(&self.supervisor)?;
        }
        let Some(mut transport) = self.acceptor.accept(
            self.launch.as_ref().ok_or(ServeError::Stopped)?,
            &self.supervisor,
        )?
        else {
            return Ok(None);
        };
        let served =
            self.server
                .serve(&mut transport, self.cx, self.observe, self.root, streams)?;
        self.served += 1;
        Ok(Some(served))
    }

    /// Poll for connections until `stop` is set, asking at most every `tick`.
    /// The supervisor owns this cadence and the stop decision; this loop never
    /// waits on a connection and never keeps one alive past its own end.
    pub(crate) fn run(
        &mut self,
        streams: &mut impl Streams,
        tick: Duration,
        stop: &AtomicBool,
    ) -> Result<usize, ServeError> {
        while !stop.load(Ordering::SeqCst) {
            match self.serve_pending(streams) {
                Ok(Some(_)) => continue,
                Ok(None) => std::thread::sleep(tick.max(Duration::from_millis(1))),
                // A refused connection is over; the session keeps serving.
                Err(error) if !error.is_terminal() => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(self.served)
    }

    /// Stop the listener and wipe the key now, keeping the session run's
    /// custody with its supervisor. Nothing this owner published is servable
    /// afterwards.
    pub(crate) fn stop(&mut self) {
        if let Some(mut launch) = self.launch.take() {
            launch.stop();
        }
    }

    /// Release the launch's custody and stop serving.
    pub(crate) fn release(mut self) {
        self.stop();
    }
}

impl<T, A, Ad, Rs, Ob, Id> Drop for ServingOwner<'_, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    fn drop(&mut self) {
        // An owner that goes away must not leave a socket or a signing key
        // behind; the supervisor still settles the run.
        self.stop();
    }
}

impl<T, A, Ad, Rs, Ob, Id> std::fmt::Debug for ServingOwner<'_, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServingOwner")
            .field("serving", &self.serving())
            .field("served", &self.served)
            .finish_non_exhaustive()
    }
}
