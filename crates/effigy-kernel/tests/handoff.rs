//! The one-shot descriptor a caller hands its own child (contract 032,
//! 052 K12; effigy#338).
//!
//! This is its own test binary on purpose: the proof needs descriptor 11 to be
//! free when the payload pipe is created, which a file full of concurrent
//! process tests cannot promise. Here the test is the only one in the process,
//! so it claims the slots it needs instead of hoping for them.

use effigy_kernel::{
    ctx::*, custody::*, gate::Wait, id::*, outcome::*, root::StateRoot, run::*, supervise::*,
};
use std::collections::BTreeMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn home() -> tempfile::TempDir {
    // Three characters at most: the fixture path plus a run id and `.sock`
    // must fit `sun_path`, and an Effigy run scratch root spends most of that
    // budget before the fixture name. A longer prefix fails the harness with
    // "cannot hold a ... fixture and its control socket".
    effigy_test_support::short_tempdir("kh-").unwrap()
}

fn fixture() -> PathBuf {
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_kernel-custody-fixture")))
}

fn context() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(20)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 64,
            log_keep: 2,
            line_max: 1024,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    )
    .effects()
    .unwrap()
}

fn invocation() -> ResolvedInvocation {
    let task = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [4; 16]),
        "",
        Surface::Published,
        "fixture",
        None,
    )
    .unwrap();
    ResolvedInvocation::new(InvocationSpec {
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
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap()
}

fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
}

fn foreground_begin(cx: &EffectContext, root: &StateRoot) -> ForegroundRun {
    ForegroundRun::begin(
        cx,
        root,
        &invocation(),
        BeginOpts {
            id: RunId::new(1, [1; 10]).unwrap(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    )
    .unwrap()
}

fn spec(home: &Path, script: &str) -> ProcessSpec {
    ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        cwd: home.into(),
        env: BTreeMap::new(),
        grace: Duration::from_millis(25),
        lifetime: Lifetime::Finite(Duration::from_secs(10)),
        terminal: Terminal::None,
        label: "handoff".into(),
    }
}

/// Claims descriptors up to and including the handoff slot, so the payload
/// pipe can be placed there deliberately.
fn occupy_through_the_slot() -> Vec<OwnedFd> {
    let mut held = Vec::new();
    loop {
        // SAFETY: a plain open of a static path; ownership transfers.
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
        assert!(fd >= 0, "open /dev/null");
        // SAFETY: the successful open returned a fresh owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let raw = fd.as_raw_fd();
        held.push(fd);
        assert!(
            raw <= HANDOFF_FD,
            "descriptor {HANDOFF_FD} was already held"
        );
        if raw == HANDOFF_FD {
            return held;
        }
    }
}

/// A caller hands its child one short-lived reader at `HANDOFF_FD`. The reader
/// can already sit at that number (a session with descriptors up to 10 open),
/// where `dup2` is a no-op that would leave CLOEXEC set: the slot's flag is
/// cleared explicitly, so the child still reads the payload after exec.
#[test]
fn a_handoff_reader_already_at_the_slot_survives_exec() {
    let mut held = occupy_through_the_slot();
    let (reader, writer) = {
        let mut fds = [-1; 2];
        // SAFETY: writable storage for exactly two descriptors.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe");
        // SAFETY: the successful pipe returned two fresh owned descriptors.
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
    };
    // Free the slot this test claimed, then move the reader onto it: this is
    // the shape that used to lose the CLOEXEC flag at exec.
    drop(held.pop().expect("the slot holder"));
    // SAFETY: the slot is free and the reader is live; ownership of the
    // duplicate transfers below.
    assert_eq!(
        unsafe { libc::dup2(reader.as_raw_fd(), HANDOFF_FD) },
        HANDOFF_FD,
        "move the payload reader onto the handoff slot"
    );
    // The parent's copy must not leak into unrelated children: only the
    // child's own spawn clears the flag.
    // SAFETY: live descriptor and a valid flag request.
    assert!(unsafe { libc::fcntl(HANDOFF_FD, libc::F_SETFD, libc::FD_CLOEXEC) } >= 0);
    drop(reader);
    // SAFETY: the duplicate at the slot is now the owned reader.
    let reader = unsafe { OwnedFd::from_raw_fd(HANDOFF_FD) };
    {
        use std::io::Write;
        let mut writer = std::fs::File::from(writer);
        writer.write_all(b"handoff-proof-payload").unwrap();
        writer.flush().unwrap();
    }

    let cx = context();
    let home = home();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = foreground_begin(&cx, &root);
    let mut process = spec(home.path(), "cat <&11 > handoff.out");
    process.lifetime = Lifetime::Finite(Duration::from_secs(10));
    let mut child =
        Custody::spawn_foreground_with_handoff(&mut run, process, &cx, true, Some(reader)).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0, "the child read its handoff after exec");
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();

    let payload = std::fs::read(home.path().join("handoff.out")).unwrap();
    assert_eq!(payload, b"handoff-proof-payload");
    drop(held);
}
