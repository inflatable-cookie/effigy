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
