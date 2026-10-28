use super::*;
use effigy_kernel::ctx::{CancelToken, Deadline, EffectClass, Limits, Placement};
use effigy_kernel::custody::SecretValue;
use std::cell::RefCell;
use std::rc::Rc;

const ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const NONCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The names the caller's session sources have in the exec client's own
/// environment, and the values they carry there. A relay forwards the names it
/// is given and nothing else, so none of these may cross into the guest
/// environment or its argv (contract 032; effigy#338; effigy#445).
const SESSION_SOURCE_NAMES: [&str; 5] = [
    "EFFIGY_SECRET_PASSPHRASE",
    "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
    "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS",
    "EFFIGY_INTERNAL_SECRET_GRANT",
    "EFFIGY_TEST_SECRETS_PASSPHRASE",
];
const SESSION_SOURCE_VALUES: [&str; 3] = [
    "relay-must-not-forward-this",
    "retired-passphrase",
    "test-passphrase",
];

/// What the guest process actually received: its whole environment, minus the
/// base variables the model and the guest shell itself provide (the model's
/// `PATH`/`MODEL_PRESERVED` stand in for the runtime's base environment, and a
/// POSIX shell exports `PWD`/`SHLVL`/`OLDPWD`/`_` of its own), plus its argv.
/// Every other name the guest sees is a name a relay delivered, so the caller
/// can assert the delivered set exactly.
const MODEL_BASE_ENV: [&str; 6] = [
    "PATH=",
    "MODEL_PRESERVED=",
    "PWD=",
    "SHLVL=",
    "OLDPWD=",
    "_=",
];

fn delivered_guest_surface(output: &str) -> (Vec<String>, Vec<String>) {
    let mut env = Vec::new();
    let mut argv = Vec::new();
    let mut inside = false;
    for line in output.lines() {
        match line {
            "env-begin" => inside = true,
            "env-end" => inside = false,
            line if line.starts_with("argv:") => argv.push(line["argv:".len()..].to_owned()),
            line if inside => {
                if !MODEL_BASE_ENV.iter().any(|base| line.starts_with(base)) {
                    env.push(line.to_owned());
                }
            }
            _ => {}
        }
    }
    env.sort();
    (env, argv)
}

fn operation(budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 1024,
            queue_bytes: 8192,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
}

fn selection() -> BackendSelection {
    BackendSelection {
        kind: super::super::backend::docker_kind(),
        engine: BackendId::docker_compose(),
        context: "ctx".into(),
        reason: super::super::BackendReason::UserPreference,
    }
}

fn colima() -> BackendSelection {
    BackendSelection {
        kind: BackendKind::Colima,
        engine: BackendId::colima_nerdctl(),
        context: "effigy".into(),
        reason: super::super::BackendReason::UserPreference,
    }
}

fn container() -> RemoteContainer {
    RemoteContainer::new(&selection(), "ctx".into(), ID)
}

fn identity() -> ContainerIdentity {
    ContainerIdentity {
        id: container().encode(),
        created: "2026-10-20T10:00:00.000000001Z".into(),
        vm_boot_id: "boot-a".into(),
    }
}

/// One modelled guest process.
#[derive(Debug, Clone)]
struct Guest {
    pid: u32,
    ppid: u32,
    pgrp: u32,
    sid: u32,
    start: u64,
    nonce: Option<String>,
    /// Ignores TERM; only KILL ends it.
    stubborn: bool,
    unreadable: bool,
}

/// A strict model of one container behind the backend CLI. It answers the
/// exact invocations the adapter makes and applies delivered signals to its
/// process table. Anything else fails the test.
#[derive(Debug)]
struct Engine {
    exists: bool,
    running: bool,
    created: String,
    boot: String,
    ns: u64,
    guests: Vec<Guest>,
    hung: bool,
    denied: bool,
    signals: Vec<(u32, String)>,
    calls: Vec<Vec<String>>,
}

impl Engine {
    fn new() -> Self {
        Self {
            exists: true,
            running: true,
            created: identity().created,
            boot: "boot-a".into(),
            ns: 401,
            guests: Vec::new(),
            hung: false,
            denied: false,
            signals: Vec::new(),
            calls: Vec::new(),
        }
    }

    fn workload(mut self) -> Self {
        // The isolated leader and a child it started, both under the nonce.
        for (pid, ppid) in [(42, 0), (43, 42)] {
            self.guests.push(Guest {
                pid,
                ppid,
                pgrp: 42,
                sid: 42,
                start: 100 + u64::from(pid - 42),
                nonce: Some(NONCE.into()),
                stubborn: false,
                unreadable: false,
            });
        }
        self
    }

    fn foreign(mut self, sid: u32) -> Self {
        self.guests.push(Guest {
            pid: 90,
            ppid: 0,
            pgrp: 90,
            sid,
            start: 500,
            nonce: None,
            stubborn: false,
            unreadable: false,
        });
        self
    }

    fn snap(&self, nonce: &str) -> String {
        let mut out = format!("boot {}\nns pid:[{}]\n", self.boot, self.ns);
        for guest in &self.guests {
            let carries = if guest.unreadable {
                "?"
            } else if guest.nonce.as_deref() == Some(nonce) {
                "y"
            } else {
                "n"
            };
            out.push_str(&format!(
                "p {} S {} {} {} {} {}\n",
                guest.pid, guest.ppid, guest.pgrp, guest.sid, guest.start, carries
            ));
        }
        out
    }

    fn signal(&mut self, pid: u32, sig: &str) {
        self.signals.push((pid, sig.to_owned()));
        if let Some(index) = self.guests.iter().position(|guest| guest.pid == pid) {
            if sig == "KILL" || (sig == "TERM" && !self.guests[index].stubborn) {
                self.guests.remove(index);
            }
        }
    }

    fn kill(&mut self, args: &[String], nonce: &str) -> String {
        if args[0] != self.boot || args[1] != format!("pid:[{}]", self.ns) {
            return "moved\n".into();
        }
        let sig = args[2].clone();
        let number = |index: usize| args[index].parse::<u64>().unwrap();
        if args[3] == "session" {
            let (sid, leader, start) = (number(4) as u32, number(5) as u32, number(6));
            let live = self
                .guests
                .iter()
                .any(|guest| guest.pid == leader && guest.start == start && guest.sid == sid);
            if !live {
                return "gone\n".into();
            }
            let members = self
                .guests
                .iter()
                .filter(|guest| guest.sid == sid)
                .map(|guest| guest.pid)
                .collect::<Vec<_>>();
            for pid in members {
                self.signal(pid, &sig);
            }
            "sent\n".into()
        } else {
            let (pid, start) = (number(4) as u32, number(5));
            let live = self.guests.iter().any(|guest| {
                guest.pid == pid && guest.start == start && guest.nonce.as_deref() == Some(nonce)
            });
            if !live {
                return "gone\n".into();
            }
            self.signal(pid, &sig);
            "sent\n".into()
        }
    }
}

#[derive(Clone)]
struct ModelCli(Rc<RefCell<Engine>>);

impl BackendCli for ModelCli {
    fn run(
        &mut self,
        program: &str,
        args: &[OsString],
        input: &[u8],
        deadline: Instant,
    ) -> Result<CliOutput, String> {
        assert!(Instant::now() < deadline, "adapter call past its deadline");
        let args = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let mut engine = self.0.borrow_mut();
        engine.calls.push(args.clone());
        if engine.hung {
            return Err("[effigy] command timed out".into());
        }
        assert_eq!(program, "docker");
        assert_eq!(&args[..2], ["--context", "ctx"], "exact backend context");
        let ok = |stdout: String| CliOutput {
            code: Some(0),
            stdout: stdout.into_bytes(),
            stderr: Vec::new(),
        };
        match args[2].as_str() {
            "inspect" => {
                assert_eq!(args[3], ID);
                if !engine.exists {
                    return Ok(CliOutput {
                        code: Some(1),
                        stdout: Vec::new(),
                        stderr: b"Error: No such object".to_vec(),
                    });
                }
                Ok(ok(format!(
                    r#"[{{"Id":"{ID}","Created":"{}","State":{{"Running":{}}},"Config":{{"Env":["SECRET=never-read"]}}}}]"#,
                    engine.created, engine.running
                )))
            }
            "ps" => Ok(ok(if engine.exists {
                format!("{ID}\n")
            } else {
                "ffff\n".into()
            })),
            "exec" => {
                assert_eq!(&args[3..8], ["-i", "-u", "0", ID, "sh"]);
                assert_eq!(args[9], HELPER);
                assert!(engine.running, "exec into a stopped container");
                if engine.denied {
                    return Ok(CliOutput {
                        code: Some(3),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                }
                let nonce = String::from_utf8_lossy(input).trim_end().to_owned();
                // The nonce never travels in argv.
                assert!(nonce.is_empty() || !args.iter().any(|arg| arg.contains(&nonce)));
                let rest = args[11..].to_vec();
                Ok(ok(match args[11].as_str() {
                    "boot" => format!("{}\n", engine.boot),
                    "snap" => engine.snap(&nonce),
                    "kill" => engine.kill(&rest[1..], &nonce),
                    other => panic!("unmodelled helper mode {other}"),
                }))
            }
            other => panic!("unmodelled backend call {other}"),
        }
    }
}

fn adapter(engine: Engine) -> (ComposeRemote<ModelCli>, Rc<RefCell<Engine>>) {
    let engine = Rc::new(RefCell::new(engine));
    (
        ComposeRemote::with_cli(ModelCli(engine.clone()), None),
        engine,
    )
}

fn registration() -> RemoteRegistration {
    RemoteRegistration {
        env: Fence {
            resource: ResourceId::environment(
                effigy_kernel::id::CheckoutId::new(
                    effigy_kernel::id::CheckoutKind::EphemeralClone,
                    [7; 16],
                ),
                "web",
            )
            .unwrap(),
            generation: Generation::new(3),
        },
        container: identity(),
        pid_ns: 401,
        sid: 42,
        leader_pid: 42,
        leader_start_ticks: 100,
        nonce: nonce(),
        session_isolated: true,
    }
}

fn nonce() -> Nonce {
    serde_json::from_value(serde_json::json!(NONCE)).unwrap()
}

#[test]
fn identity_names_the_exact_engine_context_and_full_id() {
    let encoded = container().encode();
    assert_eq!(encoded, format!("docker-compose:{ID}@ctx"));
    assert_eq!(RemoteContainer::parse(&encoded), Some(container()));
    let colima = RemoteContainer::new(&colima(), "effigy".into(), ID);
    assert_eq!(RemoteContainer::parse(&colima.encode()), Some(colima));
    let host = RemoteContainer::new(&selection(), "unix:///var/run/docker.sock".into(), ID);
    assert_eq!(RemoteContainer::parse(&host.encode()), Some(host.clone()));
    assert_eq!(
        host.runtime(vec![OsString::from("ps")]).1[..2],
        ["--host", "unix:///var/run/docker.sock"].map(OsString::from)
    );
    for bad in [
        "docker-compose:0123@ctx".to_owned(),
        format!("podman:{ID}@ctx"),
        format!("docker-compose:{ID}@"),
        format!("docker-compose:{ID}"),
    ] {
        assert_eq!(RemoteContainer::parse(&bad), None, "{bad}");
    }
    let (mut adapter, engine) = adapter(Engine::new().workload());
    let cx = operation(Duration::from_secs(5));
    let verified = adapter.identity(&container().target(), &cx);
    assert_eq!(verified.verified(), Some(&identity()));
    // Inspect plus the in-container boot read; nothing else.
    assert_eq!(engine.borrow().calls.len(), 2);
}

#[test]
fn identity_refuses_a_stopped_container_and_proves_only_inventory_absence() {
    let cx = operation(Duration::from_secs(5));
    let mut stopped = Engine::new();
    stopped.running = false;
    let (mut a, _) = adapter(stopped);
    assert!(matches!(
        a.identity(&container().target(), &cx),
        Observed::Unknown(_)
    ));
    let mut gone = Engine::new();
    gone.exists = false;
    let (mut a, _) = adapter(gone);
    assert!(matches!(
        a.identity(&container().target(), &cx),
        Observed::Absent(AbsenceProof::ContainerGone { id }) if id == container().encode()
    ));
    let mut hung = Engine::new();
    hung.hung = true;
    let (mut a, _) = adapter(hung);
    assert!(matches!(
        a.identity(&container().target(), &cx),
        Observed::Unknown(_)
    ));
}

fn exec_options() -> ExecOptions {
    ExecOptions {
        user: Some("dev".into()),
        workdir: Some("/workspace/app".into()),
        cwd: std::env::temp_dir(),
        client_env: BTreeMap::from([(OsString::from("PATH"), EnvValue::Plain("/usr/bin".into()))]),
        lifetime: Lifetime::Finite(Duration::from_secs(60)),
        label: "seed".into(),
    }
}

fn command(terminal: RemoteTerminal) -> RemoteCommand {
    RemoteCommand {
        argv: vec!["sh".into(), "-c".into(), "mysql < schema.sql".into()],
        env: BTreeMap::from([
            (
                OsString::from("EFFIGY_CUSTODY_NONCE"),
                EnvValue::Secret(SecretValue::new(NONCE.as_bytes().to_vec()).unwrap()),
            ),
            (
                OsString::from("DB_PASSWORD"),
                EnvValue::Secret(SecretValue::new(b"hunter2".to_vec()).unwrap()),
            ),
            (OsString::from("APP_ENV"), EnvValue::Plain("dev".into())),
        ]),
        terminal,
    }
}

fn strings(argv: &[OsString]) -> Vec<String> {
    argv.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn docker_launch_forwards_values_privately_and_keeps_the_operator_terminal() {
    let launcher = ComposeRemote::with_cli(
        ModelCli(Rc::new(RefCell::new(Engine::new()))),
        Some(exec_options()),
    );
    for (terminal, flags) in [
        (RemoteTerminal::None, vec![]),
        (RemoteTerminal::Stdin, vec!["-i"]),
        (RemoteTerminal::Tty, vec!["-i", "-t"]),
    ] {
        let cmd = command(terminal);
        let wrapper = strings(&cmd.wrapper_argv());
        let spec = launcher.launch_spec(&container(), cmd).unwrap();
        let argv = strings(&spec.argv);
        assert_eq!(argv[..3], ["/bin/sh", "-c", DOCKER_RELAY]);
        assert_eq!(
            argv[3..8],
            [
                "effigy-remote",
                "3",
                "APP_ENV",
                "DB_PASSWORD",
                "EFFIGY_CUSTODY_NONCE"
            ]
        );
        assert!(argv[8].ends_with("docker"));
        let mut expected = vec!["--context", "ctx", "exec"];
        expected.extend(flags);
        expected.extend([
            "-u",
            "dev",
            "-w",
            "/workspace/app",
            "--env-file",
            "/dev/fd/4",
            ID,
        ]);
        assert_eq!(argv[9..9 + expected.len()], expected);
        assert_eq!(argv[9 + expected.len()..], wrapper);
        // No value enters argv; each one is private and keeps its class.
        assert!(!argv
            .iter()
            .any(|arg| arg.contains("hunter2") || arg.contains(NONCE)));
        assert!(matches!(
            spec.env.get(&OsString::from("EFFIGY_REMOTE_V1")),
            Some(EnvValue::Secret(_))
        ));
        assert!(matches!(
            spec.env.get(&OsString::from("EFFIGY_REMOTE_V0")),
            Some(EnvValue::Plain(value)) if value == "dev"
        ));
        assert!(spec.env.contains_key(&OsString::from("PATH")));
        assert!(!spec
            .env
            .contains_key(&OsString::from("EFFIGY_CUSTODY_NONCE")));
    }
}

#[test]
fn colima_launch_relays_values_to_the_vm_and_attaches_stdin_for_a_terminal() {
    let launcher = ComposeRemote::with_cli(
        ModelCli(Rc::new(RefCell::new(Engine::new()))),
        Some(exec_options()),
    );
    let target = RemoteContainer::new(&colima(), "effigy".into(), ID);
    // The VM shell carries the values on stdin, so no terminal there: the
    // adapter reports the downgrade, and a terminal reaching launch refuses.
    let colima_id = ContainerIdentity {
        id: target.encode(),
        ..identity()
    };
    assert_eq!(
        launcher.terminal(&colima_id, RemoteTerminal::Tty),
        RemoteTerminal::Stdin
    );
    assert_eq!(
        launcher.terminal(&colima_id, RemoteTerminal::Stdin),
        RemoteTerminal::Stdin
    );
    assert_eq!(
        launcher.terminal(&identity(), RemoteTerminal::Tty),
        RemoteTerminal::Tty
    );
    assert!(launcher
        .launch_spec(&target, command(RemoteTerminal::Tty))
        .is_err());
    for (terminal, mode, flags) in [
        (RemoteTerminal::None, "none", vec![]),
        (RemoteTerminal::Stdin, "stdin", vec!["-i"]),
    ] {
        let spec = launcher.launch_spec(&target, command(terminal)).unwrap();
        let argv = strings(&spec.argv);
        assert_eq!(argv[2], COLIMA_RELAY);
        assert!(argv[8].ends_with("colima"));
        assert_eq!(argv[9..12], ["effigy", mode, COLIMA_VM]);
        let mut expected = vec!["exec"];
        expected.extend(flags);
        expected.extend([
            "-u",
            "dev",
            "-w",
            "/workspace/app",
            "-e",
            "APP_ENV",
            "-e",
            "DB_PASSWORD",
            "-e",
            "EFFIGY_CUSTODY_NONCE",
            ID,
        ]);
        assert_eq!(argv[12..12 + expected.len()], expected);
        assert!(!argv.iter().any(|arg| arg.contains("hunter2")));
    }
}

#[test]
fn launch_refuses_multi_line_values_unportable_names_and_recovery_adapters() {
    let launcher = ComposeRemote::with_cli(
        ModelCli(Rc::new(RefCell::new(Engine::new()))),
        Some(exec_options()),
    );
    let mut cmd = command(RemoteTerminal::None);
    cmd.env.insert(
        "KEY".into(),
        EnvValue::Secret(SecretValue::new(b"line\nline".to_vec()).unwrap()),
    );
    assert!(launcher.launch_spec(&container(), cmd).is_err());
    let mut cmd = command(RemoteTerminal::None);
    cmd.env
        .insert("BAD-NAME".into(), EnvValue::Plain("x".into()));
    assert!(launcher.launch_spec(&container(), cmd).is_err());
    let (recovery, _) = adapter(Engine::new());
    assert!(recovery
        .launch_spec(&container(), command(RemoteTerminal::None))
        .is_err());
}

/// The relays run as written, against fakes that model the environment
/// boundaries on the way to the guest: `ssh` carries no client environment into
/// the VM, `sudo` resets the environment and preserves only the names it is
/// told to, and the runtime exec sees exactly what those two left it.
///
/// The guest is observed whole - its complete environment and its argv - with
/// the caller's session sources sitting in the relay's own environment as they
/// do in the real exec client. The main proof asserts none of them crossed; the
/// paired control forwards a session-access *name* and asserts the harness
/// sees it, so the absence assertions are a property of the relay rather than of
/// what the fake happens to print (review 5686048338).
#[test]
fn host_relays_deliver_values_by_name_and_pass_stdin_through() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let model_path = format!("{}:/usr/bin:/bin", bin.display());
    let preserved = dir.path().join("preserved");
    let write = |name: &str, body: &str| {
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    // The container CLI shows the env file it was given, its argv, then stdin.
    let docker = write("docker", "cat /dev/fd/4; printf 'argv:%s\\n' \"$*\"; cat");
    // `colima ssh --profile P -- <cmd...>`: an ssh session carries no client
    // environment into the VM, so the VM command starts from a base only.
    let colima = write(
        "colima",
        &format!("shift 4\nexec env -i PATH='{model_path}' \"$@\""),
    );
    // `sudo --preserve-env=NAMES nerdctl ...`: sudo resets the environment and
    // keeps only the named variables, which it finds in its own environment.
    write(
        "sudo",
        &format!(
            r#"keep=
case $1 in --preserve-env=*) keep=${{1#*=}}; shift ;; esac
printf 'preserve:%s\n' "$keep"
: > '{preserved}'
for name in $(printf '%s' "$keep" | tr ',' ' '); do
    case $name in ''|*[!A-Za-z0-9_]*) exit 98 ;; esac
    eval "value=\"\$$name\""
    printf '%s=%s\n' "$name" "$value" >> '{preserved}'
done
exec env -i PATH='{model_path}' MODEL_PRESERVED='{preserved}' sh -c '
while IFS= read -r pair; do
    export "$pair"
done < "$MODEL_PRESERVED"
exec "$@"
' sh "$@"
"#,
            preserved = preserved.display(),
        ),
    );
    // The guest runtime exec reports its whole environment and its argv.
    write(
        "nerdctl",
        "printf 'argv:%s\\n' \"$*\"; printf 'env-begin\\n'; env; printf 'env-end\\n'; cat",
    );
    let run = |argv: Vec<String>, stdin: &[u8]| {
        use std::os::unix::process::CommandExt;
        // The relay ends its own process group, as the custody client's.
        let mut child = Command::new(&argv[0])
            .args(&argv[1..])
            .process_group(0)
            .env_clear()
            .env("PATH", &model_path)
            .env("EFFIGY_REMOTE_V0", "one")
            .env("EFFIGY_REMOTE_V1", "two words = more")
            // The caller's session sources sit in the relay's environment, as
            // they do in the real exec client. A relay forwards named values
            // only, so none of these may reach the guest or an argument.
            .env("EFFIGY_SECRET_PASSPHRASE", "relay-must-not-forward-this")
            .env("EFFIGY_INTERNAL_SECRET_PASSPHRASE", "retired-passphrase")
            .env("EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS", "1")
            .env("EFFIGY_INTERNAL_SECRET_GRANT", "1")
            .env("EFFIGY_TEST_SECRETS_PASSPHRASE", "test-passphrase")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let docker_command = |names: [&str; 2]| {
        let mut argv: Vec<String> = vec![
            "/bin/sh".into(),
            "-c".into(),
            DOCKER_RELAY.into(),
            "effigy-remote".into(),
            "2".into(),
        ];
        argv.extend(names.map(str::to_owned));
        argv.push(docker.display().to_string());
        argv.extend(["exec".to_owned(), "-i".to_owned()]);
        argv
    };
    let colima_command = |names: [&str; 2], mode: &str| {
        let mut argv: Vec<String> = vec![
            "/bin/sh".into(),
            "-c".into(),
            COLIMA_RELAY.into(),
            "effigy-remote".into(),
            "2".into(),
        ];
        argv.extend(names.map(str::to_owned));
        argv.push(colima.display().to_string());
        argv.extend([
            "effigy".to_owned(),
            mode.to_owned(),
            COLIMA_VM.to_owned(),
            "exec".to_owned(),
            "-e".to_owned(),
            "A".to_owned(),
        ]);
        argv
    };
    let assert_no_session_source = |surface: &str, output: &str| {
        for forbidden in SESSION_SOURCE_NAMES
            .iter()
            .chain(SESSION_SOURCE_VALUES.iter())
        {
            assert!(
                !output.contains(forbidden),
                "{surface}: `{forbidden}` crossed a relay into the guest: {output}"
            );
        }
        // Stronger than a value check: no Effigy-private name reaches the
        // guest at all.
        assert!(
            !output
                .lines()
                .any(|line| line.starts_with("EFFIGY_") || line.contains(" EFFIGY_")),
            "{surface}: an Effigy-private name reached the guest: {output}"
        );
    };

    // docker: the env file the guest is created with is the whole delivered
    // environment, and it carries the two declared values only.
    let docker_out = run(docker_command(["A", "B"]), b"typed input\n");
    assert_eq!(
        docker_out,
        "A=one\nB=two words = more\nargv:exec -i\ntyped input\n"
    );
    assert_no_session_source("docker", &docker_out);

    for (mode, tail) in [("none", ""), ("stdin", "typed input\n")] {
        let out = run(colima_command(["A", "B"], mode), b"typed input\n");
        let (env, argv) = delivered_guest_surface(&out);
        // The guest process's complete environment is exactly the two
        // declared values, and its argv names them.
        assert_eq!(env, ["A=one", "B=two words = more"], "{mode}: {out}");
        assert_eq!(argv, ["exec -e A"], "{mode}: {out}");
        assert!(out.contains("preserve:A,B\n"), "{mode}: {out}");
        assert!(out.ends_with(tail), "{mode}: {out}");
        assert_no_session_source(mode, &out);
    }

    // The control: give the same relay a session-access name to forward and
    // the harness observes it in the guest's environment and argv. Without
    // this, the absence assertions above would prove nothing about what the
    // fake prints.
    let docker_control = run(
        docker_command(["A", "EFFIGY_INTERNAL_SECRET_GRANT"]),
        b"typed input\n",
    );
    assert!(
        docker_control.contains("EFFIGY_INTERNAL_SECRET_GRANT=two words = more\n"),
        "docker control: a forwarded access name was not observed: {docker_control}"
    );
    for (mode, _) in [("none", ""), ("stdin", "typed input\n")] {
        let out = run(
            colima_command(["A", "EFFIGY_INTERNAL_SECRET_GRANT"], mode),
            b"typed input\n",
        );
        let (env, argv) = delivered_guest_surface(&out);
        assert!(
            env.iter()
                .any(|pair| pair.starts_with("EFFIGY_INTERNAL_SECRET_GRANT=")),
            "colima control ({mode}): a forwarded access name was not observed: {out}"
        );
        assert_eq!(argv, ["exec -e A".to_owned()], "{mode}: {out}");
        // ... and the passphrase value itself still never crosses, even when
        // an access name does: the relay forwards only the names it is given.
        assert!(
            !out.contains("relay-must-not-forward-this"),
            "colima control ({mode}): {out}"
        );
    }
}

#[test]
fn observation_reads_the_registered_session_and_closure_needs_no_survivor() {
    let cx = operation(Duration::from_secs(5));
    let r = registration();
    let (mut a, engine) = adapter(Engine::new().workload());
    assert!(matches!(
        a.observe_session(&r, &cx),
        Observed::Verified(SessionState::Running, _)
    ));
    engine.borrow_mut().guests.clear();
    assert!(matches!(
        a.observe_session(&r, &cx),
        Observed::Verified(SessionState::Closed, _)
    ));
    // A stopped container holds no process: closed without an exec.
    engine.borrow_mut().running = false;
    let calls = engine.borrow().calls.len();
    assert!(matches!(
        a.observe_session(&r, &cx),
        Observed::Verified(SessionState::Closed, _)
    ));
    assert_eq!(engine.borrow().calls.len(), calls + 1);
    // A new VM boot proves the session absent; a replacement is unknown.
    let mut rebooted = Engine::new().workload();
    rebooted.boot = "boot-b".into();
    let (mut a, _) = adapter(rebooted);
    assert!(matches!(
        a.observe_session(&r, &cx),
        Observed::Absent(AbsenceProof::BootChanged { .. })
    ));
    let mut replaced = Engine::new().workload();
    replaced.created = "2026-10-21T00:00:00Z".into();
    let (mut a, _) = adapter(replaced);
    assert!(matches!(a.observe_session(&r, &cx), Observed::Unknown(_)));
}

#[test]
fn nonce_search_proves_never_started_only_from_a_complete_inventory() {
    let cx = operation(Duration::from_secs(5));
    let (mut a, _) = adapter(Engine::new().foreign(90));
    assert!(matches!(
        a.find_by_nonce(&identity(), &nonce(), &cx),
        Observed::Verified(None, _)
    ));
    let mut unreadable = Engine::new().foreign(90);
    unreadable.guests[0].unreadable = true;
    let (mut a, _) = adapter(unreadable);
    assert!(matches!(
        a.find_by_nonce(&identity(), &nonce(), &cx),
        Observed::Unknown(_)
    ));
    let (mut a, _) = adapter(Engine::new().workload());
    let found = a.find_by_nonce(&identity(), &nonce(), &cx);
    let found = found.verified().unwrap().as_ref().unwrap();
    assert_eq!(
        (
            found.leader_pid,
            found.sid,
            found.leader_start_ticks,
            found.pid_ns
        ),
        (42, 42, 100, 401)
    );
    assert!(found.session_isolated);
    let mut rebooted = Engine::new().workload();
    rebooted.boot = "boot-b".into();
    let (mut a, _) = adapter(rebooted);
    assert!(matches!(
        a.find_by_nonce(&identity(), &nonce(), &cx),
        Observed::Absent(AbsenceProof::BootChanged { .. })
    ));
    let mut gone = Engine::new();
    gone.exists = false;
    let (mut a, _) = adapter(gone);
    assert!(matches!(
        a.find_by_nonce(&identity(), &nonce(), &cx),
        Observed::Absent(AbsenceProof::ContainerGone { .. })
    ));
}

/// 052 §5.10's conformance suite on the production adapter over the model:
/// kill at each step, replacement, reboot, no setsid, killed client, a
/// foreign session, a hung backend and a guest permission refusal.
#[cfg(feature = "fault-injection")]
#[test]
fn production_adapter_passes_the_remote_conformance_suite() {
    use effigy_kernel::remote::conformance::{run, Case, Fixture};
    use effigy_kernel::run::{Effect, EffectSpec, EffectState, Settlement};

    struct Model {
        adapter: ComposeRemote<ModelCli>,
        engine: Rc<RefCell<Engine>>,
    }
    impl Fixture for Model {
        fn arrange(&mut self, case: Case, _: &OperationContext) -> Result<Option<Effect>, String> {
            if case == Case::BeforeIdentity {
                return Ok(None);
            }
            let mut engine = Engine::new();
            let mut r = registration();
            let receipt = !matches!(
                case,
                Case::Intended | Case::ClientSpawned | Case::Acknowledged
            );
            match case {
                // Killed before the workload started: nothing carries the nonce.
                Case::Intended => engine = engine.foreign(90),
                Case::ContainerReplaced => engine.exists = false,
                Case::VmRestarted => {
                    engine = engine.workload();
                    engine.boot = "boot-b".into();
                }
                Case::NoSetsid => {
                    engine = engine.workload();
                    for guest in &mut engine.guests {
                        guest.sid = 7;
                    }
                    r.sid = 7;
                    r.session_isolated = false;
                }
                Case::ForeignSession => {
                    // The leader ended; its session number is reused by a
                    // process without the nonce.
                    engine = engine.foreign(42);
                }
                Case::BackendHung => {
                    engine = engine.workload();
                    engine.hung = true;
                }
                Case::GuestPermissionDenied => {
                    engine = engine.workload();
                    engine.denied = true;
                }
                _ => {
                    engine = engine.workload();
                    engine.guests[1].stubborn = true;
                }
            }
            *self.engine.borrow_mut() = engine;
            let intent = serde_json::json!({
                "env": r.env,
                "container": r.container,
                "nonce": r.nonce,
                "grace_millis": 20,
                "call_millis": 2000,
            });
            Ok(Some(Effect {
                sequence: 1,
                spec: EffectSpec {
                    target: r.env.clone(),
                    kind: "kernel:remote-process".into(),
                    payload: intent,
                    settlement: Settlement::Release(serde_json::Value::Null),
                    required: true,
                },
                state: if receipt {
                    EffectState::Applied
                } else {
                    EffectState::Intended
                },
                receipt: receipt.then(|| serde_json::to_value(&r).unwrap()),
                reason: None,
            }))
        }
        fn adapter(&mut self) -> &mut dyn RemoteExecAdapter {
            &mut self.adapter
        }
        fn verify_oracles(
            &mut self,
            case: Case,
            released: bool,
            _: &OperationContext,
        ) -> Result<(), String> {
            let engine = self.engine.borrow();
            let survivors = engine
                .guests
                .iter()
                .filter(|guest| guest.nonce.as_deref() == Some(NONCE))
                .count();
            match case {
                // Released means no nonce process survives in a live guest.
                _ if released && engine.boot == "boot-a" && engine.exists && survivors > 0 => {
                    Err(format!("{case:?}: released with {survivors} survivors"))
                }
                // A foreign session is never signalled.
                Case::ForeignSession if !engine.signals.is_empty() => {
                    Err(format!("foreign process signalled: {:?}", engine.signals))
                }
                Case::VmRestarted | Case::ContainerReplaced if !engine.signals.is_empty() => {
                    Err(format!("{case:?}: signalled across identity"))
                }
                Case::Applied if !engine.signals.iter().any(|(_, sig)| sig == "KILL") => {
                    Err("a stubborn member must reach KILL".into())
                }
                _ => Ok(()),
            }
        }
    }
    let engine = Rc::new(RefCell::new(Engine::new()));
    let mut model = Model {
        adapter: ComposeRemote::with_cli(ModelCli(engine.clone()), None),
        engine,
    };
    let cleanup = operation(Duration::from_secs(30))
        .cleanup(Duration::from_secs(30))
        .unwrap();
    let results = run(&mut model, &cleanup).unwrap();
    assert_eq!(
        results.len(),
        effigy_kernel::remote::conformance::CASES.len()
    );
}

/// The helper as written, against this host's real `/proc`.
#[cfg(target_os = "linux")]
#[test]
fn linux_helper_inventories_and_signals_only_identified_processes() {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let helper = |args: &[&str], nonce: &str| {
        let mut child = Command::new("/bin/sh")
            .args(["-c", HELPER, "effigy-custody"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{nonce}\n").as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    let mut workload = Command::new("/bin/sleep")
        .arg("30")
        .env("EFFIGY_CUSTODY_NONCE", NONCE)
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = workload.id();
    let (boot, ns, processes) = parse_snapshot(&helper(&["snap"], NONCE)).unwrap();
    let me = processes.iter().find(|process| process.pid == pid).unwrap();
    assert_eq!(me.nonce, Some(true));
    let ns = format!("pid:[{ns}]");
    // Wrong start ticks: never signalled.
    let stale = (me.start + 1).to_string();
    let pid_text = pid.to_string();
    assert_eq!(
        helper(
            &["kill", &boot, &ns, "TERM", "member", &pid_text, &stale],
            NONCE
        )
        .trim(),
        "gone"
    );
    // Another nonce: never signalled.
    let start = me.start.to_string();
    assert_eq!(
        helper(
            &["kill", &boot, &ns, "TERM", "member", &pid_text, &start],
            "b"
        )
        .trim(),
        "gone"
    );
    assert!(workload.try_wait().unwrap().is_none());
    assert_eq!(
        helper(
            &[
                "kill",
                "other-boot",
                &ns,
                "TERM",
                "member",
                &pid_text,
                &start
            ],
            NONCE
        )
        .trim(),
        "moved"
    );
    assert_eq!(
        helper(
            &["kill", &boot, &ns, "TERM", "member", &pid_text, &start],
            NONCE
        )
        .trim(),
        "sent"
    );
    let status = workload.wait().unwrap();
    assert!(!status.success());
}
