//! Remote custody in the task engine against a strict container model: the
//! model's exec client prints the in-band registration and runs the workload
//! locally; its session closes only when the workload has ended or the
//! remote stop reached it. Real-engine qualification is a separate lane.

use std::path::{Path, PathBuf};

use effigy_kernel::ctx::{CleanupContext, EffectContext, OperationContext};
use effigy_kernel::custody::{ProcessSpec, Terminal};
use effigy_kernel::id::ResourceId;
use effigy_kernel::observe::Evidence;
use effigy_kernel::remote::{
    AdapterError, ContainerIdentity, Nonce, RemoteRegistration, SessionState,
};
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
        assert_ne!(
            cmd.terminal,
            RemoteTerminal::None,
            "stdin reaches the workload"
        );
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
        env.insert(
            "MODEL_DONE".into(),
            EnvValue::Plain(self.dir.join("done").into()),
        );
        env.insert(
            "MODEL_STOP".into(),
            EnvValue::Plain(self.dir.join("stop").into()),
        );
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

/// A fresh environment resource in the engine's test store.
fn environment(path: &Path) -> ResourceId {
    let root = kernel::effects_root(path).expect("checkout root");
    let resource = ResourceId::environment(
        super::super::checkout::mint(path).expect("checkout identity"),
        &format!("model-{}", std::process::id()),
    )
    .expect("environment id");
    let cx = kernel::effects().child(Duration::from_secs(10));
    let mut store = Store::open(&cx, root).expect("store");
    store
        .txn(&cx, |txn| {
            txn.resource(&resource, "environment")?;
            txn.next_generation(&resource).map(|_| ())
        })
        .expect("environment resource");
    resource
}

/// Another environment run advances the generation (contract 053 §3).
fn advance(path: &Path, resource: &ResourceId) {
    let cx = kernel::effects().child(Duration::from_secs(10));
    let root = kernel::effects_root(path).expect("checkout root");
    let mut store = Store::open(&cx, root).expect("store");
    store
        .txn(&cx, |txn| txn.next_generation(resource).map(|_| ()))
        .expect("advance");
}

fn current(path: &Path, resource: &ResourceId) -> Result<Fence, RunnerError> {
    let cx = kernel::effects().child(Duration::from_secs(10));
    let root = kernel::effects_root(path).expect("checkout root");
    let mut store = Store::open(&cx, root).expect("store");
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
        env: environment(&root),
        root,
        model,
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
        argument_scope: None,
        label: task.to_owned(),
    };
    TaskRun::begin(TaskRunRequest {
        target: &target,
        lock_wait_ms: 0,
        custody: true,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        cwd: None,
    })
    .expect("admitted")
}

fn task<'a>(
    root: &'a Path,
    workload: &str,
    capture: bool,
    timeout_ms: Option<u64>,
) -> RemoteTask<'a> {
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
        &mut || current(&fixture.root, &env),
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
            let fence = current(&fixture.root, &env)?;
            if !fenced {
                fenced = true;
                // Between reading the fence and the intent: a clean refusal
                // the engine retries with the new generation.
                advance(&fixture.root, &env);
                let bump = env.clone();
                let root = fixture.root.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    advance(&root, &bump);
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

/// A container modelled by real host processes. The exec client starts the
/// workload in a process group of its own, as an engine runs it apart from
/// its client, so a killed client leaves the "remote" workload running. The
/// session is that group; `boot` and `down` files model an engine restart
/// and an unreachable engine.
#[derive(Clone)]
struct Engine {
    dir: PathBuf,
}

impl Engine {
    fn pid(&self, name: &str) -> Option<i32> {
        std::fs::read_to_string(self.dir.join(name))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    fn wait_for(&self, name: &str) -> i32 {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(pid) = self.pid(name) {
                return pid;
            }
            assert!(std::time::Instant::now() < deadline, "{name} never started");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Whether any process of the workload's group survives.
    fn alive(&self) -> bool {
        // SAFETY: signal 0 only probes the recorded group; nothing is sent.
        self.pid("leader")
            .is_some_and(|leader| unsafe { libc::killpg(leader, 0) } == 0)
    }

    fn signal(&self, signal: i32) {
        if let Some(leader) = self.pid("leader") {
            // SAFETY: the group this model started and recorded.
            unsafe { libc::killpg(leader, signal) };
        }
    }

    fn boot(&self) -> String {
        std::fs::read_to_string(self.dir.join("boot")).unwrap_or_else(|_| BOOT.to_owned())
    }

    fn state(&self) -> Observed<SessionState> {
        if self.dir.join("down").exists() {
            return Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
                "engine unreachable".into(),
            ));
        }
        let boot = self.boot();
        if boot != BOOT {
            return Observed::Absent(effigy_kernel::observe::AbsenceProof::BootChanged {
                recorded: BOOT.into(),
                current: boot,
            });
        }
        Observed::Verified(
            if self.alive() {
                SessionState::Running
            } else {
                SessionState::Closed
            },
            evidence(),
        )
    }
}

impl RemoteExecAdapter for Engine {
    fn identity(
        &mut self,
        target: &ContainerTarget,
        cx: &OperationContext,
    ) -> Observed<ContainerIdentity> {
        Model {
            dir: self.dir.clone(),
        }
        .identity(target, cx)
    }

    fn launch(
        &mut self,
        _: &ContainerIdentity,
        cmd: RemoteCommand,
        _: &EffectContext,
    ) -> Result<ProcessSpec, AdapterError> {
        let mut env = client_env();
        for (key, value) in cmd.env {
            let key = if key == "EFFIGY_CUSTODY_NONCE" {
                OsString::from("MODEL_NONCE")
            } else {
                key
            };
            env.insert(key, value);
        }
        env.insert(
            "ENGINE_DIR".into(),
            EnvValue::Plain(self.dir.clone().into()),
        );
        // The engine runs the workload apart from its client: in a process
        // group of its own, holding none of the client's descriptors, reaped
        // by the engine (a thread here), never by the client.
        use std::os::unix::process::CommandExt;
        let mut workload = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "{}\necho $? > \"$ENGINE_DIR/exit\"",
                cmd.argv
                    .last()
                    .cloned()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .env("ENGINE_DIR", &self.dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|error| {
                AdapterError(effigy_kernel::observe::UnknownReason::Inconsistent(
                    error.to_string(),
                ))
            })?;
        std::fs::write(self.dir.join("leader"), workload.id().to_string())
            .expect("record the workload group");
        std::thread::spawn(move || {
            let _ = workload.wait();
        });
        Ok(ProcessSpec {
            // The exec client ends with the workload, with its status, or 137
            // when the engine killed it.
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                concat!(
                    "printf '\\036EFFIGY-CUSTODY v1 %s 4242 4242 100 boot-model 401\\n' \"$MODEL_NONCE\" >&2; ",
                    "echo $$ > \"$ENGINE_DIR/client\"; leader=$(cat \"$ENGINE_DIR/leader\"); ",
                    "while [ ! -f \"$ENGINE_DIR/exit\" ] && kill -0 \"$leader\" 2>/dev/null; do sleep 0.02; done; ",
                    "[ -f \"$ENGINE_DIR/exit\" ] && exit \"$(cat \"$ENGINE_DIR/exit\")\"; exit 137"
                )
                .into(),
            ],
            cwd: self.dir.clone(),
            env,
            grace: Duration::from_millis(200),
            lifetime: Lifetime::Finite(Duration::from_secs(60)),
            terminal: Terminal::None,
            label: "remote engine model".into(),
        })
    }

    fn signal_session(
        &mut self,
        _: &RemoteRegistration,
        signal: Signal,
        _: &CleanupContext,
    ) -> Observed<SessionState> {
        let state = self.state();
        if matches!(state, Observed::Verified(SessionState::Running, _)) {
            self.signal(match signal {
                Signal::Term => libc::SIGTERM,
                Signal::Continue => libc::SIGCONT,
                Signal::Kill => libc::SIGKILL,
            });
            std::thread::sleep(Duration::from_millis(50));
            return self.state();
        }
        state
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
        Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            "the matrix records every registration".into(),
        ))
    }
}

struct Matrix {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
    env: ResourceId,
}

fn matrix() -> Matrix {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().to_path_buf();
    let engine = Engine {
        dir: root.join("engine"),
    };
    std::fs::create_dir(&engine.dir).expect("engine dir");
    let settle = engine.clone();
    kernel::TEST_REMOTE.with(|factory| {
        *factory.borrow_mut() = Some(std::rc::Rc::new(move || Box::new(settle.clone())))
    });
    Matrix {
        _dir: dir,
        env: environment(&root),
        root,
        engine,
    }
}

impl Drop for Matrix {
    fn drop(&mut self) {
        // Never leave a workload behind, whatever the assertion outcome.
        self.engine.signal(libc::SIGKILL);
    }
}

/// Runs an endless step while `during` perturbs the engine from another thread.
fn step_under(
    matrix: &Matrix,
    task: &str,
    during: impl FnOnce(Engine) + Send + 'static,
) -> (TaskRun, PlainExit) {
    let mut task_run = begin(&matrix.root, task);
    let engine = matrix.engine.clone();
    let perturb = std::thread::spawn(move || {
        engine.wait_for("client");
        engine.wait_for("leader");
        during(engine);
    });
    let mut adapter = matrix.engine.clone();
    let env = matrix.env.clone();
    let exit = run_remote_with(
        &mut task_run,
        task_args(&matrix.root),
        &mut adapter,
        ContainerTarget(CONTAINER.into()),
        &mut || current(&matrix.root, &env),
    )
    .expect("remote run");
    perturb.join().expect("perturbation");
    (task_run, exit)
}

fn task_args(root: &Path) -> RemoteTask<'_> {
    task(root, "while :; do sleep 0.05; done", true, None)
}

/// Client death: the exec client is killed while the container session runs
/// on. The step is interrupted, not given the client's status, and the run's
/// settle stops the session. Nothing survives.
#[test]
fn matrix_client_death_interrupts_the_step_and_settle_stops_the_session() {
    let matrix = matrix();
    let (task_run, exit) = step_under(&matrix, "remote:matrix:client", |engine| {
        let client = engine.pid("client").expect("client pid");
        // SAFETY: the model's own client process, recorded above.
        unsafe { libc::kill(client, libc::SIGKILL) };
    });
    assert_eq!(exit.workload, WorkloadOutcome::Interrupted);
    assert!(matrix.engine.alive(), "the session outlives its client");
    close(task_run, exit.workload);
    assert!(!matrix.engine.alive(), "settle left an orphan");
}

/// Container stop: the engine kills the workload. The step records the
/// workload's own end, and settle observes the session closed.
#[test]
fn matrix_container_stop_records_the_workload_end_and_settles_closed() {
    let matrix = matrix();
    let (task_run, exit) = step_under(&matrix, "remote:matrix:stop", |engine| {
        engine.signal(libc::SIGKILL);
    });
    assert_eq!(exit.workload, WorkloadOutcome::Failed { code: 137 });
    assert_eq!(exit.code, 137);
    close(task_run, exit.workload);
    assert!(!matrix.engine.alive());
}

/// Engine restart: the VM reboots under the step. The new boot proves the
/// session absent, so settle releases it without signalling anything.
#[test]
fn matrix_engine_restart_proves_the_session_absent() {
    let matrix = matrix();
    let (task_run, exit) = step_under(&matrix, "remote:matrix:restart", |engine| {
        std::fs::write(engine.dir.join("boot"), "boot-restarted").expect("new boot");
        engine.signal(libc::SIGKILL);
    });
    assert_eq!(exit.workload, WorkloadOutcome::Failed { code: 137 });
    close(task_run, exit.workload);
    assert!(!matrix.engine.alive());
}

/// The engine becomes unreachable and the client dies: nothing is signalled
/// on an unknown observation (R3), the run closes with residue, and the next
/// run of the task settles it once the engine answers again (K5).
#[test]
fn matrix_unreachable_engine_leaves_residue_until_the_next_run_settles_it() {
    let matrix = matrix();
    let (task_run, exit) = step_under(&matrix, "remote:matrix:down", |engine| {
        std::fs::write(engine.dir.join("down"), b"").expect("engine down");
        let client = engine.pid("client").expect("client pid");
        // SAFETY: the model's own client process, recorded above.
        unsafe { libc::kill(client, libc::SIGKILL) };
    });
    assert_eq!(exit.workload, WorkloadOutcome::Interrupted);
    let warnings = task_run.close(exit.workload);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("cleanup is incomplete")),
        "{warnings:?}"
    );
    assert!(
        matrix.engine.alive(),
        "an unknown observation grants no kill"
    );
    std::fs::remove_file(matrix.engine.dir.join("down")).expect("engine back");
    let successor = begin(&matrix.root, "remote:matrix:down");
    assert!(!matrix.engine.alive(), "the predecessor's settle stops it");
    close(successor, WorkloadOutcome::Succeeded);
}

/// The host scratch never reaches a container: the container environment is
/// built from the run's spawned env entries and the task's own values only.
#[test]
fn a_container_workload_never_receives_the_host_scratch() {
    let scratch = tempfile::tempdir().expect("scratch");
    let dir = scratch.path().join("abcdefgh");
    std::fs::create_dir(&dir).expect("dir");
    crate::runner::scratch::with_test_scope(&dir, || {
        // The host side of this run owns a scratch directory ...
        assert!(!crate::runner::scratch::temp_env().is_empty());
        let env = container_env(vec![(
            "EXPLICIT".to_owned(),
            EnvOverride::Plain("1".to_owned()),
        )])
        .expect("container env");
        // ... and the container still sees none of it.
        for key in ["TMPDIR", "TMP", "TEMP"] {
            assert!(
                !env.contains_key(std::ffi::OsStr::new(key)),
                "`{key}` must not be forwarded into a container"
            );
        }
        assert!(env.contains_key(std::ffi::OsStr::new("EXPLICIT")));
    });
}

/// A container workload is a workload: no session vault access rides in its
/// spawn environment, while the values the run does declare and the user's own
/// environment still arrive (contract 032; effigy#338; effigy#445).
#[test]
fn a_container_workload_never_receives_session_vault_access() {
    let env = container_env(vec![
        (
            "DATABASE_URL".to_owned(),
            EnvOverride::Secret("postgres://user@db/app".to_owned()),
        ),
        ("APP_ENV".to_owned(), EnvOverride::Plain("dev".to_owned())),
        (
            "EFFIGY_SECRET_PASSPHRASE".to_owned(),
            EnvOverride::Plain("caller-passphrase".to_owned()),
        ),
        (
            "EFFIGY_INTERNAL_SECRET_PASSPHRASE".to_owned(),
            EnvOverride::Plain("retired-passphrase".to_owned()),
        ),
        (
            "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS".to_owned(),
            EnvOverride::Plain("1".to_owned()),
        ),
        (
            "EFFIGY_INTERNAL_SECRET_GRANT".to_owned(),
            EnvOverride::Plain("1".to_owned()),
        ),
        (
            "EFFIGY_TEST_SECRETS_PASSPHRASE".to_owned(),
            EnvOverride::Plain("test-passphrase".to_owned()),
        ),
    ])
    .expect("container env");
    for key in [
        "EFFIGY_SECRET_PASSPHRASE",
        "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
        "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS",
        "EFFIGY_INTERNAL_SECRET_GRANT",
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
    ] {
        assert!(
            !env.contains_key(std::ffi::OsStr::new(key)),
            "`{key}` must not be forwarded into a container"
        );
    }
    // The negative controls: the run's declared secret and the user's own
    // values are exactly what a container workload is supposed to receive.
    assert!(matches!(
        env.get(std::ffi::OsStr::new("DATABASE_URL")),
        Some(EnvValue::Secret(_))
    ));
    assert!(matches!(
        env.get(std::ffi::OsStr::new("APP_ENV")),
        Some(EnvValue::Plain(value)) if value == "dev"
    ));
}
