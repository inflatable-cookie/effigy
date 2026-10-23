use super::*;
use crate::ctx::*;
use std::time::{Duration, Instant};

pub(super) fn context(placement: Placement) -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(5)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 1024,
            log_keep: 0,
            line_max: 1024,
            queue_bytes: 1024,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        placement,
    )
    .effects()
    .unwrap()
}

#[test]
fn siblings_peek_one_grant_without_any_runtime_files() {
    let fixture = effigy_test_support::Fixture::new("kernel-memory-capability-").unwrap();
    let cx = context(Placement::Host);
    let payload = serde_json::json!({"run": "random", "class": "foreground"});
    let lease = MemoryLease::new(&cx, &payload).unwrap();
    let one = lease.capability();
    let two = one.clone();
    for _ in 0..3 {
        assert_eq!(
            one.payload::<serde_json::Value>(cx.operation()).unwrap(),
            payload
        );
        assert_eq!(
            two.payload::<serde_json::Value>(cx.operation()).unwrap(),
            payload
        );
    }
    assert_eq!(std::fs::read_dir(fixture.path()).unwrap().count(), 0);
    assert!(format!("{lease:?}").contains("REDACTED"));
    assert!(!format!("{lease:?}").contains("random"));
}

#[test]
fn dropping_owner_revokes_queued_and_cloned_capabilities() {
    let cx = context(Placement::Host);
    let lease = MemoryLease::new(&cx, &42).unwrap();
    let reader = lease.capability();
    assert_eq!(reader.payload::<u32>(cx.operation()).unwrap(), 42);
    drop(lease);
    assert!(reader.payload::<u32>(cx.operation()).is_err());
    assert!(reader.clone().payload::<u32>(cx.operation()).is_err());
    assert!(reader.custody_fd(cx.operation()).is_err());
}

#[test]
fn wrong_marker_type_and_peer_identity_never_grant() {
    let cx = context(Placement::Host);
    let lease = MemoryLease::new(&cx, &42).unwrap();
    let wrong = format!("{PREFIX}{}", "0".repeat(64));
    let fd = lease.reader.stream.try_clone().unwrap().into();
    assert!(InheritedCapability::from_descriptor(cx.operation(), fd, &wrong).is_err());
    let fixture = effigy_test_support::Fixture::new("kernel-capability-wrong-fd-").unwrap();
    let file = std::fs::File::create(fixture.path().join("ordinary-file")).unwrap();
    assert!(InheritedCapability::from_descriptor(cx.operation(), file.into(), &wrong).is_err());
    let mut grant = lease.reader.grant(cx.operation()).unwrap();
    grant.owner.pid = 0;
    let bytes = serde_json::to_vec(&grant).unwrap();
    let expected = marker(&bytes);
    let (mut owner, reader) = UnixStream::pair().unwrap();
    owner
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    owner.write_all(&bytes).unwrap();
    assert!(
        InheritedCapability::from_descriptor(cx.operation(), reader.into(), &expected).is_err()
    );
    assert!(InheritedCapability::capture(cx.operation(), "just-a-run-id").is_err());
}

#[test]
fn expired_guest_and_oversized_grants_refuse() {
    let cx = context(Placement::Guest);
    assert!(MemoryLease::new(&cx, &"no host authority").is_err());
    let cx = context(Placement::Host);
    assert!(MemoryLease::new(&cx, &"x".repeat(MAX)).is_err());
    let lease = MemoryLease::new(&cx, &42).unwrap();
    let expired = OperationContext::new(
        Deadline::At(Instant::now()),
        CancelToken::default(),
        cx.operation().limits().clone(),
        EffectClass::Observe,
        Placement::Host,
    );
    assert!(lease.capability().payload::<u32>(&expired).is_err());
}

#[test]
fn custody_exports_only_a_verified_cloexec_reading_description() {
    let cx = context(Placement::Host);
    let lease = MemoryLease::new(&cx, &42).unwrap();
    let fd = lease.capability().custody_fd(cx.operation()).unwrap();
    assert!(fd.as_raw_fd() >= 128);
    // SAFETY: this fresh owned descriptor remains live during the flag read.
    assert_ne!(
        unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
    let reader =
        InheritedCapability::from_descriptor(cx.operation(), fd, lease.reader.marker()).unwrap();
    assert_eq!(reader.payload::<u32>(cx.operation()).unwrap(), 42);
    drop(lease);
    assert!(reader.payload::<u32>(cx.operation()).is_err());
}

#[test]
fn concurrent_peeks_keep_one_complete_grant_for_every_reader() {
    let cx = context(Placement::Host);
    let payload = serde_json::json!({"value": "x".repeat(2500)});
    let lease = MemoryLease::new(&cx, &payload).unwrap();
    let capability = lease.capability();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let capability = &capability;
            let payload = &payload;
            let cx = &cx;
            scope.spawn(move || {
                for _ in 0..100 {
                    assert_eq!(
                        &capability
                            .payload::<serde_json::Value>(cx.operation())
                            .unwrap(),
                        payload
                    );
                }
            });
        }
    });
}

#[test]
fn custody_pipe_pins_close_only_after_the_last_writer_and_check_access_mode() {
    use std::io::Read;
    let (mut watch, pin) = ClosurePin::new().expect("anonymous custody pipe");
    assert!(pipe_identity(&watch, libc::O_RDONLY).is_ok());
    assert!(pipe_identity(&watch, libc::O_WRONLY).is_err());
    assert!(pipe_identity(pin.writer.as_ref(), libc::O_RDONLY).is_err());
    let sibling = pin.clone();
    // SAFETY: watch owns this live descriptor across both fcntl calls.
    let flags = unsafe { libc::fcntl(watch.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0);
    // SAFETY: only O_NONBLOCK changes on this test's own pipe description.
    assert_eq!(
        unsafe { libc::fcntl(watch.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    drop(pin);
    assert_eq!(
        watch
            .read(&mut [0])
            .expect_err("sibling still pins cleanup")
            .kind(),
        io::ErrorKind::WouldBlock
    );
    drop(sibling);
    assert_eq!(watch.read(&mut [0]).expect("all writers closed"), 0);
}

#[test]
fn native_helper_receives_only_a_fresh_verified_reading_descriptor() {
    let cx = context(Placement::Host);
    let payload = serde_json::json!({"admitted": true, "class": "foreground"});
    let lease = MemoryLease::new(&cx, &payload).unwrap();
    let path = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("kernel-custody-fixture")
        });
    let mut command = std::process::Command::new(path);
    command.arg("inheritance-check").env_clear();
    lease
        .capability()
        .forward_command(cx.operation(), &mut command)
        .unwrap();
    assert!(command.status().unwrap().success());
    drop(lease);
    assert!(
        !command.status().unwrap().success(),
        "a queued grant survived peer closure"
    );
    for marker in ["memory-v1:0:", "memory-v1:10:", "memory-v1:-128:"] {
        assert!(selector_slot(&format!("{marker}{}", "0".repeat(64))).is_err());
    }
}
