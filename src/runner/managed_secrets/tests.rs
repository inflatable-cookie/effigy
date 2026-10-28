use super::*;
use effigy_test_support::process::FixtureCommandExt;
use std::io::Write;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::runner::task_engine::{TaskRunRequest, TaskTarget};
use effigy_kernel::custody::{Lifetime, ProcessSpec, SecretValue, Terminal};
use effigy_kernel::outcome::WorkloadOutcome;

const SECRET: &str = "managed-delivery-'quoted'\nsecret ending in a newline\n";
const HELPER: &str = "runner::managed_secrets::tests::secret_delivery_custody_helper";

fn run(root: &Path, custody: bool) -> TaskRun {
    let catalog =
        effigy_manifest::LoadedCatalog::for_test("", root, toml::from_str("").unwrap(), 0);
    let target = TaskTarget::for_catalog(root, "delivery", &catalog);
    TaskRun::begin_detached(TaskRunRequest {
        target: &target,
        lock_wait_ms: 0,
        custody,
        scratch: false,
        managed_run_token: None,
        args: &[],
        key: None,
        admission: None,
        cwd: Some(root),
    })
    .unwrap()
}

fn environment() -> (Vec<(String, EnvOverride)>, BTreeMap<OsString, EnvValue>) {
    let overrides = vec![
        ("DATABASE_URL".into(), EnvOverride::Secret(SECRET.into())),
        (NAMES.into(), EnvOverride::Plain("DATABASE_URL".into())),
    ];
    let env = BTreeMap::from([
        (
            "DATABASE_URL".into(),
            EnvValue::Secret(SecretValue::new(SECRET.as_bytes().to_vec()).unwrap()),
        ),
        (NAMES.into(), EnvValue::Plain("DATABASE_URL".into())),
        (
            "PATH".into(),
            EnvValue::Plain(std::env::var_os("PATH").unwrap()),
        ),
    ]);
    (overrides, env)
}

fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

// The fake VM deliberately clears host values before starting its shell.
// The fake runtime builds the container environment from -e NAME only.
const RUNTIME: &str = r#"#!/usr/bin/python3
import os, sys, json, pathlib, stat
root = pathlib.Path(os.environ['PROOF_ROOT'])
name = pathlib.Path(sys.argv[0]).name
a = sys.argv[1:]
with (root / 'argv.jsonl').open('a') as f: f.write(json.dumps([name] + a) + '\n')
if name == 'colima':
    assert a[:2] == ['ssh', '--profile']
    if os.environ.get('PROOF_FAIL') == '1': sys.exit(17)
    a = a[a.index('--')+1:]
    if '-c' in a:
        file = pathlib.Path(a[-1])
        (root / 'modes').write_text('%o %o' % (stat.S_IMODE(file.parent.stat().st_mode), stat.S_IMODE(file.stat().st_mode)))
    env = dict(os.environ)
    for key in list(env):
        if key == 'DATABASE_URL' or key.startswith('EFFIGY_INTERNAL_MANAGED_'): del env[key]
    os.execvpe(a[0], a, env)
if name == 'sudo':
    assert a[0] == '--preserve-env=DATABASE_URL'
    a = a[1:]
    os.execvpe(a[0], a, os.environ)
a = a[a.index('exec')+1:]
forward = {}
while a and a[0].startswith('-'):
    flag = a.pop(0)
    if flag == '-e':
        key = a.pop(0)
        assert '=' not in key, key
        forward[key] = os.environ[key]
    elif flag in ['-w', '-u']: a.pop(0)
    elif flag not in ['-T', '-i', '-t']: raise AssertionError(flag)
a.pop(0)
# First read removes the secret before any runtime or child is entered.
assert not list((root / '.effigy/managed-secrets').glob('*/env.*'))
forward['PATH'] = '/usr/bin:/bin'
os.execvpe(a[0], a, forward)
"#;

fn assert_argv_clean(root: &Path) {
    let captured = fs::read_to_string(root.join("argv.jsonl")).unwrap();
    let rows: Vec<Vec<String>> = captured
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(rows.iter().flatten().all(|arg| !arg.contains(SECRET)));
    assert!(rows
        .iter()
        .flatten()
        .all(|arg| !arg.contains("export DATABASE_URL=")));
    // Reversible break probe: poisoning an observed argv must trip the oracle.
    let clean = |args: &[String]| args.iter().all(|arg| !arg.contains(SECRET));
    let mut poisoned = rows.last().unwrap().clone();
    poisoned.push(format!("DATABASE_URL={SECRET}"));
    assert!(!clean(&poisoned));
    poisoned.pop();
    assert!(clean(&poisoned));
}

#[test]
fn managed_delivery_fake_runtime_matrix_keeps_host_guest_and_wrapper_argv_clean() {
    let _guard = crate::contract_test_support::lock_test();
    for colima in [false, true] {
        for compose in [false, true] {
            for occupied in [false, true] {
                let fixture =
                    effigy_test_support::Fixture::workspace("managed-delivery-matrix-").unwrap();
                let root = fixture.path().canonicalize().unwrap();
                let bin = root.join("bin");
                fs::create_dir(&bin).unwrap();
                for name in ["colima", "nerdctl", "sudo", "docker"] {
                    executable(&bin.join(name), RUNTIME);
                }
                let mut owner = run(&root, false);
                let (overrides, mut env) = environment();
                let pending =
                    prepare_in(&mut owner, &overrides, &mut env, occupied, Some(&root)).unwrap();
                if let Some(file) = &pending {
                    assert_eq!(
                        fs::metadata(file).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                    assert_eq!(
                        fs::metadata(file.parent().unwrap())
                            .unwrap()
                            .permissions()
                            .mode()
                            & 0o777,
                        0o700
                    );
                }
                let program = bin.join(if colima { "colima" } else { "docker" });
                let mut args: Vec<OsString> = if colima {
                    vec![
                        "nerdctl".into(),
                        "--profile".into(),
                        "fixture".into(),
                        "--".into(),
                    ]
                } else {
                    vec![]
                };
                if compose {
                    args.extend(["compose", "-p", "fixture"].map(OsString::from));
                }
                args.extend(
                    ["exec", "-e", "DATABASE_URL", "fixture", "/bin/sh", "-c"].map(OsString::from),
                );
                let command = if occupied {
                    "IFS= read -r input; printf '%s' \"$input\" > input; printf '%s' \"$DATABASE_URL\" > received; printf '%s' \"$DATABASE_URL\"; /bin/ps -ww -o args= -p $$ > child-argv"
                } else {
                    "printf '%s' \"$DATABASE_URL\" > received; printf '%s' \"$DATABASE_URL\"; /bin/ps -ww -o args= -p $$ > child-argv"
                };
                args.push(command.into());
                let _env = crate::contract_test_support::EnvGuard::set_many(&[
                    ("PATH", Some(format!("{}:/usr/bin:/bin", bin.display()))),
                    (
                        "EFFIGY_COMPOSE_BACKEND",
                        Some(if colima { "colima" } else { "docker" }.into()),
                    ),
                ]);
                let wrapper = if compose {
                    let mut policy = crate::runner::test_support::effective_container_policy(
                        "fixture",
                        "fixture",
                        "fixture",
                        root.join("compose.yaml"),
                    );
                    policy.repo_root = root.clone();
                    crate::runner::managed_shell::render_inline_managed_exec_command(
                        crate::runner::managed_shell::InlineManagedExecRequest {
                            repo_root: &root,
                            policy: &policy,
                            process_cwd: &root,
                            container_repo_root: None,
                            setup_command: None,
                            command,
                            exec_group: None,
                            env_names: &["DATABASE_URL".into()],
                        },
                    )
                } else {
                    relay(program.as_os_str(), &args, &["DATABASE_URL".into()])
                };
                assert!(!wrapper.contains(SECRET));
                assert!(!wrapper.contains(&root.join(ROOT).display().to_string()));
                // Spawn the actual relay under the same kernel custody as a
                // managed workload, including its secret preflight/redaction.
                env.insert(
                    "PROOF_ROOT".into(),
                    EnvValue::Plain(root.as_os_str().to_owned()),
                );
                env.insert(
                    "PATH".into(),
                    EnvValue::Plain(format!("{}:/usr/bin:/bin", bin.display()).into()),
                );
                let spec = ProcessSpec {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        format!("set -xv; {wrapper}").into(),
                    ],
                    cwd: root.clone(),
                    env,
                    grace: Duration::from_secs(1),
                    lifetime: Lifetime::Finite(Duration::from_secs(20)),
                    terminal: if occupied {
                        Terminal::Pty
                    } else {
                        Terminal::None
                    },
                    label: "delivery".into(),
                };
                let mut child = owner.spawn(spec, false).unwrap();
                if occupied {
                    child.write_input(b"terminal input\n").unwrap();
                }
                let deadline = Instant::now() + Duration::from_secs(20);
                let exit = loop {
                    if let Observed::Verified(exit, _) = child.poll() {
                        break exit;
                    }
                    assert!(Instant::now() < deadline, "relay did not exit");
                    std::thread::sleep(Duration::from_millis(20));
                };
                let tail = String::from_utf8_lossy(&child.tail()).into_owned();
                assert_eq!(exit.code, 0, "{tail}");
                owner.complete(&child, exit).unwrap();
                assert!(!tail.contains(SECRET));
                assert!(
                    tail.contains("[REDACTED]"),
                    "guest output must use custody redaction: {tail}"
                );
                assert!(!tail.contains("export DATABASE_URL="));
                assert_eq!(fs::read_to_string(root.join("received")).unwrap(), SECRET);
                assert!(!fs::read_to_string(root.join("child-argv"))
                    .unwrap()
                    .contains(SECRET));
                if occupied {
                    assert_eq!(
                        fs::read_to_string(root.join("input")).unwrap(),
                        "terminal input"
                    );
                }
                assert!(pending.as_ref().is_none_or(|file| !file.exists()));
                assert_argv_clean(&root);
                let warnings = owner.close(WorkloadOutcome::Succeeded);
                assert!(warnings.is_empty(), "{warnings:?}");
                let status =
                    crate::runner::task_engine::root_task_status_for_tests(&root, "delivery");
                assert_eq!(
                    status.history.len(),
                    1,
                    "the JSON proof must inspect this run"
                );
                let json = serde_json::to_string(&status).unwrap();
                assert!(!json.contains(SECRET));
                assert!(!json.contains(&root.join(ROOT).display().to_string()));
                assert!(!json.contains("export DATABASE_URL="));
                if root.join(ROOT).exists() {
                    assert_eq!(fs::read_dir(root.join(ROOT)).unwrap().count(), 0);
                }
            }
        }
    }
}

#[test]
fn managed_delivery_failed_launch_removes_only_its_file_and_settles_directory() {
    let fixture = effigy_test_support::Fixture::workspace("managed-delivery-failure-").unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let mut owner = run(&root, false);
    let (overrides, mut env) = environment();
    let file = prepare_in(&mut owner, &overrides, &mut env, true, Some(&root))
        .unwrap()
        .unwrap();
    let runtime = root.join("colima");
    executable(&runtime, "#!/bin/sh\nexit 17\n");
    let wrapper = relay(
        runtime.as_os_str(),
        &[
            "nerdctl".into(),
            "--profile".into(),
            "fixture".into(),
            "--".into(),
            "exec".into(),
            "fixture".into(),
        ],
        &["DATABASE_URL".into()],
    );
    let spec = ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), wrapper.into()],
        cwd: root.clone(),
        env,
        grace: Duration::from_secs(1),
        lifetime: Lifetime::Finite(Duration::from_secs(10)),
        terminal: Terminal::None,
        label: "failed delivery".into(),
    };
    let mut child = owner.spawn(spec, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Observed::Verified(exit, _) = child.poll() {
            break exit;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(exit.code, 17);
    owner.complete(&child, exit).unwrap();
    assert!(
        file.exists(),
        "a failed launch is not a read acknowledgement"
    );
    let warnings = owner.close(WorkloadOutcome::Failed { code: 17 });
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!file.exists());
    assert!(!file.parent().unwrap().exists());
}

#[test]
fn managed_delivery_custody_rejects_a_secret_in_argv_break_probe() {
    let fixture = effigy_test_support::Fixture::workspace("managed-delivery-break-").unwrap();
    let mut owner = run(fixture.path(), false);
    let (_, env) = environment();
    let spec = ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("printf '%s' '{SECRET}'").into(),
        ],
        cwd: fixture.path().into(),
        env,
        grace: Duration::from_secs(1),
        lifetime: Lifetime::Finite(Duration::from_secs(10)),
        terminal: Terminal::None,
        label: "break probe".into(),
    };
    assert!(owner.spawn(spec, false).is_err());
    assert!(owner.close(WorkloadOutcome::NotStarted).is_empty());
}

#[test]
fn secret_delivery_custody_helper() {
    let Some(root) = std::env::var_os("EFFIGY_DELIVERY_HELPER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut owner = run(&root, true);
    let (overrides, mut env) = environment();
    let file = prepare_in(&mut owner, &overrides, &mut env, true, Some(&root))
        .unwrap()
        .unwrap();
    fs::write(root.join("ready"), file.as_os_str().as_encoded_bytes()).unwrap();
    loop {
        if root.join("close").exists() {
            assert!(owner
                .close(WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::Requested,
                    signal: None
                })
                .is_empty());
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn managed_delivery_stop_kill_and_supervisor_loss_settle_the_exact_owned_file() {
    for kill in [false, true] {
        let fixture = effigy_test_support::Fixture::workspace("managed-delivery-custody-").unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", HELPER, "--nocapture"])
            .env("EFFIGY_DELIVERY_HELPER_ROOT", &root)
            .env("HOME", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn_owned()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !root.join("ready").exists() {
            if let Some(exit) = child.try_wait().unwrap() {
                panic!("helper exited {exit}");
            }
            assert!(Instant::now() < deadline, "helper did not prepare delivery");
            std::thread::sleep(Duration::from_millis(20));
        }
        let file = PathBuf::from(fs::read_to_string(root.join("ready")).unwrap());
        assert!(file.exists());
        if kill {
            child.kill().unwrap();
        } else {
            fs::write(root.join("close"), "").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        let exit = loop {
            if let Some(exit) = child.try_wait().unwrap() {
                break exit;
            }
            assert!(Instant::now() < deadline, "helper did not stop");
            std::thread::sleep(Duration::from_millis(20));
        };
        if kill {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(exit.signal(), Some(libc::SIGKILL));
        } else {
            assert!(exit.success(), "stop helper exited {exit}");
        }
        while file.parent().unwrap().exists() {
            assert!(
                Instant::now() < deadline,
                "sentinel did not settle secret delivery after owner loss"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!file.exists());
    }
}

fn directory(root: &Path, name: &str, owner: ProcessIdentity) -> DeliveryDirectory {
    let root = super::root(root).unwrap();
    let path = root.join(name);
    private_dir(&path).unwrap();
    let meta = fs::metadata(&path).unwrap();
    let record = DirectoryOwner {
        identity: owner.clone(),
        device: meta.dev(),
        inode: meta.ino(),
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path.join("owner"))
        .unwrap();
    file.write_all(&serde_json::to_vec(&record).unwrap())
        .unwrap();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path.join("env.delivery"))
        .unwrap();
    file.write_all(SECRET.as_bytes()).unwrap();
    DeliveryDirectory { path, owner }
}

#[test]
fn managed_delivery_stale_sweep_keeps_live_reused_unknown_and_foreign_ownership_separate() {
    let fixture = effigy_test_support::Fixture::workspace("managed-delivery-sweep-").unwrap();
    let root = fixture.path();
    let owner = ident::process_identity(std::process::id())
        .verified()
        .unwrap()
        .clone();
    let live = directory(root, "live", owner.clone());
    let mut dead = owner.clone();
    // Same PID, different start: a stale run is dead even after PID reuse.
    dead.start = effigy_kernel::ident::ProcessStartIdentity::LinuxTicks(0);
    let stale = directory(root, "stale", dead);
    let mut unknown = owner.clone();
    unknown.pid = 0; // invalid is Unknown, never permission to erase.
    let unknown = directory(root, "unknown", unknown);
    let foreign = directory(root, "foreign", owner.clone());
    let external = root.join("external");
    fs::write(&external, "untouched").unwrap();
    fs::remove_file(foreign.path.join("env.delivery")).unwrap();
    symlink(&external, foreign.path.join("env.delivery")).unwrap();
    symlink(&foreign.path, root.join(ROOT).join("foreign-link")).unwrap();
    sweep_in(root).unwrap();
    assert!(!stale.path.exists());
    assert!(live.path.join("env.delivery").exists());
    assert!(unknown.path.join("env.delivery").exists());
    assert!(remove(&foreign).is_err());
    assert_eq!(fs::read_to_string(external).unwrap(), "untouched");
    assert!(
        foreign.path.join("owner").exists(),
        "validate the entire directory before deleting anything"
    );
    remove(&live).unwrap();
    remove(&unknown).unwrap();
}

#[test]
fn managed_delivery_cleanup_refuses_a_replaced_directory_and_changed_owner() {
    let fixture = effigy_test_support::Fixture::workspace("managed-delivery-ownership-").unwrap();
    let root = fixture.path();
    let owner = ident::process_identity(std::process::id())
        .verified()
        .unwrap()
        .clone();
    let owned = directory(root, "owned", owner.clone());
    let original = fs::metadata(&owned.path).unwrap();
    let moved = owned.path.with_file_name("original");
    fs::rename(&owned.path, &moved).unwrap();
    let replacement = directory(root, "owned", owner);
    let mut kernel_owner = run(root, false);
    let target = kernel_owner.run_mut().unwrap().fence().clone();
    let effect = effigy_kernel::run::Effect {
        sequence: 0,
        spec: EffectSpec {
            target,
            kind: EFFECT.into(),
            payload: serde_json::to_value(&owned).unwrap(),
            settlement: Settlement::Release(serde_json::Value::Null),
            required: true,
        },
        state: effigy_kernel::run::EffectState::Applied,
        receipt: Some(serde_json::json!({"device": original.dev(), "inode": original.ino()})),
        reason: None,
    };
    assert!(kernel_owner.close(WorkloadOutcome::Succeeded).is_empty());
    assert!(release(&effect).is_err());
    assert!(replacement.path.join("env.delivery").exists());
    // An owner record copied into another inode is also refused by stale sweep.
    fs::copy(moved.join("owner"), replacement.path.join("owner")).unwrap();
    assert!(remove(&replacement).is_err());
    assert!(replacement.path.join("env.delivery").exists());
}
