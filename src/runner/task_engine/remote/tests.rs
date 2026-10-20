//! Remote custody in the task engine against a strict container model: the
//! model's exec client prints the in-band registration and runs the workload
//! locally; its session closes only when the workload has ended or the
//! remote stop reached it. Real-engine qualification is a separate lane.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use effigy_kernel::ctx::{CleanupContext, EffectContext, OperationContext};
use effigy_kernel::custody::{ProcessSpec, Terminal};
use effigy_kernel::id::{CheckoutId, CheckoutKind, ResourceId};
use effigy_kernel::observe::Evidence;
use effigy_kernel::remote::{
    AdapterError, ContainerIdentity, Nonce, RemoteRegistration, SessionState,
};
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::Store;

use super::*;
use crate::runner::task_engine::{TaskRunRequest, TaskTarget};

const CONTAINER: &str = "model-container";
const BOOT: &str = "boot-model";

fn evidence() -> Evidence {
    Evidence::new("remote model".into(), None).expect("fixed evidence")
}

/// One modelled container session. Files under `dir` are the remote state:
/// `done` once the workload ended, `stop` once a remote TERM arrived.
#[derive(Clone)]
struct Model {
    dir: PathBuf,
}

impl Model {
    fn closed(&self) -> bool {
        self.dir.join("done").exists()
    }
}

impl RemoteExecAdapter for Model {
    fn identity(
        &mut self,
        target: &ContainerTarget,
        _: &OperationContext,
    ) -> Observed<ContainerIdentity> {
        assert_eq!(target.0, CONTAINER);
        Observed::Verified(
            ContainerIdentity {
                id: CONTAINER.into(),
                created: "created".into(),
                vm_boot_id: BOOT.into(),
            },
            evidence(),
        )
    }

    fn launch(
        &mut self,
        _: &ContainerIdentity,
        cmd: RemoteCommand,
        _: &EffectContext,
    ) -> Result<ProcessSpec, AdapterError> {
        assert_ne!(cmd.terminal, RemoteTerminal::None, "stdin reaches the workload");
        let mut env = client_env();
        for (key, value) in cmd.env {
            // The exec client and the guest have distinct custody nonces.
            let key = if key == "EFFIGY_CUSTODY_NONCE" {
                OsString::from("MODEL_NONCE")
            } else {
                key
            };
            env.insert(key, value);
        }
        env.insert("MODEL_DONE".into(), EnvValue::Plain(self.dir.join("done").into()));
        env.insert("MODEL_STOP".into(), EnvValue::Plain(self.dir.join("stop").into()));
        env.insert(
            "MODEL_WORKLOAD".into(),
            EnvValue::Plain(cmd.argv.last().cloned().unwrap_or_default()),
        );
        Ok(ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf '\\036EFFIGY-CUSTODY v1 %s 4242 4242 100 boot-model 401\\n' \"$MODEL_NONCE\" >&2; trap 'touch \"$MODEL_DONE\"' EXIT; eval \"$MODEL_WORKLOAD\"".into(),
            ],
            cwd: self.dir.clone(),
            env,
            grace: Duration::from_millis(200),
            lifetime: Lifetime::Finite(Duration::from_secs(20)),
            terminal: Terminal::None,
            label: "remote model".into(),
        })
    }

    fn signal_session(
        &mut self,
        _: &RemoteRegistration,
        signal: Signal,
        _: &CleanupContext,
    ) -> Observed<SessionState> {
        if signal != Signal::Continue {
            std::fs::write(self.dir.join("stop"), b"").expect("remote stop");
        }
        self.state()
    }

    fn observe_session(
        &mut self,
        _: &RemoteRegistration,
        _: &OperationContext,
    ) -> Observed<SessionState> {
        self.state()
    }

    fn find_by_nonce(
        &mut self,
        _: &ContainerIdentity,
        _: &Nonce,
        _: &OperationContext,
    ) -> Observed<Option<RemoteRegistration>> {
        Observed::Verified(None, evidence())
    }
}

impl Model {
    fn state(&self) -> Observed<SessionState> {
        Observed::Verified(
            if self.closed() {
                SessionState::Closed
            } else {
                SessionState::Running
            },
            evidence(),
        )
    }
}

static ENVIRONMENTS: AtomicUsize = AtomicUsize::new(1);

/// A fresh environment resource in the engine's test store.
fn environment() -> ResourceId {
    let index = ENVIRONMENTS.fetch_add(1, Ordering::Relaxed);
    let resource = ResourceId::environment(
        CheckoutId::new(CheckoutKind::EphemeralClone, [index as u8; 16]),
        &format!("model-{}", std::process::id()),
    )
    .expect("environment id");
    let cx = kernel::effects().child(Duration::from_secs(10));
    let root = StateRoot::effects(&cx, &kernel::test_state_home()).expect("root");
    let mut store = Store::open(&cx, &root).expect("store");
    store
        .txn(&cx, |txn| {
            txn.resource(&resource, "environment")?;
            txn.next_generation(&resource).map(|_| ())
        })
        .expect("environment resource");
    resource
}

/// Another environment run advances the generation (contract 053 §3).
fn advance(resource: &ResourceId) {
    let cx = kernel::effects().child(Duration::from_secs(10));
    let root = StateRoot::effects(&cx, &kernel::test_state_home()).expect("root");
    let mut store = Store::open(&cx, &root).expect("store");
    store
        .txn(&cx, |txn| txn.next_generation(resource).map(|_| ()))
        .expect("advance");
}

fn current(resource: &ResourceId) -> Result<Fence, RunnerError> {
    let cx = kernel::effects().child(Duration::from_secs(10));
    let root = StateRoot::effects(&cx, &kernel::test_state_home()).expect("root");
    let mut store = Store::open(&cx, &root).expect("store");
    let record = store
        .txn(&cx, |txn| txn.resource(resource, "environment"))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Fence {
        resource: resource.clone(),
        generation: record.generation,
    })
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    model: Model,
    env: ResourceId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().to_path_buf();
    let model = Model {
        dir: root.join("remote"),
    };
    std::fs::create_dir(&model.dir).expect("remote dir");
    let settle = model.clone();
    kernel::TEST_REMOTE.with(|factory| {
        *factory.borrow_mut() = Some(std::rc::Rc::new(move || Box::new(settle.clone())))
    });
    Fixture {
        _dir: dir,
        root,
        model,
        env: environment(),
    }
}

fn begin(root: &Path, task: &str) -> TaskRun {
    let target = TaskTarget {
        root: root.to_path_buf(),
        catalog: String::new(),
        task: task.to_owned(),
        surface: effigy_core::task_selection::TaskSurface::Published,
        profile: None,
        shared_lock: None,
        label: task.to_owned(),
    };
    TaskRun::begin(TaskRunRequest {
        target: &target,
        lock_wait_ms: 0,
        custody: true,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        cwd: None,
    })
    .expect("admitted")
}

fn task<'a>(root: &'a Path, workload: &str, capture: bool, timeout_ms: Option<u64>) -> RemoteTask<'a> {
    RemoteTask {
        repo_root: root,
        container_id: CONTAINER,
        argv: vec!["sh".into(), "-lc".into(), workload.into()],
        env: vec![
            ("APP_ENV".into(), EnvOverride::Plain("dev".into())),
            ("DB_PASSWORD".into(), EnvOverride::Secret("hunter2".into())),
        ],
        user: None,
        workdir: Some("/workspace".into()),
        capture,
        timeout_ms,
        label: "seed".into(),
    }
}

fn run(fixture: &Fixture, run: &mut TaskRun, task: RemoteTask<'_>) -> PlainExit {
    let mut model = fixture.model.clone();
    let env = fixture.env.clone();
    run_remote_with(
        run,
        task,
        &mut model,
        ContainerTarget(CONTAINER.into()),
        &mut || current(&env),
    )
    .expect("remote run")
}

fn close(run: TaskRun, workload: WorkloadOutcome) {
    let warnings = run.close(workload);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_container_step_completes_truthfully_and_its_session_settles() {
    let fixture = fixture();
    let mut task_run = begin(&fixture.root, "remote:success");
    let exit = run(
        &fixture,
        &mut task_run,
        task(
            &fixture.root,
            "printf '%s %s\\n' \"$APP_ENV\" \"$DB_PASSWORD\"; printf 'warn\\n' >&2",
            true,
            None,
        ),
    );
    assert_eq!(exit.workload, WorkloadOutcome::Succeeded);
    assert_eq!(exit.code, 0);
    // The frame never reaches output; secrets are redacted at the source.
    assert_eq!(exit.stdout, b"dev [REDACTED]\n");
    assert_eq!(exit.stderr, b"warn\n");
    assert!(fixture.model.closed());
    close(task_run, exit.workload);
}

#[test]
fn a_failing_container_step_records_its_exit_code() {
    let fixture = fixture();
    let mut task_run = begin(&fixture.root, "remote:failure");
    let exit = run(
        &fixture,
        &mut task_run,
        task(&fixture.root, "exit 3", true, None),
    );
    assert_eq!(exit.workload, WorkloadOutcome::Failed { code: 3 });
    assert_eq!(exit.code, 3);
    close(task_run, exit.workload);
}

/// Text relays as output arrives and JSON captures it; the run is the same
/// (K11): one engine, one outcome, one journal shape.
#[test]
fn text_and_json_run_a_container_step_the_same_way() {
    let mut outcomes = Vec::new();
    for capture in [false, true] {
        let fixture = fixture();
        let mut task_run = begin(&fixture.root, &format!("remote:parity:{capture}"));
        let exit = run(
            &fixture,
            &mut task_run,
            task(&fixture.root, "printf 'step\\n'; exit 4", capture, None),
        );
        assert_eq!(exit.stdout.is_empty(), !capture);
        outcomes.push((exit.workload.clone(), exit.code));
        close(task_run, exit.workload);
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[0], (WorkloadOutcome::Failed { code: 4 }, 4));
}

/// A concurrent bring-up of the same environment advances its generation
/// before the acknowledgement and again before completion. The step still
/// records its own exit (052 K7, §5.10).
#[test]
fn a_concurrent_environment_bring_up_leaves_the_outcome_truthful() {
    let fixture = fixture();
    let mut task_run = begin(&fixture.root, "remote:concurrent");
    let env = fixture.env.clone();
    let mut model = fixture.model.clone();
    let mut fenced = false;
    let exit = run_remote_with(
        &mut task_run,
        task(&fixture.root, "sleep 1; printf 'done\\n'", true, None),
        &mut model,
        ContainerTarget(CONTAINER.into()),
        &mut || {
            let fence = current(&env)?;
            if !fenced {
                fenced = true;
                // Between reading the fence and the intent: a clean refusal
                // the engine retries with the new generation.
                advance(&env);
                let bump = env.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    advance(&bump);
                });
            }
            Ok(fence)
        },
    )
    .expect("remote run");
    assert_eq!(exit.workload, WorkloadOutcome::Succeeded);
    assert_eq!(exit.stdout, b"done\n");
    close(task_run, exit.workload);
}

/// A timeout stops the remote session, not only the local client; the
/// run's settle observes the session closed.
#[test]
fn a_timeout_reaches_the_remote_session_and_settles_it() {
    let fixture = fixture();
    let mut task_run = begin(&fixture.root, "remote:timeout");
    let exit = run(
        &fixture,
        &mut task_run,
        task(
            &fixture.root,
            "while [ ! -f \"$MODEL_STOP\" ]; do sleep 0.05; done; exit 143",
            true,
            Some(300),
        ),
    );
    assert!(matches!(exit.workload, WorkloadOutcome::TimedOut { .. }));
    assert_eq!(exit.code, 124);
    assert!(fixture.model.dir.join("stop").exists());
    assert!(fixture.model.closed());
    close(task_run, exit.workload);
}
