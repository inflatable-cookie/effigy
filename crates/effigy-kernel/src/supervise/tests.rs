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

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod image;

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
        let dir = tempfile::Builder::new()
            .prefix("kh-")
            .tempdir_in(std::fs::canonicalize("/tmp").unwrap())
            .unwrap();
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
    /// Oracle: no helper log or private image is left in the state root.
    fn assert_no_residue(&self) {
        let root = self.home().join(".effigy/state");
        let residue: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with("helper-") || name.starts_with("exe-"))
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
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let exe = std::env::current_exe().unwrap();
            exe.parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("kernel-custody-fixture")
        })
}

fn helper_exited(error: RunError) -> (String, HelperExit, String) {
    match error {
        RunError::HelperExited {
            helper,
            exit,
            stderr_tail,
            ..
        } => (helper, exit, stderr_tail),
        other => panic!("expected a typed helper death, got {other:?}"),
    }
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
        let (helper, exit, stderr) = helper_exited(error);
        assert_eq!(helper, SENTINEL, "{body}");
        assert_eq!(exit, expected, "{body}");
        assert!(stderr.contains(tail), "{body}: {stderr:?}");
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
    let (helper, exit, stderr) = helper_exited(error);
    assert_eq!((helper.as_str(), exit), (SENTINEL, HelperExit::Code(5)));
    assert!(stderr.contains("child helper died"), "{stderr:?}");
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
    let (helper, exit, stderr) = helper_exited(error);
    assert_eq!((helper.as_str(), exit), (SUPERVISOR, HelperExit::Code(6)));
    assert!(stderr.contains("supervisor broke"), "{stderr:?}");
    world.assert_helper_reaped();
    world.assert_no_residue();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    world.assert_owner_free(&root, "detached", 2);
}

/// The real entry reports its own refusal on stderr before exiting, so the
/// parent's typed outcome names the cause (here: a replaced binary, I-SV4).
#[test]
fn sentinel_refusal_reaches_the_parent_stderr_tail() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let fixture = fixture();
    let executable = Executable::capture(&fixture).unwrap();
    // The config names a stale identity for the sentinel's own image, as a
    // binary replaced after the parent spawned it would: verify refuses.
    let mut stale = executable.clone();
    stale.changed.1 = stale.changed.1.wrapping_add(1);
    let config = SentinelConfig {
        home: world.home(),
        machine: false,
        machine_gates: None,
        owner: owner("refusal"),
        run: RunId::new(1, [1; 10]).unwrap(),
        next: vec![b"recover".to_vec()],
        executable: stale,
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
    let (_, exit, stderr) = helper_exited(error);
    assert_eq!(exit, HelperExit::Code(1));
    assert!(stderr.contains("startup binary replaced"), "{stderr:?}");
}

/// The effigy#239 cause: a rebuild replaces the startup path between this
/// process's verify and the sentinel's own (`cargo run` of Effigy beside a
/// live run). The first sentinel replaces its own path, as the rebuild would,
/// and refuses unacknowledged; admission retries once from the private image
/// of the unchanged startup image, which acknowledges. No private image remains.
#[test]
fn startup_path_replaced_during_handshake_retries_from_the_startup_image() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper(
        r#"case "$0" in
*/.effigy/state/exe-*|/dev/fd/*) printf '\001'; exec cat >/dev/null ;;
esac
cp "$0" "$0.rebuilt" && mv "$0.rebuilt" "$0"
echo 'sentinel refused: startup binary replaced' >&2
exit 1"#,
    );
    let mut run = Run::begin(
        &cx,
        &root,
        &inv("rebuilt"),
        opts(1, Some(plan.clone())),
        &cleanup(&cx),
        &mut Strict,
    )
    .expect("the retry from the startup image is admitted");
    assert!(run.has_sentinel());
    assert!(plan.executable.verify().is_err(), "the path was replaced");
    run.interrupt(&cleanup(&cx), crate::outcome::WorkloadOutcome::NotStarted)
        .unwrap();
    let settled = run.settle(&cleanup(&cx), &mut Strict).unwrap();
    assert_eq!(settled.cleanup(), crate::outcome::CleanupOutcome::Complete);
    world.assert_no_residue();
    world.assert_owner_free(&root, "rebuilt", 2);
}

/// A replaced path is retried once only: a startup image whose every run
/// refuses still ends in the typed outcome, with the second refusal's tail.
#[test]
fn a_refusing_startup_image_is_retried_once_then_typed() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper(
        r#"case "$0" in
*/.effigy/state/exe-*|/dev/fd/*) echo 'private image refused' >&2; exit 2 ;;
esac
cp "$0" "$0.rebuilt" && mv "$0.rebuilt" "$0"
exit 1"#,
    );
    let error = Run::begin(
        &cx,
        &root,
        &inv("refusing"),
        opts(1, Some(plan)),
        &cleanup(&cx),
        &mut Strict,
    )
    .expect_err("both attempts refused");
    let (_, exit, stderr) = helper_exited(error);
    assert_eq!(exit, HelperExit::Code(2));
    assert!(stderr.contains("private image refused"), "{stderr:?}");
    world.assert_closed_not_started(&root, "refusing", RunId::new(1, [1; 10]).unwrap());
    world.assert_no_residue();
    world.assert_owner_free(&root, "refusing", 2);
}

/// A replacement that lands before spawn is refused by this process's own
/// verify: typed `NotStarted` naming the refusal, never store I/O. The
/// admission retry treats it like a refusing sentinel (nothing ran).
#[test]
fn a_path_replaced_before_spawn_is_typed_not_started() {
    let world = World::new();
    let cx = cx();
    let root = StateRoot::effects(&cx, &world.home()).unwrap();
    let plan = world.helper("exit 0");
    let path = world.dir.path().join("bin/helper");
    let rebuilt = world.dir.path().join("bin/helper.rebuilt");
    std::fs::copy(&path, &rebuilt).unwrap();
    std::fs::rename(&rebuilt, &path).unwrap();
    let log = helper_log(&root).unwrap();
    let error =
        spawn_helper(cx.operation(), &plan.executable, SENTINEL, &log, None).expect_err("refused");
    match error {
        RunError::HelperExited {
            helper,
            exit,
            cause,
            ..
        } => {
            assert_eq!((helper.as_str(), exit), (SENTINEL, HelperExit::NotStarted));
            assert!(cause.contains("startup binary replaced"), "{cause}");
        }
        other => panic!("expected a typed refusal, got {other:?}"),
    }
    assert!(plan.executable.replaced_image().is_some());
    world.assert_no_residue();
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
        &crate::migration::CheckoutLocation {
            root: checkout.clone(),
            git_directory: None,
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
