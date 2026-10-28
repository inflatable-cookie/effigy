//! Helper death before acknowledgement (effigy#239). Each helper is a script
//! that fails one way; the oracles hold for every death: a typed outcome
//! with the helper's own exit and stderr, a truthful `Closed(NotStarted)`
//! admission, a free owner gate, a reaped helper and no state-root residue.
use super::*;
use crate::ctx::*;
use crate::gate::Wait;
use crate::id::*;
use crate::observe::Observed;
use crate::outcome::Invocation;
use crate::run::{history, BeginOpts, Effect, Fence, Phase, Reconciled, Run, RunError};
use crate::store::Store;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

fn cx() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(90)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        crate::ctx::Placement::Host,
    )
    .effects()
    .unwrap()
}
fn cleanup(cx: &EffectContext) -> CleanupContext {
    // Generous: a loaded host can spend seconds on admission alone.
    cx.operation().cleanup(Duration::from_secs(60)).unwrap()
}
fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
}
fn owner(name: &str) -> ResourceId {
    task(name).resource()
}
fn task(name: &str) -> TaskId {
    TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [6; 16]),
        "",
        Surface::Published,
        name,
        None,
    )
    .unwrap()
}
fn inv(name: &str) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        owner: owner(name),
        task: task(name),
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
fn opts(n: u8, custody: Option<LifecyclePlan>) -> BeginOpts {
    BeginOpts {
        id: RunId::new(1, [n; 10]).unwrap(),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody,
    }
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
/// A fresh `mktemp -d` holding the HOME and, beside it, the helper scripts.
struct World {
    dir: tempfile::TempDir,
}
impl World {
    fn new() -> Self {
        // A short path: the run's control socket path is bounded (`sun_path`),
        // and `[scratch]` puts the temp directory on a short volume path.
        let dir = effigy_test_support::short_tempdir("kh-").unwrap();
        std::fs::create_dir(dir.path().join("h")).unwrap();
        std::fs::create_dir(dir.path().join("bin")).unwrap();
        Self { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn pid_file(&self) -> PathBuf {
        self.dir.path().join("helper-pid")
    }
    /// A helper that ignores its entry argument and runs `body`.
    fn helper(&self, body: &str) -> LifecyclePlan {
        let path = self.dir.path().join("bin/helper");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho $$ > '{}'\n{body}\n",
                self.pid_file().display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        LifecyclePlan {
            executable: Executable::capture(&path).unwrap(),
            placement: super::Placement::Inline,
        }
    }
    /// Oracle: the helper process is gone, never left running or unreaped.
    fn assert_helper_reaped(&self) {
        let pid: i32 = std::fs::read_to_string(self.pid_file())
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // SAFETY: signal 0 only checks existence; our own reaped child is gone.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        assert!(!alive, "helper {pid} outlived its failed handshake");
    }
    /// Oracle: no helper log is left in the state root.
    fn assert_no_residue(&self) {
        let root = self.home().join(".effigy/state");
        let residue: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with("helper-"))
            .collect();
        assert!(residue.is_empty(), "state-root residue: {residue:?}");
    }
    /// Oracle: the admitted run closed truthfully with nothing started.
    fn assert_closed_not_started(&self, root: &StateRoot, name: &str, id: RunId) {
        let observe = OperationContext::new(
            Deadline::At(Instant::now() + Duration::from_secs(10)),
            CancelToken::default(),
            cx().operation().limits().clone(),
            EffectClass::Observe,
            crate::ctx::Placement::Host,
        )
        .observe()
        .unwrap();
        let Observed::Verified(reader, _) = Store::read(&observe, root) else {
            panic!("store unreadable");
        };
        let Observed::Verified(runs, _) = history(&reader, &owner(name), 10) else {
            panic!("history unreadable");
        };
        let run = runs.iter().find(|r| r.id == id).expect("admitted run row");
        assert_eq!(run.phase, Phase::Closed);
        assert_eq!(
            run.workload,
            Some(crate::outcome::WorkloadOutcome::NotStarted)
        );
        assert_eq!(run.cleanup_complete, Some(true));
    }
    /// Oracle: the owner gate is free; the next admission needs no repair.
    fn assert_owner_free(&self, root: &StateRoot, name: &str, n: u8) {
        let cx = cx();
        let run = Run::begin(
            &cx,
            root,
            &inv(name),
            opts(n, None),
            &cleanup(&cx),
            &mut Strict,
        )
        .expect("owner re-admits without repair");
        run.settle(&cleanup(&cx), &mut Strict).unwrap();
    }
}
fn fixture() -> PathBuf {
    crate::fixture::custody_fixture()
}

/// A helper death reads as unavailable and names the helper, how it ended and
/// the stderr tail it left.
fn helper_exited(error: RunError, helper: &str, exit: &HelperExit) {
    let RunError::Unavailable(detail) = error else {
        panic!("a helper death must read as unavailable, got {error:?}");
    };
    let expected = format!("kernel helper `{helper}` {exit} before acknowledging");
    assert!(detail.contains(&expected), "{detail:?}");
}

/// Every way a sentinel can die before acknowledging, during admission.
#[test]
fn helper_death_before_acknowledgement_during_admission_is_typed_and_settles() {
    for (body, expected, tail) in [
        (
            "echo 'sentinel refused: startup binary replaced' >&2; exit 3",
            HelperExit::Code(3),
            "startup binary replaced",
        ),
        ("head -c 4 >/dev/null; exit 4", HelperExit::Code(4), ""),
        ("kill -9 $$", HelperExit::Signal(libc::SIGKILL), ""),
        (
            "echo 'wedged' >&2; exec 1>&-; exec sleep 30",
            HelperExit::Stopped,
            "wedged",
        ),
    ] {
        let world = World::new();
        let cx = cx();
        let root = StateRoot::effects(&cx, &world.home()).unwrap();
        let plan = world.helper(body);
        let error = Run::begin(
            &cx,
            &root,
            &inv("admission"),
            opts(1, Some(plan)),
            &cleanup(&cx),
            &mut Strict,
        )
        .expect_err(body);
        let tails = error.to_string();
        helper_exited(error, SENTINEL, &expected);
        assert!(tails.contains(tail), "{body}: {tails:?}");
        world.assert_helper_reaped();
        world.assert_closed_not_started(&root, "admission", RunId::new(1, [1; 10]).unwrap());
        world.assert_no_residue();
        world.assert_owner_free(&root, "admission", 2);
    }
}

/// A child's sentinel dies: the child closes `NotStarted` and the parent,
/// whose admission is unaffected, still settles completely.
#[test]
fn helper_death_before_acknowledgement_during_child_admission_is_typed_and_settles() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let mut parent = Run::begin(
        &cx,
        &root,
        &inv("parent"),
        opts(1, None),
        &cleanup(&cx),
        &mut Strict,
    )
    .unwrap();
    let error = parent
        .begin_child(
            &inv("child"),
            opts(
                2,
                Some(world.helper("echo 'child helper died' >&2; exit 5")),
            ),
            &cleanup(&cx),
            &mut Strict,
        )
        .expect_err("child sentinel died");
    let detail = error.to_string();
    helper_exited(error, SENTINEL, &HelperExit::Code(5));
    assert!(detail.contains("child helper died"), "{detail:?}");
    world.assert_helper_reaped();
    world.assert_closed_not_started(&root, "child", RunId::new(1, [2; 10]).unwrap());
    let settled = parent.settle(&cleanup(&cx), &mut Strict).unwrap();
    assert_eq!(
        settled.cleanup(),
        crate::outcome::CleanupOutcome::Complete,
        "the parent settles its never-admitted child"
    );
    world.assert_no_residue();
    world.assert_owner_free(&root, "child", 3);
    world.assert_owner_free(&root, "parent", 4);
}

/// A detached supervisor that dies before `admitted` is the same typed
/// outcome, never a store error; it admitted nothing.
#[test]
fn supervisor_death_before_admission_is_typed() {
    let world = World::new();
    let cx = cx();
    let plan = world.helper("echo 'supervisor broke' >&2; exit 6");
    let error = start_detached(
        &cx,
        &world.home(),
        &inv("detached"),
        RunId::new(1, [1; 10]).unwrap(),
        &next(),
        vec![],
        &plan.executable,
        ClientMode::Owning,
    )
    .expect_err("supervisor died");
    let detail = error.to_string();
    helper_exited(error, SUPERVISOR, &HelperExit::Code(6));
    assert!(detail.contains("supervisor broke"), "{detail:?}");
    world.assert_helper_reaped();
    world.assert_no_residue();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    world.assert_owner_free(&root, "detached", 2);
}

/// A helper from a binary that speaks another protocol refuses the frame and
/// names both versions, so a binary replaced during a long run says what to do.
#[test]
fn a_helper_protocol_mismatch_reaches_the_parent_stderr_tail() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let executable = Executable::capture(&fixture()).unwrap();
    let other = HELPER_PROTOCOL + 1;
    let config = SentinelConfig {
        protocol: other,
        home: world.home(),
        machine: false,
        machine_gates: None,
        owner: owner("protocol"),
        run: RunId::new(1, [1; 10]).unwrap(),
        next: vec![b"recover".to_vec()],
        executable: executable.clone(),
    };
    let log = helper_log(&root).unwrap();
    let mut child = helper(&executable, SENTINEL)
        .unwrap()
        .stdout(Stdio::piped())
        .stderr(log.try_clone().unwrap())
        .spawn()
        .unwrap();
    let mut writer = child.stdin.take().unwrap();
    let mut ready = child.stdout.take().unwrap();
    let result = (|| {
        nonblocking(writer.as_raw_fd())?;
        write_frame(&mut writer, &config)?;
        nonblocking(ready.as_raw_fd())?;
        read_exact(&mut ready, &mut [0u8])
    })();
    let error = helper_failed(SENTINEL, &mut child, &log, result.expect_err("refused"));
    let detail = error.to_string();
    helper_exited(error, SENTINEL, &HelperExit::Code(1));
    let message = format!("this helper speaks protocol {other} and this binary speaks");
    assert!(detail.contains(&message), "{detail:?}");
}

/// The version is read before the frame's shape, so an update that changed a
/// field still reads as a version mismatch instead of as a broken store value
/// (F8).
#[test]
fn a_helper_frame_with_a_changed_shape_and_a_new_protocol_names_the_version() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let executable = Executable::capture(&fixture()).unwrap();
    let other = HELPER_PROTOCOL + 1;
    // A frame from a newer binary: the field this one requires is named
    // differently, so full deserialisation cannot read it.
    let frame = serde_json::json!({
        "protocol": other,
        "home": world.home(),
        "machine": false,
        "owner": serde_json::to_value(owner("reshaped")).unwrap(),
        "run": RunId::new(1, [1; 10]).unwrap().to_string(),
        "next_argv": ["recover"],
        "executable": serde_json::to_value(executable.clone()).unwrap(),
    });
    let log = helper_log(&root).unwrap();
    for entry in [SENTINEL, SUPERVISOR] {
        let mut child = helper(&executable, entry)
            .unwrap()
            .stdout(Stdio::piped())
            .stderr(log.try_clone().unwrap())
            .spawn()
            .unwrap();
        let mut writer = child.stdin.take().unwrap();
        let mut ready = child.stdout.take().unwrap();
        let result = (|| {
            nonblocking(writer.as_raw_fd())?;
            write_frame(&mut writer, &frame)?;
            nonblocking(ready.as_raw_fd())?;
            read_exact(&mut ready, &mut [0u8])
        })();
        let error = helper_failed(entry, &mut child, &log, result.expect_err("refused"));
        let detail = error.to_string();
        assert!(
            detail.contains(&format!("this helper speaks protocol {other}")),
            "{entry}: {detail:?}"
        );
        assert!(
            !detail.contains("not readable") && !detail.contains("Effigy is broken"),
            "{entry}: a version mismatch never reads as a broken value: {detail:?}"
        );
    }
}

/// A rebuild or reinstall replaces the binary at the path during a run. The
/// next helper execs whatever is there; with the same protocol it admits.
#[test]
fn a_binary_replaced_mid_run_still_admits_its_helper() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper("printf '\\001'; exec cat >/dev/null");
    std::fs::rename(&plan.executable.path, world.dir.path().join("old-helper")).unwrap();
    std::fs::write(
        &plan.executable.path,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nprintf '\\001'; exec cat >/dev/null\n",
            world.pid_file().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        &plan.executable.path,
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("rebuilt"),
        opts(1, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .expect("the replaced binary speaks the same protocol");
    assert!(run.has_sentinel());
    world.assert_no_residue();
    run.interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .unwrap();
    run.settle(&cleanup(&cx), &mut Strict).unwrap();
    world.assert_helper_reaped();
    world.assert_owner_free(&root, "rebuilt", 2);
}

#[test]
fn helper_restores_machine_resource_gate_domain_instead_of_a_checkout_shadow() {
    let world = World::new();
    let checkout = world.dir.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    let cx = cx();
    let root = StateRoot::checkout_effects(
        &cx,
        &world.home(),
        &crate::location::CheckoutLocation {
            root: checkout.clone(),
        },
    )
    .unwrap();
    let data = ResourceId::data("docker", "private", "shared").unwrap();
    let _held = crate::gate::Gate::acquire(&cx, &root, std::slice::from_ref(&data), Wait::FailFast)
        .unwrap();
    let helper = helper_root(&cx, &checkout, false, root.machine_gate_address()).unwrap();
    assert!(matches!(
        crate::gate::Gate::acquire(&cx, &helper, &[data], Wait::FailFast),
        Err(crate::gate::GateError::Busy { .. })
    ));
    assert!(!helper.path().join("gates/machine").exists());
    assert!(!world.home().join(".effigy/state/machine.db").exists());
}

/// 052 §8.11: the reservation a host dispatcher holds reaches a detached
/// engine supervisor in its private config frame, so the engine's own process
/// can admit under it.
#[test]
fn engine_supervisor_config_carries_the_workspace_admission() {
    let fixture = effigy_test_support::Fixture::new("effigy-engine-config-").unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let key = crate::run::AdmissionKey {
        session: crate::run::Session {
            run: RunId::new(1, [2; 10]).unwrap(),
            generation: Generation::new(3),
        },
        request: crate::run::RequestId::new([4; 16]),
    };
    let digest = Digest::of(b"effigy.supervise.admission.v1");
    let run = RunId::new(1, [5; 10]).unwrap();
    let ticket = match crate::run::reserve(&cx, &root, key, digest, run).unwrap() {
        crate::run::Reservation::Fresh(ticket) => ticket,
        other => panic!("expected the one reservation, saw {other:?}"),
    };
    let config = SupervisorConfig {
        protocol: HELPER_PROTOCOL,
        home: fixture.path().into(),
        machine: false,
        machine_gates: None,
        executable: Executable::current().unwrap(),
        work: Work::Engine {
            payload: b"engine payload".to_vec(),
            admission: Some(ticket.clone()),
        },
    };
    let frame = serde_json::to_vec(&config).unwrap();
    frame_protocol(&frame).expect("this binary reads its own config frame");
    let read: SupervisorConfig = serde_json::from_slice(&frame).unwrap();
    match read.work {
        Work::Engine { payload, admission } => {
            assert_eq!(payload, b"engine payload");
            assert_eq!(admission.as_ref(), Some(&ticket));
            assert_eq!(admission.map(|ticket| ticket.run()), Some(run));
        }
        _ => panic!("the config frame lost the engine work"),
    }
}
