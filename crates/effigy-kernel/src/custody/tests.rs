use super::platform::{Member, ProcessTable};
use super::*;
use crate::ctx::*;
use crate::id::*;
use crate::root::StateRoot;
use crate::run::*;
use crate::supervise::{Executable, LifecyclePlan};
use std::path::Path;
use std::process::{Child, Command, Stdio};

fn cx() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(20)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 128,
            log_keep: 2,
            line_max: 128,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        crate::ctx::Placement::Host,
    )
    .effects()
    .unwrap()
}
fn id() -> RunId {
    RunId::new(1, [3; 10]).unwrap()
}
fn next() -> crate::outcome::Invocation {
    crate::outcome::Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
}
fn inv() -> ResolvedInvocation {
    let task = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [5; 16]),
        "",
        Surface::Published,
        "proof",
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
        placement: crate::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap()
}
#[derive(Debug)]
struct Strict;
impl SettlementAdapter for Strict {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("unmodelled adapter observation")
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("unmodelled adapter release")
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("unmodelled adapter reconcile")
    }
}
fn home() -> tempfile::TempDir {
    // Three characters at most: the fixture path plus a run id and `.sock`
    // must fit `sun_path`, and an Effigy run scratch root spends most of that
    // budget before the fixture name. A longer prefix fails the harness with
    // "cannot hold a ... fixture and its control socket".
    effigy_test_support::short_tempdir("kf-").unwrap()
}
fn executable_path() -> PathBuf {
    crate::fixture::custody_fixture()
}
fn executable() -> Executable {
    Executable::capture(&executable_path()).unwrap()
}
fn begin<'a>(cx: &EffectContext, root: &'a StateRoot) -> Result<Run<'a>, RunError> {
    Run::begin(
        cx,
        root,
        &inv(),
        BeginOpts {
            id: id(),
            wait: crate::gate::Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: executable(),
                placement: crate::supervise::Placement::Inline,
            }),
        },
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait(mut yes: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !yes() {
        assert!(Instant::now() < deadline, "proof deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn recovery_foreign_reused_group_unknown_and_nonce_evidence_oracle() {
    #[derive(Debug)]
    struct Fake {
        leader: Observed<ProcessIdentity>,
        members: Vec<Member>,
        signals: Vec<SignalDelivered>,
        race: bool,
        vanished_on_cont: bool,
        /// A member without the nonce that the re-inventory no longer lists.
        vanished: bool,
    }
    impl ProcessTable for Fake {
        fn leader(&mut self, _: &ProcessReceipt) -> Observed<ProcessIdentity> {
            self.leader.clone()
        }
        fn members(&mut self, _: u32) -> Observed<Vec<Member>> {
            Observed::Verified(self.members.clone(), evidence())
        }
        fn present(&mut self, identity: &ProcessIdentity) -> Observed<bool> {
            if !self.vanished {
                return Observed::Verified(true, evidence());
            }
            self.members.retain(|other| other.identity != *identity);
            Observed::Verified(false, evidence())
        }
        fn signal_leader(&mut self, r: &ProcessReceipt, s: i32) -> Observed<SignalDelivered> {
            if self.vanished_on_cont && s == libc::SIGCONT {
                return Observed::Absent(AbsenceProof::NoSuchProcess {
                    pid: r.identity.pid,
                });
            }
            if self.race {
                return Observed::Unknown(unknown("identity changed before signal"));
            }
            let sent = SignalDelivered {
                pid: r.pgid,
                group: true,
                signal: s,
            };
            self.signals.push(sent.clone());
            self.members.clear();
            Observed::Verified(sent, evidence())
        }
        fn signal_member(
            &mut self,
            m: &Member,
            _: &ProcessReceipt,
            s: i32,
        ) -> Observed<SignalDelivered> {
            let sent = SignalDelivered {
                pid: m.identity.pid,
                group: false,
                signal: s,
            };
            self.signals.push(sent.clone());
            self.members.retain(|other| other.identity != m.identity);
            Observed::Verified(sent, evidence())
        }
    }
    let identity = crate::ident::process_identity(std::process::id())
        .verified()
        .cloned()
        .unwrap();
    let r = ProcessReceipt {
        pgid: identity.pid,
        identity: identity.clone(),
        nonce: "run:effect".into(),
        grace_millis: 0,
    };
    let cleanup = cx().operation().cleanup(Duration::from_secs(1)).unwrap();
    for case in 0..7 {
        let mut own = identity.clone();
        own.pid += 1;
        let mut foreign = identity.clone();
        foreign.pid += 2;
        let members = match case {
            0 | 3 | 6 => vec![Member {
                identity: foreign,
                nonce: Observed::Unknown(unknown("foreign or cleared environment")),
            }],
            1 => vec![Member {
                identity: own,
                nonce: Observed::Verified(r.nonce.clone(), evidence()),
            }],
            2 => vec![
                Member {
                    identity: own,
                    nonce: Observed::Verified(r.nonce.clone(), evidence()),
                },
                Member {
                    identity: foreign,
                    nonce: Observed::Unknown(unknown("foreign")),
                },
            ],
            _ => vec![],
        };
        let mut fake = Fake {
            leader: if (4..=5).contains(&case) {
                Observed::Verified(identity.clone(), evidence())
            } else {
                Observed::Absent(AbsenceProof::NoSuchProcess { pid: r.pgid })
            },
            members,
            signals: vec![],
            race: case == 4,
            vanished_on_cont: case == 5,
            // Case 6 is the inventory/evidence race: the member without the
            // nonce has left the group, so the re-inventory no longer lists
            // it and recovery closes instead of reporting residue.
            vanished: case == 6,
        };
        let result = platform::recover_with(&mut fake, &r, &cleanup, &mut || false);
        if case == 5 {
            assert!(matches!(result, Observed::Verified(..)));
            assert_eq!(fake.signals.len(), 1);
            assert_eq!(fake.signals[0].signal, libc::SIGKILL);
        } else if case == 6 {
            assert!(
                matches!(result, Observed::Verified(..)),
                "a vanished member is not residue: {result:?}"
            );
            assert!(
                fake.signals.is_empty(),
                "nothing to signal: {:?}",
                fake.signals
            );
        } else if case == 1 {
            assert!(matches!(result, Observed::Verified(..)));
            assert!(fake
                .signals
                .iter()
                .all(|s| !s.group && s.pid == identity.pid + 1));
        } else {
            assert!(matches!(result, Observed::Unknown(_)));
            assert!(fake
                .signals
                .iter()
                .all(|s| !s.group && s.pid == identity.pid + 1));
        }
    }
}
#[test]
fn redaction_never_exposes_prefixes_or_debug_bytes() {
    let mut r = Redactor::new(vec![b"abcdef".to_vec(), b"abcd".to_vec()]);
    assert!(r.push(b"abc", false).is_empty());
    assert!(!format!("{r:?}").contains("abc"));
    let mut clean = r.push(b"def!abcd", false);
    clean.extend(r.push(&[], true));
    assert_eq!(clean, b"[REDACTED]![REDACTED]");
}
#[cfg(feature = "fault-injection")]
#[test]
fn fault_child() {
    let Some(home) = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FAULT_HOME") else {
        return;
    };
    use crate::fault::{Fault, Point};
    let point = match std::env::var("EFFIGY_KERNEL_CUSTODY_FAULT_POINT")
        .unwrap()
        .as_str()
    {
        "anchor-spawn" => Point::AnchorSpawn,
        "anchor-record" => Point::AnchorRecord,
        "anchor-stop" => Point::AnchorStop,
        "anchor-stopped" => Point::AnchorStopped,
        "fork" => Point::SpawnFork,
        "record" => Point::SpawnBeforeRecord,
        "go" => Point::SpawnBeforeGo,
        "after" => Point::SpawnAfterGo,
        "reg-before" => Point::LifecycleBefore,
        "reg-commit" => Point::LifecycleCommit,
        "reg-after" => Point::LifecycleAfter,
        "socket" => Point::SocketBind,
        // Bound but unrecorded: the receipt is the socket's first `applied`.
        "socket-receipt" => Point::AppliedBefore,
        _ => panic!("unmodelled point"),
    };
    let fault = if std::env::var("EFFIGY_KERNEL_CUSTODY_FAULT_KIND").unwrap() == "kill" {
        Fault::Kill
    } else {
        Fault::Enospc
    };
    crate::fault::arm_point(point, fault);
    let cx = cx();
    let root = StateRoot::effects(&cx, Path::new(&home)).unwrap();
    if let Ok(mut run) = begin(&cx, &root) {
        if matches!(point, Point::SocketBind | Point::AppliedBefore) {
            let _ = run.control_socket();
        } else {
            let process = ProcessSpec {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "printf started > workload-started; /bin/sleep 30".into(),
                ],
                cwd: home.into(),
                env: BTreeMap::new(),
                grace: Duration::from_millis(10),
                lifetime: Lifetime::Finite(Duration::from_secs(30)),
                terminal: Terminal::None,
                label: "fault".into(),
            };
            let _ = Custody::spawn(&mut run, process, &cx);
        }
        let _ = run.settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        );
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn fault_matrix_registration_spawn_and_socket_kill_enospc() {
    use std::os::unix::process::ExitStatusExt;
    let mut foreign = OwnedChild(
        Command::new("/bin/sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for point in [
        "anchor-spawn",
        "anchor-record",
        "anchor-stop",
        "anchor-stopped",
        "fork",
        "record",
        "go",
        "after",
        "reg-before",
        "reg-commit",
        "reg-after",
        "socket",
        "socket-receipt",
    ] {
        for fault in ["kill", "enospc"] {
            let home = home();
            let mut child = OwnedChild(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "custody::tests::fault_child", "--nocapture"])
                    .env("EFFIGY_KERNEL_CUSTODY_FAULT_HOME", home.path())
                    .env("EFFIGY_KERNEL_CUSTODY_FAULT_POINT", point)
                    .env("EFFIGY_KERNEL_CUSTODY_FAULT_KIND", fault)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let mut exit = None;
            wait(|| {
                exit = child.0.try_wait().unwrap();
                exit.is_some()
            });
            if fault == "kill" {
                assert_eq!(exit.unwrap().signal(), Some(libc::SIGKILL));
            } else {
                assert!(exit.unwrap().success());
            }
            let cx = cx();
            let root = StateRoot::effects(&cx, home.path()).unwrap();
            let cleanup = cx.operation().cleanup(Duration::from_secs(5)).unwrap();
            wait(|| {
                match settle_interrupted(
                    &cx,
                    &root,
                    &inv().spec().owner,
                    id(),
                    next(),
                    &cleanup,
                    &mut Strict,
                ) {
                    Ok(out) => {
                        assert_eq!(
                            out.cleanup(),
                            crate::outcome::CleanupOutcome::Complete,
                            "{point}/{fault}: {out:?}"
                        );
                        true
                    }
                    Err(RunError::Gate(crate::gate::GateError::Busy { .. })) => false,
                    Err(e) => panic!("{point}/{fault}: {e}"),
                }
            });
            let store = crate::store::Store::open(&cx, &root).unwrap();
            let (owner, sentinel): (Option<String>, Option<String>) = store
                .conn()
                .query_row(
                    "SELECT owner_identity,sentinel_identity FROM runs WHERE run_id=?1",
                    [id().to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(
                owner.is_some(),
                sentinel.is_some(),
                "registration partial at {point}/{fault}"
            );
            if point.starts_with("socket") {
                assert!(
                    !root.path().join(format!("sock/{}.sock", id())).exists(),
                    "{point}/{fault}: the run's socket outlived its close"
                );
                let result = status(
                    &cx.operation().observe().unwrap(),
                    &root,
                    crate::store::Store::read(&cx.operation().observe().unwrap(), &root)
                        .verified()
                        .unwrap(),
                    id(),
                    next(),
                );
                assert!(matches!(
                    result,
                    Observed::Verified(
                        RunStatus::Closed {
                            workload: crate::outcome::WorkloadOutcome::NotStarted,
                            complete: true,
                            ..
                        },
                        _
                    )
                ));
            }
            if [
                "anchor-spawn",
                "anchor-record",
                "fork",
                "record",
                "go",
                "socket",
                "socket-receipt",
                "reg-before",
                "reg-commit",
                "reg-after",
            ]
            .contains(&point)
            {
                assert!(!home.path().join("workload-started").exists());
            }
            assert!(
                foreign.0.try_wait().unwrap().is_none(),
                "foreign fixture was signalled"
            );
        }
    }
}
#[test]
fn journal_writes_are_owned_by_run_not_supervision_or_custody() {
    for source in [
        include_str!("../supervise.rs"),
        include_str!("../custody.rs"),
        include_str!("platform.rs"),
        include_str!("spawn.rs"),
    ] {
        for forbidden in [
            "UPDATE runs",
            "INSERT INTO runs",
            "UPDATE events",
            "INSERT INTO events",
        ] {
            assert!(!source.contains(forbidden));
        }
    }
}

#[test]
fn kernel_nonce_is_observed_from_real_spawn_environment() {
    let home = home();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root).unwrap();
    let mut child = Custody::spawn(
        &mut run,
        ProcessSpec {
            argv: vec![
                std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").expect("fixture"),
                "nonce-workload".into(),
            ],
            cwd: home.path().into(),
            env: BTreeMap::new(),
            grace: Duration::from_millis(10),
            lifetime: Lifetime::Finite(Duration::from_secs(5)),
            terminal: Terminal::None,
            label: "nonce".into(),
        },
        &cx,
    )
    .unwrap();
    let receipt = child.identity();
    let seen = platform::read_nonce(receipt.identity.pid);
    let report = child.stop(&cx.operation().cleanup(Duration::from_secs(5)).unwrap());
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    assert_eq!(
        seen.verified(),
        Some(&receipt.nonce),
        "{seen:?}; {report:?}"
    );
}

/// A stop never inherits the budget its caller minted earlier. A supervisor
/// mints its cleanup budget when its run begins, so a workload that outlives
/// that budget would otherwise be reported as "group closure unverified" with
/// an expired budget standing in for the observation, and it was never killed
/// (effigy#357).
#[test]
fn a_stop_after_a_spent_budget_still_escalates_and_verifies_closure() {
    let home = home();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root).unwrap();
    let mut child = Custody::spawn(
        &mut run,
        ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "trap '' TERM; exec /bin/sleep 120".into(),
            ],
            cwd: home.path().into(),
            env: BTreeMap::new(),
            grace: Duration::from_millis(100),
            lifetime: Lifetime::Open(OpenReason::DevServer),
            terminal: Terminal::None,
            label: "spent budget stop".into(),
        },
        &cx,
    )
    .unwrap();
    // The caller's budget is minted before the stop begins and is gone by the
    // time it does, which is the shape a long-lived supervisor has.
    let spent = cx.operation().cleanup(Duration::from_millis(10)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        spent.operation().check("proof").is_err(),
        "the fixture budget must be spent before the stop"
    );
    let report = child.stop(&spent);
    assert!(
        matches!(report.group, Observed::Verified(..) | Observed::Absent(_)),
        "a spent budget is not an observation of the group: {report:?}"
    );
    assert!(
        report
            .signals
            .iter()
            .any(|sent| sent.signal == libc::SIGKILL),
        "the member that ignored SIGTERM is killed after the grace: {report:?}"
    );
    assert!(report.residue.is_empty(), "{report:?}");
    assert!(
        report.leader.verified().is_some(),
        "the closed group reaps its leader: {report:?}"
    );
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
    .unwrap();
}

#[test]
fn log_enospc_keeps_redacted_ring_and_success_outcome() {
    let home = home();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root).unwrap();
    let mut child = Custody::spawn(
        &mut run,
        ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf '%s' \"$TOKEN\"".into(),
            ],
            cwd: home.path().into(),
            env: BTreeMap::from([(
                "TOKEN".into(),
                EnvValue::Secret(SecretValue::new(b"token-test999".to_vec()).unwrap()),
            )]),
            grace: Duration::from_millis(20),
            lifetime: Lifetime::Finite(Duration::from_secs(5)),
            terminal: Terminal::None,
            label: "capture proof".into(),
        },
        &cx,
    )
    .unwrap();
    crate::fault::arm_point(
        crate::fault::Point::CaptureWrite,
        crate::fault::Fault::Enospc,
    );
    let exit = *child.wait(cx.operation()).verified().unwrap();
    crate::fault::arm(None);
    child.complete(&mut run, exit).unwrap();
    run.finish(crate::outcome::WorkloadOutcome::Succeeded)
        .unwrap();
    let report = child.stop(&cx.operation().cleanup(Duration::from_secs(5)).unwrap());
    assert!(matches!(report.capture, CaptureState::Truncated(_)));
    assert_eq!(child.tail(), b"[REDACTED]");
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(
        outcome.workload(),
        &crate::outcome::WorkloadOutcome::Succeeded
    );
    assert_eq!(outcome.cleanup(), crate::outcome::CleanupOutcome::Complete);
}

#[test]
fn replacement_cannot_reconstruct_or_itself_contain_a_secret() {
    for (secrets, input) in [
        (vec![b"R".to_vec()], b"R".to_vec()),
        (vec![b"[REDACTED]".to_vec()], b"[REDACTED]".to_vec()),
        (vec![b"ab[".to_vec(), b"X".to_vec()], b"abX".to_vec()),
    ] {
        let mut redactor = Redactor::new(secrets.clone());
        let mut output = Vec::new();
        for byte in input {
            output.extend(redactor.push(&[byte], false));
        }
        output.extend(redactor.push(&[], true));
        for secret in secrets {
            assert!(!output.windows(secret.len()).any(|w| w == secret));
        }
    }
}

fn fixture_identity(pid: u32) -> ProcessIdentity {
    crate::ident::process_identity(pid)
        .verified()
        .unwrap()
        .clone()
}
// Even a failed assertion retires only the generations this proof launched.
struct AnchorFixture(Vec<ProcessIdentity>);
impl Drop for AnchorFixture {
    fn drop(&mut self) {
        for identity in &self.0 {
            if platform::generation(identity).verified().is_some() {
                platform::numeric(identity.pid, false, libc::SIGKILL);
            }
            let _ = spawn::reap(identity.pid);
        }
    }
}

#[test]
fn anchor_recovery_reaped_nonce_free_descendant_and_lost_anchor() {
    for (terminal, mode) in [
        (Terminal::None, 0),
        (Terminal::Pty, 0),
        (Terminal::None, 1),
        (Terminal::None, 2),
    ] {
        let home = home();
        let cx = cx();
        let spec = ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), "trap '' HUP; /usr/bin/env -i /bin/sh -c 'trap \"\" HUP; trap \"printf signal > signalled\" TERM CONT; echo $$ > descendant; while :; do if [ -f check ]; then : > checked; fi; /bin/sleep 0.05; done' & while [ ! -f descendant ]; do /bin/sleep 0.05; done".into()],
            cwd: home.path().into(), env: BTreeMap::new(),
            grace: Duration::from_millis(10), lifetime: Lifetime::Finite(Duration::from_secs(60)), terminal, label: "anchor proof".into(),
        };
        let image = crate::supervise::anchor::Image::prepare(&executable()).unwrap();
        let mut child =
            spawn::Blocked::fork(&spec, "anchor-proof", cx.operation(), &image, None, None)
                .unwrap();
        let anchor = fixture_identity(child.anchor.unwrap());
        let r = ProcessReceipt {
            identity: fixture_identity(child.pid),
            pgid: child.pid,
            nonce: "anchor-proof".into(),
            grace_millis: 10,
        };
        let mut fixture = AnchorFixture(vec![r.identity.clone(), anchor.clone()]);
        child.go().unwrap();
        child.executed(cx.operation()).unwrap();
        wait(|| spawn::reap(child.pid).unwrap().is_some());
        let pid: u32 = std::fs::read_to_string(home.path().join("descendant"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let descendant = fixture_identity(pid);
        fixture.0.push(descendant.clone());
        assert_ne!(platform::read_nonce(pid).verified(), Some(&r.nonce));
        if mode == 1 {
            // SAFETY: exact recorded fixture identity, checked immediately.
            assert!(platform::generation(&anchor).verified().is_some());
            platform::numeric(anchor.pid, false, libc::SIGKILL);
            wait(|| {
                !matches!(
                    crate::ident::process_state(anchor.pid),
                    Observed::Verified(
                        crate::ident::ProcessState::Active | crate::ident::ProcessState::Stopped,
                        _
                    )
                )
            });
        }
        let cleanup = cx.operation().cleanup(Duration::from_secs(2)).unwrap();
        let result = if mode == 2 {
            let old =
                serde_json::from_value::<ProcessRecord>(serde_json::to_value(&r).unwrap()).unwrap();
            assert!(old.anchor.is_none());
            platform::recover(&old.process, &cleanup, &mut || false)
        } else {
            anchor::recover(&r, &anchor, &cleanup, &mut || false)
        };
        let survived = platform::generation(&descendant).verified().is_some()
            && matches!(
                crate::ident::process_state(pid),
                Observed::Verified(
                    crate::ident::ProcessState::Active | crate::ident::ProcessState::Stopped,
                    _
                )
            );
        if mode != 0 && survived {
            // Acknowledgement proves the descendant ran after recovery and
            // processed pending traps before we inspect the signal marker.
            std::fs::write(home.path().join("check"), "check").unwrap();
            wait(|| home.path().join("checked").exists());
        }
        let signalled = home.path().join("signalled").exists();
        // Always retire our recorded descendant before asserting the verdict.
        if survived {
            platform::numeric(pid, false, libc::SIGKILL);
        }
        if mode == 2 {
            wait(|| {
                matches!(
                    anchor::closed(&r, Some(&anchor), None),
                    Observed::Verified(..) | Observed::Absent(_)
                )
            });
            anchor::stop(&r, Some(&anchor), &cleanup);
        }
        wait(|| {
            matches!(
                platform::group_closed(r.pgid, None),
                Observed::Verified(..) | Observed::Absent(_)
            )
        });
        if mode != 0 {
            assert!(matches!(result, Observed::Unknown(_)), "{result:?}");
            assert!(survived, "lost anchor must grant no signal authority");
            assert!(!signalled, "refusal must deliver neither TERM nor CONT");
        } else {
            assert!(
                matches!(result, Observed::Verified(..) | Observed::Absent(_)),
                "{result:?}"
            );
            assert!(!survived);
        }
    }
}

/// Q-014 (Tom 2026-10-27): an anchor is group evidence, not an indefinite
/// lease. SIGTERM retires an idle anchor within a bounded grace, so a harness
/// that stops its own tree never needs SIGKILL for custody evidence.
#[test]
fn idle_anchor_exits_on_term_within_a_bounded_grace() {
    let home = home();
    let cx = cx();
    let spec = ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "while :; do /bin/sleep 0.05; done".into(),
        ],
        cwd: home.path().into(),
        env: BTreeMap::new(),
        grace: Duration::from_millis(10),
        lifetime: Lifetime::Finite(Duration::from_secs(60)),
        terminal: Terminal::None,
        label: "anchor term proof".into(),
    };
    let image = crate::supervise::anchor::Image::prepare(&executable()).unwrap();
    let mut child =
        spawn::Blocked::fork(&spec, "anchor-term", cx.operation(), &image, None, None).unwrap();
    let anchor = fixture_identity(child.anchor.unwrap());
    let leader = fixture_identity(child.pid);
    let _fixture = AnchorFixture(vec![leader.clone(), anchor.clone()]);
    child.go().unwrap();
    child.executed(cx.operation()).unwrap();
    let anchor_live = || {
        matches!(
            crate::ident::process_state(anchor.pid),
            Observed::Verified(
                crate::ident::ProcessState::Active | crate::ident::ProcessState::Stopped,
                _
            )
        )
    };
    assert!(anchor_live(), "the anchor pins the live group");
    platform::numeric(anchor.pid, false, libc::SIGTERM);
    let deadline = Instant::now() + Duration::from_secs(2);
    while anchor_live() {
        assert!(
            Instant::now() < deadline,
            "an idle anchor must exit on TERM within two seconds"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        matches!(
            crate::ident::process_state(leader.pid),
            Observed::Verified(crate::ident::ProcessState::Active, _)
        ),
        "TERM retires the evidence, never the still-live workload"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn fork_only_anchor_cannot_obtain_the_effect_nonce_or_join_a_pty_session() {
    let nonce = format!("fork-only-proof-{}", std::process::id());
    let previous = std::env::var_os("EFFIGY_CUSTODY_NONCE");
    std::env::set_var("EFFIGY_CUSTODY_NONCE", &nonce);
    // SAFETY: the fork child only calls async-signal-safe pause; no Rust
    // allocator, environment access or lock is used there.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        loop {
            // SAFETY: pause has no arguments and only waits for a signal.
            unsafe {
                libc::pause();
            }
        }
    }
    if let Some(previous) = previous {
        std::env::set_var("EFFIGY_CUSTODY_NONCE", previous);
    } else {
        std::env::remove_var("EFFIGY_CUSTODY_NONCE");
    }
    assert!(pid > 0);
    let _fixture = AnchorFixture(vec![fixture_identity(pid as u32)]);
    let observed = platform::read_nonce(pid as u32);
    eprintln!("fork-only nonce observation: {observed:?}");
    assert_ne!(
        observed.verified(),
        Some(&nonce),
        "KERN_PROCARGS2 must not be confused with the changed libc environment"
    );
    let home = home();
    let spec = ProcessSpec {
        argv: vec!["/bin/sleep".into(), "1".into()],
        cwd: home.path().into(),
        env: BTreeMap::new(),
        grace: Duration::ZERO,
        lifetime: Lifetime::Finite(Duration::from_secs(1)),
        terminal: Terminal::Pty,
        label: "session proof".into(),
    };
    let child =
        spawn::Blocked::fork_with_nonce(&spec, "EFFIGY_PROBE_NONCE", "session", cx().operation())
            .unwrap();
    // SAFETY: setpgid attempts to move only this disposable test process.
    // A helper in the sentinel's separate session cannot join the PTY group.
    assert_eq!(unsafe { libc::setpgid(0, child.pid as i32) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EPERM));
}

#[test]
fn anchor_spawn_overhead_is_measured() {
    let mut host_load = [0f64; 3];
    // SAFETY: getloadavg writes at most three initialized doubles here.
    assert_eq!(unsafe { libc::getloadavg(host_load.as_mut_ptr(), 3) }, 3);
    let output = std::env::var_os("EFFIGY_KERNEL_ANCHOR_BENCH");
    assert!(
        output.is_none() || !cfg!(debug_assertions),
        "acceptance benchmark requires --release"
    );
    let samples = if output.is_some() { 24 } else { 2 };
    let load: usize = std::env::var("EFFIGY_KERNEL_ANCHOR_LOAD")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    assert!(load <= 4, "bound this proof's owned CPU load");
    let _load: Vec<_> = (0..load)
        .map(|_| {
            OwnedChild(
                Command::new("/usr/bin/yes")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        })
        .collect();
    let home = home();
    let spec = ProcessSpec {
        argv: vec![
            executable_path().into_os_string(),
            "anchor-no-children".into(),
        ],
        cwd: home.path().into(),
        env: BTreeMap::new(),
        grace: Duration::ZERO,
        lifetime: Lifetime::Finite(Duration::from_secs(1)),
        terminal: Terminal::None,
        label: "overhead".into(),
    };
    let image_path = std::env::var_os("EFFIGY_KERNEL_ANCHOR_IMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(executable_path);
    let executable = Executable::capture(&image_path).unwrap();
    let mut baseline = Vec::new();
    let mut anchored = Vec::new();
    let mut added = Vec::new();
    // Paired samples alternate order; two complete pairs warm both paths.
    for pair in 0..samples + 2 {
        let mut times = [0i64; 2];
        for enabled in if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let cx = self::cx();
            let started = Instant::now();
            let image =
                enabled.then(|| crate::supervise::anchor::Image::prepare(&executable).unwrap());
            let mut child = if enabled {
                spawn::Blocked::fork(
                    &spec,
                    "overhead",
                    cx.operation(),
                    image.as_ref().unwrap(),
                    None,
                    None,
                )
            } else {
                spawn::Blocked::fork_with_nonce(
                    &spec,
                    "EFFIGY_PROBE_NONCE",
                    "overhead",
                    cx.operation(),
                )
            }
            .unwrap();
            let r = ProcessReceipt {
                identity: fixture_identity(child.pid),
                pgid: child.pid,
                nonce: "overhead".into(),
                grace_millis: 0,
            };
            let anchor = child.anchor.map(fixture_identity);
            let mut fixture = AnchorFixture(vec![r.identity.clone()]);
            fixture.0.extend(anchor.clone());
            child.go().unwrap();
            child.executed(cx.operation()).unwrap();
            times[usize::from(enabled)] = started.elapsed().as_micros() as i64;
            let mut exit = None;
            wait(|| {
                exit = spawn::reap(child.pid).unwrap();
                exit.is_some()
            });
            assert_eq!(
                exit.unwrap().code,
                0,
                "the workload must have no hidden child"
            );
            assert!(matches!(
                anchor::stop(
                    &r,
                    anchor.as_ref(),
                    &cx.operation().cleanup(Duration::from_secs(2)).unwrap()
                ),
                Observed::Verified(..) | Observed::Absent(_)
            ));
        }
        if pair >= 2 {
            baseline.push(times[0]);
            anchored.push(times[1]);
            added.push(times[1] - times[0]);
        }
    }
    baseline.sort();
    anchored.sort();
    added.sort();
    let percentiles = |v: &[i64]| serde_json::json!({"p50_us":v[(v.len()-1)/2], "p90_us":v[(v.len()*9).div_ceil(10)-1]});
    let result = serde_json::json!({"samples":samples,"load_workers":load,"host_load":host_load,
        "release":!cfg!(debug_assertions),"image":image_path,"image_bytes":std::fs::metadata(&image_path).unwrap().len(),
        "baseline":percentiles(&baseline),"anchored":percentiles(&anchored),"added":percentiles(&added)});
    eprintln!("anchor spawn overhead: {result}");
    if let Some(path) = output {
        std::fs::write(path, result.to_string()).unwrap();
    }
}

/// The no-anchor path (`Blocked::fork_with_nonce`) in its own session with a
/// real controlling terminal. The owner gives the terminal to one foreground
/// child and then to a second while the first still holds it: a `tcsetpgrp`
/// from a background owner must not stop the owner. Every handover names the
/// child's own process group, and the terminal returns to the owner's group.
#[test]
fn nononce_foreground_takes_the_terminal_from_the_background_and_hands_it_back() {
    const OWNER: &str = "EFFIGY_KERNEL_TERMINAL_OWNER";
    if let Some(home) = std::env::var_os(OWNER) {
        nononce_foreground_owner(Path::new(&home));
        return;
    }
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let home = home();
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: initialized writable slots; null settings request OS defaults.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty transferred two fresh distinct descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    // SAFETY: live descriptors; observe the master nonblocking, never inherit it.
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC),
            0
        );
    }
    // Re-enter this same test binary in its own session rather than fork Rust
    // in a threaded child: the owner needs a controlling terminal of its own.
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "custody::tests::nononce_foreground_takes_the_terminal_from_the_background_and_hands_it_back",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .env(OWNER, home.path())
        .stdin(slave)
        .stdout(std::fs::File::create(home.path().join("owner-output")).unwrap());
    // SAFETY: only session and terminal syscalls run after fork, before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut owner = OwnedChild(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut await_marker = |marker: &str| {
        while !home.path().join(marker).exists() {
            assert!(Instant::now() < deadline, "owner never wrote {marker}");
            assert!(!owner_stopped(owner.0.id()), "the owner was stopped");
            assert!(owner.0.try_wait().unwrap().is_none(), "owner exited early");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    await_marker("ready");
    master.write_all(b"queued-input\n").unwrap();
    await_marker("owner-done");
    // macOS waits for its PTY peer during session-leader exit.
    drop(master);
    let status = owner.0.wait().unwrap();
    assert!(
        status.success(),
        "terminal owner failed: {status:?}; {}",
        std::fs::read_to_string(home.path().join("owner-output")).unwrap()
    );
}

/// A stopped owner cannot write any marker: the whole point of the proof.
fn owner_stopped(pid: u32) -> bool {
    matches!(
        crate::ident::process_state(pid),
        Observed::Verified(crate::ident::ProcessState::Stopped, _)
    )
}

fn nononce_foreground_owner(home: &Path) {
    let cx = cx();
    std::fs::write(home.join("ready"), b"yes").unwrap();
    let child = |script: &str, gate: &Path| {
        let spec = ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                script.into(),
                "sh".into(),
                gate.as_os_str().into(),
            ],
            cwd: home.into(),
            env: BTreeMap::new(),
            grace: Duration::from_millis(50),
            lifetime: Lifetime::Finite(Duration::from_secs(15)),
            terminal: Terminal::Foreground,
            label: "no-anchor terminal proof".into(),
        };
        let mut blocked = spawn::Blocked::fork_with_nonce(
            &spec,
            "EFFIGY_CUSTODY_NONCE",
            "terminal-proof",
            cx.operation(),
        )
        .unwrap();
        let pid = blocked.pid;
        // The lease is taken before go, exactly as custody does: the workload
        // never reads a background terminal.
        let (stdout, stderr, lease) = blocked.take_streams();
        let lease = lease.expect("a foreground child holds this owner's terminal");
        blocked.go().unwrap();
        blocked.executed(cx.operation()).unwrap();
        drop(stderr);
        (pid, lease, stdout)
    };
    let gate = |name: &str| home.join(name);
    // The holder writes to the terminal, then waits (bounded) for its gate.
    let (holder, holder_lease, holder_out) = child(
        "printf 'a-ok' > /dev/tty; i=0; while [ ! -f \"$1\" ] && [ $i -lt 500 ]; do i=$((i+1)); sleep 0.02; done; printf 'a-done' > /dev/tty",
        &gate("release-a"),
    );
    drop(holder_out);
    assert_terminal(holder);
    // The reader is spawned while the holder still owns the terminal, so this
    // take happens from the background.
    let (reader, reader_lease, mut reader_out) = child(
        "i=0; while [ ! -f \"$1\" ] && [ $i -lt 500 ]; do i=$((i+1)); sleep 0.02; done; IFS= read -r line < /dev/tty && printf 'b-read:%s' \"$line\"",
        &gate("release-b"),
    );
    assert_terminal(reader);
    std::fs::write(gate("release-b"), b"go").unwrap();
    let mut output = Vec::new();
    read_to_eof(&mut reader_out, &mut output);
    assert_eq!(exit_of(reader), 0);
    drop(reader_lease);
    // The holder still runs: the terminal is its own group's again.
    assert_terminal(holder);
    std::fs::write(gate("release-a"), b"go").unwrap();
    assert_eq!(exit_of(holder), 0);
    drop(holder_lease);
    // SAFETY: observe this owner's own controlling terminal and group.
    assert_eq!(
        unsafe { libc::tcgetpgrp(0) },
        unsafe { libc::getpgrp() },
        "the owner did not get its terminal back"
    );
    assert_eq!(output, b"b-read:queued-input");
    // All terminal assertions are complete. The parent now closes the PTY
    // to let macOS finish session-leader exit; its HUP must not race libtest's
    // successful return in this isolated helper process.
    // SAFETY: changes only this dedicated helper's teardown signal policy,
    // after its workloads were reaped and terminal restoration was proved.
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
    std::fs::write(home.join("owner-done"), b"yes").unwrap();
}

fn read_to_eof(pipe: &mut std::os::fd::OwnedFd, into: &mut Vec<u8>) {
    use std::os::fd::AsRawFd;
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        let mut buffer = [0u8; 256];
        // SAFETY: live nonblocking pipe and an initialized buffer.
        let count =
            unsafe { libc::read(pipe.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            into.extend_from_slice(&buffer[..count as usize]);
        } else if count == 0 {
            return;
        } else {
            let error = std::io::Error::last_os_error();
            assert!(
                matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ),
                "terminal proof pipe: {error}"
            );
        }
        assert!(Instant::now() < end, "workload output did not end");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The terminal's foreground group is `pid`'s own group, as the workload set it.
fn assert_terminal(pid: u32) {
    // SAFETY: observe the live child's group and this owner's terminal.
    let (foreground, group) = unsafe { (libc::tcgetpgrp(0), libc::getpgid(pid as i32)) };
    assert_eq!(group, pid as i32, "the workload does not lead its group");
    assert_eq!(foreground, group, "terminal group for workload {pid}");
}

fn exit_of(pid: u32) -> i32 {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        match spawn::reap(pid) {
            Ok(Some(exit)) => return exit.code,
            Ok(None) => {}
            Err(error) => panic!("reap {pid}: {error}"),
        }
        assert!(Instant::now() < end, "workload {pid} did not exit");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn redaction_covers_multiline_pty_newlines_without_exposing_split_prefixes() {
    let secret = b"pty-private-first\npty-private-last\n".to_vec();
    for input in [
        secret.clone(),
        b"pty-private-first\r\npty-private-last\r\n".to_vec(),
    ] {
        let mut redactor = Redactor::new(vec![secret.clone()]);
        let mut captured = Vec::new();
        for byte in input {
            captured.extend(redactor.push(&[byte], false));
        }
        captured.extend(redactor.push(&[], true));
        assert_eq!(captured, b"[REDACTED]");
    }
}

/// A deterministic stand-in for one secret-bearing custody child: the shipping
/// `OutputPipeline` and real capture logs, fed bytes in a chosen order. No
/// process runs, so every read boundary and stream boundary is exact.
struct FakeCapture {
    root: StateRoot,
    /// Kept alive for the state root above; dropped after it.
    _home: tempfile::TempDir,
    core: Arc<Mutex<ProcessCore>>,
}
impl FakeCapture {
    fn new(home: tempfile::TempDir, cx: &EffectContext, secrets: Vec<Vec<u8>>) -> Self {
        let root = StateRoot::effects(cx, home.path()).unwrap();
        let limits = cx.operation().limits();
        let logs = [Stream::Stdout, Stream::Stderr]
            .map(|stream| Some(capture::Log::open(&root, id(), 1, stream, limits).unwrap()));
        let pid = std::process::id();
        let receipt = ProcessReceipt {
            identity: crate::ident::process_identity(pid)
                .verified()
                .expect("own process identity")
                .clone(),
            pgid: pid,
            nonce: "fake-capture".into(),
            grace_millis: 20,
        };
        let core = ProcessCore {
            receipt,
            anchor: None,
            logs,
            output: VecDeque::new(),
            streams: [VecDeque::new(), VecDeque::new()],
            queue_max: 4096,
            stdout: None,
            stderr: None,
            stdin: None,
            pipeline: OutputPipeline::new(secrets),
            tail: VecDeque::new(),
            tail_max: 4096,
            capture: CaptureState::Complete,
            exit: None,
            reaped: false,
            deadline: None,
            terminal: None,
            pty: false,
            registration: None,
        };
        Self {
            root,
            _home: home,
            core: Arc::new(Mutex::new(core)),
        }
    }
    /// One read of `stream`: exactly the bytes a live read would hand `sink`.
    fn feed(&self, stream: Stream, bytes: &[u8]) {
        let index = usize::from(stream == Stream::Stderr);
        lock(&self.core).sink(index, bytes, false);
    }
    /// The merged stream's EOF, which is the only flush the pipeline takes.
    fn close(&self) {
        lock(&self.core).sink(0, &[], true);
    }
    fn tail(&self) -> Vec<u8> {
        lock(&self.core).tail.iter().copied().collect()
    }
    fn merged(&self) -> Vec<u8> {
        lock(&self.core).output.iter().copied().collect()
    }
    fn streams(&self) -> (Vec<u8>, Vec<u8>) {
        let core = lock(&self.core);
        (
            core.streams[0].iter().copied().collect(),
            core.streams[1].iter().copied().collect(),
        )
    }
    /// The capture state a stop report reads.
    fn capture(&self) -> CaptureState {
        lock(&self.core).capture.clone()
    }
    /// The bounded stderr excerpt a settle or doctor report quotes.
    fn excerpt(&self) -> String {
        diagnostic_tail(&self.core)
    }
    /// The stored per-stream log, read back the way a follower reads it.
    fn log(&self, cx: &EffectContext, stream: Stream) -> Vec<u8> {
        let observe = cx.operation().observe().unwrap();
        match crate::run::read_log(&observe, &self.root, id(), 1, stream, 0, 64 * 1024) {
            Observed::Verified(read, _) => read.bytes,
            other => panic!("{} log must be readable: {other:?}", stream.name()),
        }
    }
    /// Every sink a secret could reach, named for a failure message.
    fn sinks(&self, cx: &EffectContext) -> Vec<(String, Vec<u8>)> {
        let (stdout, stderr) = self.streams();
        let json = serde_json::json!({
            "stdout": String::from_utf8_lossy(&stdout),
            "stderr": String::from_utf8_lossy(&stderr),
            "tail": String::from_utf8_lossy(&self.tail()),
        })
        .to_string();
        vec![
            ("stdout log".into(), self.log(cx, Stream::Stdout)),
            ("stderr log".into(), self.log(cx, Stream::Stderr)),
            ("stdout queue".into(), stdout),
            ("stderr queue".into(), stderr),
            ("merged output".into(), self.merged()),
            ("merged tail".into(), self.tail()),
            ("error excerpt".into(), self.excerpt().into_bytes()),
            ("json".into(), json.into_bytes()),
        ]
    }
}

/// The bytes the managed TUI's `sanitize_log_text` drops *and joins across*.
/// The CR of a line ending is excluded: a reader drops it too, but the LF beside
/// it stays a separator, so nothing is joined there.
fn is_reader_removed(byte: u8) -> bool {
    matches!(
        byte,
        0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1a | 0x1c..=0x1f | 0x7f
    )
}

/// The reader's view of a sink: the filter the front end applies, CR included.
fn reader_view(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|byte| *byte != b'\r' && !is_reader_removed(*byte))
        .collect()
}

/// A failing proof names the sink and the pattern's length, never its bytes.
fn assert_absent(what: &str, bytes: &[u8], secret: &[u8]) {
    assert!(
        !bytes.windows(secret.len()).any(|window| window == secret),
        "{what} carries a known secret ({} bytes)",
        secret.len()
    );
    // A control a reader drops is a join it makes: a redaction replacement of
    // that shape rebuilds the very secret it removed.
    assert!(
        !bytes.iter().any(|byte| is_reader_removed(*byte)),
        "{what} still carries a control a reader drops"
    );
    // Well-formed text only: a renderer drops an invalid byte or a C1 control
    // (U+0080..=U+009F), which joins the runs either side of it.
    let text = std::str::from_utf8(bytes);
    assert!(
        text.is_ok(),
        "{what} still carries a byte no renderer prints"
    );
    assert!(
        !text
            .as_deref()
            .unwrap_or_default()
            .chars()
            .any(|character| ('\u{80}'..='\u{9f}').contains(&character)),
        "{what} still carries a C1 control"
    );
    assert!(
        !reader_view(bytes)
            .windows(secret.len())
            .any(|window| window == secret),
        "{what} carries a known secret once a reader drops its controls ({} bytes)",
        secret.len()
    );
    // A carriage return survives only as part of a line ending: a lone one
    // erases what a display covers or joins the runs either side of it.
    assert!(
        !bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n')),
        "{what} still carries a lone carriage return"
    );
}

/// One scripted child: merged arrival order is the order of these steps.
struct FakeWrite {
    name: &'static str,
    steps: Vec<(Stream, &'static [u8])>,
    eof: bool,
    /// Whether the whole known secret is present in the visible stream, so
    /// the sink must show the redaction marker.
    marker: bool,
}

const STD: Stream = Stream::Stdout;
const ERR: Stream = Stream::Stderr;

#[test]
fn redaction_sanitizes_controls_split_into_known_secrets_before_every_sink() {
    // The value a managed task hands its child: one token, no controls.
    const SECRET: &[u8] = b"custody-split-secret-value";
    let shapes = [
        // The counterexample: a front end's own parser reassembles these two
        // runs into one visible value, so a redactor over raw bytes misses it.
        FakeWrite {
            name: "csi",
            steps: vec![(STD, b"custody-split-\x1b[1msecret-value\n")],
            eof: true,
            marker: true,
        },
        FakeWrite {
            name: "osc-bel",
            steps: vec![(STD, b"custody-split-\x1b]0;title\x07secret-value\n")],
            eof: true,
            marker: true,
        },
        FakeWrite {
            name: "osc-st",
            steps: vec![(STD, b"custody-split-\x1b]0;title\x1b\\secret-value\n")],
            eof: true,
            marker: true,
        },
        // A read boundary inside the sequence.
        FakeWrite {
            name: "csi-across-reads",
            steps: vec![(STD, b"custody-split-\x1b[3"), (STD, b"1msecret-value\n")],
            eof: true,
            marker: true,
        },
        FakeWrite {
            name: "osc-across-reads",
            steps: vec![
                (STD, b"custody-\x1b]0;ti"),
                (STD, b"tle\x07split-secret-value\n"),
            ],
            eof: true,
            marker: true,
        },
        // A stream boundary inside the sequence: stdout opens it, stderr ends it.
        FakeWrite {
            name: "osc-across-streams",
            steps: vec![
                (STD, b"custody-split-\x1b]0;ti"),
                (ERR, b"tle\x07secret-value\n"),
            ],
            eof: true,
            marker: true,
        },
        FakeWrite {
            name: "csi-across-streams",
            steps: vec![(STD, b"custody-split-\x1b["), (ERR, b"0msecret-value\n")],
            eof: true,
            marker: true,
        },
        // Interleaved streams with a control between the halves.
        FakeWrite {
            name: "interleaved",
            steps: vec![
                (STD, b"custody-split"),
                (ERR, b"-\x1b[0m"),
                (STD, b"secret-value\n"),
            ],
            eof: true,
            marker: true,
        },
        // Malformed: the control aborts and its bytes are dropped, so the
        // halves still meet inside one redaction frame.
        FakeWrite {
            name: "malformed-csi",
            steps: vec![(STD, b"custody\x1b[\x01-split-secret-value\n")],
            eof: true,
            marker: true,
        },
        // A two-byte escape between the halves: the whole sequence is dropped.
        FakeWrite {
            name: "two-byte-escape",
            steps: vec![(STD, b"custody-split-\x1bs secret-value\n")],
            eof: true,
            // The escape eats one byte, so the two halves are not the value.
            marker: false,
        },
        // No control at all: a secret split across streams stays whole.
        FakeWrite {
            name: "split-streams",
            steps: vec![(STD, b"custody-split-"), (ERR, b"secret-value\n")],
            eof: true,
            marker: true,
        },
    ];
    let cx = cx();
    for shape in shapes {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in &shape.steps {
            capture.feed(*stream, bytes);
        }
        if shape.eof {
            capture.close();
        }
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&format!("{}: {what}", shape.name), &bytes, SECRET);
        }
        let tail = String::from_utf8_lossy(&capture.tail()).into_owned();
        assert_eq!(
            tail.contains("[REDACTED]"),
            shape.marker,
            "{}: marker expectation: {tail:?}",
            shape.name
        );
    }
}

#[test]
fn redaction_drops_an_unterminated_control_and_its_tail_at_eof() {
    const SECRET: &[u8] = b"custody-split-secret-value";
    let cx = cx();
    for shape in [
        FakeWrite {
            name: "unterminated-osc",
            steps: vec![
                (STD, b"custody-split-\x1b]0;title-sec"),
                (ERR, b"ret-value"),
            ],
            eof: true,
            marker: false,
        },
        FakeWrite {
            name: "unterminated-csi",
            steps: vec![(STD, b"custody-split-\x1b[1;2")],
            eof: true,
            marker: false,
        },
        FakeWrite {
            name: "unterminated-string",
            steps: vec![(STD, b"custody-split-\x1bP1;2secret-value")],
            eof: true,
            marker: false,
        },
        FakeWrite {
            name: "unterminated-at-a-read",
            // No EOF: a later read after the open sequence stays inside it.
            steps: vec![
                (STD, b"custody-split-\x1b]0;title"),
                (STD, b"secret-value\n"),
                (STD, b"later\n"),
            ],
            eof: false,
            marker: false,
        },
    ] {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in &shape.steps {
            capture.feed(*stream, bytes);
        }
        if shape.eof {
            capture.close();
        }
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&format!("{}: {what}", shape.name), &bytes, SECRET);
        }
        // Fail closed: the open control swallowed the rest, and the redactor
        // is still holding a possible prefix when the stream has not ended.
        let expected = if shape.eof { "custody-split-" } else { "" };
        assert_eq!(
            String::from_utf8_lossy(&capture.tail()),
            expected,
            "{}",
            shape.name
        );
    }
}

#[test]
fn redaction_keeps_terminal_colour_for_a_process_without_secrets() {
    let cx = cx();
    let capture = FakeCapture::new(home(), &cx, vec![]);
    let painted = b"\x1b[31mred\x1b[0m and \x1b]0;title\x07text\n";
    capture.feed(Stream::Stdout, painted);
    capture.close();
    assert_eq!(&capture.tail()[..], &painted[..]);
    assert_eq!(&capture.log(&cx, Stream::Stdout)[..], &painted[..]);
    assert_eq!(&capture.merged()[..], &painted[..]);
}

#[test]
fn redaction_covers_the_visible_form_of_a_secret_that_carries_a_control() {
    // A secret may itself hold a terminal control. Stripping the control is
    // what a reader shows, so that visible form is a pattern too: otherwise
    // sanitizing would hand the redactor a value no pattern matches.
    const SECRET: &[u8] = b"visible-\x1b[1mform";
    let cx = cx();
    for shape in [
        vec![(STD, SECRET)],
        // The same value with the control already removed.
        vec![(STD, &b"visible-form"[..])],
        // Split so the control falls in the middle of a read.
        vec![
            (STD, &b"visi"[..]),
            (STD, &b"ble-\x1b[1mfo"[..]),
            (STD, &b"rm"[..]),
        ],
    ] {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in shape {
            capture.feed(stream, bytes);
        }
        capture.close();
        assert_eq!(&capture.tail()[..], b"[REDACTED]");
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&what, &bytes, b"visible-form");
        }
    }
}

#[test]
fn redaction_redacts_a_projection_that_joins_a_secret_across_an_interleaved_stream() {
    // Arrival order never carries this value contiguously — the other stream's
    // visible bytes sit between the halves — so the merged redaction correctly
    // leaves it alone. Projecting one stream's bytes into its own log removes
    // the other stream's bytes, which would put the halves back together, so
    // each projection is redacted again over its own byte stream.
    const SECRET: &[u8] = b"custody-split-secret-value";
    let cx = cx();
    for steps in [
        // stdout holds both halves, stderr sits between them.
        vec![
            (STD, &b"custody-split-"[..]),
            (ERR, &b"noise"[..]),
            (STD, &b"secret-value\n"[..]),
        ],
        // Several interleaves, and the halves split further.
        vec![
            (STD, &b"custody-"[..]),
            (ERR, &b"x"[..]),
            (STD, &b"split-"[..]),
            (ERR, &b"\x1b[1my\n"[..]),
            (STD, &b"secret-value"[..]),
        ],
        // The other stream interleaves with its own newline-terminated line.
        vec![
            (STD, &b"prefix custody-split-"[..]),
            (ERR, &b"a whole stderr line\n"[..]),
            (STD, &b"secret-value suffix\n"[..]),
        ],
    ] {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in steps {
            capture.feed(stream, bytes);
        }
        capture.close();
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&what, &bytes, SECRET);
        }
        // The merged capture keeps arrival order, so it never held the value.
        assert!(!capture
            .tail()
            .windows(SECRET.len())
            .any(|window| window == SECRET));
        // The projected stream the halves landed in shows the marker instead.
        let (stdout, _) = capture.streams();
        assert!(
            String::from_utf8_lossy(&stdout).contains("[REDACTED]"),
            "the projection must be redacted on its own bytes"
        );
    }
}

#[test]
fn redaction_drops_a_lone_carriage_return_and_keeps_a_line_ending() {
    // A lone CR is a horizontal control: a display erases what it covers, and
    // a reader that strips it joins the runs either side, so a value split by
    // one is hidden from a redactor over the retained bytes. It is dropped
    // before redaction, which joins the halves in the frame a display could
    // show. The CR of a line ending stays, so a PTY's rows still return to
    // column 0.
    const SECRET: &[u8] = b"custody-split-secret-value";
    let cx = cx();
    for (name, steps, marker) in [
        // One read: a CR inside the value, then a CRLF line ending.
        (
            "lone-cr-one-read",
            vec![(STD, &b"custody-split-\rsecret-value\r\n"[..])],
            true,
        ),
        // The CR at a read boundary.
        (
            "lone-cr-across-reads",
            vec![
                (STD, &b"custody-split-\r"[..]),
                (STD, &b"secret-value\n"[..]),
            ],
            true,
        ),
        // The CR at a stream boundary.
        (
            "lone-cr-across-streams",
            vec![
                (STD, &b"custody-split-\r"[..]),
                (ERR, &b"secret-value\n"[..]),
            ],
            true,
        ),
        // A CR followed by a control byte in the next read: the byte after the
        // CR is still read as text.
        (
            "lone-cr-then-control",
            vec![
                (STD, &b"custody-split-\r"[..]),
                (STD, &b"\x1b[1msecret-value\n"[..]),
            ],
            true,
        ),
        // A CR that never reaches a line ending is dropped at EOF.
        (
            "lone-cr-at-eof",
            vec![(STD, &b"custody-split-\r"[..])],
            false,
        ),
    ] {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in steps {
            capture.feed(stream, bytes);
        }
        capture.close();
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&format!("{name}: {what}"), &bytes, SECRET);
        }
        let tail = String::from_utf8_lossy(&capture.tail()).into_owned();
        assert_eq!(
            tail.contains("[REDACTED]"),
            marker,
            "{name}: marker expectation: {tail:?}"
        );
        if name == "lone-cr-at-eof" {
            assert_eq!(tail, "custody-split-", "{name}");
        }
        if name == "lone-cr-one-read" {
            // The line ending itself is untouched, so a PTY row still returns
            // to column 0.
            assert!(tail.ends_with("\r\n"), "{name}: {tail:?}");
        }
    }
    // A CRLF that separates the halves is a line ending, not a hidden split:
    // it stays, and the two lines never become one value.
    let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
    capture.feed(Stream::Stdout, b"custody-split-\r\nsecret-value\r\n");
    capture.close();
    let tail = capture.tail();
    for (what, bytes) in capture.sinks(&cx) {
        assert_absent(&what, &bytes, SECRET);
    }
    assert!(
        tail.windows(2).any(|window| window == b"\r\n"),
        "a line ending keeps its carriage return"
    );
    // The hazard this closes: a reader that strips a lone CR instead of
    // applying it shows the value, so no sink may hold one.
    let pre_fix = b"custody-split-\rsecret-value\r\n";
    let stripped = pre_fix
        .iter()
        .copied()
        .filter(|byte| *byte != b'\r')
        .collect::<Vec<_>>();
    assert!(
        stripped
            .windows(SECRET.len())
            .any(|window| window == SECRET),
        "a CR-stripping reader reassembles the pre-fix shape"
    );
    // A secret that itself carries a CR is matched in the form a display
    // shows, which is the CR-free one.
    let capture = FakeCapture::new(home(), &cx, vec![b"cr-\rinside".to_vec()]);
    capture.feed(Stream::Stdout, b"cr-\rinside\n");
    capture.close();
    assert_eq!(&capture.tail()[..], b"[REDACTED]\n");
    for (what, bytes) in capture.sinks(&cx) {
        assert_absent(&what, &bytes, b"cr-inside");
    }
}

#[test]
fn redaction_replaces_a_reformed_secret_with_a_byte_no_reader_drops() {
    // A secret can contain the display marker, or a marker can join adjacent
    // bytes into another secret. The second streaming pass removes that
    // occurrence, and what it removes it with must survive a reader: a NUL is
    // dropped by the managed TUI, which joins the bytes either side of it and
    // rebuilds the secret this pass took out.
    let cx = cx();
    // A marker-substring secret, a secret that a marker joins, and a secret
    // set whose join only appears after replacement.
    type Case = (&'static str, Vec<Vec<u8>>, &'static [u8]);
    let cases: [Case; 3] = [
        ("marker substring", vec![b"R".to_vec()], b"R"),
        ("marker joins", vec![b"ab[".to_vec(), b"X".to_vec()], b"abX"),
        (
            "marker substring and join",
            vec![b"A".to_vec(), b"RED".to_vec(), b"[ACTED]".to_vec()],
            b"A",
        ),
    ];
    for (name, secrets, input) in cases {
        let capture = FakeCapture::new(home(), &cx, secrets.clone());
        capture.feed(Stream::Stdout, input);
        capture.close();
        let tail = capture.tail();
        for secret in &secrets {
            for (what, bytes) in capture.sinks(&cx) {
                assert_absent(&format!("{name}: {what}"), &bytes, secret);
            }
        }
        // What the pass removed is replaced by a printable byte, so the sink
        // holds nothing a reader drops, and the reader's view is the sink.
        assert!(
            tail.iter().any(|byte| (0x20..=0x7e).contains(byte)),
            "{name}: the replacement stays printable"
        );
        assert_eq!(reader_view(&tail), tail, "{name}");
    }
    // The chosen replacement is one byte value no secret contains, so it can
    // neither hold a secret nor be spanned by one.
    let secrets = vec![b"R".to_vec(), b"A".to_vec()];
    let mut redactor = Redactor::new(secrets.clone());
    let mut captured = redactor.push(b"RA", false);
    captured.extend(redactor.push(&[], true));
    assert!(
        captured
            .iter()
            .all(|byte| !secrets.iter().any(|secret| secret.contains(byte))),
        "no emitted byte can be part of a secret"
    );
}

#[test]
fn redaction_stops_capture_when_no_reader_safe_replacement_exists() {
    // Every replacement byte a reader keeps is a byte some secret contains, so
    // no replacement could be both readable and unjoinable: the capture stops
    // rather than emit one a reader would drop.
    let cx = cx();
    let secrets = (0x20..=0x7e)
        .map(|byte| vec![byte])
        .chain(std::iter::once(vec![b'\t']))
        .collect::<Vec<_>>();
    assert!(final_replacement(&secrets).is_none());
    let capture = FakeCapture::new(home(), &cx, secrets.clone());
    capture.feed(Stream::Stdout, b"[REDACTED] A R\n");
    capture.close();
    assert_eq!(&capture.tail()[..], b"");
    assert_eq!(&capture.merged()[..], b"");
    assert_eq!(capture.streams(), (Vec::new(), Vec::new()));
    assert!(matches!(
        capture.capture(),
        CaptureState::Truncated(ref reason) if reason.contains("reader-safe")
    ));
    // Every stored sink is empty, so the JSON envelope carries nothing either.
    for (what, bytes) in capture.sinks(&cx) {
        if what == "json" {
            continue;
        }
        assert!(bytes.is_empty(), "{what} must hold nothing when blocked");
    }
}

#[test]
fn redaction_drops_utf8_c1_controls_and_invalid_bytes_before_redaction() {
    // A renderer does not print a C1 control (U+0080..=U+009F) or an invalid
    // byte: it drops it and joins the runs either side, so a value split by one
    // is a split a redactor over the retained bytes never sees. They are
    // dropped before redaction, which joins the halves in the frame a display
    // could show, and what reaches a sink is well-formed text only.
    const SECRET: &[u8] = b"custody-split-secret-value";
    type Steps = Vec<(Stream, &'static [u8])>;
    type Shape = (&'static str, Steps);
    let cx = cx();
    let shapes: Vec<Shape> = vec![
        // U+009B CSI between the halves: the renderer drops it.
        (
            "c1-csi",
            vec![(STD, &b"custody-split-\xc2\x9bsecret-value\n"[..])],
        ),
        // U+009D OSC and U+009C ST, both dropped, so the halves meet.
        (
            "c1-osc",
            vec![(STD, &b"custody-split-\xc2\x9d\xc2\x9csecret-value\n"[..])],
        ),
        // U+0085 NEL and U+0080 padding.
        (
            "c1-nel",
            vec![(STD, &b"custody-split-\xc2\x85\xc2\x80secret-value\n"[..])],
        ),
        // Across reads, and across streams.
        (
            "c1-across-reads",
            vec![
                (STD, &b"custody-split-\xc2"[..]),
                (STD, &b"\x9bsecret-value\n"[..]),
            ],
        ),
        (
            "c1-across-streams",
            vec![
                (STD, &b"custody-split-\xc2\x9b"[..]),
                (ERR, &b"secret-value\n"[..]),
            ],
        ),
        // An invalid byte is dropped by the renderer too.
        (
            "invalid-byte",
            vec![(STD, &b"custody-split-\xffsecret-value\n"[..])],
        ),
        // A bare continuation byte, and an overlong lead.
        (
            "stray-continuation",
            vec![(STD, &b"custody-split-\x9bsecret-value\n"[..])],
        ),
        (
            "overlong-lead",
            vec![(STD, &b"custody-split-\xc0\x9bsecret-value\n"[..])],
        ),
        // A surrogate and a scalar beyond U+10FFFF.
        (
            "surrogate",
            vec![(STD, &b"custody-split-\xed\xa0\x80secret-value\n"[..])],
        ),
        (
            "beyond-unicode",
            vec![(STD, &b"custody-split-\xf4\x90\x80\x80secret-value\n"[..])],
        ),
    ];
    for (name, steps) in shapes {
        let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
        for (stream, bytes) in steps {
            capture.feed(stream, bytes);
        }
        capture.close();
        for (what, bytes) in capture.sinks(&cx) {
            assert_absent(&format!("{name}: {what}"), &bytes, SECRET);
        }
        let tail = String::from_utf8_lossy(&capture.tail()).into_owned();
        assert!(
            tail.contains("[REDACTED]"),
            "{name}: tail keeps the marker: {tail:?}"
        );
    }
    // A well-formed, control-free character survives untouched.
    let capture = FakeCapture::new(home(), &cx, vec![SECRET.to_vec()]);
    capture.feed(Stream::Stdout, "s\u{e9}par\u{e9} \u{1f642}\n".as_bytes());
    capture.close();
    assert_eq!(capture.tail(), "s\u{e9}par\u{e9} \u{1f642}\n".as_bytes());
    for (what, bytes) in capture.sinks(&cx) {
        assert_absent(&what, &bytes, SECRET);
    }
    // A secret that itself carries a C1 control is matched in the form a
    // display shows, which is the C1-free one.
    let capture = FakeCapture::new(home(), &cx, vec![b"c1-\xc2\x9binside".to_vec()]);
    capture.feed(Stream::Stdout, b"c1-\xc2\x9binside\n");
    capture.close();
    assert_eq!(&capture.tail()[..], b"[REDACTED]\n");
    for (what, bytes) in capture.sinks(&cx) {
        assert_absent(&what, &bytes, b"c1-inside");
    }
}
