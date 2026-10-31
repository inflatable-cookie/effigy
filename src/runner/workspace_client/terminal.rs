//! The workspace terminal owner and its retained stream adapter (effigy#485).
//!
//! The kernel already defines the two seams this module fills, and owns both
//! the retained output and the wire:
//!
//! - [`effigy_kernel::workspace_channel::Terminal`] is handed the live
//!   [`Attachment`] for every validated input and resize, so the owner's own
//!   last check before it writes is `Attachment::attached_observed`.
//! - [`effigy_kernel::workspace_channel::RetainedStreams`] publishes a run's
//!   captured logs by absolute retained position, so a client cursor, a frame
//!   sequence and a rotation gap are one coordinate system and no second replay
//!   store exists.
//!
//! [`WorkspaceTerminal`] is the missing owner `RetainedStreams<T>` was written
//! for: it holds one admitted run's terminal writer, its resize handle and its
//! retirement, and it re-checks the owning attachment immediately before every
//! effect. [`TerminalCustody`] is the session-bound `Streams` the merged #507
//! serving owner ([`super::serving::ServingOwner::serve_pending`],
//! [`super::supervisor::ServiceOwner`]) serves its connections with: one
//! retained reader and one terminal owner per admitted run.
//!
//! Two boundaries are deliberate:
//!
//! - **The writer is injected, never fabricated.** [`TerminalWriter`] is the
//!   child terminal's writer, resize handle and close. The live binding — the
//!   container-up caller handing over the child PTY, and the two-real-PTY
//!   process proof — is effigy#519 after #512. This slice proves the owner and
//!   the adapter with production-shaped fakes through the real `Server::serve`
//!   loop; it claims no process binding.
//! - **Input is owned, output is shared.** A run's captured output is not
//!   client-private, so any live attachment of the run may publish it. Its
//!   input is the owning client's: the terminal binds to the run's owner
//!   attachment, and a second terminal's frames refuse with no effect rather
//!   than writing to a writer they do not own.

use std::collections::VecDeque;
use std::io;

use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Attachment, AttachmentId, Client, Session};
use effigy_kernel::workspace_channel::{
    Cursors, Refusal, RetainedStreams, StreamChunk, StreamStart, Streams, Terminal, TerminalSize,
};

/// The most input one owner holds for a writer that is not taking it.
///
/// A stream frame decodes at most 32 KiB, so this bounds a burst to two frames
/// and refuses past it. The alternative — an unbounded queue between a fast
/// client and a slow child — would let a client grow host memory with nothing
/// backpressuring it, and the channel's own window only bounds *outbound*
/// frames.
pub(crate) const MAX_PENDING_INPUT: usize = 64 * 1024;

/// One admitted run's terminal, as the host writes to it.
///
/// The live binding (effigy#519) supplies the child's terminal writer, its
/// resize handle and its close. Every method is bounded and never waits: a
/// writer that cannot take the whole slice reports how much it did take, and
/// `WouldBlock` says it is momentarily full, so the owner — not the writer —
/// decides what to hold.
pub(crate) trait TerminalWriter {
    /// Write as many bytes as the writer accepts now and report how many it
    /// took. `Ok(0)` and `WouldBlock` both mean "not now"; the bytes stay with
    /// the owner.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize>;
    /// Apply one geometry. `WouldBlock` keeps the newest size with the owner to
    /// apply on a later turn; an earlier size is never queued behind it.
    fn resize(&mut self, size: TerminalSize) -> io::Result<()>;
    /// The owning client's lifetime ended, or the writer failed: retire the
    /// writer exactly once, closing the child's terminal.
    fn retire(&mut self) -> io::Result<()>;
}

/// Which attachment a run's terminal is typed into.
///
/// It is recorded from the attachment that attached as the run's owning client,
/// so a released row, a replaced terminal, a second terminal or another run's
/// attachment is refused by identity before anything is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalBinding {
    attachment: AttachmentId,
    session: Session,
}

/// One admitted run's terminal owner: the writer its client types into, the
/// newest geometry, and the retained output the run publishes.
///
/// The owner outlives any one connection. Every effect re-reads the owning
/// attachment row first, so a reconnect that presents the same owner row resumes
/// typing where it stopped, and a row released, replaced or ended by its run's
/// closure is refused with zero bytes and zero resize.
pub(crate) struct WorkspaceTerminal {
    observe: ObserveContext,
    root: StateRoot,
    /// The session this terminal belongs to. A terminal opened for one session
    /// is never typed into by another (§8.11).
    session: Session,
    run: RunId,
    binding: Option<TerminalBinding>,
    writer: Box<dyn TerminalWriter>,
    /// Input the writer has not taken yet, bounded by [`MAX_PENDING_INPUT`].
    pending: VecDeque<u8>,
    /// The newest geometry the owner holds. There is no queue of earlier sizes:
    /// a newer size overwrites this one, and one the writer refused is retried
    /// only if it is still the newest.
    latest: Option<TerminalSize>,
    applied: Option<TerminalSize>,
    retired: bool,
    delivered: usize,
    refused: usize,
}

impl std::fmt::Debug for WorkspaceTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceTerminal")
            .field("run", &self.run)
            .field("session", &self.session)
            .field("owner", &self.owner())
            .field("pending", &self.pending.len())
            .field("latest", &self.latest)
            .field("retired", &self.retired)
            .finish_non_exhaustive()
    }
}

impl WorkspaceTerminal {
    /// The terminal owner for `session`'s admitted run `run`, writing to the
    /// child's `writer`.
    pub(crate) fn new(
        observe: ObserveContext,
        root: StateRoot,
        session: Session,
        run: RunId,
        writer: Box<dyn TerminalWriter>,
    ) -> Self {
        Self {
            observe,
            root,
            session,
            run,
            binding: None,
            writer,
            pending: VecDeque::new(),
            latest: None,
            applied: None,
            retired: false,
            delivered: 0,
            refused: 0,
        }
    }

    /// The run this terminal belongs to.
    pub(crate) fn run(&self) -> RunId {
        self.run
    }

    /// The attachment the run's owning client attached with, once it has.
    pub(crate) fn owner(&self) -> Option<AttachmentId> {
        self.binding.map(|binding| binding.attachment)
    }

    /// Whether the writer has been retired: its client's lifetime ended, or the
    /// writer itself refused for good.
    pub(crate) fn retired(&self) -> bool {
        self.retired
    }

    /// Bytes the writer has accepted.
    pub(crate) fn delivered(&self) -> usize {
        self.delivered
    }

    /// Input this owner refused with no effect.
    pub(crate) fn refused(&self) -> usize {
        self.refused
    }

    /// Input the writer has not accepted yet.
    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }

    /// The newest geometry the writer applied.
    pub(crate) fn applied(&self) -> Option<TerminalSize> {
        self.applied
    }

    /// Bind the run's terminal to the attachment that attached as its owning
    /// client. The first owner binding wins, so a resumed owner row keeps the
    /// writer and a second terminal never takes it over.
    pub(crate) fn bind(&mut self, attachment: &Attachment) {
        if self.retired || self.binding.is_some() {
            return;
        }
        if attachment.run() != self.run
            || attachment.session() != self.session
            || attachment.client() != Client::Owner
        {
            return;
        }
        self.binding = Some(TerminalBinding {
            attachment: attachment.id(),
            session: attachment.session(),
        });
    }

    /// Deliver the writer's pending input and its newest geometry under the live
    /// attachment. The merged serving loop calls this once per pump turn, so a
    /// reader that never types again still drains what the writer refused.
    pub(crate) fn service(&mut self, attachment: &Attachment) -> Result<(), Refusal> {
        self.admit(attachment)?;
        self.drain()?;
        self.apply_latest()
    }

    /// The one gate every effect passes: is the presented attachment the live
    /// owning attachment this terminal belongs to?
    ///
    /// It reads the durable row immediately before the effect, so a release, a
    /// replacement or a run closure between the channel's own check and this one
    /// stops the write. A released owning row is the owning client's end of
    /// stream, which retires the writer.
    fn admit(&mut self, attachment: &Attachment) -> Result<(), Refusal> {
        if self.retired {
            return Err(self.note(Refusal::unavailable(
                "this run's terminal was retired when its owning client's stream ended",
            )));
        }
        if attachment.run() != self.run || attachment.session() != self.session {
            return Err(self.note(Refusal::forbidden(
                "this terminal belongs to another run or workspace session",
            )));
        }
        let Some(binding) = self.binding else {
            return Err(self.note(Refusal::invalid(
                "no terminal owner holds this run's writer",
            )));
        };
        if binding.attachment != attachment.id() || binding.session != attachment.session() {
            // A second terminal, a resumed terminal that replaced this one, or an
            // attachment this owner never bound: none of them may type into the
            // owning client's writer.
            return Err(self.note(Refusal::forbidden(
                "this terminal is typed into by the run's owning attachment",
            )));
        }
        match attachment.attached_observed(&self.observe, &self.root) {
            Ok(true) => Ok(()),
            Ok(false) => {
                // The owning row is gone: the owning client's end of stream (or
                // its run's closure) ended this terminal, so the writer retires
                // instead of holding a child nobody can type into.
                self.retire();
                Err(self.note(Refusal::invalid(
                    "this terminal's owning attachment is no longer attached",
                )))
            }
            Err(error) => Err(self.note(Refusal::unavailable(format!(
                "this terminal's attachment lifetime could not be read ({error})"
            )))),
        }
    }

    fn note(&mut self, refusal: Refusal) -> Refusal {
        self.refused = self.refused.saturating_add(1);
        refusal
    }

    /// Retire the writer exactly once. A writer that cannot report its close has
    /// nothing left to release: the process it referred to is gone.
    fn retire(&mut self) {
        if self.retired {
            return;
        }
        self.retired = true;
        self.pending.clear();
        self.latest = None;
        let _ = self.writer.retire();
    }

    /// Write what the writer takes from the bounded pending input.
    fn drain(&mut self) -> Result<(), Refusal> {
        while !self.pending.is_empty() {
            let taken = {
                let bytes = self.pending.make_contiguous();
                self.writer.write(bytes)
            };
            match taken {
                // The writer is momentarily full: the rest stays with the owner,
                // which is what bounds a burst.
                Ok(0) => return Ok(()),
                Ok(written) => {
                    let written = written.min(self.pending.len());
                    self.pending.drain(..written);
                    self.delivered = self.delivered.saturating_add(written);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(_) => {
                    self.retire();
                    return Err(
                        self.note(Refusal::unavailable("this run's terminal writer closed"))
                    );
                }
            }
        }
        Ok(())
    }

    /// Apply the newest geometry, if it is not already applied. A size the
    /// writer refused stays the newest one; an older size never replaces it.
    fn apply_latest(&mut self) -> Result<(), Refusal> {
        let Some(size) = self.latest else {
            return Ok(());
        };
        if self.applied == Some(size) {
            return Ok(());
        }
        match self.writer.resize(size) {
            Ok(()) => {
                self.applied = Some(size);
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(_) => {
                self.retire();
                Err(self.note(Refusal::unavailable(
                    "this run's terminal resize handle closed",
                )))
            }
        }
    }
}

impl Terminal for WorkspaceTerminal {
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        self.admit(attachment)?;
        // The bound is checked before a byte is accepted, so a refusal never
        // leaves a partial frame behind.
        if self.pending.len().saturating_add(bytes.len()) > MAX_PENDING_INPUT {
            return Err(self.note(Refusal::unavailable(
                "this run's terminal is not accepting input fast enough",
            )));
        }
        self.pending.extend(bytes);
        self.drain()
    }

    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        self.admit(attachment)?;
        // Latest wins: one size is held and no queue of earlier ones exists, so a
        // burst cannot grow into a backlog.
        self.latest = Some(size);
        self.apply_latest()
    }
}

/// Why one admitted run's terminal was not opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalRefusal {
    /// This custody already owns a terminal for that run. Opening another would
    /// give one run two writers, and only one of them can be the child's.
    AlreadyOwned,
}

impl std::fmt::Display for TerminalRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyOwned => formatter
                .write_str("this session already owns a terminal for that run; it is not replaced"),
        }
    }
}

/// The kernel's retained reader for one admitted run, owned by the terminal
/// that types into it.
///
/// The kernel type is the whole output side: it reads the run's captured logs by
/// absolute retained position and publishes nothing else, so this factory adds no
/// store, no cursor and no second replay.
pub(crate) fn retained_streams(
    observe: ObserveContext,
    root: StateRoot,
    run: RunId,
    plan: Vec<(i64, Stream)>,
    terminal: WorkspaceTerminal,
) -> RetainedStreams<WorkspaceTerminal> {
    RetainedStreams::new(observe, root, run, plan, terminal)
}

/// One session's terminal custody: a terminal owner and a retained reader per
/// admitted run, served as the connection's [`Streams`].
///
/// This is the value the merged #507 serving owner retains across
/// `serve_pending` turns. It is bound to one session, so a terminal opened here
/// can never be typed into by another session's client, and a run with no entry
/// refuses terminal frames instead of dropping them.
#[derive(Debug)]
pub(crate) struct TerminalCustody {
    session: Session,
    observe: ObserveContext,
    root: StateRoot,
    runs: Vec<RetainedStreams<WorkspaceTerminal>>,
}

impl TerminalCustody {
    /// The custody of `session`, reading and fencing under `observe` and `root`.
    pub(crate) fn new(session: Session, observe: ObserveContext, root: StateRoot) -> Self {
        Self {
            session,
            observe,
            root,
            runs: Vec::new(),
        }
    }

    /// The session this custody belongs to.
    pub(crate) fn session(&self) -> Session {
        self.session
    }

    /// Open one admitted run's terminal and its retained output.
    ///
    /// #452's container-up caller calls this once per run it admits, with the
    /// plan of the run's captured streams and the writer the guest types into.
    /// A run this session already owns is refused rather than replaced: two
    /// writers for one child is not a state the host can serve.
    pub(crate) fn open(
        &mut self,
        run: RunId,
        plan: Vec<(i64, Stream)>,
        writer: Box<dyn TerminalWriter>,
    ) -> Result<(), TerminalRefusal> {
        if self.runs.iter().any(|streams| streams.run() == run) {
            return Err(TerminalRefusal::AlreadyOwned);
        }
        let terminal = WorkspaceTerminal::new(
            self.observe.clone(),
            self.root.clone(),
            self.session,
            run,
            writer,
        );
        self.runs.push(retained_streams(
            self.observe.clone(),
            self.root.clone(),
            run,
            plan,
            terminal,
        ));
        Ok(())
    }

    /// The runs this custody owns a terminal for.
    pub(crate) fn runs(&self) -> usize {
        self.runs.len()
    }

    /// One run's terminal, mutably: the writer a caller inspects between turns.
    pub(crate) fn terminal_mut(&mut self, run: RunId) -> Option<&mut WorkspaceTerminal> {
        self.runs
            .iter_mut()
            .find(|streams| streams.run() == run)
            .map(|streams| streams.terminal_mut())
    }

    fn entry_mut(&mut self, run: RunId) -> Option<&mut RetainedStreams<WorkspaceTerminal>> {
        self.runs.iter_mut().find(|streams| streams.run() == run)
    }
}

impl Streams for TerminalCustody {
    fn next_chunk(&mut self, attachment: Option<&Attachment>) -> Option<StreamChunk> {
        // Publication is the run's, not the client's: any live attachment of the
        // run may read it. Input and resize stay the owning attachment's.
        let attachment = attachment?;
        let entry = self.entry_mut(attachment.run())?;
        // Drain what the writer refused on the previous turn, and apply the
        // newest geometry, under this live attachment. A refused or retired
        // terminal publishes nothing on its own account; the retained reader
        // still re-checks the row.
        let _ = entry.terminal_mut().service(attachment);
        entry.next_chunk(Some(attachment))
    }

    fn resume(&mut self, run: RunId, cursors: &Cursors) -> Option<StreamStart> {
        // A run whose terminal this session does not own is refused before the
        // attachment opens: opening it would promise a terminal nothing can type
        // into.
        self.entry_mut(run)?.resume(run, cursors)
    }

    fn start(&mut self, attachment: &Attachment, start: &StreamStart) {
        let Some(entry) = self.entry_mut(attachment.run()) else {
            return;
        };
        entry.terminal_mut().bind(attachment);
        entry.start(attachment, start);
    }

    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        let Some(entry) = self.entry_mut(attachment.run()) else {
            // No owner holds this run's writer, so there is nothing to deliver
            // input to and no writer to invent one for.
            return Err(Refusal::unavailable(
                "this session owns no terminal for that run",
            ));
        };
        entry.terminal_mut().input(attachment, bytes)
    }

    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        let Some(entry) = self.entry_mut(attachment.run()) else {
            return Err(Refusal::unavailable(
                "this session owns no terminal to resize for that run",
            ));
        };
        entry.terminal_mut().resize(attachment, size)
    }
}
