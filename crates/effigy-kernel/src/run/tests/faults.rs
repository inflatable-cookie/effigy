use super::*;
use crate::fault::{self, Fault, Point};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

const HELPER: &str = "run::tests::faults::run_fault_child";
fn prepare(home: &Path) {
    let cx = context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut fake = Fake::new(home);
    active_environment(&cx, &root, &mut fake);
    let store = Store::open(&cx, &root).unwrap();
    // Strict durable release log: a crash cannot erase evidence of double release.
    store.conn().execute_batch("CREATE TABLE attachment_releases(environment TEXT,run_id TEXT,PRIMARY KEY(environment,run_id)); \
        CREATE TRIGGER attachment_release AFTER UPDATE OF state ON attachments WHEN NEW.state='released' AND OLD.state<>'released' \
        BEGIN INSERT INTO attachment_releases VALUES (NEW.environment,NEW.run_id); END;").unwrap();
}
fn scenario(home: &Path, residue: bool) -> Result<(), RunError> {
    let cx = context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut fake = Fake::new(home);
    let mut run = Run::begin(&cx, &root, &inv(owner()), opts(1), &cleanup(&cx), &mut fake)?;
    let env_fence = Fence {
        resource: env(),
        generation: Generation::new(1),
    };
    Attachments::attach(&mut run, &env_fence, AttachMode::Borrow, Wait::FailFast)?;
    Claims::reserve(
        &mut run,
        &[ResourceId::data("fake", "local", "data").unwrap()],
        Duration::ZERO,
    )?;
    for key in ["owned-a", "owned-b"] {
        let ticket = run.intend(spec(&run, key, Settlement::Release(Value::Null)))?;
        crate::fault_point!(AdapterBefore)?;
        fake.apply(&run, &ticket);
        crate::fault_point!(AdapterAfter)?;
        run.applied(&ticket, Value::Bool(true))?;
    }
    run.advance(Phase::Active)?;
    let ticket = run.intend(spec(&run, "failed", Settlement::Release(Value::Null)))?;
    run.failed(
        &ticket,
        AbsenceProof::NoFile {
            name: "failed".into(),
        },
    )?;
    request(
        &cx,
        &root,
        &Request {
            id: id(20),
            run: run.id(),
            expected: run.fence().clone(),
            action: RequestAction::Stop,
        },
    )?;
    run.finish(WorkloadOutcome::Failed { code: 9 })?;
    fake.unknown = residue;
    run.settle(&cleanup(&cx), &mut fake)?;
    if residue {
        // Traces a Closed+Incomplete predecessor retry, never a silent admission.
        let _next = Run::begin(&cx, &root, &inv(owner()), opts(2), &cleanup(&cx), &mut fake)?;
        panic!("residue admitted a successor");
    }
    Ok(())
}
fn recover_and_check(home: &Path) {
    let cx = context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut fake = Fake::new(home);
    let store = Store::open(&cx, &root).unwrap();
    let present: i64 = store
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE run_id=?1",
            [id(1).to_string()],
            |r| r.get(0),
        )
        .unwrap();
    if present == 1 {
        let record = journal::run(store.conn(), id(1)).unwrap();
        if record.phase != Phase::Closed {
            let view = reader(&cx, &root);
            assert!(
                matches!(status(&cx.operation().observe().unwrap(),&root,&view,id(1),next()),Observed::Verified(RunStatus::Interrupted { ref next,.. },_) if next.argv()==["recover"])
            ); // O2
        }
        settle_interrupted(
            &cx,
            &root,
            &owner(),
            id(1),
            next(),
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        let record = journal::run(store.conn(), id(1)).unwrap();
        assert_eq!(record.phase, Phase::Closed);
        assert!(journal::effects(store.conn(), id(1))
            .unwrap()
            .iter()
            .all(|e| e.state.terminal()));
        let attachments: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM attachments WHERE run_id=?1",
                [id(1).to_string()],
                |r| r.get(0),
            )
            .unwrap();
        let releases: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM attachment_releases WHERE run_id=?1",
                [id(1).to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(releases, attachments); // O5 exactly once, including crash after release
    }
    for key in ["owned-a", "owned-b"] {
        assert!(!home.join(key).exists());
    } // O1a strict fake custody closure
    let active: i64 = store
        .conn()
        .query_row(
            "SELECT count(*) FROM attachments WHERE state<>'released'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active, 0);
    let live_claims: i64 = store
        .conn()
        .query_row(
            "SELECT count(*) FROM claims WHERE state IN ('live','reserved')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(live_claims, 0);
    let run = begin(&cx, &root, owner(), 3, &mut fake);
    let open: i64 = run
        .store
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE owner=?1 AND phase<>'closed'",
            [owner().name().to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 1); // O3/O13
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    store.check_integrity().unwrap();
    let calls = fake.calls();
    let unique = std::collections::BTreeSet::from_iter(calls.iter());
    assert_eq!(calls.len(), unique.len());
}
#[test]
#[ignore]
fn run_fault_child() {
    if std::env::var("EFFIGY_KERNEL_RUN_CHILD").as_deref() != Ok("yes") {
        return;
    }
    let home = std::env::var_os("HOME").unwrap();
    let home = Path::new(&home);
    fault::arm_from_env();
    let _ = scenario(
        home,
        std::env::var("EFFIGY_KERNEL_RUN_RESIDUE").as_deref() == Ok("yes"),
    );
}
fn exercise(residue: bool) {
    let initial = home();
    prepare(initial.path());
    fault::arm(None);
    let _ = scenario(initial.path(), residue);
    let trace = fault::trace();
    fault::arm(None);
    for point in [
        Point::BeginBefore,
        Point::BeginGated,
        Point::BeginRecovered,
        Point::BeginAdmitted,
        Point::IntendBefore,
        Point::IntendAfter,
        Point::AppliedBefore,
        Point::AppliedAfter,
        Point::FailedBefore,
        Point::FailedAfter,
        Point::SettleBefore,
        Point::SettleStopping,
        Point::SettleEffectBefore,
        Point::SettleEffectAfter,
        Point::SettleCloseBefore,
        Point::SettleCloseAfter,
    ] {
        assert!(trace.contains(&point), "missing {point:?}");
    }
    for (index, point) in trace.iter().enumerate() {
        for fault_kind in [Fault::Enospc, Fault::Eperm] {
            let home = home();
            prepare(home.path());
            fault::arm(Some((index, fault_kind)));
            let _ = scenario(home.path(), residue);
            fault::arm(None);
            recover_and_check(home.path());
        }
        let home = home();
        prepare(home.path());
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([HELPER, "--exact", "--ignored", "--test-threads=1"])
            .env("HOME", home.path())
            .env("EFFIGY_KERNEL_RUN_CHILD", "yes")
            .env(
                "EFFIGY_KERNEL_RUN_RESIDUE",
                if residue { "yes" } else { "no" },
            )
            .env("EFFIGY_KERNEL_TEST_FAULT", format!("{index}:kill"))
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("fault child exceeded bounded wait at {index}");
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(
            exit.signal(),
            Some(libc::SIGKILL),
            "point {index}: {:?}",
            point
        );
        recover_and_check(home.path());
    }
}
#[test]
fn run_fault_every_boundary_kill_enospc_eperm_recovers() {
    exercise(false);
}
#[test]
fn run_fault_closed_residue_predecessor_boundaries_recover() {
    exercise(true);
}
#[test]
fn attachments_unknown_injection_preserves_holder_and_no_destructive_calls() {
    let home = home();
    prepare(home.path());
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(
        &mut holder,
        &Fence {
            resource: env(),
            generation: Generation::new(1),
        },
        AttachMode::Borrow,
        Wait::FailFast,
    )
    .unwrap();
    apply(&mut holder, &fake, "owned");
    let view = reader(&cx, &root);
    let mut transition = begin(&cx, &root, env(), 91, &mut fake);
    fault::set_unknown(true);
    assert!(matches!(
        status(
            &cx.operation().observe().unwrap(),
            &root,
            &view,
            id(1),
            next()
        ),
        Observed::Unknown(_)
    ));
    assert!(matches!(
        Attachments::holders(&cx.operation().observe().unwrap(), &root, &view, &env()),
        Observed::Unknown(_)
    ));
    assert!(Attachments::permit(&mut transition).is_err());
    let result = holder.settle(&cleanup(&cx), &mut fake).unwrap();
    fault::set_unknown(false);
    assert_eq!(result.cleanup(), CleanupOutcome::Incomplete);
    assert!(home.path().join("owned").exists());
    assert_eq!(fake.calls(), ["apply owned"]);
    assert!(Attachments::permit(&mut transition).is_err());
    settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(1),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert!(Attachments::permit(&mut transition).is_ok());
    transition.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn run_failed_journal_write_poisoning_prevents_later_effects() {
    for point in [
        Point::IntendBefore,
        Point::IntendAfter,
        Point::AppliedBefore,
        Point::AppliedAfter,
    ] {
        let sample = home();
        let cx = context();
        let root = StateRoot::effects(&cx, sample.path()).unwrap();
        let mut fake = Fake::new(sample.path());
        let mut run = begin(&cx, &root, owner(), 1, &mut fake);
        fault::arm(None);
        let ticket = run
            .intend(spec(&run, "owned", Settlement::Release(Value::Null)))
            .unwrap();
        fake.apply(&run, &ticket);
        run.applied(&ticket, Value::Null).unwrap();
        let trace = fault::trace();
        fault::arm(None);
        drop(run);
        let index = trace.iter().position(|p| *p == point).unwrap();
        let home = home();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut run = begin(&cx, &root, owner(), 1, &mut fake);
        fault::arm(Some((index, Fault::Enospc)));
        let result = run
            .intend(spec(&run, "owned", Settlement::Release(Value::Null)))
            .and_then(|ticket| {
                fake.apply(&run, &ticket);
                run.applied(&ticket, Value::Null)
            });
        fault::arm(None);
        assert!(result.is_err());
        assert!(run.intend(spec(&run, "late", Settlement::Retain)).is_err());
        run.settle(&cleanup(&cx), &mut fake).unwrap();
        assert!(!home.path().join("owned").exists());
    }
}

#[test]
fn run_log_read_fault_is_unknown_never_an_empty_log() {
    use crate::custody::Stream;
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut log = crate::custody::capture::Log::open(
        &root,
        id(40),
        3,
        Stream::Stdout,
        cx.operation().limits(),
    )
    .unwrap();
    log.append(b"bytes").unwrap();
    for injected in [Fault::Eperm, Fault::Enospc] {
        fault::arm_point(Point::LogRead, injected);
        let read = log_read(&cx, &root, Stream::Stdout, 0, 64);
        fault::arm(None);
        assert!(
            matches!(read, Observed::Unknown(_)),
            "an unreadable log is unknown, never empty: {read:?}"
        );
    }
    assert!(matches!(
        log_read(&cx, &root, Stream::Stdout, 0, 64),
        Observed::Verified(LogRead { ref bytes, .. }, _) if bytes == b"bytes"
    ));
}
