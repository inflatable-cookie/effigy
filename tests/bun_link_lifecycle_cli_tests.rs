//! The Bun link owner retires the registration it created.
//!
//! `effigy deps link bun` registers each package in the machine-wide Bun
//! install so the consumer's save-less `bun link` can resolve it. The link
//! itself is the consumer's `node_modules` symlink plus the ledger, so the
//! registration is retired when the owner that created it finishes, and an
//! interrupted owner retires only its own claim before the signal takes its
//! default course.
//!
//! These proofs run the real CLI against a fake `bun` on PATH, so they observe
//! the same process, index and registration paths a machine does.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PACKAGE: &str = "@acme/core";

/// One isolated machine: a fake HOME, a consumer checkout, a library and a
/// fake `bun` that records registrations under the fake HOME.
struct Machine {
    _fixture: effigy_test_support::Fixture,
    root: PathBuf,
    home: PathBuf,
    consumer: PathBuf,
    library: PathBuf,
    bin: PathBuf,
}

impl Machine {
    fn new(prefix: &str) -> Self {
        let fixture = effigy_test_support::Fixture::workspace(prefix).expect("fixture root");
        let root = fs::canonicalize(fixture.path()).expect("canonical fixture root");
        let home = root.join("home");
        let consumer = root.join("consumer");
        let library = root.join("library");
        let bin = root.join("bin");
        for directory in [&home, &consumer, &library, &bin] {
            fs::create_dir_all(directory).unwrap();
        }
        // A checkout boundary, so ownership evidence is recorded the way a real
        // worktree records it.
        fs::create_dir_all(consumer.join(".git")).unwrap();
        write(
            &library.join("package.json"),
            "{\"name\":\"@acme/core\",\"version\":\"0.1.0\"}\n",
        );
        write(
            &consumer.join("package.json"),
            &format!("{{\"name\":\"consumer\",\"dependencies\":{{\"{PACKAGE}\":\"1.2.3\"}}}}\n"),
        );
        write(&consumer.join("bun.lock"), "fixture-lock\n");
        write(
            &home.join("tree.txt"),
            &format!("consumer node_modules (1)\n└── {PACKAGE}@1.2.3\n"),
        );
        let bun = bin.join("bun");
        write(&bun, FAKE_BUN);
        fs::set_permissions(&bun, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _fixture: fixture,
            root,
            home,
            consumer,
            library,
            bin,
        }
    }

    fn registration(&self) -> PathBuf {
        self.home
            .join(".bun/install/global/node_modules")
            .join(PACKAGE)
    }

    fn foreign_registration(&self) -> PathBuf {
        self.home
            .join(".bun/install/global/node_modules/@other/pkg")
    }

    fn index_path(&self) -> PathBuf {
        self.home.join(".effigy/deps/bun-registrations.json")
    }

    fn ledger_path(&self) -> PathBuf {
        self.consumer.join(".effigy/local/dependency-links.json")
    }

    fn consumer_link(&self) -> PathBuf {
        self.consumer.join("node_modules").join(PACKAGE)
    }

    fn mark(&self, name: &str) {
        write(&self.home.join(name), "");
    }

    fn marked(&self, name: &str) -> bool {
        self.home.join(name).exists()
    }

    /// Plant an unrelated registration: retirement must never touch it.
    fn plant_foreign_registration(&self) {
        let target = self.root.join("other-package");
        fs::create_dir_all(&target).unwrap();
        replace_symlink(&target, &self.foreign_registration());
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(args)
            .env("HOME", &self.home)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.bin.display()),
            )
            .env("FAKE_BUN_HOME", &self.home)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn link_args(&self) -> Vec<String> {
        vec![
            "deps".to_owned(),
            "link".to_owned(),
            "bun".to_owned(),
            self.library.display().to_string(),
            "--repo".to_owned(),
            self.consumer.display().to_string(),
            "--json".to_owned(),
        ]
    }

    fn unlink_args(&self) -> Vec<String> {
        vec![
            "deps".to_owned(),
            "unlink".to_owned(),
            "bun".to_owned(),
            self.library.display().to_string(),
            "--repo".to_owned(),
            self.consumer.display().to_string(),
            "--json".to_owned(),
        ]
    }

    fn run(&self, args: &[String]) -> (bool, String, String) {
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        let output = self
            .command(&args)
            .output()
            .expect("run the effigy CLI in the fixture");
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    fn claim_consumers(&self) -> Vec<PathBuf> {
        let raw = match fs::read_to_string(self.index_path()) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => panic!("read claim index: {error}"),
        };
        let index: serde_json::Value = serde_json::from_str(&raw).unwrap();
        index["registrations"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|registration| {
                registration["consumers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|consumer| consumer["consumer_repo"].as_str())
                    .map(PathBuf::from)
            })
            .collect()
    }

    /// A sibling consumer of the same registration: a different checkout that
    /// also claims it.
    fn seed_sibling_claim(&self) -> PathBuf {
        let sibling = self.root.join("sibling");
        fs::create_dir_all(sibling.join(".git")).unwrap();
        let library = self.library.clone();
        let sibling = fs::canonicalize(&sibling).unwrap();
        let package_path = self.library.clone();
        let index = serde_json::json!({
            "schema": "effigy.deps.bun-registrations.v1",
            "schema_version": 1,
            "registrations": [{
                "package_name": PACKAGE,
                "package_path": package_path.display().to_string(),
                "effigy_created": true,
                "consumers": [{
                    "consumer_repo": sibling.display().to_string(),
                    "library_path": library.display().to_string(),
                    "checkout_root": sibling.display().to_string(),
                }],
            }],
        });
        write(&self.index_path(), &format!("{index}\n"));
        sibling
    }
}

const FAKE_BUN: &str = r#"#!/bin/sh
set -eu
home="${FAKE_BUN_HOME:?FAKE_BUN_HOME must name the fixture HOME}"
registration_root="$home/.bun/install/global/node_modules"
echo "$* @ $PWD" >> "$home/calls.log"
package_name() {
    sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$PWD/package.json" | head -1
}
case "$*" in
  "pm ls --all")
    cat "$home/tree.txt"
    ;;
  "link --no-save")
    package="$(package_name)"
    if [ -f "$home/hold-register-tree" ]; then
      # A wrapper whose descendant would create the registration after the
      # owner was interrupted, unless the whole tree is stopped.
      echo "$$" > "$home/register-tree-child.pid"
      registration="$registration_root/$package"
      export registration
      (
        echo "$$" > "$home/register-tree-descendant.pid"
        sleep 2
        mkdir -p "$(dirname "$registration")"
        ln -sfn "$PWD" "$registration"
        echo wrote > "$home/register-tree-descendant-wrote"
      ) &
      wait
      exit 0
    fi
    if [ -f "$home/hold-register" ]; then
      echo "$$" > "$home/register-child.pid"
      sleep 2
    fi
    mkdir -p "$(dirname "$registration_root/$package")"
    ln -sfn "$PWD" "$registration_root/$package"
    ;;
  "link "*)
    package=""
    for argument in "$@"; do
      case "$argument" in
        link|--no-save) ;;
        *) package="$argument" ;;
      esac
    done
    mkdir -p "$PWD/node_modules/$(dirname "$package")"
    ln -sfn "$(readlink "$registration_root/$package")" "$PWD/node_modules/$package"
    if [ -f "$home/hold-consumer-link" ]; then
      echo "$$" > "$home/held-child.pid"
      exec sleep 300
    fi
    ;;
  "unlink --no-save")
    package="$(package_name)"
    if [ -f "$home/fail-unlink" ]; then
      echo "fake bun: unregister refused" >&2
      exit 23
    fi
    rm -f "$registration_root/$package"
    ;;
  *)
    echo "fake bun: unexpected arguments: $*" >&2
    exit 64
    ;;
esac
"#;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = fs::File::create(path).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

fn replace_symlink(target: &Path, path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    if fs::symlink_metadata(path).is_ok() {
        fs::remove_file(path).unwrap();
    }
    std::os::unix::fs::symlink(target, path).unwrap();
}

/// The deps payload, with or without the CLI's command envelope.
fn payload(stdout: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(stdout).expect("deps JSON output");
    value.get("result").cloned().unwrap_or(value)
}

fn linked_target(path: &Path) -> Option<PathBuf> {
    fs::read_link(path).ok().map(|target| {
        if target.is_absolute() {
            target
        } else {
            path.parent().unwrap_or_else(|| Path::new(".")).join(target)
        }
    })
}

fn registration_target(machine: &Machine) -> Option<String> {
    fs::canonicalize(machine.registration())
        .ok()
        .map(|path| path.display().to_string())
}

#[test]
fn completion_retires_the_registration_the_link_created() {
    let machine = Machine::new("effigy-bun-link-completion-");
    machine.plant_foreign_registration();

    let (success, stdout, stderr) = machine.run(&machine.link_args());

    assert!(success, "link failed: {stdout}\n{stderr}");
    let report = payload(&stdout);
    assert_eq!(report["report"]["outcome"], "applied");
    assert_eq!(
        report["report"]["registration_retirement"]["unregistered"],
        serde_json::json!([PACKAGE])
    );
    assert_eq!(
        fs::symlink_metadata(machine.registration())
            .map(|_| ())
            .ok(),
        None,
        "the machine-wide registration belongs to no live owner"
    );
    assert!(machine.claim_consumers().is_empty());
    // The link itself stays: the consumer symlink and the ledger.
    assert_eq!(
        fs::canonicalize(machine.consumer_link()).unwrap(),
        machine.library
    );
    assert!(machine.ledger_path().exists());
    // Another package's registration is not Effigy's to retire.
    assert!(linked_target(&machine.foreign_registration())
        .unwrap()
        .ends_with("other-package"));
}

#[test]
fn a_shared_registration_stays_and_only_this_consumers_claim_is_released() {
    let machine = Machine::new("effigy-bun-link-shared-");
    let sibling = machine.seed_sibling_claim();
    let library = fs::canonicalize(&machine.library).unwrap();
    replace_symlink(&library, &machine.registration());

    let (success, stdout, stderr) = machine.run(&machine.link_args());

    assert!(success, "link failed: {stdout}\n{stderr}");
    let report = payload(&stdout);
    assert_eq!(
        report["report"]["registration_retirement"]["released_shared"],
        serde_json::json!([PACKAGE])
    );
    assert_eq!(
        registration_target(&machine),
        Some(library.display().to_string())
    );
    assert_eq!(machine.claim_consumers(), vec![sibling]);
}

#[test]
fn interrupt_retires_only_the_registration_the_owner_created() {
    let machine = Machine::new("effigy-bun-link-interrupt-");
    machine.plant_foreign_registration();
    machine.mark("hold-consumer-link");
    let args = machine.link_args();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut owner = RunningOwner {
        machine: &machine,
        child: machine
            .command(&args)
            .spawn()
            .expect("spawn the effigy CLI in the fixture"),
    };
    let registration = machine.registration();
    if wait_until(Duration::from_secs(60), || {
        linked_target(&registration).is_some()
    }) {
        // The registration exists and `bun link` inside the consumer is
        // blocked: this is the window an interrupt has to clean up.
        wait_until(Duration::from_secs(60), || machine.marked("held-child.pid"));
        interrupt(&mut owner.child);
        let status = wait_for_exit(&mut owner.child);
        stop_recorded_child(&machine, "held-child.pid");
        assert!(
            !status.success(),
            "interrupted owner must not report success: {status:?}"
        );
        assert_eq!(
            fs::symlink_metadata(&registration).map(|_| ()).ok(),
            None,
            "the interrupted owner left its registration behind"
        );
        assert!(
            linked_target(&machine.foreign_registration()).is_some(),
            "the interrupt retired another package's registration"
        );
        // The interrupted owner never committed a claim, so nothing points at
        // the consumer either; only the registration needed retiring.
        assert!(machine.claim_consumers().is_empty());
        fs::remove_file(machine.home.join("hold-consumer-link")).unwrap();
        let (success, stdout, stderr) = machine.run(&machine.link_args());
        assert!(success, "relink after interrupt failed: {stdout}\n{stderr}");
        assert!(machine.claim_consumers().is_empty());
        assert_eq!(
            fs::symlink_metadata(machine.registration())
                .map(|_| ())
                .ok(),
            None
        );
        assert_eq!(
            fs::canonicalize(machine.consumer_link()).unwrap(),
            machine.library
        );
    } else {
        let status = wait_for_exit(&mut owner.child);
        stop_recorded_child(&machine, "held-child.pid");
        panic!(
            "the fixture never reached the interrupt window: {status:?}; calls: {}",
            fs::read_to_string(machine.home.join("calls.log")).unwrap_or_default()
        );
    }
}

#[test]
fn interrupt_stops_an_in_flight_registration_before_it_leaves_residue() {
    let machine = Machine::new("effigy-bun-link-register-race-");
    machine.mark("hold-register");
    let args = machine.link_args();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut owner = RunningOwner {
        machine: &machine,
        child: machine
            .command(&args)
            .spawn()
            .expect("spawn the effigy CLI in the fixture"),
    };
    assert!(
        wait_until(Duration::from_secs(60), || machine
            .marked("register-child.pid")),
        "the fixture never started registering; calls: {}",
        fs::read_to_string(machine.home.join("calls.log")).unwrap_or_default()
    );
    interrupt(&mut owner.child);
    let status = wait_for_exit(&mut owner.child);
    // The fixture child is stopped by the proof's Drop guard, not here: this
    // proof is that the owner's own cleanup stopped it.
    assert!(
        !status.success(),
        "interrupted owner must not report success: {status:?}"
    );
    // The stopped `bun link` would otherwise create the registration a moment
    // after its owner retired it.
    std::thread::sleep(Duration::from_millis(2_500));
    assert_eq!(
        fs::symlink_metadata(machine.registration())
            .map(|_| ())
            .ok(),
        None,
        "an in-flight registration process recreated residue after its owner died"
    );
    assert!(machine.claim_consumers().is_empty());
}

#[test]
fn interrupt_stops_a_wrapper_and_its_descendant_before_they_can_register() {
    let machine = Machine::new("effigy-bun-link-tree-");
    machine.mark("hold-register-tree");
    let args = machine.link_args();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut owner = RunningOwner {
        machine: &machine,
        child: machine
            .command(&args)
            .spawn()
            .expect("spawn the effigy CLI in the fixture"),
    };
    assert!(
        wait_until(Duration::from_secs(60), || machine
            .marked("register-tree-descendant.pid")),
        "the fixture never started the descendant; calls: {}",
        fs::read_to_string(machine.home.join("calls.log")).unwrap_or_default()
    );
    interrupt(&mut owner.child);
    let status = wait_for_exit(&mut owner.child);
    assert!(
        !status.success(),
        "interrupted owner must not report success: {status:?}"
    );
    // The descendant sleeps two seconds and then registers: nothing of the
    // wrapper's tree may be left to do that.
    std::thread::sleep(Duration::from_millis(2_500));
    assert_eq!(
        fs::symlink_metadata(machine.registration())
            .map(|_| ())
            .ok(),
        None,
        "a wrapper's descendant recreated the registration after cleanup"
    );
    assert!(
        !machine.marked("register-tree-descendant-wrote"),
        "the descendant survived the interrupt cleanup"
    );
    assert!(machine.claim_consumers().is_empty());
}

#[test]
fn a_refused_retirement_keeps_the_claim_for_unlink() {
    let machine = Machine::new("effigy-bun-link-refused-retirement-");
    machine.mark("fail-unlink");

    let (success, stdout, stderr) = machine.run(&machine.link_args());

    assert!(success, "link failed: {stdout}\n{stderr}");
    let report = payload(&stdout);
    assert_eq!(
        report["report"]["registration_retirement"]["failures"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    // Reversible break probe: the refused retirement leaves the registration
    // and its claim, and the exact-owned unlink retires both.
    assert!(registration_target(&machine).is_some());
    assert_eq!(machine.claim_consumers().len(), 1);

    fs::remove_file(machine.home.join("fail-unlink")).unwrap();
    let (success, stdout, stderr) = machine.run(&machine.unlink_args());
    assert!(success, "unlink failed: {stdout}\n{stderr}");
    assert_eq!(
        fs::symlink_metadata(machine.registration())
            .map(|_| ())
            .ok(),
        None
    );
    assert!(machine.claim_consumers().is_empty());
}

/// The running owner, stopped by recorded PID if the proof panics mid-window.
struct RunningOwner<'a> {
    machine: &'a Machine,
    child: Child,
}

impl Drop for RunningOwner<'_> {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Both recorded fixture children, so a panicking proof leaves none.
        stop_recorded_child(self.machine, "held-child.pid");
        stop_recorded_child(self.machine, "register-child.pid");
        stop_recorded_child(self.machine, "register-tree-child.pid");
        stop_recorded_child(self.machine, "register-tree-descendant.pid");
    }
}

fn wait_until(timeout: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn interrupt(child: &mut Child) {
    let pid = child.id() as i32;
    let sent = unsafe { libc::kill(pid, libc::SIGINT) };
    assert_eq!(sent, 0, "send SIGINT to the running owner");
}

fn wait_for_exit(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the interrupted owner did not exit");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The blocked fake `bun` is this fixture's own process; stop it by the PID it
/// recorded, never by pattern. The record is removed so a second stop cannot
/// reach a recycled PID.
fn stop_recorded_child(machine: &Machine, record: &str) {
    let record = machine.home.join(record);
    let Ok(raw) = fs::read_to_string(&record) else {
        return;
    };
    let _ = fs::remove_file(&record);
    let Ok(pid) = raw.trim().parse::<i32>() else {
        return;
    };
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
}
