//! Owned fake-backend fixture. This exercises HostCli and the real task
//! engine; it is not a real-container or native qualification receipt.
use super::*;
use effigy_kernel::ident::{self, ProcessIdentity, ProcessState};
use effigy_kernel::remote::{qualification_owned_registration, QualificationSubject};
use effigy_kernel::run::Phase;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHILD: &str = "EFFIGY_SIGNAL_HOSTCLI_FIXTURE_CHILD";
const DISCOVERY: &str = "effigy-fixture-definitely-absent-backend";
const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn wait_file(path: &Path, deadline: Instant) -> String {
    loop {
        if let Ok(value) = fs::read_to_string(path) {
            if !value.is_empty() {
                return value;
            }
        }
        assert!(
            Instant::now() < deadline,
            "fixture did not publish {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The fixture retains its directory. Only the two exact, identity-checked
/// processes it started are eligible for emergency cleanup.
struct Owned {
    owner: Child,
    guest: Option<ProcessIdentity>,
    grandchild: Option<ProcessIdentity>,
    root: PathBuf,
    fixture: PathBuf,
    complete: bool,
}

fn recorded_guest(root: &Path, pid_key: &str, start_key: &str) -> Option<ProcessIdentity> {
    let complete = fs::read(root.join("guest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let (record, pid_key, start_key) = match complete {
        Some(record) => (record, pid_key, start_key),
        None => {
            let file = if pid_key == "pid" {
                "leader.json"
            } else {
                "grandchild.json"
            };
            let record = serde_json::from_slice(&fs::read(root.join(file)).ok()?).ok()?;
            (record, "pid", "host_start")
        }
    };
    let pid = u32::try_from(record[pid_key].as_u64()?).ok()?;
    let expected = record[start_key].as_str()?;
    let identity = ident::process_identity(pid).verified()?.clone();
    let matches = match &identity.start {
        ident::ProcessStartIdentity::LinuxTicks(ticks) => ticks.to_string() == expected,
        ident::ProcessStartIdentity::MacOs { .. } => {
            let output = Command::new("/bin/ps")
                .args(["-p", &pid.to_string(), "-o", "lstart="])
                .output()
                .ok()?;
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == expected
        }
    };
    matches.then_some(identity)
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        if matches!(self.owner.try_wait(), Ok(None)) {
            let _ = self.owner.kill();
            let _ = self.owner.wait();
        }
        let _ = effigy_test_support::process::wait_kernel_helpers_within(
            &self.root,
            Duration::from_secs(10),
        );
        if self.guest.is_none() {
            let until = Instant::now() + Duration::from_secs(1);
            while self.guest.is_none() && Instant::now() < until {
                self.guest = recorded_guest(&self.fixture, "pid", "host_start");
                if self.guest.is_none() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        if self.grandchild.is_none() {
            let until = Instant::now() + Duration::from_secs(1);
            while self.grandchild.is_none() && Instant::now() < until {
                self.grandchild =
                    recorded_guest(&self.fixture, "grandchild_pid", "grandchild_start");
                if self.grandchild.is_none() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        for guest in [self.guest.as_ref(), self.grandchild.as_ref()]
            .into_iter()
            .flatten()
        {
            if ident::process_identity(guest.pid).verified() == Some(guest) {
                // Only the fixture's freshly rechecked guest may be stopped.
                if let Ok(pid) = i32::try_from(guest.pid) {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                    let _ = fs::write(
                        self.fixture
                            .join(format!("emergency-cleanup-{}", guest.pid)),
                        format!("pid={} signal=9 exit=unknown\n", guest.pid),
                    );
                    let until = Instant::now() + Duration::from_secs(10);
                    while Instant::now() < until
                        && matches!(
                            ident::process_state(guest.pid),
                            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
                        )
                    {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }
        let _ = effigy_test_support::process::wait_kernel_helpers_within(
            &self.root,
            Duration::from_secs(10),
        );
    }
}

fn fake_snapshot(backend: &Path, nonce: &str) -> String {
    let helper = fs::read_to_string(
        backend
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("expected-helper"),
    )
    .unwrap();
    let mut child = Command::new(backend)
        .args([
            "--context",
            "fixture",
            "exec",
            "-i",
            "-u",
            "0",
            ID,
            "sh",
            "-c",
            &helper,
            "effigy-custody",
            "snap",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{nonce}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "fake snapshot failed: {:?}",
        output.status
    );
    String::from_utf8(output.stdout).unwrap()
}

fn packet_consumer(
    root: &Path,
    python: &Path,
    registration: &RemoteRegistration,
    owner: &ProcessIdentity,
    snapshot: &str,
    expected: bool,
) {
    fs::write(
        root.join("common.py"),
        include_str!("../../../../../docs/qualification/task-engine-signal-matrix/common.py"),
    )
    .unwrap();
    fs::write(
        root.join("execute.py"),
        include_str!("../../../../../docs/qualification/task-engine-signal-matrix/execute.py"),
    )
    .unwrap();
    let script = r#"import json,sys
sys.path.insert(0,sys.argv[1])
import common,execute
value=json.load(sys.stdin)
result={'kind':'verified','code':0,'stdout':json.dumps(value['binding']).encode()}
binding=execute.registration_from_observer(result,value['identity'],'fixture-created','fixture-boot',401,value['pid'])
snap=common.observe_snapshot({'kind':'verified','code':0,'stdout':value['snapshot'].encode()},nonce_supplied=True)
matched=binding is not None and execute.matches_registration_snapshot(binding[0],snap,'fixture-boot')
sys.exit(0 if matched == value['expected'] else 1)
"#;
    let mut child = Command::new(python)
        .args(["-I", "-B", "-c", script])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = serde_json::json!({"binding": {"registration": registration, "owner": owner},
        "identity": format!("docker-compose:{ID}@fixture"), "pid": owner.pid,
        "snapshot": snapshot, "expected": expected});
    serde_json::to_writer(child.stdin.take().unwrap(), &input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "packet consumer refused fixture: {:?}",
        output.status
    );
}

#[test]
fn cleared_environment_discovery_child() {
    if std::env::var_os("PATH").is_some() {
        return;
    }
    assert!(std::env::var_os("TMPDIR").is_none());
    // The resolver returning an unresolved name is not evidence of a usable
    // backend. This deliberately never starts a real runtime.
    assert_eq!(
        effigy_containers::compose::resolve_host_cli_program(DISCOVERY),
        std::ffi::OsString::from(DISCOVERY)
    );
}

#[test]
fn recovery_discovery_without_environment_does_not_invent_a_backend() {
    let scratch = std::env::var_os("TMPDIR").expect("run TMPDIR");
    let root = tempfile::Builder::new()
        .prefix("signal-discovery-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, rest)| rest);
    let child_name = format!("{module}::cleared_environment_discovery_child");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name])
        .current_dir(&root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cleared-environment discovery fixture failed"
    );
}

#[test]
fn hostcli_fixture_child() {
    let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) else {
        return;
    };
    let router =
        effigy_kernel::signal::SignalRouter::install(effigy_kernel::signal::RouterPolicy::Inline)
            .expect("owned fixture signal router");
    kernel::adopt_signal_router(router);
    let repo = root.join("repo");
    let resource = environment(&repo);
    let target = TaskTarget {
        root: repo.clone(),
        catalog: String::new(),
        task: "hold".into(),
        surface: effigy_core::task_selection::TaskSurface::Published,
        profile: None,
        shared_lock: None,
        argument_scope: None,
        label: "hold".into(),
    };
    let key = crate::runner::task_engine::RunKey::parse("owned-hostcli-fixture").unwrap();
    let mut run = TaskRun::begin(TaskRunRequest {
        target: &target,
        lock_wait_ms: 0,
        custody: true,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: Some(&key),
        admission: None,
        workspace_admission: None,
        cwd: None,
    })
    .unwrap();
    fs::write(root.join("run-id"), run.id().to_string()).unwrap();
    let task = task(&repo, "sleep 300", true, None);
    let mut adapter =
        ComposeRemote::<effigy_containers::environment::HostCli>::launcher(exec_options(&task));
    let target = ContainerTarget(format!("docker-compose:{ID}@fixture"));
    let exit = run_remote_with(&mut run, task, &mut adapter, target, &mut || {
        current(&repo, &resource)
    })
    .unwrap();
    close(run, exit.workload);
}

fn hostcli_signal_case(signal: i32, changed_backend: bool) {
    let scratch = std::env::var_os("TMPDIR").expect("run TMPDIR");
    let root = tempfile::Builder::new()
        .prefix("signal-hostcli-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let repo = root.join("repo");
    let home = root.join("home");
    fs::create_dir(&repo).unwrap();
    fs::create_dir(&home).unwrap();
    let kernel_source = include_str!("../../../../../crates/effigy-kernel/src/remote.rs");
    let container_source =
        include_str!("../../../../../crates/effigy-containers/src/environment/remote.rs");
    let constant = |source: &'static str, marker: &str| -> &'static str {
        source
            .split_once(marker)
            .expect("fixture constant")
            .1
            .split_once("\"#;")
            .expect("fixture constant end")
            .0
    };
    fs::write(
        root.join("expected-wrapper"),
        constant(kernel_source, "const WRAPPER: &str = r#\""),
    )
    .unwrap();
    fs::write(
        root.join("expected-register"),
        constant(kernel_source, "const REGISTER: &str = r#\""),
    )
    .unwrap();
    fs::write(
        root.join("expected-helper"),
        constant(container_source, "pub(crate) const HELPER: &str = r#\""),
    )
    .unwrap();
    let marker = root.join("signal-fixture-owned");
    fs::write(&marker, root.file_name().unwrap().as_encoded_bytes()).unwrap();
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("python3"))
        .find(|path| path.is_file())
        .expect("fixture python3");
    let backend = bin.join("docker");
    fs::write(
        &backend,
        format!(
            "#!{}\n{}",
            python.display(),
            include_str!("../hostcli_fake_backend.py")
        ),
    )
    .unwrap();
    fs::set_permissions(&backend, fs::Permissions::from_mode(0o700)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    // Keep the fixture owner and every re-exec on one privately owned image.
    let candidate = root.join("candidate-test");
    fs::copy(std::env::current_exe().unwrap(), &candidate).unwrap();
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o500)).unwrap();
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, rest)| rest);
    let child_name = format!("{module}::hostcli_fixture_child");
    let child = Command::new(&candidate)
        .args(["--exact", &child_name, "--nocapture"])
        .current_dir(&root)
        .env(CHILD, &root)
        .env("HOME", &home)
        .env("TMPDIR", &root)
        .env("PATH", path)
        .env("EFFIGY_CUSTODY_NONCE", "packet-injected-not-kernel")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut owned = Owned {
        owner: child,
        guest: None,
        grandchild: None,
        root: repo.clone(),
        fixture: root.clone(),
        complete: false,
    };
    let owner_pid = owned.owner.id();
    let deadline = Instant::now() + Duration::from_secs(60);
    let run: effigy_kernel::id::RunId = wait_file(&root.join("run-id"), deadline).parse().unwrap();
    let guest: serde_json::Value =
        serde_json::from_str(&wait_file(&root.join("guest.json"), deadline)).unwrap();
    let guest_pid = u32::try_from(guest["pid"].as_u64().unwrap()).unwrap();
    let grandchild_pid = u32::try_from(guest["grandchild_pid"].as_u64().unwrap()).unwrap();
    owned.guest = ident::process_identity(guest_pid).verified().cloned();
    owned.grandchild = ident::process_identity(grandchild_pid).verified().cloned();
    assert!(owned.guest.is_some(), "guest identity must be observable");
    assert!(
        owned.grandchild.is_some(),
        "grandchild identity must be observable"
    );
    let owner = ident::process_identity(owner_pid)
        .verified()
        .unwrap()
        .clone();
    let observe = kernel::observe();
    let state = kernel::observe_root(&observe, &repo);
    let state = state.verified().unwrap();
    let dirs = (
        state.owning_directory().identity().unwrap(),
        state.state_directory().identity().unwrap(),
    );
    let registration = loop {
        let observe = kernel::observe();
        let reader = Store::read(&observe, state);
        let reader = reader.verified().unwrap();
        let summary = effigy_kernel::run::summary(reader, run);
        let summary = summary.verified().unwrap();
        let task = &summary.invocation.as_ref().unwrap().spec().task;
        let effects = effigy_kernel::run::effects(reader, run);
        let effects = effects.verified().unwrap();
        if let Some(effect) = effects
            .iter()
            .find(|effect| effect.spec.kind == "kernel:remote-process")
        {
            let container = ContainerIdentity {
                id: format!("docker-compose:{ID}@fixture"),
                created: "fixture-created".into(),
                vm_boot_id: "fixture-boot".into(),
            };
            let subject = QualificationSubject {
                run,
                task,
                owner: &owner,
                env: &effect.spec.target,
                container: &container,
            };
            if let Ok(registration) =
                qualification_owned_registration(&observe, state, dirs, &subject)
            {
                break registration;
            }
        }
        assert!(
            Instant::now() < deadline,
            "actual registration did not arrive"
        );
        assert!(
            owned.owner.try_wait().unwrap().is_none(),
            "owner exited before registration"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        registration.nonce.as_str(),
        guest["nonce"].as_str().unwrap()
    );
    assert_ne!(registration.nonce.as_str(), "packet-injected-not-kernel");
    assert_eq!(registration.leader_pid, guest_pid);
    {
        let observe = kernel::observe();
        let reader = Store::read(&observe, state);
        let reader = reader.verified().unwrap();
        let summary = effigy_kernel::run::summary(reader, run);
        let summary = summary.verified().unwrap();
        let task = &summary.invocation.as_ref().unwrap().spec().task;
        let effects = effigy_kernel::run::effects(reader, run);
        let effects = effects.verified().unwrap();
        let effect = effects
            .iter()
            .find(|effect| effect.spec.kind == "kernel:remote-process")
            .unwrap();
        let container = registration.container.clone();
        let subject = QualificationSubject {
            run,
            task,
            owner: &owner,
            env: &effect.spec.target,
            container: &container,
        };
        let wrong_run = QualificationSubject {
            run: effigy_kernel::id::RunId::new(1, [7; 10]).unwrap(),
            ..subject
        };
        assert!(qualification_owned_registration(&observe, state, dirs, &wrong_run).is_err());
        let mut stale = effect.spec.target.clone();
        stale.generation = effigy_kernel::id::Generation::new(stale.generation.get() + 1);
        let wrong_fence = QualificationSubject {
            env: &stale,
            ..subject
        };
        assert!(qualification_owned_registration(&observe, state, dirs, &wrong_fence).is_err());
        let mut foreign = container.clone();
        foreign.id = format!("docker-compose:{}@fixture", "b".repeat(64));
        let wrong_container = QualificationSubject {
            container: &foreign,
            ..subject
        };
        assert!(qualification_owned_registration(&observe, state, dirs, &wrong_container).is_err());
        let mut moved = dirs;
        moved.0.inode = moved.0.inode.wrapping_add(1);
        assert!(qualification_owned_registration(&observe, state, moved, &subject).is_err());
        let mut other_owner = owner.clone();
        other_owner.pid = other_owner.pid.saturating_add(1);
        let wrong_owner = QualificationSubject {
            owner: &other_owner,
            ..subject
        };
        assert!(qualification_owned_registration(&observe, state, dirs, &wrong_owner).is_err());
        let mut changed_boot = container.clone();
        changed_boot.vm_boot_id = "other-boot".into();
        let wrong_boot = QualificationSubject {
            container: &changed_boot,
            ..subject
        };
        assert!(qualification_owned_registration(&observe, state, dirs, &wrong_boot).is_err());
        let mut wrong_effects = effects.to_vec();
        let remote = wrong_effects
            .iter_mut()
            .find(|effect| effect.spec.kind == "kernel:remote-process")
            .unwrap();
        remote.receipt = Some(serde_json::json!({"nonce": "foreign"}));
        assert!(effigy_kernel::remote::qualification_registration(
            &wrong_effects,
            &effect.spec.target,
            &container
        )
        .is_err());
    }
    let snapshot = fake_snapshot(&backend, registration.nonce.as_str());
    packet_consumer(&root, &python, &registration, &owner, &snapshot, true);
    let wrong_nonce = if registration.nonce.as_str() == "f".repeat(64) {
        "e".repeat(64)
    } else {
        "f".repeat(64)
    };
    let wrong_snapshot = fake_snapshot(&backend, &wrong_nonce);
    packet_consumer(
        &root,
        &python,
        &registration,
        &owner,
        &wrong_snapshot,
        false,
    );
    let moved_snapshot = snapshot.replace(" 100 y\n", " 101 y\n");
    assert_ne!(snapshot, moved_snapshot);
    packet_consumer(
        &root,
        &python,
        &registration,
        &owner,
        &moved_snapshot,
        false,
    );
    assert_eq!(
        ident::process_identity(guest_pid).verified(),
        owned.guest.as_ref()
    );
    assert_eq!(
        ident::process_identity(grandchild_pid).verified(),
        owned.grandchild.as_ref()
    );
    assert_eq!(
        wait_file(&root.join("sentinel-environment"), deadline),
        "cleared"
    );
    let prior_calls = fs::read_to_string(root.join("backend-argv.jsonl"))
        .unwrap()
        .lines()
        .count();
    if changed_backend {
        writeln!(
            fs::OpenOptions::new().append(true).open(&backend).unwrap(),
            "\n# replacement"
        )
        .unwrap();
    }
    assert_eq!(ident::process_identity(owner_pid).verified(), Some(&owner));
    // The direct child's start identity was rechecked immediately above.
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(i32::try_from(owner_pid).unwrap()),
        nix::sys::signal::Signal::try_from(signal).unwrap(),
    )
    .unwrap();
    let settle_deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = owned.owner.try_wait().unwrap() {
            break exit;
        }
        assert!(
            Instant::now() < settle_deadline,
            "owner did not exit in signal window"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    fs::write(
        root.join("owner-exit"),
        format!("signal={:?}\n", exit.signal()),
    )
    .unwrap();
    if signal == libc::SIGKILL {
        assert_eq!(exit.signal(), Some(libc::SIGKILL));
    } else {
        assert!(exit.success(), "routed fixture owner failed");
    }
    if changed_backend {
        loop {
            let observe = kernel::observe();
            let reader = Store::read(&observe, state);
            let summary = reader
                .verified()
                .and_then(|r| effigy_kernel::run::summary(r, run).verified().cloned());
            if summary
                .as_ref()
                .is_some_and(|s| s.phase == Phase::Closed && s.cleanup_complete == Some(false))
            {
                break;
            }
            assert!(
                Instant::now() < settle_deadline,
                "changed backend did not leave truthful residue"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            fs::read_to_string(root.join("backend-argv.jsonl"))
                .unwrap()
                .lines()
                .count(),
            prior_calls,
            "changed backend must not be executed during recovery"
        );
        assert!(matches!(
            ident::process_state(guest_pid),
            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
        ));
        assert!(matches!(
            ident::process_state(grandchild_pid),
            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
        ));
        effigy_test_support::process::wait_kernel_helpers_within(&repo, Duration::from_secs(10))
            .unwrap();
        drop(owned);
        assert!(matches!(
            ident::process_state(guest_pid),
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_)
        ));
        assert!(matches!(
            ident::process_state(grandchild_pid),
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_)
        ));
        return;
    }
    loop {
        let observe = kernel::observe();
        let reader = Store::read(&observe, state);
        let summary = reader
            .verified()
            .and_then(|r| effigy_kernel::run::summary(r, run).verified().cloned());
        if let Some(summary) = summary
            .as_ref()
            .filter(|s| s.phase == Phase::Closed && s.cleanup_complete == Some(true))
        {
            if root.join("guest-exit").is_file() && root.join("grandchild-exit").is_file() {
                if signal == libc::SIGKILL {
                    assert_eq!(summary.workload, Some(WorkloadOutcome::Interrupted));
                } else {
                    assert!(
                        matches!(&summary.workload, Some(WorkloadOutcome::Cancelled { signal: Some(actual), .. }) if *actual == signal)
                    );
                }
                break;
            }
        }
        assert!(
            Instant::now() < settle_deadline,
            "retained settlement did not finish within signal window"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        fs::read_to_string(root.join("guest-exit")).unwrap(),
        "exit=0\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("grandchild-exit")).unwrap(),
        "exit=0\n"
    );
    let argv_log = fs::read_to_string(root.join("backend-argv.jsonl")).unwrap();
    assert!(
        !argv_log.contains(registration.nonce.as_str()),
        "nonce leaked into backend argv"
    );
    for line in argv_log.lines() {
        let argv: Vec<String> = serde_json::from_str(line).unwrap();
        assert_eq!(argv.first().map(String::as_str), Some("--context"));
        assert_eq!(argv.get(1).map(String::as_str), Some("fixture"));
        assert!(matches!(
            argv.get(2).map(String::as_str),
            Some("inspect" | "exec")
        ));
    }
    if signal == libc::SIGKILL {
        let journal = fs::read_to_string(root.join("backend-argv.jsonl")).unwrap();
        let calls: Vec<Vec<String>> = journal
            .lines()
            .skip(prior_calls)
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(
            calls
                .iter()
                .filter(|argv| argv.get(11).map(String::as_str) == Some("snap"))
                .count()
                >= 2,
            "settlement must re-observe the delayed guest"
        );
        assert!(calls
            .iter()
            .any(|argv| argv.get(11).map(String::as_str) == Some("kill")));
    }
    assert!(
        matches!(
            ident::process_state(guest_pid),
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_)
        ),
        "guest closure was not positively observed"
    );
    assert!(
        matches!(
            ident::process_state(grandchild_pid),
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_)
        ),
        "grandchild closure was not positively observed"
    );
    effigy_test_support::process::wait_kernel_helpers_within(&repo, Duration::from_secs(10))
        .unwrap();
    owned.complete = true;
}

#[test]
fn hostcli_registration_and_signals_settle_through_real_owner_and_sentinel() {
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGKILL] {
        hostcli_signal_case(signal, false);
    }
}

#[test]
fn hostcli_changed_backend_refuses_owner_death_recovery() {
    hostcli_signal_case(libc::SIGKILL, true);
}
