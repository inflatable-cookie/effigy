//! The workspace terminal owner and its retained stream adapter (effigy#485).
//!
//! The kernel already defines the two seams this module fills, and owns both
//! the retained output and the wire:
//!
//! - [`effigy_kernel::workspace_channel::Terminal`] is handed the live
//!   [`Attachment`] for every validated input and resize, so the owner's own
//!   last check before it accepts work is `Attachment::attached_observed`.
//! - [`effigy_kernel::workspace_channel::RetainedStreams`] publishes a run's
//!   captured logs by absolute retained position, so a client cursor, a frame
//!   sequence and a rotation gap are one coordinate system and no second replay
//!   store exists.
//!
//! [`WorkspaceTerminal`] is the missing owner `RetainedStreams<T>` was written
//! for: it holds one admitted run's terminal writer, its resize handle and its
//! retirement, and it re-checks the owning attachment before *every* effect.
//! [`TerminalCustody`] is the session-bound `Streams` the merged #507 serving
//! owner ([`super::serving::ServingOwner::serve_pending`],
//! [`super::supervisor::ServiceOwner`]) serves its connections with: one
//! retained reader and one terminal owner per admitted run.
//!
//! Three facts shape the design:
//!
//! - **The writer is injected, never fabricated.** [`TerminalWriter`] is the
//!   child terminal's writer, resize handle and close. The live binding — the
//!   container-up caller handing over the child PTY, and the two-real-PTY
//!   process proof — is effigy#519 after #512. This slice proves the owner and
//!   the adapter with production-shaped fakes through the real `Server::serve`
//!   loop; it claims no process binding.
//! - **The writer has its own pump.** The channel's loop blocks on its
//!   transport, so a writer that was momentarily full when a frame arrived gets
//!   no turn until more traffic arrives. Each terminal therefore owns a pump
//!   thread that drains queued input and applies the newest geometry on its own
//!   cadence, independent of the connection. The connection only fences the
//!   frame and queues it (bounded), so a slow child never grows host memory and
//!   never needs the client to type again.
//! - **The owning attachment, not the connection, is the lifetime.** The owner
//!   resumes the run's owning handle ([`effigy_kernel::run::resume_owner`]) when
//!   it opens, and the pump re-reads that exact row immediately before every
//!   write, resize and idle tick. Owner EOF — the owning client's end of stream
//!   releasing its row, or the run closing — therefore retires the writer even
//!   when the connection is idle and no further frame will ever arrive.
//!
//! Input is owned, output is shared: a run's captured output is not
//! client-private, so any live attachment of the run may publish it, while its
//! input is the owning client's. The terminal binds to the run's owner
//! attachment and a second terminal's frames refuse with no effect rather than
//! writing to a writer they do not own.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use effigy_kernel::ctx::{EffectContext, ObserveContext};
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{self, Attachment, AttachmentId, Client, Session};
use effigy_kernel::workspace_channel::{
    Cursors, Refusal, RetainedStreams, StreamChunk, StreamStart, Streams, Terminal, TerminalSize,
};

/// The most input one owner queues for a writer that is not taking it.
///
/// A stream frame decodes at most 32 KiB, so this bounds a burst to two frames
/// and refuses past it. The alternative — an unbounded queue between a fast
/// client and a slow child — would let a client grow host memory with nothing
/// backpressuring it, and the channel's own window only bounds *outbound*
/// frames.
pub(crate) const MAX_PENDING_INPUT: usize = 64 * 1024;

/// How often the writer's own pump re-reads the owning attachment and retries a
/// writer that was full. Input and geometry never wait for more connection
/// traffic, and retirement never waits for the connection to end.
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// One admitted run's terminal, as the host writes to it.
///
/// The live binding (effigy#519) supplies the child's terminal writer, its
/// resize handle and its close. Every method is bounded and never waits: a
/// writer that cannot take the whole slice reports how much it did take, and
/// `WouldBlock` says it is momentarily full, so the pump — not the connection's
/// read loop — decides when to try again.
pub(crate) trait TerminalWriter: Send {
    /// Write as many bytes as the writer accepts now and report how many it
    /// took. `Ok(0)` and `WouldBlock` both mean "not now"; the bytes stay with
    /// the pump.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize>;
    /// Apply one geometry. `WouldBlock` keeps the newest size with the pump to
    /// apply on a later tick; an older size is never queued behind it.
    fn resize(&mut self, size: TerminalSize) -> io::Result<()>;
    /// The owning client's lifetime ended, or the writer failed: retire the
    /// writer exactly once, closing the child's terminal.
    fn retire(&mut self) -> io::Result<()>;
}

/// What the connection has accepted and the pump has not yet written.
#[derive(Debug, Default)]
struct Outbox {
    /// Input accepted while the owning attachment was live, bounded by
    /// [`MAX_PENDING_INPUT`].
    pending: VecDeque<u8>,
    /// The newest geometry the owner holds. There is no queue of earlier sizes:
    /// a newer size overwrites this one, and one the writer refused is retried
    /// only while it is still the newest.
    latest: Option<TerminalSize>,
    applied: Option<TerminalSize>,
    delivered: usize,
    /// The writer was retired: the owning client's lifetime ended, or the
    /// writer itself failed.
    retired: bool,
    /// The owner is being dropped; the pump must stop.
    stop: bool,
}

/// What one pump turn did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pumped {
    /// Bytes or a size were handed to the writer; ask again immediately.
    Progress,
    /// The writer is momentarily full, or there is nothing to do.
    Idle,
    /// The writer failed; retire it and stop.
    Closed,
}

/// The outbox and the pump's wake-up.
#[derive(Debug, Default)]
struct Shared {
    outbox: Mutex<Outbox>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Outbox> {
        self.outbox.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wake the pump because there is new work.
    fn notify(&self) {
        self.wake.notify_all();
    }
}

/// One admitted run's terminal owner: the writer its client types into, the
/// newest geometry, and the retained output the run publishes.
///
/// The owner outlives any one connection. The connection's fence and the pump's
/// fence are the same question asked twice: the channel re-reads the row before
/// it calls here, this owner re-reads the presented live handle before it
/// accepts the frame, and the pump re-reads the run's owning handle immediately
/// before every writer effect.
pub(crate) struct WorkspaceTerminal {
    observe: ObserveContext,
    root: StateRoot,
    /// The session this terminal belongs to. A terminal opened for one session
    /// is never typed into by another (§8.11).
    session: Session,
    run: RunId,
    /// The id of the run's owning attachment, from the kernel's own handle
    /// resumed when the terminal opened. A frame is accepted only for this row,
    /// and the pump re-checks the row itself before every effect.
    owner_id: AttachmentId,
    shared: Arc<Shared>,
    pump: Option<JoinHandle<()>>,
    /// The attachment the connection attached with, once it has. Only the
    /// owning client types into the run's terminal.
    binding: Option<AttachmentId>,
    refused: usize,
}

impl std::fmt::Debug for WorkspaceTerminal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkspaceTerminal")
            .field("run", &self.run)
            .field("session", &self.session)
            .field("owner", &self.owner_id)
            .field("binding", &self.binding)
            .field("retired", &self.retired())
            .field("pending", &self.pending())
            .finish_non_exhaustive()
    }
}

impl WorkspaceTerminal {
    /// Open the terminal owner for `session`'s admitted run `run`, writing to
    /// the child's `writer`.
    ///
    /// `effect` is the host's own effect context: the owner resumes the run's
    /// owning attachment with it, so the pump's lifetime fence is the kernel's
    /// real row rather than a remembered flag. A run this session does not own
    /// refuses here instead of opening a terminal nothing can type into.
    pub(crate) fn open(
        effect: &EffectContext,
        observe: ObserveContext,
        root: StateRoot,
        session: Session,
        run: RunId,
        writer: Box<dyn TerminalWriter>,
    ) -> Result<Self, TerminalRefusal> {
        let owner =
            run::resume_owner(effect, &root, session, run).map_err(|_| TerminalRefusal::NoOwner)?;
        let owner_id = owner.id();
        let shared = Arc::new(Shared::default());
        // The pump keeps the resumed owning handle: it is the lifetime every
        // effect is fenced on.
        let pump = spawn_pump(shared.clone(), observe.clone(), root.clone(), owner, writer)
            .map_err(|_| TerminalRefusal::Unavailable)?;
        Ok(Self {
            observe,
            root,
            session,
            run,
            owner_id,
            shared,
            pump: Some(pump),
            binding: None,
            refused: 0,
        })
    }

    /// The run this terminal belongs to.
    pub(crate) fn run(&self) -> RunId {
        self.run
    }

    /// The attachment the run's owning client attached with, once it has.
    pub(crate) fn owner(&self) -> Option<AttachmentId> {
        self.binding
    }

    /// Whether the writer has been retired: its client's lifetime ended, or the
    /// writer itself refused for good.
    pub(crate) fn retired(&self) -> bool {
        self.shared.lock().retired
    }

    /// Bytes the writer has accepted.
    pub(crate) fn delivered(&self) -> usize {
        self.shared.lock().delivered
    }

    /// Input this owner refused with no effect.
    pub(crate) fn refused(&self) -> usize {
        self.refused
    }

    /// Input the writer has not accepted yet.
    pub(crate) fn pending(&self) -> usize {
        self.shared.lock().pending.len()
    }

    /// The newest geometry the writer applied.
    pub(crate) fn applied(&self) -> Option<TerminalSize> {
        self.shared.lock().applied
    }

    /// Bind the run's terminal to the attachment that attached as its owning
    /// client. The first owner binding wins, so a resumed owner row keeps the
    /// writer and a second terminal never takes it over.
    pub(crate) fn bind(&mut self, attachment: &Attachment) {
        if self.retired() || self.binding.is_some() {
            return;
        }
        if attachment.run() != self.run
            || attachment.session() != self.session
            || attachment.client() != Client::Owner
            || attachment.id() != self.owner_id
        {
            return;
        }
        self.binding = Some(self.owner_id);
    }

    /// Re-check the owning attachment and wake the pump. The merged serving loop
    /// calls this on every pump turn, so a released row is noticed even when the
    /// connection has nothing left to publish.
    pub(crate) fn service(&mut self, attachment: &Attachment) -> Result<(), Refusal> {
        self.admit(attachment)?;
        self.shared.notify();
        Ok(())
    }

    /// The one gate every accepted effect passes: is the presented attachment the
    /// live owning attachment this terminal belongs to?
    ///
    /// It reads the durable row immediately before the owner accepts the frame,
    /// so a release, a replacement or a run closure between the channel's own
    /// check and this one refuses with no effect. The pump makes the same check
    /// against the run's owning handle immediately before it writes.
    fn admit(&mut self, attachment: &Attachment) -> Result<(), Refusal> {
        if self.retired() {
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
        if binding != attachment.id() {
            // A second terminal, a resumed or replaced terminal, or an
            // attachment this owner never bound: none of them may type into the
            // owning client's writer.
            return Err(self.note(Refusal::forbidden(
                "this terminal is typed into by the run's owning attachment",
            )));
        }
        match attachment.attached_observed(&self.observe, &self.root) {
            Ok(true) => Ok(()),
            // The owning row is gone: the owning client's end of stream (or its
            // run's closure) ended this terminal. The pump retires the writer on
            // its own tick; this refuses the frame now.
            Ok(false) => Err(self.note(Refusal::invalid(
                "this terminal's owning attachment is no longer attached",
            ))),
            Err(error) => Err(self.note(Refusal::unavailable(format!(
                "this terminal's attachment lifetime could not be read ({error})"
            )))),
        }
    }

    fn note(&mut self, refusal: Refusal) -> Refusal {
        self.refused = self.refused.saturating_add(1);
        refusal
    }
}

impl Drop for WorkspaceTerminal {
    fn drop(&mut self) {
        {
            let mut outbox = self.shared.lock();
            outbox.stop = true;
            outbox.pending.clear();
            outbox.latest = None;
        }
        self.shared.notify();
        // `TerminalWriter::write` never blocks, so the pump is never stuck in a
        // call this join has to wait out.
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
    }
}

impl Terminal for WorkspaceTerminal {
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        self.admit(attachment)?;
        let accepted = {
            let mut outbox = self.shared.lock();
            if outbox.retired {
                Err("this run's terminal writer was retired")
            } else if outbox.pending.len().saturating_add(bytes.len()) > MAX_PENDING_INPUT {
                // The bound is checked before a byte is accepted, so a refusal
                // never leaves a partial frame behind.
                Err("this run's terminal is not accepting input fast enough")
            } else {
                outbox.pending.extend(bytes);
                Ok(())
            }
        };
        match accepted {
            Ok(()) => {
                // The pump writes it as the writer takes it, whether or not this
                // connection ever sends another frame.
                self.shared.notify();
                Ok(())
            }
            Err(message) => Err(self.note(Refusal::unavailable(message))),
        }
    }

    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        self.admit(attachment)?;
        let accepted = {
            let mut outbox = self.shared.lock();
            if outbox.retired {
                Err("this run's terminal writer was retired")
            } else {
                // Latest wins: one size is held and no queue of earlier ones
                // exists, so a burst cannot grow into a backlog.
                outbox.latest = Some(size);
                Ok(())
            }
        };
        match accepted {
            Ok(()) => {
                self.shared.notify();
                Ok(())
            }
            Err(message) => Err(self.note(Refusal::unavailable(message))),
        }
    }
}

/// Start the writer's own pump. The thread owns the writer and the run's owning
/// handle; the connection only queues work.
fn spawn_pump(
    shared: Arc<Shared>,
    observe: ObserveContext,
    root: StateRoot,
    owner: Attachment,
    writer: Box<dyn TerminalWriter>,
) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("effigy-workspace-terminal".to_owned())
        .spawn(move || pump(shared, observe, root, owner, writer))
}

/// The pump: re-read the owning attachment immediately before each effect, and
/// hand the writer exactly one effect at a time.
///
/// "One effect" is one partial write or one resize, so *every* writer effect —
/// including the second and later chunks of a large frame, and the geometry that
/// follows them — has its own lifetime read immediately before it. A row
/// released between turns, or between two partial writes, therefore stops the
/// next effect instead of letting it land. The row is also read on every idle
/// tick, so owner EOF retires the writer without waiting for connection traffic.
fn pump(
    shared: Arc<Shared>,
    observe: ObserveContext,
    root: StateRoot,
    owner: Attachment,
    mut writer: Box<dyn TerminalWriter>,
) {
    loop {
        // What is there to do? Nothing is effected here.
        let (wants_write, wants_resize) = {
            let outbox = shared.lock();
            if outbox.stop {
                return;
            }
            (
                !outbox.pending.is_empty(),
                outbox
                    .latest
                    .filter(|size| Some(*size) != outbox.applied)
                    .is_some(),
            )
        };
        if !wants_write && !wants_resize {
            // An idle turn still reads the owning row, so owner EOF retires the
            // writer with no traffic and no connection turn to wait for.
            if !owner_live(&owner, &observe, &root) {
                retire(&shared, writer.as_mut());
                return;
            }
            if !wait(&shared) {
                return;
            }
            continue;
        }
        // Exactly one effect, with the owning row read immediately before it.
        if !owner_live(&owner, &observe, &root) {
            retire(&shared, writer.as_mut());
            return;
        }
        let step = if wants_write {
            write_once(&shared, writer.as_mut())
        } else {
            resize_once(&shared, writer.as_mut())
        };
        match step {
            Pumped::Closed => {
                retire(&shared, writer.as_mut());
                return;
            }
            // Ask again immediately. The next turn re-reads the owning row
            // before the next partial write, the geometry, or the idle tick.
            Pumped::Progress => {}
            // A full writer keeps the remainder queued; retry on the next tick.
            Pumped::Idle => {
                if !wait(&shared) {
                    return;
                }
            }
        }
    }
}

/// Whether the run's owning attachment is still attached, read now.
fn owner_live(owner: &Attachment, observe: &ObserveContext, root: &StateRoot) -> bool {
    matches!(owner.attached_observed(observe, root), Ok(true))
}

/// Wait for new work or the next tick. `false` means the owner is gone.
fn wait(shared: &Shared) -> bool {
    let outbox = shared.lock();
    if outbox.stop {
        return false;
    }
    let _ = shared
        .wake
        .wait_timeout(outbox, DRAIN_INTERVAL)
        .unwrap_or_else(PoisonError::into_inner);
    true
}

/// Retire the writer exactly once and stop the pump.
fn retire(shared: &Shared, writer: &mut dyn TerminalWriter) {
    {
        let mut outbox = shared.lock();
        outbox.pending.clear();
        outbox.latest = None;
        outbox.retired = true;
        outbox.stop = true;
    }
    let _ = writer.retire();
}

/// One partial write, and nothing else. The bytes stay queued until a writer
/// takes them, so a burst is bounded here rather than in the writer.
fn write_once(shared: &Shared, writer: &mut dyn TerminalWriter) -> Pumped {
    let mut outbox = shared.lock();
    if outbox.stop {
        return Pumped::Closed;
    }
    if outbox.pending.is_empty() {
        return Pumped::Idle;
    }
    let taken = {
        let bytes = outbox.pending.make_contiguous();
        writer.write(bytes)
    };
    match taken {
        Ok(0) => Pumped::Idle,
        Ok(written) => {
            let written = written.min(outbox.pending.len());
            outbox.pending.drain(..written);
            outbox.delivered = outbox.delivered.saturating_add(written);
            Pumped::Progress
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Pumped::Idle,
        Err(_) => Pumped::Closed,
    }
}

/// One resize of the newest geometry, and nothing else. An older size is never
/// applied: the newest one stays held and is retried only while it is newest.
fn resize_once(shared: &Shared, writer: &mut dyn TerminalWriter) -> Pumped {
    let mut outbox = shared.lock();
    if outbox.stop {
        return Pumped::Closed;
    }
    let Some(size) = outbox.latest.filter(|size| Some(*size) != outbox.applied) else {
        return Pumped::Idle;
    };
    match writer.resize(size) {
        Ok(()) => {
            outbox.applied = Some(size);
            Pumped::Progress
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Pumped::Idle,
        Err(_) => Pumped::Closed,
    }
}

/// Why one admitted run's terminal was not opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalRefusal {
    /// This custody already owns a terminal for that run. Opening another would
    /// give one run two writers, and only one of them can be the child's.
    AlreadyOwned,
    /// This session has no owning attachment for that run — it did not admit
    /// it, or the owning client's lifetime already ended. There is no writer to
    /// hand input to.
    NoOwner,
    /// The writer's pump could not start, so nothing could ever be delivered.
    Unavailable,
}

impl std::fmt::Display for TerminalRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyOwned => formatter
                .write_str("this session already owns a terminal for that run; it is not replaced"),
            Self::NoOwner => formatter.write_str(
                "this session has no owning attachment for that run; it owns no terminal",
            ),
            Self::Unavailable => {
                formatter.write_str("the run's terminal writer could not be started")
            }
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
    /// host's own effect context, the plan of the run's captured streams and the
    /// writer the guest types into. It refuses a run this session does not own a
    /// writer for (`NoOwner`) and a run it already owns a terminal for
    /// (`AlreadyOwned`): two writers for one child is not a state the host can
    /// serve.
    pub(crate) fn open(
        &mut self,
        effect: &EffectContext,
        run: RunId,
        plan: Vec<(i64, Stream)>,
        writer: Box<dyn TerminalWriter>,
    ) -> Result<(), TerminalRefusal> {
        if self.runs.iter().any(|streams| streams.run() == run) {
            return Err(TerminalRefusal::AlreadyOwned);
        }
        let terminal = WorkspaceTerminal::open(
            effect,
            self.observe.clone(),
            self.root.clone(),
            self.session,
            run,
            writer,
        )?;
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
        // Re-check the owning row and give the pump its turn. A refused or
        // retired terminal publishes nothing on its own account; the retained
        // reader still re-checks the row.
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
