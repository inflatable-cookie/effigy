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
    tempfile::Builder::new()
        .prefix("kernel-fault-")
        .tempdir_in(std::fs::canonicalize("/tmp").unwrap())
        .unwrap()
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
                executable: Executable::capture(Path::new(
                    &std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").unwrap_or_else(|| {
                        std::env::current_exe()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("kernel-custody-fixture")
                            .into_os_string()
                    }),
                ))
                .unwrap(),
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
            argv: vec!["/bin/sh".into(), "-c".into(), "trap '' HUP; /usr/bin/env -i /bin/sh -c 'trap \"\" HUP TERM; echo $$ > descendant; exec /bin/sleep 60' & while [ ! -f descendant ]; do :; done".into()],
            cwd: home.path().into(), env: BTreeMap::new(),
            grace: Duration::from_millis(10), lifetime: Lifetime::Finite(Duration::from_secs(60)), terminal, label: "anchor proof".into(),
        };
        let mut child = spawn::Blocked::fork(&spec, "anchor-proof", cx.operation()).unwrap();
        let anchor = crate::ident::process_identity(child.anchor.unwrap())
            .verified()
            .unwrap()
            .clone();
        let r = ProcessReceipt {
            identity: crate::ident::process_identity(child.pid)
                .verified()
                .unwrap()
                .clone(),
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
        let descendant = crate::ident::process_identity(pid)
            .verified()
            .unwrap()
            .clone();
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
        } else {
            assert!(
                matches!(result, Observed::Verified(..) | Observed::Absent(_)),
                "{result:?}"
            );
            assert!(!survived);
        }
    }
}

#[test]
fn anchor_receipt_is_additive_and_spawn_overhead_is_measured() {
    let home = home();
    let cx = cx();
    let spec = ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
        cwd: home.path().into(),
        env: BTreeMap::new(),
        grace: Duration::ZERO,
        lifetime: Lifetime::Finite(Duration::from_secs(1)),
        terminal: Terminal::None,
        label: "overhead".into(),
    };
    let mut totals = Vec::new();
    for anchored in [false, true] {
        let mut elapsed = Duration::ZERO;
        for _ in 0..12 {
            let started = Instant::now();
            let mut child = if anchored {
                spawn::Blocked::fork(&spec, "overhead", cx.operation())
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
                identity: crate::ident::process_identity(child.pid)
                    .verified()
                    .unwrap()
                    .clone(),
                pgid: child.pid,
                nonce: "overhead".into(),
                grace_millis: 0,
            };
            let anchor = child.anchor.map(|pid| {
                crate::ident::process_identity(pid)
                    .verified()
                    .unwrap()
                    .clone()
            });
            let mut fixture = AnchorFixture(vec![r.identity.clone()]);
            fixture.0.extend(anchor.clone());
            let old = serde_json::to_value(&r).unwrap();
            assert!(serde_json::from_value::<ProcessRecord>(old)
                .unwrap()
                .anchor
                .is_none());
            child.go().unwrap();
            child.executed(cx.operation()).unwrap();
            elapsed += started.elapsed();
            wait(|| spawn::reap(child.pid).unwrap().is_some());
            let cleanup = cx.operation().cleanup(Duration::from_secs(2)).unwrap();
            assert!(matches!(
                anchor::stop(&r, anchor.as_ref(), &cleanup),
                Observed::Verified(..) | Observed::Absent(_)
            ));
        }
        totals.push(elapsed.as_micros() / 12);
    }
    let result =
        serde_json::json!({"samples":12,"baseline_mean_us":totals[0],"anchored_mean_us":totals[1]});
    eprintln!("anchor spawn overhead: {result}");
    if let Some(path) = std::env::var_os("EFFIGY_KERNEL_ANCHOR_BENCH") {
        std::fs::write(path, result.to_string()).unwrap();
    }
}
