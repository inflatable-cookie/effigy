//! Deterministic strict fake-runtime controls for the original unexplained
//! detached container `Exited (141)` report (papercut 8064c5b9, effigy#300).
//!
//! The controls answer one question with real native pipes and real process
//! lifetimes: which reader could have closed under the service's stdout. They
//! do not claim the historical nerdctl pipe identity, because no retained
//! evidence names it. What they do establish, on this checkout:
//!
//! - a detached Compose client is launched with a regular file on stdout and
//!   stderr, so a service writer that outlives the client has no reader to
//!   lose and never sees `SIGPIPE`;
//! - a bounded inventory/inspect capture does own a pipe, and cancelling it
//!   signals only the client's process group. An independent writer that
//!   inherited that pipe survives the cancellation and dies of `SIGPIPE`
//!   (141 = 128 + 13) only when Effigy closes its own read end;
//! - a pending inventory or an exhausted deadline refuses without spawning an
//!   activation, and a foreign or unknown ownership observation is refused;
//! - a genuine service failure is not reported as `SIGPIPE`.
//!
//! Every fixture is a fresh private temporary directory. Recorded child PIDs
//! are stopped before a control returns.

use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nix::sys::signal::kill;
use nix::unistd::Pid;
use serde_json::{json, Value};

use super::process::run_command_capture_with_absolute_deadline;

const PROJECT: &str = "private-exit141";
const SERVICE: &str = "app";
const CONTAINER: &str = "private-exit141-app-1";
const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORKING_DIR: &str = "/workspace-root/app";

const MANIFEST: &str = "[containers]\ndefault = \"web\"\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nworking_dir = \"/workspace-root/app\"\nproject_name = \"private-exit141\"\nprofile = \"private-exit141\"\n";
const COMPOSE: &str = "services:\n  app:\n    image: fixture.invalid/app\n";

/// The strict fake runtime. It models only the observations and effects these
/// controls need, refuses everything else with exit 90, and records every
/// invocation with the caller's argv and the kind of fd 1/2 it inherited.
const FAKE_RUNTIME: &str = r###"#!/usr/bin/env python3
import json, os, signal, stat, subprocess, sys, time

FIXTURE = "__FIXTURE__"
WRITER = "__WRITER__"
WITNESS = "__WITNESS__"
PROJECT = "__PROJECT__"
SERVICE = "__SERVICE__"
CONTAINER = "__CONTAINER__"
WORKING_DIR = "__WORKING_DIR__"
IDENTITY = "__IDENTITY__"


def stamp():
    return round(time.time(), 4)


def shape(fd):
    try:
        info = os.fstat(fd)
    except OSError:
        return {"kind": "closed"}
    if stat.S_ISREG(info.st_mode):
        kind = "regular"
    elif stat.S_ISFIFO(info.st_mode):
        kind = "fifo"
    elif stat.S_ISCHR(info.st_mode):
        kind = "char"
    else:
        kind = "other"
    return {"kind": kind, "inode": info.st_ino}


def append(name, value):
    with open(os.path.join(FIXTURE, name), "a") as handle:
        handle.write(json.dumps(value, sort_keys=True) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def write_json(name, value):
    path = os.path.join(FIXTURE, name)
    temporary = path + ".tmp"
    with open(temporary, "w") as handle:
        json.dump(value, handle, sort_keys=True)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


def control():
    with open(os.path.join(FIXTURE, "control.json")) as handle:
        return json.load(handle)


def command_of(args):
    if len(args) >= 2 and args[0] in ("--context", "--host"):
        args = args[2:]
    if args and args[0] == "nerdctl":
        args = args[args.index("--") + 1:] if "--" in args else args[1:]
    if not args:
        return ""
    if args[0] != "compose":
        return args[0]
    index = 1
    while index < len(args):
        token = args[index]
        if token in ("-f", "-p", "--project-directory", "--env-file", "--profile"):
            index += 2
            continue
        if token.startswith("-"):
            index += 1
            continue
        return token
    return ""


def sleep_ms(mode):
    time.sleep(float(mode.get("delay_ms", 0)) / 1000.0)


def inspected(mode):
    checkout = os.path.realpath(FIXTURE)
    if mode == "foreign":
        checkout = os.path.realpath(os.path.join(FIXTURE, "foreign-checkout"))
    labels = {
        "com.docker.compose.project": PROJECT,
        "com.docker.compose.service": SERVICE,
        "com.docker.compose.oneoff": "false",
        "com.docker.compose.project.working_dir": checkout,
        "com.effigy.checkout": checkout,
    }
    record = {
        "Id": IDENTITY,
        "Name": "/" + CONTAINER,
        "State": {"Status": "running", "Running": True},
        "Config": {"Labels": labels, "WorkingDir": WORKING_DIR},
        "Mounts": [{
            "Type": "bind",
            "Source": os.path.realpath(FIXTURE),
            "Destination": WORKING_DIR,
        }],
        "NetworkSettings": {"Ports": {
            "80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18080"}],
        }},
    }
    if mode == "stale":
        # A deleted or restarted profile leaves the container's labels intact
        # while its canonical identity can no longer be read.
        record["Id"] = ""
    if mode == "stale_foreign":
        record["Id"] = ""
        foreign = os.path.realpath(os.path.join(FIXTURE, "foreign-checkout"))
        record["Config"]["Labels"]["com.effigy.checkout"] = foreign
        record["Config"]["Labels"]["com.docker.compose.project.working_dir"] = foreign
    return [record]


def main():
    argv = sys.argv[1:]
    command = command_of(argv)
    settings = control()
    append("runtime-calls.jsonl", {
        "argv": argv,
        "command": command,
        "at": stamp(),
        "pid": os.getpid(),
        "ppid": os.getppid(),
        "pgrp": os.getpgrp(),
        "stdout": shape(1),
        "stderr": shape(2),
    })
    if command == "ps":
        mode = settings.get("ps", {"mode": "running"})
        sleep_ms(mode)
        if mode["mode"] == "pending":
            sys.stdout.write(CONTAINER + "\tUp")
            sys.stdout.flush()
            while True:
                time.sleep(0.05)
        if mode["mode"] == "empty":
            return 0
        if mode["mode"] == "sibling":
            sys.stdout.write("%s\tUp 5 seconds\t\t%s\t%s\tworker\t\n" % (CONTAINER, PROJECT, os.path.realpath(FIXTURE)))
            return 0
        if mode["mode"] == "stopped":
            sys.stdout.write("%s\tExited (0)\t\t%s\t%s\t%s\t\n" % (CONTAINER, PROJECT, os.path.realpath(FIXTURE), SERVICE))
            return 0
        if mode["mode"] == "malformed":
            sys.stdout.write("unlabelled Up\n")
            return 0
        if mode["mode"] != "running":
            sys.stderr.write("strict fake runtime refuses ps mode %r\n" % mode["mode"])
            return 90
        sys.stdout.write("%s\tUp 5 seconds\t\t%s\t%s\t%s\t\n"
                         % (CONTAINER, PROJECT, os.path.realpath(FIXTURE), SERVICE))
        sys.stdout.flush()
        return 0
    if command == "inspect":
        mode = settings.get("inspect", {"mode": "owned"})
        sleep_ms(mode)
        if mode["mode"] == "pending":
            sys.stdout.write("[")
            sys.stdout.flush()
            while True:
                time.sleep(0.05)
        if mode["mode"] == "unknown":
            sys.stdout.write("[]")
            sys.stdout.flush()
            return 0
        if mode["mode"] not in ("owned", "foreign", "stale", "stale_foreign"):
            sys.stderr.write("strict fake runtime refuses inspect mode %r\n" % mode["mode"])
            return 90
        sys.stdout.write(json.dumps(inspected(mode["mode"])))
        sys.stdout.flush()
        return 0
    if command == "rm":
        sys.stdout.write(CONTAINER + "\n")
        sys.stdout.flush()
        return 0
    if command == "up":
        mode = settings.get("up", {"mode": "ok"})
        if mode["mode"] == "ok":
            sys.stdout.write("[fake] detached up complete\n")
            sys.stdout.flush()
            return 0
        if mode["mode"] == "detached-writer":
            sys.stdout.write("[fake] detached up complete\n")
            sys.stdout.flush()
            os.environ["EFFIGY_EXIT141_WRITER_SECONDS"] = str(mode.get("writer_seconds", 1.2))
            writer = subprocess.Popen([sys.executable, WRITER], start_new_session=True)
            write_json("detached-client.json", {
                "pid": os.getpid(),
                "pgrp": os.getpgrp(),
                "writer_pid": writer.pid,
                "recorded_at": stamp(),
                "stdout": shape(1),
                "stderr": shape(2),
            })
            return 0
        sys.stderr.write("strict fake runtime refuses up mode %r\n" % mode["mode"])
        return 90
    if command == "attach":
        write_json("capture-client.json", {
            "pid": os.getpid(),
            "pgrp": os.getpgrp(),
            "at": stamp(),
            "stdout": shape(1),
            "stderr": shape(2),
        })
        os.environ["EFFIGY_EXIT141_WRITER_SECONDS"] = "30"
        subprocess.Popen([sys.executable, WITNESS], start_new_session=True)
        while True:
            time.sleep(0.05)
    sys.stderr.write("strict fake runtime refuses `%s`\n" % command)
    return 90


if __name__ == "__main__":
    sys.exit(main())
"###;

/// A writer with native `SIGPIPE` disposition, writing to whatever fd 1 is.
/// It records the fd kind and every tick, and only writes a completion receipt
/// when the run is not killed by a closed pipe.
const WRITER: &str = r###"#!/usr/bin/env python3
import json, os, signal, stat, sys, time

FIXTURE = "__FIXTURE__"
signal.signal(signal.SIGPIPE, signal.SIG_DFL)


def shape(fd):
    info = os.fstat(fd)
    if stat.S_ISREG(info.st_mode):
        kind = "regular"
    elif stat.S_ISFIFO(info.st_mode):
        kind = "fifo"
    elif stat.S_ISCHR(info.st_mode):
        kind = "char"
    else:
        kind = "other"
    return {"kind": kind, "inode": info.st_ino}


def write_json(name, value):
    path = os.path.join(FIXTURE, name)
    temporary = path + ".tmp"
    with open(temporary, "w") as handle:
        json.dump(value, handle, sort_keys=True)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


started = time.time()
write_json("service-writer.json", {
    "pid": os.getpid(),
    "ppid": os.getppid(),
    "pgrp": os.getpgrp(),
    "stdout": shape(1),
    "stderr": shape(2),
    "started_at": round(started, 4),
})
seconds = float(os.environ.get("EFFIGY_EXIT141_WRITER_SECONDS", "1.2"))
ticks = 0
deadline = started + seconds
while time.time() < deadline:
    os.write(1, b"service-writer tick\n")
    ticks += 1
    time.sleep(0.01)
write_json("service-writer-done.json", {
    "pid": os.getpid(),
    "ticks": ticks,
    "sigpipe": False,
    "finished_at": round(time.time(), 4),
})
"###;

/// The witness runs in its own session, forks the writer, and records the
/// writer's raw wait status. That is how this suite observes which process,
/// and which signal, settled a writer that outlived the cancelled client.
const WITNESS: &str = r###"#!/usr/bin/env python3
import json, os, subprocess, sys, time

FIXTURE = "__FIXTURE__"
WRITER = "__WRITER__"


def write_json(name, value):
    path = os.path.join(FIXTURE, name)
    temporary = path + ".tmp"
    with open(temporary, "w") as handle:
        json.dump(value, handle, sort_keys=True)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


writer = subprocess.Popen([sys.executable, WRITER])
status = os.waitpid(writer.pid, 0)[1]
receipt = {
    "witness_pid": os.getpid(),
    "writer_pid": writer.pid,
    "recorded_at": round(time.time(), 4),
    "signal": os.WTERMSIG(status) if os.WIFSIGNALED(status) else None,
    "exit": os.WEXITSTATUS(status) if os.WIFEXITED(status) else None,
}
write_json("witness.json", receipt)
"###;

fn wall_clock() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("wall clock after the epoch")
        .as_secs_f64()
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).expect("private fixture script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("executable private fixture script");
}

struct Fixture {
    _dir: Option<tempfile::TempDir>,
    root: PathBuf,
}

impl Fixture {
    /// Attach to a fixture another instance of this test binary created.
    fn attach(root: PathBuf) -> Self {
        Self { _dir: None, root }
    }

    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fresh private exit141 fixture");
        let root = dir
            .path()
            .canonicalize()
            .expect("canonical private fixture");
        for name in ["bin", "foreign-checkout", "home"] {
            fs::create_dir(root.join(name)).expect("private fixture directory");
        }
        let root_text = root.to_string_lossy().into_owned();
        let writer = root.join("writer.py");
        write_script(&writer, &WRITER.replace("__FIXTURE__", &root_text));
        let witness = root.join("witness.py");
        let witness_body = WITNESS
            .replace("__FIXTURE__", &root_text)
            .replace("__WRITER__", &writer.to_string_lossy());
        write_script(&witness, &witness_body);
        let runtime = FAKE_RUNTIME
            .replace("__FIXTURE__", &root_text)
            .replace("__WRITER__", &writer.to_string_lossy())
            .replace("__WITNESS__", &witness.to_string_lossy())
            .replace("__PROJECT__", PROJECT)
            .replace("__SERVICE__", SERVICE)
            .replace("__CONTAINER__", CONTAINER)
            .replace("__WORKING_DIR__", WORKING_DIR)
            .replace("__IDENTITY__", ID);
        for name in ["docker", "colima"] {
            write_script(&root.join("bin").join(name), &runtime);
        }
        fs::write(root.join("effigy.toml"), MANIFEST).expect("private manifest");
        fs::write(root.join("compose.yml"), COMPOSE).expect("private compose declaration");
        let fixture = Self {
            _dir: Some(dir),
            root,
        };
        fixture.control(json!({
            "ps": {"mode": "running"},
            "inspect": {"mode": "owned"},
            "up": {"mode": "ok"},
        }));
        fixture
    }

    fn control(&self, value: Value) {
        fs::write(
            self.root.join("control.json"),
            serde_json::to_vec(&value).expect("control json"),
        )
        .expect("private runtime control");
    }

    fn docker(&self) -> PathBuf {
        self.root.join("bin").join("docker")
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join("runtime-calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("recorded runtime call"))
            .collect()
    }

    fn wait_json(&self, name: &str, budget: Duration) -> Value {
        let path = self.root.join(name);
        let deadline = Instant::now() + budget;
        loop {
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "private fixture receipt `{name}` did not appear within {budget:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn pid_gone(&self, pid: i64) -> bool {
        kill(Pid::from_raw(pid as i32), None).is_err()
    }

    fn wait_pid_gone(&self, pid: i64, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            if self.pid_gone(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Stop a fixture process this control started, by its recorded PID.
    fn stop_recorded(&self, pid: i64) {
        if !self.pid_gone(pid) {
            let _ = kill(Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL);
            let _ = self.wait_pid_gone(pid, Duration::from_secs(5));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Recorded writers and witnesses must not outlive their control. The
        // receipts are the only authority for which PIDs this fixture started.
        for name in [
            "service-writer.json",
            "witness.json",
            "capture-client.json",
            "detached-client.json",
        ] {
            let Ok(bytes) = fs::read(self.root.join(name)) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            for key in ["pid", "writer_pid", "witness_pid"] {
                if let Some(pid) = value.get(key).and_then(Value::as_i64) {
                    self.stop_recorded(pid);
                }
            }
        }
    }
}

/// A slow inspect that finishes inside the caller's absolute budget settles,
/// and its observation keeps the full native identity/binding.
/// A client whose output writer outlives it: cancelling the capture signals
/// only the client's process group, and the still-attached writer dies of
/// SIGPIPE only once Effigy closes its own read end.
#[test]
fn exit141_cancelled_capture_client_never_signals_the_attached_writer() {
    let fixture = Fixture::new();
    fixture.control(json!({
        "ps": {"mode": "running"},
        "inspect": {"mode": "owned"},
        "up": {"mode": "ok"},
    }));
    let program = fixture.docker();
    let error = run_command_capture_with_absolute_deadline(
        &fixture.root,
        program.to_str().expect("private path"),
        &["attach"],
        "private attached capture",
        Instant::now() + Duration::from_millis(600),
        Duration::from_millis(600),
    )
    .expect_err("a client that never exits is cancelled");
    let message = error.to_string();
    assert!(message.contains("timed out"), "{message}");
    assert!(
        message.contains("writer provenance and settlement unknown"),
        "the cancelled capture says what it could not prove: {message}"
    );
    assert!(
        !message.contains("stdout/stderr EOF verified"),
        "the writer never closed the pipe by itself: {message}"
    );
    let client = fixture.wait_json("capture-client.json", Duration::from_secs(5));
    assert_eq!(
        client["stdout"]["kind"], "fifo",
        "the captured client inherits an Effigy pipe"
    );
    let client_pid = client["pid"].as_i64().expect("client pid");
    assert!(
        fixture.wait_pid_gone(client_pid, Duration::from_secs(5)),
        "Effigy positively reaped its owned client"
    );
    let writer = fixture.wait_json("service-writer.json", Duration::from_secs(5));
    assert_eq!(
        writer["stdout"]["kind"], "fifo",
        "the service writer inherits the same Effigy pipe"
    );
    let witness = fixture.wait_json("witness.json", Duration::from_secs(15));
    assert_eq!(
        witness["signal"],
        json!(13),
        "the writer settled by SIGPIPE (141 = 128 + 13), not by any signal Effigy sent: {witness}"
    );
    let writer_pid = writer["pid"].as_i64().expect("writer pid");
    assert!(
        fixture.wait_pid_gone(writer_pid, Duration::from_secs(5)),
        "the fixture writer settled"
    );
    let witness_pid = witness["witness_pid"].as_i64().expect("witness pid");
    assert!(
        fixture.wait_pid_gone(witness_pid, Duration::from_secs(5)),
        "the fixture witness settled"
    );
}

/// The detached Compose launch owns no reader lifetime: its client writes to a
/// regular file, and a service writer that outlives the reaped client keeps
/// writing there and never sees SIGPIPE.
#[test]
fn exit141_detached_client_exit_keeps_the_service_writer_on_a_regular_file() {
    let fixture = Fixture::new();
    fixture.control(json!({
        "ps": {"mode": "running"},
        "inspect": {"mode": "owned"},
        "up": {"mode": "detached-writer", "writer_seconds": 1.2},
    }));
    let compose = fixture.root.join("compose.yml");
    let args = [
        OsString::from("compose"),
        OsString::from("-f"),
        compose.into_os_string(),
        OsString::from("-p"),
        OsString::from(PROJECT),
        OsString::from("up"),
        OsString::from("-d"),
    ];
    let program = fixture.docker();
    let output = crate::exec::run_backend_invocation(
        &fixture.root,
        OsStr::new(program.to_str().expect("private path")),
        &args,
        "private detached up",
        &[],
    )
    .expect("the detached client exits successfully");
    let returned_at = wall_clock();
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "a detached launch returns no captured backend bytes"
    );
    let record = crate::exec::retained_backend_log_path(&fixture.root, PROJECT);
    let retained = fs::read_to_string(&record).expect("retained detached record");
    assert!(
        retained.contains("[fake] detached up complete"),
        "{retained}"
    );
    let client = fixture.wait_json("detached-client.json", Duration::from_secs(5));
    assert_eq!(
        client["stdout"]["kind"], "regular",
        "the detached client writes to a regular file, never an Effigy pipe"
    );
    assert_eq!(client["stderr"]["kind"], "regular");
    let writer = fixture.wait_json("service-writer.json", Duration::from_secs(5));
    assert_eq!(
        writer["stdout"]["kind"], "regular",
        "the surviving service writer inherits the retained regular file"
    );
    let client_pid = client["pid"].as_i64().expect("client pid");
    assert!(
        fixture.wait_pid_gone(client_pid, Duration::from_secs(5)),
        "the detached client is reaped before the adapter returns"
    );
    let done = fixture.wait_json("service-writer-done.json", Duration::from_secs(15));
    assert_eq!(done["sigpipe"], json!(false), "{done}");
    assert!(
        done["ticks"].as_i64().unwrap_or(0) > 20,
        "the writer kept writing after the client was reaped: {done}"
    );
    assert!(
        done["finished_at"].as_f64().unwrap_or_default() >= returned_at + 0.5,
        "the writer outlived the detached client and the adapter return: {done}"
    );
    let retained = fs::read_to_string(&record).expect("retained detached record");
    assert!(
        retained.contains("service-writer tick"),
        "the writer's bytes reached the retained file: {retained}"
    );
    let writer_pid = writer["pid"].as_i64().expect("writer pid");
    assert!(
        fixture.wait_pid_gone(writer_pid, Duration::from_secs(5)),
        "the fixture writer settled"
    );
}

/// Inventory and ownership controls run through the production routes that
/// resolve the backend program from `PATH`, so each runs in a child instance of
/// this test binary with a private `PATH`. The parent process keeps its own
/// environment and backend selection untouched.
#[test]
fn exit141_inventory_and_ownership_controls() {
    let fixture = Fixture::new();
    let path = format!(
        "{}:{}",
        fixture.bin().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--ignored",
            "--exact",
            "exec::exit141_tests::inventory_and_ownership_child",
            "--nocapture",
        ])
        .current_dir(&fixture.root)
        .env("PATH", path)
        .env("EFFIGY_COMPOSE_BACKEND", "docker")
        .env("EFFIGY_EXIT141_FIXTURE", &fixture.root)
        .env("HOME", fixture.root.join("home"))
        .output()
        .expect("run the private exit141 child");
    assert!(
        output.status.success(),
        "private exit141 child failed\nstdout:\n{}\nstderr:\n{}\n",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The child half of [`exit141_inventory_and_ownership_controls`].
#[test]
#[ignore]
fn inventory_and_ownership_child() {
    let root = std::env::var("EFFIGY_EXIT141_FIXTURE")
        .map(PathBuf::from)
        .expect("the parent names the private fixture");
    std::env::set_current_dir(&root).expect("private fixture cwd");
    let fixture = Fixture::attach(root.clone());
    let policy = crate::with_test_effigy_home(&root.join("home"), || {
        crate::load_container_policy(&root, None)
    })
    .expect("private container policy");
    crate::with_test_compose_backend(crate::compose::ComposeBackend::Docker, || {
        // Warm the private runtime first. Its first spawn pays interpreter
        // startup and the first read of a script on whichever volume `TMPDIR`
        // points at (a configured scratch root may be a separate drive), and
        // neither cost belongs to the observation this case bounds. The
        // recorded call count is the baseline the bounded case adds to.
        fixture.control(json!({
            "ps": {"mode": "running"},
            "inspect": {"mode": "owned"},
            "up": {"mode": "ok"},
        }));
        crate::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            &root,
            &policy,
            Some(Instant::now() + Duration::from_secs(10)),
        )
        .expect("the warmed runtime answers its first inventory");
        let warmed = fixture.calls().len();
        assert_eq!(warmed, 1, "the warm-up is exactly one observation");

        // A pending inventory is an unavailable observation: bounded, refused,
        // and it never reaches an activation.
        fixture.control(json!({
            "ps": {"mode": "pending"},
            "inspect": {"mode": "owned"},
            "up": {"mode": "ok"},
        }));
        let started = Instant::now();
        let error =
            crate::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
                &root,
                &policy,
                Some(Instant::now() + Duration::from_millis(900)),
            )
            .expect_err("a pending inventory never reports a stack");
        assert!(
            error.to_string().contains("timed out"),
            "the pending inventory fails as an observation, not a stack: {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(8),
            "the pending inventory stays inside its bound: {:?}",
            started.elapsed()
        );
        let pending = fixture.calls();
        assert_eq!(
            pending.len(),
            warmed + 1,
            "one bounded observation ran: {pending:#?}"
        );
        assert_eq!(pending[warmed]["command"], "ps");
        for call in &pending {
            assert!(
                !matches!(
                    call["command"].as_str(),
                    Some("up" | "start" | "restart" | "down" | "stop")
                ),
                "an unavailable observation is never restart authority: {call}"
            );
        }

        // A slow-but-within-budget inventory and inspect settle and keep the
        // full native binding: reverify compares id, checkout, scope, profile
        // and backend against a fresh observation.
        fixture.control(json!({
            "ps": {"mode": "running", "delay_ms": 250},
            "inspect": {"mode": "owned", "delay_ms": 250},
            "up": {"mode": "ok"},
        }));
        let rows = crate::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            &root,
            &policy,
            Some(Instant::now() + Duration::from_secs(10)),
        )
        .expect("the slow inventory settles inside its budget");
        assert_eq!(rows.len(), 1, "{rows:#?}");
        assert_eq!(rows[0].container_name, CONTAINER);
        assert_eq!(rows[0].project_name.as_deref(), Some(PROJECT));
        assert_eq!(rows[0].service.as_deref(), Some(SERVICE));
        let verified = crate::exec::verify_owned_running_compose_identities(
            &root,
            &policy,
            &rows,
            Instant::now() + Duration::from_secs(10),
        )
        .expect("the slow inspect is admitted");
        assert_eq!(verified.len(), 1);
        verified[0]
            .reverify(&root, &policy, Instant::now() + Duration::from_secs(10))
            .expect("the full id/checkout/scope/profile/backend binding is preserved");

        // An exhausted deadline never spawns an observation.
        let before = fixture.calls().len();
        let expired = crate::exec::verify_owned_running_compose_identities(
            &root,
            &policy,
            &rows,
            Instant::now() - Duration::from_millis(1),
        )
        .expect_err("an exhausted deadline fails closed");
        assert!(
            expired.to_string().contains("timed out"),
            "an exhausted deadline reports a deadline, not a service: {expired}"
        );
        assert_eq!(
            fixture.calls().len(),
            before,
            "an exhausted deadline never spawns the runtime"
        );

        // A foreign or unknown ownership observation never becomes a binding.
        fixture.control(json!({
            "ps": {"mode": "running"},
            "inspect": {"mode": "unknown"},
            "up": {"mode": "ok"},
        }));
        let unknown = crate::exec::verify_owned_running_compose_identities(
            &root,
            &policy,
            &rows,
            Instant::now() + Duration::from_secs(10),
        )
        .expect_err("an unknown identity never becomes a binding");
        assert!(
            unknown
                .to_string()
                .contains("canonical full container identity is missing"),
            "{unknown}"
        );

        let selected = crate::environment::BackendSelection {
            kind: crate::environment::BackendKind::DockerDesktop,
            engine: crate::BackendId::docker_compose(),
            context: "decision-fixture".into(),
            reason: crate::environment::BackendReason::EnvironmentVariable("docker".into()),
        };
        for mode in ["empty", "sibling", "stopped", "malformed", "unavailable"] {
            fixture.control(
                json!({ "ps": {"mode": mode}, "inspect": {"mode": "owned"}, "up": {"mode": "ok"} }),
            );
            let observed = crate::exec::decision_project_inventory(
                &root,
                &policy,
                &selected,
                Instant::now() + Duration::from_secs(10),
            );
            match mode {
                "empty" => assert!(observed.unwrap().is_empty()),
                "sibling" => assert_eq!(observed.unwrap()[0].service.as_deref(), Some("worker")),
                "stopped" => assert_eq!(observed.unwrap()[0].status, "Exited (0)"),
                _ => assert!(
                    observed.is_err(),
                    "malformed or failed runtime cannot prove absence"
                ),
            }
        }
        let before = fixture.calls().len();
        assert!(crate::exec::decision_project_inventory(
            &root,
            &policy,
            &selected,
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
        assert_eq!(
            fixture.calls().len(),
            before,
            "expired deadline never spawns"
        );
        for call in fixture.calls() {
            assert!(
                !matches!(
                    call["command"].as_str(),
                    Some("up" | "start" | "restart" | "down" | "stop")
                ),
                "no ownership refusal reached a mutating backend command: {call}"
            );
        }
    });
}
