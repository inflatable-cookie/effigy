//! Proofs for the Level 1 forwarding seam.
//!
//! The forwarder is driven by a production-shaped fake channel: it answers the
//! same intents the real `WorkspaceClient` sends (an invoke, one owning attach,
//! captured-output observes and replayed invokes), enforces the same cursor
//! ordering the real observe enforces, and records every request id it saw. The
//! captured-output host half is proved against a real kernel journal root and
//! real captured segment files, so the bytes, offsets, bounds and rotation
//! report are the kernel's own.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::PathBuf;

use effigy_cli::{Command, ContainerArgs, ContainerSubcommand, WorkspaceArgs};
use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::root::{Family, StateRoot};
use effigy_kernel::run::{AttachmentId, Client, RequestId};
use effigy_kernel::workspace_channel::{self as channel, AttachView, Cursors, ExitStatus, Streams};
use serde_json::json;

use super::*;

fn run_id() -> RunId {
    RunId::new(1, [9; 10]).expect("fixed run identity")
}

/// A guest channel that answers the Level 1 forwarding protocol.
///
/// It is deliberately strict: an observe must name the run and exactly the
/// offsets the guest has consumed, the attach is accepted once, and every invoke
/// is recorded so a second admission would be visible.
#[derive(Debug, Clone)]
struct FakeChannel {
    run: RunId,
    /// Successive captured stdout chunks; a read past the end delivers nothing.
    stdout: VecDeque<Vec<u8>>,
    stderr: VecDeque<Vec<u8>>,
    /// Output reads served so far.
    reads: usize,
    /// The output-read count at which a replayed invoke reports completion.
    closed_after: usize,
    /// Make the read at this index report a truncated capture.
    truncated_at: Option<usize>,
    /// Refuse the owning attachment with this message.
    refuse_attach: Option<String>,
    /// The recorded outcome a completed invoke answers with.
    outcome: serde_json::Value,
    /// The exit status a completed invoke answers with.
    exit: ExitStatus,
    /// How the next authentication answers. A `Unknown` is the indeterminate
    /// delivery that must be replayed on a fresh connection.
    authenticate: Auth,
    authentications: usize,
    invoked: Vec<RequestId>,
    attaches: usize,
    /// The offsets each observe named, in order.
    observed: Vec<[u64; 2]>,
    /// The offset each observe left behind: the position the next read must
    /// name.
    consumed: Option<[u64; 2]>,
}

impl Default for FakeChannel {
    fn default() -> Self {
        Self {
            run: run_id(),
            stdout: VecDeque::new(),
            stderr: VecDeque::new(),
            reads: 0,
            closed_after: 0,
            truncated_at: None,
            refuse_attach: None,
            outcome: json!({"workload": "Succeeded", "cleanup_complete": true}),
            exit: ExitStatus::UNKNOWN,
            authenticate: Auth::Accept,
            authentications: 0,
            invoked: Vec::new(),
            attaches: 0,
            observed: Vec::new(),
            consumed: None,
        }
    }
}

impl FakeChannel {
    fn new() -> Self {
        Self::default()
    }

    fn stdout_chunk(mut self, bytes: &[u8]) -> Self {
        self.stdout.push_back(bytes.to_vec());
        self
    }

    fn stderr_chunk(mut self, bytes: &[u8]) -> Self {
        self.stderr.push_back(bytes.to_vec());
        self
    }

    fn closed_after(mut self, reads: usize) -> Self {
        self.closed_after = reads;
        self
    }

    fn truncated_at(mut self, read: usize) -> Self {
        self.truncated_at = Some(read);
        self
    }

    /// Make the next authentication's delivery indeterminate.
    fn lost_authentication(mut self) -> Self {
        self.authenticate = Auth::Unknown;
        self
    }

    fn refuse_attach(mut self, message: &str) -> Self {
        self.refuse_attach = Some(message.to_owned());
        self
    }

    fn outcome(mut self, outcome: serde_json::Value, exit: ExitStatus) -> Self {
        self.outcome = outcome;
        self.exit = exit;
        self
    }

    fn completed(&self) -> bool {
        // The first invoke admits. A later invoke is a replay, and it sees the
        // run as closed once the scripted reads have happened.
        !self.invoked.is_empty() && self.reads >= self.closed_after
    }

    fn observe(&mut self, op: &str) -> Result<serde_json::Value, ChannelError> {
        let (run, cursors) = parse_output_operation(op)
            .ok_or_else(|| ChannelError::Local(format!("unexpected observe operation `{op}`")))?;
        if run != self.run {
            return Err(ChannelError::Local(format!(
                "the fake channel serves run {} only",
                self.run
            )));
        }
        // The guest may only ask from the position it has acknowledged.
        let expected = self.consumed.unwrap_or([0, 0]);
        if cursors != expected {
            return Err(ChannelError::Local(format!(
                "the observe named {cursors:?} after {expected:?}; a consumed position never moves backwards or skips"
            )));
        }
        self.observed.push(cursors);
        let truncated = self.truncated_at == Some(self.reads);
        let stdout = read_once(cursors[0], &mut self.stdout, truncated);
        let stderr = read_once(cursors[1], &mut self.stderr, false);
        self.consumed = Some([stdout.next, stderr.next]);
        self.reads += 1;
        Ok(serde_json::to_value(OutputRead {
            schema: "effigy.workspace-output.v1".to_owned(),
            run,
            stdout,
            stderr,
        })
        .expect("output read"))
    }
}

/// How a fake connection answers authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Auth {
    Accept,
    Unknown,
}

impl ChannelRequest for FakeChannel {
    fn authenticate(&mut self, _auth: super::super::GuestAuth) -> Result<(), ChannelError> {
        self.authentications += 1;
        match self.authenticate {
            Auth::Accept => Ok(()),
            Auth::Unknown => Err(ChannelError::Unknown {
                message: "the connection failed before the host answered authentication".to_owned(),
            }),
        }
    }

    fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        match &intent.body {
            channel::RequestBody::Invoke(_) => {
                let first = self.invoked.is_empty();
                self.invoked.push(intent.id());
                if !first && self.completed() {
                    Ok(ChannelOutcome::Completed {
                        outcome: self.outcome.clone(),
                        exit: self.exit,
                    })
                } else {
                    Ok(ChannelOutcome::Accepted {
                        run: self.run,
                        placement: RunPlacement::Host,
                    })
                }
            }
            channel::RequestBody::Attach(_) => {
                self.attaches += 1;
                if let Some(message) = &self.refuse_attach {
                    return Err(ChannelError::Refused {
                        code: channel::RefusalCode::Forbidden.as_str(),
                        message: message.clone(),
                    });
                }
                Ok(ChannelOutcome::Attached(AttachView {
                    attachment: AttachmentId::new([7u8; 16]),
                    run: self.run,
                    client: Client::Owner,
                    cursor: 0,
                }))
            }
            channel::RequestBody::Observe(body) => {
                if let Some(rest) = body.op.strip_prefix("runs:outcome:") {
                    let expected = format!("{}:{}", self.invoked[0], self.run);
                    if rest != expected {
                        return Err(ChannelError::Local("outcome identity mismatch".into()));
                    }
                    let value = if self.completed() {
                        json!({"state": "completed", "outcome": self.outcome})
                    } else {
                        json!({"state": "pending"})
                    };
                    Ok(ChannelOutcome::Observed(value))
                } else {
                    self.observe(&body.op).map(ChannelOutcome::Observed)
                }
            }
            other => Err(ChannelError::Local(format!(
                "a Level 1 forwarder sent {other:?}"
            ))),
        }
    }

    fn detach(&mut self, _run: RunId) -> Result<(), ChannelError> {
        // A Level 1 connection ends with its forwarded command. `LevelOneChannel`
        // must refuse a detach by name before this is reached.
        panic!("a Level 1 connection acknowledged a detach")
    }
}

/// One bounded read of one fake captured stream.
fn read_once(cursor: u64, chunks: &mut VecDeque<Vec<u8>>, truncated: bool) -> StreamRead {
    let bytes = chunks.pop_front().unwrap_or_default();
    StreamRead {
        from: cursor,
        next: cursor.saturating_add(bytes.len() as u64),
        bytes: base64::engine::general_purpose::STANDARD.encode(&bytes),
        gap: None,
        truncated,
    }
}

/// A sink that records what the guest printed.
#[derive(Debug, Default)]
struct RecordingSinks {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// Fail the next stdout write at this call count.
    fail_stdout_at: Option<usize>,
    writes: usize,
}

impl CommandSinks for RecordingSinks {
    fn stdout(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writes += 1;
        if self.fail_stdout_at == Some(self.writes) {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.stdout.extend_from_slice(bytes);
        Ok(())
    }

    fn stderr(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writes += 1;
        self.stderr.extend_from_slice(bytes);
        Ok(())
    }
}

/// The fake channel, shared between the forwarder's connection and its replay
/// path the way one guest connection and its fresh replay connections are.
#[derive(Clone)]
struct SharedChannel(std::rc::Rc<std::cell::RefCell<FakeChannel>>);

impl SharedChannel {
    fn new(channel: FakeChannel) -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(channel)))
    }

    fn inspect(&self) -> std::cell::Ref<'_, FakeChannel> {
        self.0.borrow()
    }
}

impl ChannelRequest for SharedChannel {
    fn authenticate(&mut self, auth: super::super::GuestAuth) -> Result<(), ChannelError> {
        self.0.borrow_mut().authenticate(auth)
    }

    fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        self.0.borrow_mut().send(intent)
    }

    fn detach(&mut self, run: RunId) -> Result<(), ChannelError> {
        self.0.borrow_mut().detach(run)
    }
}

/// The forwarding intent one task command sends.
fn task_intent() -> Intent {
    Intent::new(channel::RequestBody::Invoke(channel::InvocationBody {
        selector: Selector::Task("build".to_owned()),
        cwd: ".".to_owned(),
        target: channel::Target::Host,
        args: Vec::new(),
        output: channel::OutputMode::Text,
    }))
}

impl ForwardChannel for SharedChannel {
    fn client(&mut self) -> &mut dyn ChannelRequest {
        self
    }

    fn send_initial(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        self.send(intent)
    }
}

/// Drive the forwarding protocol over one fake connection.
fn drive(
    fake: &FakeChannel,
    sinks: &mut dyn CommandSinks,
) -> (Result<String, RunnerError>, SharedChannel) {
    let shared = SharedChannel::new(fake.clone());
    let mut client = shared.clone();
    let intent = task_intent();
    let result = super::drive(&mut client, &intent, sinks, false);
    (result, shared)
}

#[test]
fn forward_streams_distinct_streams_incrementally_and_reports_the_status() {
    let fake = FakeChannel::new()
        .stdout_chunk(b"hel")
        .stderr_chunk(b"warning: ")
        .stdout_chunk(b"lo\n")
        .stderr_chunk(b"still here\n")
        .closed_after(2)
        .outcome(
            json!({"workload": {"Failed": {"code": 3}}, "cleanup_complete": true}),
            ExitStatus {
                code: None,
                signal: None,
            },
        );
    let mut sinks = RecordingSinks::default();
    let (result, seen) = drive(&fake, &mut sinks);
    // The command already wrote its own output; the caller exits with its
    // recorded status and adds nothing.
    assert!(
        matches!(result, Err(RunnerError::RunSettled { code: 3 })),
        "{result:?}"
    );
    assert_eq!(sinks.stdout, b"hello\n");
    assert_eq!(sinks.stderr, b"warning: still here\n");
    let seen = seen.inspect();
    assert_eq!(seen.attaches, 1, "one owning attachment, never a second");
    assert_eq!(seen.invoked.len(), 1, "polls never invoke or admit");
    assert!(
        seen.reads >= 3,
        "output was read before the outcome and drained after it"
    );
}

#[test]
fn forward_delivers_output_before_the_run_closes() {
    // Two reads complete while the run is live: output is incremental, not a
    // single delivery at exit.
    let fake = FakeChannel::new()
        .stdout_chunk(b"first\n")
        .stdout_chunk(b"second\n")
        .closed_after(3);
    let mut sinks = RecordingSinks::default();
    let (result, seen) = drive(&fake, &mut sinks);
    assert_eq!(result.expect("a succeeded command"), "");
    assert_eq!(sinks.stdout, b"first\nsecond\n");
    assert!(seen.inspect().reads >= 3);
    assert_eq!(seen.inspect().attaches, 1);
}

#[test]
fn a_no_output_success_is_a_zero_status() {
    let fake = FakeChannel::new().closed_after(1);
    let mut sinks = RecordingSinks::default();
    let (result, _) = drive(&fake, &mut sinks);
    assert_eq!(result.expect("a quiet success"), "");
    assert!(sinks.stdout.is_empty());
    assert!(sinks.stderr.is_empty());
}

#[test]
fn an_unrecorded_outcome_never_reports_success() {
    let fake = FakeChannel::new().closed_after(0).outcome(
        json!({"workload": "Interrupted", "cleanup_complete": true}),
        ExitStatus::UNKNOWN,
    );
    let mut sinks = RecordingSinks::default();
    let (result, _) = drive(&fake, &mut sinks);
    let error = result.expect_err("unknown status is never success");
    assert!(error.to_string().contains("unknown"), "{error}");
}

#[test]
fn a_signalled_outcome_reports_the_signal() {
    let fake = FakeChannel::new().closed_after(0).outcome(
        json!({"workload": {"Cancelled": {"cause": {"Signal": 15}, "signal": 15}}, "cleanup_complete": true}),
        ExitStatus::UNKNOWN,
    );
    let mut sinks = RecordingSinks::default();
    let (result, _) = drive(&fake, &mut sinks);
    assert!(
        matches!(result, Err(RunnerError::RunSettled { code: 143 })),
        "{result:?}"
    );
}

#[test]
fn a_truncated_capture_is_not_reported_as_success() {
    let fake = FakeChannel::new()
        .stdout_chunk(b"partial\n")
        .closed_after(2)
        .truncated_at(1);
    let mut sinks = RecordingSinks::default();
    let (result, seen) = drive(&fake, &mut sinks);
    let error = result.expect_err("incomplete output is never success");
    assert!(error.to_string().contains("incomplete"), "{error}");
    assert_eq!(sinks.stdout, b"partial\n");
    assert_eq!(seen.inspect().attaches, 1);
}

#[test]
fn a_failed_sink_write_consumes_nothing_and_reports_unknown() {
    let fake = FakeChannel::new()
        .stdout_chunk(b"unprintable\n")
        .stdout_chunk(b"never read\n")
        .closed_after(3);
    let mut sinks = RecordingSinks {
        fail_stdout_at: Some(1),
        ..RecordingSinks::default()
    };
    let (result, seen) = drive(&fake, &mut sinks);
    let error = result.expect_err("a failed write is not success");
    assert!(
        error.to_string().contains("could not be written"),
        "{error}"
    );
    // The position was never advanced past the bytes that failed to print, so
    // no later read acknowledged them.
    assert_eq!(seen.inspect().observed.last(), Some(&[0, 0]));
    assert_eq!(sinks.stdout, b"");
}

#[test]
fn an_owning_attachment_refused_while_live_is_not_adopted() {
    let fake = FakeChannel::new()
        .refuse_attach("the run is held by another owning client")
        .closed_after(usize::MAX);
    let mut sinks = RecordingSinks::default();
    let (result, seen) = drive(&fake, &mut sinks);
    let error = result.expect_err("a live run without custody is unknown");
    assert!(error.to_string().contains("could not cancel"), "{error}");
    assert_eq!(seen.inspect().attaches, 1);
}

#[test]
fn an_owning_attachment_refused_for_a_closed_run_drains_the_output() {
    let fake = FakeChannel::new()
        .refuse_attach("this run is closed")
        .stdout_chunk(b"tail\n")
        .closed_after(0)
        .outcome(
            json!({"workload": {"Failed": {"code": 2}}, "cleanup_complete": true}),
            ExitStatus::UNKNOWN,
        );
    let mut sinks = RecordingSinks::default();
    let (result, _) = drive(&fake, &mut sinks);
    assert!(
        matches!(result, Err(RunnerError::RunSettled { code: 2 })),
        "{result:?}"
    );
    assert_eq!(sinks.stdout, b"tail\n");
}

#[test]
fn output_operation_round_trips_and_refuses_malformed_names() {
    let run = run_id();
    let op = output_operation(run, [7, 11]);
    assert_eq!(parse_output_operation(&op), Some((run, [7, 11])));
    for malformed in [
        "runs:output".to_owned(),
        "runs:output:".to_owned(),
        format!("runs:output:{run}:7"),
        format!("runs:output:{run}:7:11:3"),
        format!("runs:output:{run}:x:11"),
        "runs:list:foo".to_owned(),
    ] {
        assert_eq!(parse_output_operation(&malformed), None, "{malformed}");
    }
}

/// A private kernel journal root for the captured-output proofs.
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "effigy-noninteractive-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Host {
    root: StateRoot,
    observe: ObserveContext,
}
impl Host {
    fn new(fixture: &Fixture) -> Self {
        let cx = crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::Shell);
        let root = StateRoot::effects(&cx, &fixture.root).expect("private state root");
        let observe = crate::runner::task_engine::observe_context(Duration::from_secs(30));
        Self { root, observe }
    }
}

/// The kernel's captured segment layout, exactly as custody writes it.
fn write_segment(root: &StateRoot, run: RunId, stream: Stream, bytes: &[u8]) {
    write_segment_at(root, run, stream, 0, bytes);
}

/// One captured segment at a retained base position, so a rotation is
/// distinguishable from a first write.
fn write_segment_at(root: &StateRoot, run: RunId, stream: Stream, base: u64, bytes: &[u8]) {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let directory = root
        .path()
        .join(Family::Runs.dir_name())
        .join(run.to_string());
    assert!(directory.starts_with(root.path()));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .expect("private captured-log directory");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(format!(
            "{}.{}.{base}.log",
            effigy_kernel::supervise::OUTPUT_SEQUENCE,
            stream.name()
        )))
        .expect("fresh captured segment");
    use std::io::Write as _;
    file.write_all(bytes).expect("captured segment bytes");
}

#[test]
fn level_one_streams_answer_the_owning_attach_and_refuse_terminal_frames() {
    let fixture = Fixture::new();
    let host = Host::new(&fixture);
    let session = effigy_kernel::run::Session {
        run: run_id(),
        generation: effigy_kernel::id::Generation::new(1),
    };
    let mut streams = LevelOneStreams::new(TerminalCustody::new(
        session,
        host.observe.clone(),
        host.root.clone(),
    ));
    // A forwarded run's owning attachment opens: it carries no stream, so a
    // fresh cursor is accepted and a resume position is refused rather than
    // invented.
    assert_eq!(
        streams.resume(run_id(), &Cursors::fresh()),
        Some(Default::default())
    );
    assert_eq!(
        streams.resume(
            run_id(),
            &Cursors {
                primary: 4,
                streams: BTreeMap::new()
            }
        ),
        None
    );
    // No stream is published for a run the terminal custody does not own.
    assert_eq!(streams.next_chunk(None), None);
    // The Level 2 messages are the named ones.
    assert_eq!(InteractiveRequest::Input.refusal(), LEVEL_TWO_INPUT);
    assert_eq!(InteractiveRequest::Resize.refusal(), LEVEL_TWO_RESIZE);
    assert_eq!(InteractiveRequest::Detach.refusal(), LEVEL_TWO_DETACH);
}

#[test]
fn interactive_commands_refuse_before_any_connection() {
    let workspace = Command::Workspace(WorkspaceArgs {
        workspace: None,
        system: None,
        repo_override: None,
        output_json: false,
    });
    assert_eq!(
        interactive_command_refusal(&workspace),
        Some(LEVEL_TWO_ATTACH)
    );
    assert!(matches!(
        super::super::classify(&workspace),
        super::super::GuestOperation::Refused(message) if message == LEVEL_TWO_ATTACH
    ));

    let shell = Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name: None,
            service: None,
            command: None,
            bounded: None,
        },
        repo_override: None,
        output_json: false,
    });
    assert_eq!(
        interactive_command_refusal(&shell),
        Some(LEVEL_TWO_INTERACTIVE)
    );
    assert!(matches!(
        super::super::classify(&shell),
        super::super::GuestOperation::Refused(message) if message == LEVEL_TWO_INTERACTIVE
    ));

    // Every named Level 2 refusal is distinct and names the level.
    let refusals = [
        InteractiveRequest::Attach,
        InteractiveRequest::Pty,
        InteractiveRequest::Input,
        InteractiveRequest::Resize,
        InteractiveRequest::Detach,
    ];
    for (index, request) in refusals.iter().enumerate() {
        assert!(request.refusal().contains("Level 2"));
        for other in &refusals[index + 1..] {
            assert_ne!(request.refusal(), other.refusal());
        }
    }
}

#[test]
fn captured_output_reads_the_real_log_in_bounded_distinct_chunks() {
    let fixture = Fixture::new();
    let host = Host::new(&fixture);
    let run = run_id();
    write_segment(&host.root, run, Stream::Stdout, b"hello\n");
    write_segment(&host.root, run, Stream::Stderr, b"warning\n");
    let op = output_operation(run, [0, 0]);
    let value = observe_output(&host.observe, &host.root, &op).expect("captured output");
    let read: OutputRead = serde_json::from_value(value).expect("output document");
    assert_eq!(read.schema, "effigy.workspace-output.v1");
    assert_eq!(read.run, run);
    let stdout = read.stdout.decode().expect("stdout bytes");
    let stderr = read.stderr.decode().expect("stderr bytes");
    assert_eq!(stdout, b"hello\n");
    assert_eq!(stderr, b"warning\n");
    assert_ne!(stdout, stderr, "the two streams stay distinct");
    assert_eq!(read.stdout.from, 0);
    assert_eq!(read.stdout.next, 6);
    assert!(read.stdout.gap.is_none());

    // A fully consumed read is empty, not missing.
    let consumed = output_operation(run, [6, 8]);
    let value = observe_output(&host.observe, &host.root, &consumed).expect("empty write end");
    let read: OutputRead = serde_json::from_value(value).expect("output document");
    assert!(read.stdout.bytes.is_empty());
    assert!(read.stderr.bytes.is_empty());

    // A consumed position whose retained log is gone is a refusal, never a
    // finished stream.
    let unpublished = RunId::new(2, [3; 10]).expect("second run");
    let removed = output_operation(unpublished, [2, 0]);
    let refusal = observe_output(&host.observe, &host.root, &removed).expect_err("missing");
    assert!(refusal.diagnostic.contains("incomplete"), "{refusal:?}");

    // A rotation reports the bounded range it no longer holds as a gap, and
    // still delivers the bytes that resume after it.
    let rotated = RunId::new(3, [4; 10]).expect("third run");
    write_segment_at(&host.root, rotated, Stream::Stdout, 4, b"tail");
    let op = output_operation(rotated, [0, 0]);
    let value = observe_output(&host.observe, &host.root, &op).expect("rotated read");
    let read: OutputRead = serde_json::from_value(value).expect("output document");
    assert_eq!(read.stdout.gap, Some(GapRange { from: 0, to: 4 }));
    assert_eq!(read.stdout.from, 4);
    assert_eq!(read.stdout.decode().expect("resumed bytes"), b"tail");

    // A malformed operation refuses rather than reading something else.
    assert!(observe_output(&host.observe, &host.root, "runs:output:nonsense").is_err());
}

#[test]
fn captured_output_reads_stop_at_the_configured_bound() {
    let fixture = Fixture::new();
    let host = Host::new(&fixture);
    let run = run_id();
    let large = vec![b'x'; OUTPUT_CHUNK * 3 + 5];
    write_segment(&host.root, run, Stream::Stdout, &large);
    let mut cursor = 0;
    let mut delivered = Vec::new();
    let mut reads = 0;
    loop {
        let op = output_operation(run, [cursor, 0]);
        let value = observe_output(&host.observe, &host.root, &op).expect("bounded read");
        let read: OutputRead = serde_json::from_value(value).expect("output document");
        let bytes = read.stdout.decode().expect("bytes");
        assert!(bytes.len() <= OUTPUT_CHUNK, "one read stays bounded");
        if bytes.is_empty() {
            break;
        }
        delivered.extend_from_slice(&bytes);
        cursor = read.stdout.next;
        reads += 1;
    }
    assert_eq!(delivered, large);
    assert_eq!(reads, 4, "three full reads and the remainder");
}

#[test]
fn recorded_status_never_invents_a_terminal_status() {
    let unknown = ExitStatus::UNKNOWN;
    assert_eq!(
        recorded_status(unknown, &json!({"workload": "Succeeded"})),
        Some(ExitStatus {
            code: Some(0),
            signal: None
        })
    );
    assert_eq!(
        recorded_status(unknown, &json!({"workload": {"Failed": {"code": 3}}})),
        Some(ExitStatus {
            code: Some(3),
            signal: None
        })
    );
    assert_eq!(
        recorded_status(
            unknown,
            &json!({"workload": {"Cancelled": {"cause": "Requested", "signal": 15}}})
        ),
        Some(ExitStatus {
            code: None,
            signal: Some(15)
        })
    );
    assert_eq!(
        recorded_status(
            unknown,
            &json!({"workload": {"Cancelled": {"cause": {"Signal": 9}, "signal": null}}})
        ),
        Some(ExitStatus {
            code: None,
            signal: Some(9)
        })
    );
    // The host's own exit status wins when it has one.
    assert_eq!(
        recorded_status(
            ExitStatus {
                code: Some(7),
                signal: None
            },
            &json!({"workload": "Succeeded"})
        ),
        Some(ExitStatus {
            code: Some(7),
            signal: None
        })
    );
    for unprovable in [
        json!({"workload": "Interrupted"}),
        json!({"workload": {"TimedOut": {"phase": "run"}}}),
        json!({"workload": "NotStarted"}),
        json!({"workload": {"Cancelled": {"cause": "Requested", "signal": null}}}),
        json!({}),
    ] {
        assert_eq!(recorded_status(unknown, &unprovable), None, "{unprovable}");
    }
}

/// The host run a forwarded command carries starts with `Front::Detached` and
/// `terminal: None` (`managed_run::start_reserved`), which is a child with a
/// closed stdin and no terminal on any descriptor. Prove that shape with a real
/// child, and prove the distinct streams and real status a guest receives.
#[test]
fn a_forwarded_command_shape_has_no_pty_eof_stdin_and_a_real_status() {
    use std::io::Read as _;
    use std::process::{Command as Process, Stdio};
    let mut child = Process::new("/bin/sh")
        .arg("-c")
        .arg(
            "if [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; then exit 9; fi\n\
             if read line; then exit 7; fi\n\
             echo out\n\
             echo err >&2\n\
             exit 3",
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("real child");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_end(&mut stdout)
        .expect("captured stdout");
    child
        .stderr
        .take()
        .expect("piped stderr")
        .read_to_end(&mut stderr)
        .expect("captured stderr");
    let status = child.wait().expect("child status");
    assert_eq!(status.code(), Some(3), "the child's own exit status");
    assert_eq!(stdout, b"out\n", "stdout keeps its own bytes");
    assert_eq!(stderr, b"err\n", "stderr keeps its own bytes");
    assert_ne!(stdout, stderr);
    // No terminal reached the child, and its stdin read returned EOF at once
    // rather than exit 7 (a successful read) or exit 9 (a terminal).
    assert!(!stdout.starts_with(b"err"), "the streams are not merged");

    // A signalled command's status is a real signal, not a code the client
    // invented.
    use std::os::unix::process::ExitStatusExt as _;
    let mut signalled = Process::new("/bin/sh")
        .arg("-c")
        .arg("kill -TERM $$")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("signalled child");
    let status = signalled.wait().expect("signalled status");
    assert_eq!(status.signal(), Some(15));
}

#[test]
fn the_host_dispatcher_serves_the_captured_output_read_and_nothing_new_else() {
    let fixture = Fixture::new();
    let host = Host::new(&fixture);
    let run = run_id();
    write_segment(&host.root, run, Stream::Stdout, b"dispatched\n");
    let mut dispatch =
        super::super::host_dispatch::HostDispatch::new(fixture.root.clone(), host.root.clone());
    let body = channel::ObserveBody {
        op: output_operation(run, [0, 0]),
        target: Some(channel::Target::Host),
        query: None,
        run_id: None,
    };
    let value = channel::Observer::observe(&mut dispatch, &host.observe, &body)
        .expect("the captured-output read");
    let read: OutputRead = serde_json::from_value(value).expect("output document");
    assert_eq!(read.stdout.decode().expect("bytes"), b"dispatched\n");

    // An operation the host does not serve still refuses with its name rather
    // than being mistaken for an output read.
    let body = channel::ObserveBody {
        op: "runs:output-is-not-the-read".to_owned(),
        target: Some(channel::Target::Host),
        query: None,
        run_id: None,
    };
    let refusal = channel::Observer::observe(&mut dispatch, &host.observe, &body)
        .expect_err("an unserved observe still refuses");
    assert_eq!(refusal.code, channel::RefusalCode::Forbidden);
}

/// Scripted connections for the production replay state machine.
struct MemoryConnections {
    channels: Vec<SharedChannel>,
    next: usize,
}

impl Connections for MemoryConnections {
    fn open(&mut self) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        let channel =
            self.channels.get(self.next).cloned().ok_or_else(|| {
                ChannelError::Local("the proof ran out of connections".to_owned())
            })?;
        self.next += 1;
        Ok(Box::new(channel))
    }
}

fn level_one_auth() -> super::super::GuestAuth {
    super::super::GuestAuth::Capability(channel::Secret::new([4u8; 32]))
}

#[test]
fn indeterminate_authentication_does_not_reconnect_or_invoke() {
    let first = SharedChannel::new(FakeChannel::new().lost_authentication());
    let second = SharedChannel::new(FakeChannel::new());
    let memory = MemoryConnections {
        channels: vec![first.clone(), second.clone()],
        next: 0,
    };
    let mut session = super::Session::connect(Box::new(memory)).unwrap();
    session.authenticate_with(level_one_auth());
    let result = super::drive(
        &mut session,
        &task_intent(),
        &mut RecordingSinks::default(),
        false,
    );
    assert!(result.is_err());
    assert_eq!(first.inspect().authentications, 1);
    assert_eq!(second.inspect().authentications, 0);
    assert!(first.inspect().invoked.is_empty());
}

#[test]
fn a_level_one_connection_refuses_an_acknowledged_detach_by_name() {
    let fake = SharedChannel::new(FakeChannel::new().closed_after(usize::MAX));
    let memory = MemoryConnections {
        channels: vec![fake.clone()],
        next: 0,
    };
    let mut session = super::Session::connect(Box::new(memory)).expect("the connection opened");
    session.authenticate_with(level_one_auth());
    let admitted = session.send_initial(&task_intent()).expect("the admission");
    assert!(matches!(admitted, ChannelOutcome::Accepted { .. }));
    // The Level 1 client refuses the named Level 2 request before any frame, so
    // the fake's detach (which panics) is never reached.
    let refusal = session
        .client()
        .detach(run_id())
        .expect_err("a detach is not offered in Level 1");
    assert!(
        matches!(&refusal, ChannelError::Local(message) if message == LEVEL_TWO_DETACH),
        "{refusal:?}"
    );
    assert_eq!(
        fake.inspect().authentications,
        1,
        "the refusal is local and sends no frame"
    );
}

#[test]
fn the_credential_is_presented_only_by_the_first_request() {
    // `forward` opens the pinned connection (pin check and negotiation), then
    // reads the credential, then hands it to the session. Nothing is presented
    // until the first request, and the session presents it exactly once.
    let fake = SharedChannel::new(FakeChannel::new().closed_after(usize::MAX));
    let memory = MemoryConnections {
        channels: vec![fake.clone()],
        next: 0,
    };
    let mut session = super::Session::connect(Box::new(memory)).expect("the connection opened");
    assert_eq!(
        fake.inspect().authentications,
        0,
        "opening a connection presents nothing"
    );
    session.authenticate_with(level_one_auth());
    assert_eq!(
        fake.inspect().authentications,
        0,
        "holding the credential presents nothing"
    );
    let admitted = session.send_initial(&task_intent()).expect("the admission");
    assert!(matches!(admitted, ChannelOutcome::Accepted { .. }));
    assert_eq!(
        fake.inspect().authentications,
        1,
        "the first request presents the credential once"
    );
}

#[test]
fn json_nonzero_preserves_outcome_and_exit_status() {
    let result = report(
        json!({"workload": {"Failed": {"code": 3}}}),
        ExitStatus {
            code: Some(3),
            signal: None,
        },
        &Delivered::default(),
        true,
    );
    match result {
        Err(RunnerError::RunSettledJson { code, output }) => {
            assert_eq!(code, 3);
            let value: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert_eq!(value["status"]["code"], 3);
            assert_eq!(value["outcome"]["workload"]["Failed"]["code"], 3);
        }
        other => panic!("expected settled JSON failure: {other:?}"),
    }
}
