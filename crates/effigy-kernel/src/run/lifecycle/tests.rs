use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const RECOVERY_CHILD: &str = "run::lifecycle::tests::layered_recovery_child";

fn layered_owner() -> ResourceId {
    let checkout = layered_checkout();
    ResourceId::environment(checkout, "layered-sentinel-recovery").unwrap()
}

fn layered_checkout() -> crate::id::CheckoutId {
    crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16])
}

#[test]
#[ignore]
fn layered_recovery_child() {
    let Some(home) = std::env::var_os("EFFIGY_LAYERED_RECOVERY_HOME") else {
        return;
    };
    let mode = std::env::var("EFFIGY_LAYERED_RECOVERY_MODE").unwrap();
    let home = std::path::Path::new(&home);
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let owner = layered_owner();
    let mut fake = super::super::tests::Fake::new(home);
    if mode == "owner" {
        let reservation = crate::workspace_session::WorkspaceActivationReservation::reserve(
            &cx,
            &root,
            &owner,
            Wait::FailFast,
        )
        .unwrap();
        let mut run = Run::begin_claimed_recoverable(
            &cx,
            &root,
            &super::super::tests::inv(owner),
            super::super::tests::opts(1),
            || Ok(super::super::tests::cleanup(&cx)),
            &mut fake,
            reservation.claim().unwrap(),
        )
        .unwrap_or_else(|(error, _)| panic!("claim failed: {error}"));
        let plan = LifecyclePlan {
            executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
                .unwrap(),
            placement: crate::supervise::Placement::Detached,
        };
        crate::fault::arm_from_env();
        let stage = std::env::var("EFFIGY_LAYERED_STAGE").unwrap();
        let marker = home.join("fault-stage");
        crate::fault::hook(Some(Box::new(move |point| {
            if format!("{point:?}") == stage {
                std::fs::write(&marker, &stage).expect("write exact fault-stage marker");
            }
        })));
        let result = run.enable_workspace_custody(&plan);
        panic!("owner fault did not kill process: {result:?}");
    }
    assert_eq!(mode, "recover");
    crate::fault::arm_from_env();
    let before = Store::open(&cx, &root).unwrap();
    let record = journal::run(before.conn(), super::super::tests::id(1)).unwrap();
    eprintln!("pre-recovery phase: {:?}", record.phase);
    assert!(
        journal::effects(before.conn(), super::super::tests::id(1))
            .unwrap()
            .is_empty(),
        "this test does not cover external integration effects"
    );
    drop(before);
    super::super::settle::settle_interrupted_wait(
        &cx,
        &root,
        (&owner, super::super::tests::id(1)),
        super::super::tests::next(),
        &super::super::tests::cleanup(&cx),
        &mut fake,
        Wait::Until(Instant::now() + Duration::from_secs(20)),
    )
    .expect("actual dead-owner settlement");
    let reservation = {
        let observed = Store::read(&cx.operation().observe().unwrap(), &root);
        let reader = observed.verified().expect("journal reobservation");
        assert!(
            matches!(
                super::super::status(
                    &cx.operation().observe().unwrap(),
                    &root,
                    reader,
                    super::super::tests::id(1),
                    super::super::tests::next()
                ),
                Observed::Verified(super::super::RunStatus::Closed { complete: true, .. }, _)
            ),
            "closed and complete is required"
        );
        crate::workspace_session::WorkspaceActivationReservation::recover(reader, &owner)
            .verified()
            .cloned()
            .flatten()
            .expect("retained reservation")
    };
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("cancel after settlement");
    let next = crate::workspace_session::WorkspaceActivationReservation::reserve(
        &cx,
        &root,
        &owner,
        Wait::FailFast,
    )
    .expect("fresh reservation");
    let mut run = Run::begin_claimed_recoverable(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(2),
        || Ok(super::super::tests::cleanup(&cx)),
        &mut fake,
        next.claim().unwrap(),
    )
    .unwrap_or_else(|(error, _)| panic!("fresh admission failed: {error}"));
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    run.enable_workspace_custody(&plan)
        .expect("fresh sentinel and checkout backup startup");
    assert!(run.sentinel.is_some(), "fresh sentinel must be retained");
    run.verify_custody().expect("fresh checkout fence");
    run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake)
        .unwrap_or_else(|(error, _)| panic!("fresh cleanup failed: {error}"));
    next.cancel(&cx, &root, Wait::FailFast)
        .expect("fresh reservation cleanup");
    let final_store = Store::open(&cx, &root).unwrap();
    let active: i64 = final_store
        .conn()
        .query_row(
            "SELECT count(*) FROM runs WHERE owner=?1 AND phase<>'closed'",
            [layered_owner().name().to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let reservations: i64 = final_store.conn().query_row("SELECT count(*) FROM workspace_activation_reservations WHERE environment=?1 AND state='reserved'", [layered_owner().name().to_string()], |row| row.get(0)).unwrap();
    let custody: i64 = final_store
        .conn()
        .query_row(
            "SELECT count(*) FROM workspace_session_custody",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        (active, reservations, custody),
        (0, 0, 0),
        "no open run, reservation or session custody may remain"
    );
    drop(final_store);
    let final_read = Store::read(&cx.operation().observe().unwrap(), &root);
    let reader = final_read.verified().expect("final journal reobservation");
    for id in [super::super::tests::id(1), super::super::tests::id(2)] {
        assert!(
            matches!(
                super::super::status(
                    &cx.operation().observe().unwrap(),
                    &root,
                    reader,
                    id,
                    super::super::tests::next()
                ),
                Observed::Verified(super::super::RunStatus::Closed { complete: true, .. }, _)
            ),
            "both exact runs must be closed complete"
        );
    }
}

struct LayeredHome {
    dir: Option<tempfile::TempDir>,
    complete: bool,
}
impl LayeredHome {
    fn new() -> Self {
        Self {
            dir: Some(effigy_test_support::short_tempdir("lr-").unwrap()),
            complete: false,
        }
    }
    fn path(&self) -> &std::path::Path {
        self.dir.as_ref().unwrap().path()
    }
    fn finish(&mut self) {
        self.complete = true;
    }
}
impl Drop for LayeredHome {
    fn drop(&mut self) {
        if !self.complete {
            if let Some(dir) = self.dir.take() {
                let path = dir.keep();
                match std::fs::read_to_string(path.join("helpers.jsonl"))
                    .ok()
                    .and_then(|text| layered_helper_identities(&text).ok())
                {
                    Some(helpers) if !helpers.is_empty() => {
                        let deadline = Instant::now() + Duration::from_secs(35);
                        while !helpers.iter().all(layered_helper_gone) && Instant::now() < deadline
                        {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        if !helpers.iter().all(layered_helper_gone) {
                            eprintln!("helper custody Unknown: {helpers:?}");
                        }
                    }
                    _ => eprintln!("helper identity inventory Unknown"),
                }
                eprintln!("unsettled fixture retained at {path:?}");
            }
        }
    }
}

struct LayeredChild {
    child: Option<std::process::Child>,
    fixture: std::path::PathBuf,
    pid: u32,
    identity: Option<ProcessIdentity>,
    argv: Vec<Vec<u8>>,
    fail_kill: bool,
    fail_wait: bool,
}
impl LayeredChild {
    fn spawn(
        command: &mut std::process::Command,
        fixture: &std::path::Path,
    ) -> std::io::Result<Self> {
        use std::os::unix::ffi::OsStrExt;
        let argv = std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|arg| arg.as_bytes().to_vec())
            .collect();
        let child = command.spawn()?;
        let pid = child.id();
        // Install the child guard before the fallible identity observation.
        let mut owned = Self {
            child: Some(child),
            fixture: fixture.into(),
            pid,
            identity: None,
            argv,
            fail_kill: false,
            fail_wait: false,
        };
        owned.identity = crate::ident::process_identity(pid).verified().cloned();
        Ok(owned)
    }
    fn record_unknown(&self, reason: &str) -> bool {
        use std::os::unix::ffi::OsStrExt;
        let record = self
            .fixture
            .join(format!("child-{}-unknown.json", self.pid));
        let message = if self.identity.is_some() {
            "harness failed; child state Unknown; clean by recorded PID after identity check"
        } else {
            "harness failed; child state Unknown; do not signal without independently established identity"
        };
        let body = serde_json::json!({ "message": message, "pid": self.pid, "identity": self.identity, "argv_bytes": self.argv, "fixture": self.fixture.to_string_lossy(), "fixture_bytes": self.fixture.as_os_str().as_bytes(), "reason": reason });
        let write = std::fs::write(&record, body.to_string());
        match &write {
            Ok(()) => eprintln!("{message}: pid {} identity {:?} argv {:?} fixture {:?}; record {:?}; {reason}", self.pid, self.identity, self.argv, self.fixture, record),
            Err(error) => eprintln!("{message}: pid {} identity {:?} argv {:?} fixture {:?}; adjacent record unavailable ({error}); {reason}", self.pid, self.identity, self.argv, self.fixture),
        }
        write.is_ok()
    }
    fn kill(&mut self) -> std::io::Result<()> {
        let result = if self.fail_kill {
            Err(std::io::Error::other("injected kill failure"))
        } else {
            self.child().kill()
        };
        if let Err(error) = &result {
            self.record_unknown(&format!("kill failed: {error}"));
        }
        result
    }
    fn child(&mut self) -> &mut std::process::Child {
        self.child.as_mut().unwrap()
    }
    fn wait(&mut self, limit: Duration) -> std::io::Result<std::process::ExitStatus> {
        if self.fail_wait {
            let error = std::io::Error::other("injected wait failure");
            self.record_unknown(&format!("wait failed: {error}"));
            return Err(error);
        }
        let deadline = Instant::now() + limit;
        loop {
            match self.child().try_wait() {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok(status);
                }
                Ok(None) => {}
                Err(error) => {
                    self.record_unknown(&format!("wait failed: {error}"));
                    return Err(error);
                }
            }
            if Instant::now() >= deadline {
                let pid = self.child().id();
                let kill = self.kill();
                self.record_unknown(&format!("wait timed out; kill result {kill:?}"));
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("owned fixture child {pid} timed out; kill result {kill:?}; custody retained for cleanup")));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for LayeredChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let pid = child.id();
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => {
                self.record_unknown(&format!("drop state check failed: {error}"));
                return;
            }
        }
        let kill = child.kill();
        if let Err(error) = &kill {
            self.record_unknown(&format!("drop kill failed: {error}"));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    self.record_unknown(&format!("drop wait failed: {error}"));
                    return;
                }
            }
        }
        self.record_unknown(&format!("child {pid} reap timed out after kill {kill:?}"));
    }
}

fn layered_helper_gone(identity: &ProcessIdentity) -> bool {
    match crate::ident::process_identity(identity.pid) {
        Observed::Absent(_) => true,
        Observed::Verified(current, _) if current != *identity => true,
        Observed::Verified(_, _) => matches!(
            crate::ident::process_state(identity.pid),
            Observed::Verified(crate::ident::ProcessState::Zombie, _)
        ),
        Observed::Unknown(_) => false,
    }
}

fn layered_helper_identities(log: &str) -> Result<Vec<ProcessIdentity>, serde_json::Error> {
    log.lines()
        .map(|line| serde_json::from_str(line.strip_prefix("backup ").unwrap_or(line)))
        .collect()
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "child entry for the image-observation fixture"]
fn image_observation_child() {
    use std::io::Read;
    let mut byte = [0u8; 1];
    let _ = std::io::stdin().read(&mut byte);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_live_child_image_is_path_evidence_not_mapped_image_proof() {
    use std::process::{Command, Stdio};

    let mut home = LayeredHome::new();
    let image = crate::supervise::image_identity::TestCreatorImage::capture()
        .expect("creator image observation");
    let mut command = Command::new(image.launch_path().expect("creator path"));
    command
        .args([
            "--ignored",
            "--exact",
            "run::lifecycle::tests::image_observation_child",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut same = LayeredChild::spawn(&mut command, home.path()).expect("same-image child");
    let identity = same.identity.clone().expect("same-image child identity");
    assert!(
        same.child().try_wait().unwrap().is_none(),
        "child must be live"
    );
    image.verify(&identity).expect("matching child observation");
    drop(same.child().stdin.take());
    assert!(same
        .wait(Duration::from_secs(3))
        .expect("reap same-image child")
        .success());

    let mut other_command = Command::new("/bin/sleep");
    other_command
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut other = LayeredChild::spawn(&mut other_command, home.path()).expect("different child");
    let other_identity = other.identity.clone().expect("different child identity");
    assert_eq!(
        image.verify(&other_identity).unwrap_err().to_string(),
        "helper image or process identity changed"
    );
    other.kill().expect("stop exact different child");
    other
        .wait(Duration::from_secs(3))
        .expect("reap different child");
    // proc_pidpath plus stat observes a pathname, not the mapped image; an ABA
    // replacement between observation and exec remains an accepted limitation.
    home.finish();
}

#[cfg(target_os = "macos")]
#[test]
fn macos_creator_same_path_replacement_refuses() {
    let mut home = LayeredHome::new();
    let path = home.path().join("image");
    let replacement = home.path().join("replacement");
    std::fs::write(&path, b"original image").unwrap();
    let image = crate::supervise::image_identity::TestCreatorImage::capture_fixture_path(&path)
        .expect("capture fixture image");
    std::fs::write(&replacement, b"replacement image").unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert_eq!(
        image.launch_path().unwrap_err().to_string(),
        "creator image changed"
    );
    std::fs::remove_file(&path).unwrap();
    home.finish();
}

#[test]
#[ignore = "requires an explicitly supplied older Effigy executable"]
fn old_helpers_wait_for_config_and_exit_on_eof() {
    use std::process::{Command, Stdio};

    let binary =
        std::env::var_os("EFFIGY_OLD_CUSTODY_BINARY").expect("name the older Effigy executable");
    for entry in ["__kernel-sentinel", "__kernel-supervise"] {
        let mut home = LayeredHome::new();
        let mut command = Command::new(&binary);
        command
            .arg(entry)
            .env_clear()
            .env("HOME", home.path())
            .env("TMPDIR", home.path())
            .current_dir(home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = LayeredChild::spawn(&mut command, home.path()).unwrap();
        assert!(child.identity.is_some(), "old helper identity unavailable");
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            child.child().try_wait().unwrap().is_none(),
            "{entry} exited before EOF"
        );
        assert_eq!(
            std::fs::read_dir(home.path()).unwrap().count(),
            0,
            "{entry} wrote before config"
        );
        drop(child.child().stdin.take());
        let status = child.wait(Duration::from_secs(3));
        assert!(status.is_ok(), "{entry} did not exit after EOF: {status:?}");
        assert_eq!(
            std::fs::read_dir(home.path()).unwrap().count(),
            0,
            "{entry} wrote after EOF"
        );
        home.finish();
    }
}

#[test]
#[ignore = "requires an explicitly supplied older Effigy executable"]
fn diagnose_old_sentinel_before_config() {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let binary =
        std::env::var_os("EFFIGY_OLD_CUSTODY_BINARY").expect("name the older Effigy executable");
    let mut home = LayeredHome::new();
    let log_path = home.path().join("stderr.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut command = Command::new(binary);
    command
        .arg("__kernel-sentinel")
        .env_clear()
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .current_dir(home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    let mut child = LayeredChild::spawn(&mut command, home.path()).unwrap();
    assert!(child.identity.is_some(), "old helper identity unavailable");
    std::thread::sleep(Duration::from_millis(200));
    let early = child.child().try_wait().unwrap();
    drop(child.child().stdin.take());
    let status = child
        .wait(Duration::from_secs(3))
        .expect("reap exact child");
    let mut err = String::new();
    std::fs::File::open(&log_path)
        .unwrap()
        .take(8192)
        .read_to_string(&mut err)
        .unwrap();
    std::fs::remove_file(log_path).unwrap();
    let entries = std::fs::read_dir(home.path()).unwrap().count();
    println!("early={early:?}; final={status:?}; stderr={err:?}; fixture_entries={entries}");
    assert_eq!(entries, 0, "fixture changed before or after EOF");
    home.finish();
}

#[test]
fn layered_fixture_unwind_reaps_its_exact_owned_child() {
    let mut path = std::path::PathBuf::new();
    let mut pid = 0u32;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let home = LayeredHome::new();
        path = home.path().into();
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("30");
        let mut owner = LayeredChild::spawn(&mut command, home.path()).unwrap();
        pid = owner.child().id();
        let _owner = owner;
        panic!("test-only unwind after owned child spawn");
    }));
    assert_ne!(pid, 0, "fixture spawn failed; retained fixture {path:?}");
    let mut status = 0;
    // SAFETY: this is our exact child; WNOHANG does not block or signal it.
    let waited = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
    let already_reaped =
        waited == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
    if waited == pid as i32 || already_reaped {
        std::fs::remove_dir_all(&path).unwrap();
    }
    assert!(outcome.is_err());
    assert!(
        waited == pid as i32 || already_reaped,
        "exact child {pid} not reaped; retained fixture {path:?}"
    );
}

#[test]
fn layered_fixture_records_injected_wait_and_kill_unknowns() {
    let mut home = LayeredHome::new();
    let mut command = std::process::Command::new("/bin/sleep");
    command.arg("30");
    let mut child = LayeredChild::spawn(&mut command, home.path()).unwrap();
    let pid = child.pid;
    let record = home.path().join(format!("child-{pid}-unknown.json"));
    child.fail_wait = true;
    let wait_error = child.wait(Duration::from_secs(1));
    let wait_record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    child.fail_wait = false;
    child.fail_kill = true;
    let kill_error = child.kill();
    let kill_record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    let identity = child.identity.take();
    let unknown_written = child.record_unknown("injected identity unavailable");
    let unknown_record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    child.identity = identity;
    let fixture = child.fixture.clone();
    child.fixture = home.path().join("missing-directory");
    let unavailable = child.record_unknown("injected unwritable record");
    child.fixture = fixture;
    child.fail_kill = false;
    let killed = child.kill();
    let reaped = child.wait(Duration::from_secs(3));
    if killed.is_ok() && reaped.is_ok() {
        home.finish();
    }
    assert!(wait_error.is_err() && kill_error.is_err());
    assert!(unknown_written && !unavailable);
    assert!(unknown_record["identity"].is_null());
    assert!(unknown_record["message"]
        .as_str()
        .unwrap()
        .contains("do not signal without independently established identity"));
    for (record, expected) in [(wait_record, "wait failed"), (kill_record, "kill failed")] {
        assert_eq!(record["pid"].as_u64(), Some(u64::from(pid)));
        assert!(record["reason"].as_str().unwrap().contains(expected));
        assert!(!record["argv_bytes"].as_array().unwrap().is_empty());
        assert_eq!(record["fixture"].as_str(), home.path().to_str());
        assert!(
            !record["identity"].is_null(),
            "test child identity must be observed"
        );
    }
    assert!(
        killed.is_ok() && reaped.is_ok(),
        "exact test child not reaped; fixture retained"
    );
}

#[test]
fn layered_workspace_sentinel_dead_owner_recovery() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    for point in [
        crate::fault::Point::WorkspaceSentinelAfterSpawn,
        crate::fault::Point::WorkspaceSentinelPartialConfig,
        crate::fault::Point::WorkspaceSentinelAfterConfig,
        crate::fault::Point::WorkspaceSentinelAfterAck,
    ] {
        let mut home = LayeredHome::new();
        let log = home.path().join("helpers.jsonl");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([RECOVERY_CHILD, "--exact", "--ignored", "--test-threads=1"])
            .env("EFFIGY_LAYERED_RECOVERY_HOME", home.path())
            .env("EFFIGY_LAYERED_RECOVERY_MODE", "owner")
            .env("EFFIGY_LAYERED_STAGE", format!("{point:?}"))
            .env("EFFIGY_KERNEL_TEST_HELPERS", &log)
            .env(
                "EFFIGY_KERNEL_TEST_FAULT",
                crate::fault::env_spec(point, 0, crate::fault::Fault::Stall),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        let mut owner = LayeredChild::spawn(&mut command, home.path()).unwrap();
        let owner_pid = owner.child().id();
        let deadline = Instant::now() + Duration::from_secs(30);
        let stopped = loop {
            if matches!(
                crate::ident::process_state(owner_pid),
                Observed::Verified(crate::ident::ProcessState::Stopped, _)
            ) {
                break true;
            }
            if Instant::now() >= deadline || matches!(owner.child().try_wait(), Ok(Some(_))) {
                break false;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let evidence =
            (|| -> Result<(ProcessIdentity, crate::gate::CheckoutFenceIdentity), String> {
                if !stopped {
                    return Err("owner did not stop at the fault".into());
                }
                let marker = std::fs::read_to_string(home.path().join("fault-stage"))
                    .map_err(|error| error.to_string())?;
                if marker != format!("{point:?}") {
                    return Err(format!("fault-stage marker differs: {marker}"));
                }
                let actual = crate::ident::process_identity(owner_pid)
                    .verified()
                    .cloned()
                    .ok_or("owner identity unavailable")?;
                let cx = super::super::tests::context();
                let root =
                    StateRoot::effects(&cx, home.path()).map_err(|error| error.to_string())?;
                let store = Store::open(&cx, &root).map_err(|error| error.to_string())?;
                let recorded: Option<String> = store
                    .conn()
                    .query_row(
                        "SELECT owner_identity FROM runs WHERE run_id=?1",
                        [super::super::tests::id(1).to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let recorded: ProcessIdentity =
                    serde_json::from_str(recorded.as_deref().ok_or("owner identity not recorded")?)
                        .map_err(|error| error.to_string())?;
                if recorded != actual {
                    return Err("recorded owner identity differs from stopped process".into());
                }
                let binding: (String, Option<String>) = store.conn().query_row("SELECT state,environment_run FROM workspace_activation_reservations WHERE environment=?1", [layered_owner().name().to_string()], |row| Ok((row.get(0)?, row.get(1)?))).map_err(|error| error.to_string())?;
                if binding
                    != (
                        "reserved".into(),
                        Some(super::super::tests::id(1).to_string()),
                    )
                {
                    return Err(format!("reservation binding differs: {binding:?}"));
                }
                let probe = Gate::probe(
                    &cx.operation()
                        .observe()
                        .map_err(|error| format!("{error:?}"))?,
                    &root,
                    &layered_owner(),
                );
                let token = crate::store::token(&layered_checkout());
                let checkout_fence = Gate::checkout_fence_identity(
                    &cx.operation()
                        .observe()
                        .map_err(|error| format!("{error:?}"))?,
                    &root,
                    &token,
                )
                .verified()
                .copied()
                .ok_or("checkout fence identity unavailable")?;
                match probe {
                    Observed::Verified(crate::gate::GateState::Held(holder), _)
                        if holder.verified().is_some_and(|holder| {
                            holder.pid == actual.pid
                                && holder.run.as_deref()
                                    == Some(super::super::tests::id(1).to_string().as_str())
                        }) =>
                    {
                        Ok((actual, checkout_fence))
                    }
                    other => Err(format!("exact stopped owner gate not observed: {other:?}")),
                }
            })();
        let kill = owner.kill();
        let owner_status = owner.wait(Duration::from_secs(40));
        let log_text = match std::fs::read_to_string(&log) {
            Ok(text) => text,
            Err(error) => {
                panic!(
                    "{point:?}: helper identity log unavailable ({error}); fixture {:?} retained",
                    home.path()
                );
            }
        };
        let helpers = match layered_helper_identities(&log_text) {
            Ok(helpers) => helpers,
            Err(error) => {
                panic!(
                    "{point:?}: invalid helper identity ({error}); fixture {:?} retained",
                    home.path()
                );
            }
        };
        if helpers.is_empty() {
            panic!(
                "{point:?}: no verified helper identity; fixture {:?} retained",
                home.path()
            );
        }
        let deadline = Instant::now() + Duration::from_secs(40);
        while !helpers.iter().all(layered_helper_gone) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !helpers.iter().all(layered_helper_gone) {
            panic!("{point:?}: helper custody remains live or unknown; fixture {:?} retained, identities {helpers:?}", home.path());
        }
        let (owner_identity, checkout_fence) =
            evidence.unwrap_or_else(|error| panic!("{point:?}: pre-kill evidence failed: {error}"));
        assert!(
            kill.is_ok(),
            "{point:?}: could not signal exact owned process: {kill:?}"
        );
        assert!(
            matches!(owner_status, Ok(status) if status.signal() == Some(libc::SIGKILL)),
            "{point:?}: owner was not killed at the requested boundary"
        );
        assert_eq!(
            helpers.len(),
            1,
            "{point:?}: exact helper identity is required"
        );
        let cx = super::super::tests::context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        assert!(
            matches!(
                Gate::probe(&cx.operation().observe().unwrap(), &root, &layered_owner()),
                Observed::Verified(crate::gate::GateState::Free, _)
            ),
            "{point:?}: owner gate not free"
        );
        drop(root);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                RECOVERY_CHILD,
                "--exact",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("EFFIGY_LAYERED_RECOVERY_HOME", home.path())
            .env("EFFIGY_LAYERED_RECOVERY_MODE", "recover")
            .env("EFFIGY_LAYERED_BACKUP_ROLES", "1")
            .env("EFFIGY_KERNEL_TEST_HELPERS", &log)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        let mut recovery = LayeredChild::spawn(&mut command, home.path()).unwrap();
        let recovered = recovery.wait(Duration::from_secs(45));
        let all_log = match std::fs::read_to_string(&log) {
            Ok(text) => text,
            Err(error) => {
                panic!(
                    "{point:?}: fresh helper log unavailable ({error}); fixture {:?} retained",
                    home.path()
                );
            }
        };
        let all_helpers = match layered_helper_identities(&all_log) {
            Ok(helpers) => helpers,
            Err(error) => {
                panic!(
                    "{point:?}: invalid fresh helper identity ({error}); fixture {:?} retained",
                    home.path()
                );
            }
        };
        let roles: Vec<bool> = all_log
            .lines()
            .map(|line| line.starts_with("backup "))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(40);
        while !all_helpers.iter().all(layered_helper_gone) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !all_helpers.iter().all(layered_helper_gone) {
            panic!("{point:?}: fresh helper custody remains live or unknown; fixture {:?} retained, identities {all_helpers:?}", home.path());
        }
        if all_helpers.len() != 3 {
            panic!("{point:?}: expected old sentinel, fresh sentinel and fresh backup identities; observed {all_helpers:?}, recovery {recovered:?}; fixture {:?} retained", home.path());
        }
        assert_eq!(
            roles,
            [false, false, true],
            "{point:?}: original/fresh sentinel and backup roles must be explicit"
        );
        assert_eq!(
            all_helpers.first(),
            helpers.first(),
            "{point:?}: original sentinel identity must remain accounted for"
        );
        assert!(
            matches!(recovered, Ok(status) if status.success()),
            "{point:?}: real recovery or fresh start failed: {recovered:?}"
        );
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        assert!(
            matches!(
                Gate::probe(&cx.operation().observe().unwrap(), &root, &layered_owner()),
                Observed::Verified(crate::gate::GateState::Free, _)
            ),
            "{point:?}: owner gate stranded after recovery"
        );
        let token = crate::store::token(&layered_checkout());
        let retirement =
            Gate::checkout_retirement(&cx, &root, &token, checkout_fence, Wait::FailFast)
                .expect("exact checkout fence must have no remaining shared custody");
        drop(retirement);
        eprintln!("{point:?}: stopped owner {:?} and reserved gate verified, exact owner killed, original helper {:?} not active, recovery entry and fresh Env sentinel/backup startup and settlement succeeded; observed helpers {:?} not active (zombie reaping not determined)", owner_identity, helpers[0], all_helpers);
        home.finish();
    }
}

fn reap_test_child(identity: &ProcessIdentity) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut status = 0;
        // SAFETY: this is the exact helper child spawned by the test.
        let waited = unsafe { libc::waitpid(identity.pid as i32, &mut status, libc::WNOHANG) };
        if waited != 0 {
            break;
        }
        if Instant::now() >= deadline {
            // SAFETY: only the exact unreaped child spawned by this test.
            unsafe {
                libc::kill(identity.pid as i32, libc::SIGKILL);
                libc::waitpid(identity.pid as i32, &mut status, 0);
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn check_workspace_sentinel_start_fault(point: crate::fault::Point) {
    let home = effigy_test_support::short_tempdir("ls-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "sentinel-start-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner.clone()),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let helper_log = home.path().join("sentinel.jsonl");
    crate::fault::set_helper_log(Some(helper_log.clone()));
    crate::fault::arm_point(point, crate::fault::Fault::Enospc);
    let start = run.enable_workspace_custody(&plan);
    let run_id = run.id();
    let fired = crate::fault::fired();
    crate::fault::arm(None);
    let before_retry = run
        .partial_sentinel
        .as_ref()
        .map(|partial| partial.child_id());
    let retry = run.enable_workspace_custody(&plan);
    let after_retry = run
        .partial_sentinel
        .as_ref()
        .map(|partial| partial.child_id());
    crate::fault::set_helper_log(None);
    let settlement =
        run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    let unsettled = settlement.is_err();
    let retained = settlement.as_ref().err().and_then(|(_, owner)| {
        owner
            .partial_sentinel
            .as_ref()
            .map(|partial| (owner.id(), partial.child_id()))
    });
    let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
    let log = std::fs::read_to_string(helper_log).unwrap();
    let mut recorded = Vec::new();
    for line in log.lines() {
        if let Ok(child) = serde_json::from_str::<ProcessIdentity>(line) {
            recorded.push(child.pid);
            reap_test_child(&child);
        }
    }
    drop(settlement); // Fixture teardown, not a cleanup receipt.
    assert!(fired && start.is_err());
    assert_eq!(recorded.len(), 1);
    assert!(
        retry.err().is_some_and(|error| error
            .to_string()
            .contains("workspace sentinel startup remains unsettled")),
        "unsettled startup must name retained partial custody"
    );
    assert_eq!(before_retry, after_retry);
    assert_eq!(retained, Some((run_id, recorded[0])));
    assert!(matches!(
        gate,
        Observed::Verified(crate::gate::GateState::Held(_), _)
    ));
    assert!(
        unsettled,
        "a started sentinel with unknown custody must not settle cleanly"
    );
}

#[test]
fn sentinel_start_failure_keeps_workspace_custody_unsettled() {
    for point in [
        crate::fault::Point::WorkspaceSentinelAfterSpawn,
        crate::fault::Point::WorkspaceSentinelPartialConfig,
        crate::fault::Point::WorkspaceSentinelAfterConfig,
        crate::fault::Point::WorkspaceSentinelAfterAck,
    ] {
        check_workspace_sentinel_start_fault(point);
    }
}

#[test]
fn partial_sentinel_close_retains_owner_on_fault_and_reaps_exact_child() {
    let home = effigy_test_support::short_tempdir("sr-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "partial-sentinel-close").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner.clone()),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let helper_log = home.path().join("sentinel.jsonl");
    crate::fault::set_helper_log(Some(helper_log.clone()));
    crate::fault::arm_point(
        crate::fault::Point::WorkspaceSentinelAfterSpawn,
        crate::fault::Fault::Enospc,
    );
    let start = run.enable_workspace_custody(&plan);
    crate::fault::arm(None);
    crate::fault::set_helper_log(None);
    let run_id = run.id();
    let child_id = run.partial_sentinel.as_ref().map(|child| child.child_id());
    let prechecks = run.require_environment_gate().is_ok()
        && journal::check_run_fence(run.store.conn(), run.id, &run.owner).is_ok();
    let expired = cx.operation().cleanup(Duration::ZERO).unwrap();
    let expired_result = run.close_partial_workspace_sentinel(&expired);
    let input_after_expiry = run
        .partial_sentinel
        .as_ref()
        .is_some_and(|child| child.input_open());
    let after_expiry = run.partial_sentinel.as_ref().map(|child| child.child_id());
    crate::fault::arm_point(
        crate::fault::Point::WorkspaceSentinelCloseWait,
        crate::fault::Fault::Enospc,
    );
    let first = run.close_partial_workspace_sentinel(&super::super::tests::cleanup(&cx));
    let fired = crate::fault::fired();
    crate::fault::arm(None);
    let after_fault = run.partial_sentinel.as_ref().map(|child| child.child_id());
    let retry = run.close_partial_workspace_sentinel(&super::super::tests::cleanup(&cx));
    let exact_run_guard = run.guard.covers(&owner);
    let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
    let settlement =
        run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    let retained = settlement.as_ref().err().and_then(|(_, owner)| {
        owner
            .partial_sentinel
            .as_ref()
            .map(|child| (owner.id(), child.child_id()))
    });
    let log = std::fs::read_to_string(helper_log).unwrap();
    let identities: Vec<ProcessIdentity> = log
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if retry.is_err() {
        for child in &identities {
            reap_test_child(child);
        }
    }
    drop(settlement); // Fixture teardown only; not a settlement receipt.
    assert!(start.is_err() && first.is_err() && fired);
    assert!(expired_result.is_err() && input_after_expiry);
    assert!(prechecks, "gate and fence must pass before expired cleanup");
    assert_eq!(after_expiry, child_id);
    assert_eq!(identities.len(), 1);
    assert_eq!(child_id, Some(identities[0].pid));
    assert_eq!(after_fault, child_id);
    let status = retry.expect("the exact pre-config child should be reaped");
    assert!(
        !status.success(),
        "a pre-config failure is not a clean exit"
    );
    assert_eq!(retained, Some((run_id, identities[0].pid)));
    assert!(
        exact_run_guard,
        "the same run must retain the environment gate"
    );
    assert!(matches!(
        gate,
        Observed::Verified(crate::gate::GateState::Held(_), _)
    ));
}

#[test]
fn configured_partial_sentinel_close_preserves_unknown_custody() {
    let home = effigy_test_support::short_tempdir("su-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "configured-partial-close").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner.clone()),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let helper_log = home.path().join("sentinel.jsonl");
    crate::fault::set_helper_log(Some(helper_log.clone()));
    crate::fault::arm_point(
        crate::fault::Point::WorkspaceSentinelAfterConfig,
        crate::fault::Fault::Enospc,
    );
    let start = run.enable_workspace_custody(&plan);
    crate::fault::arm(None);
    crate::fault::set_helper_log(None);
    let before = run.partial_sentinel.as_ref().map(|child| child.child_id());
    let input_before = run
        .partial_sentinel
        .as_ref()
        .is_some_and(|child| child.input_open());
    let close = run.close_partial_workspace_sentinel(&super::super::tests::cleanup(&cx));
    let after = run.partial_sentinel.as_ref().map(|child| child.child_id());
    let input_after = run
        .partial_sentinel
        .as_ref()
        .is_some_and(|child| child.input_open());
    let exact_run_guard = run.guard.covers(&owner);
    let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
    let settlement =
        run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    let unsettled = settlement.is_err();
    let retained = settlement.as_ref().err().and_then(|(_, run)| {
        run.partial_sentinel.as_ref().map(|child| {
            (
                child.child_id(),
                child.input_open(),
                run.guard.covers(&owner),
            )
        })
    });
    let log = std::fs::read_to_string(helper_log).unwrap();
    let identities: Vec<ProcessIdentity> = log
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    for child in &identities {
        reap_test_child(child);
    }
    drop(settlement); // Fixture teardown only.
    assert!(start.is_err() && close.is_err());
    assert!(unsettled && exact_run_guard);
    assert_eq!(retained, before.map(|id| (id, true, true)));
    assert_eq!(identities.len(), 1);
    assert_eq!(before, Some(identities[0].pid));
    assert_eq!(after, before);
    assert!(
        input_before && input_after,
        "ambiguous configuration must keep the input pipe"
    );
    assert!(matches!(
        gate,
        Observed::Verified(crate::gate::GateState::Held(_), _)
    ));
}

fn check_workspace_sentinel_interruption(deadline: bool) {
    let home = effigy_test_support::short_tempdir("sc-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "sentinel-cancel-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let helper_log = home.path().join("sentinel.jsonl");
    crate::fault::set_helper_log(Some(helper_log.clone()));
    let cancel = cx.operation().cancel_token().clone();
    let until = Instant::now() + Duration::from_secs(2);
    if deadline {
        run.cx = run.cx.child_until(until);
    }
    let reached = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&reached);
    crate::fault::hook(Some(Box::new(move |point| {
        if point == crate::fault::Point::WorkspaceSentinelAfterSpawn {
            observed.fetch_add(1, Ordering::SeqCst);
            if deadline {
                while Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(5));
                }
            } else {
                cancel.cancel(crate::ctx::StopCause::Requested);
            }
        }
    })));
    let result = run.enable_workspace_custody(&plan);
    crate::fault::hook(None);
    crate::fault::set_helper_log(None);
    let retained = run
        .partial_sentinel
        .as_ref()
        .map(|partial| partial.child_id());
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    let logged: Vec<ProcessIdentity> = std::fs::read_to_string(helper_log)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    for child in &logged {
        reap_test_child(child);
    }
    drop(cleanup); // Exact-child fixture teardown only.
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    let error = result.expect_err("interrupted startup must refuse");
    if deadline {
        assert!(
            error.to_string().contains("TimedOut"),
            "deadline must cause refusal: {error}"
        );
    }
    assert_eq!(logged.len(), 1);
    assert_eq!(
        retained,
        Some(logged[0].pid),
        "interruption must retain the started child"
    );
}

#[test]
fn workspace_sentinel_cancellation_after_spawn_retains_partial() {
    check_workspace_sentinel_interruption(false);
}

#[test]
fn workspace_sentinel_deadline_after_spawn_retains_partial() {
    check_workspace_sentinel_interruption(true);
}

#[test]
fn enabling_existing_custody_does_not_spawn_a_second_sentinel() {
    let home = effigy_test_support::short_tempdir("ld-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "duplicate-sentinel-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    run.enable_custody(&plan).unwrap();
    let identity = run.sentinel.as_ref().unwrap().identity.clone();
    let helper_log = home.path().join("new-helpers.jsonl");
    crate::fault::set_helper_log(Some(helper_log.clone()));
    let retry = run.enable_custody(&plan);
    crate::fault::set_helper_log(None);
    let logged = match std::fs::read_to_string(helper_log) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    };
    let same = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    if let Ok(contents) = &logged {
        for line in contents.lines() {
            if let Ok(child) = serde_json::from_str::<ProcessIdentity>(line) {
                reap_test_child(&child);
            }
        }
    }
    let logged = logged.expect("helper log must be absent or readable");
    assert!(retry
        .err()
        .is_some_and(|error| error.to_string().contains("sentinel already registered")));
    assert_eq!(same, Some(identity));
    assert!(
        logged.is_empty(),
        "duplicate enablement spawned a helper: {logged}"
    );
    assert!(cleanup.is_ok());
}

#[test]
fn cancelled_registration_keeps_the_exact_sentinel_for_cleanup() {
    let home = effigy_test_support::short_tempdir("lc-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "cancelled-sentinel-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let lease =
        crate::supervise::spawn_sentinel(&run, &root, &plan, &super::super::tests::next()).unwrap();
    let identity = lease.identity.clone();
    cx.operation()
        .cancel_token()
        .cancel(crate::ctx::StopCause::Requested);
    let result = run.register_lifecycle(lease, &plan);
    let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    reap_test_child(&identity);
    assert!(result.is_err());
    assert_eq!(
        retained,
        Some(identity),
        "cancelled registration must retain its exact lease"
    );
    assert!(cleanup.is_ok(), "fresh cleanup must settle the exact owner");
}

#[test]
fn failed_registration_and_close_keep_the_exact_sentinel_lease() {
    let home = effigy_test_support::short_tempdir("ls-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "workspace-sentinel-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let lease =
        crate::supervise::spawn_sentinel(&run, &root, &plan, &super::super::tests::next()).unwrap();
    let identity = lease.identity.clone();
    let before_count = Arc::new(AtomicUsize::new(0));
    let close_count = Arc::new(AtomicUsize::new(0));
    let before_seen = Arc::clone(&before_count);
    let close_seen = Arc::clone(&close_count);
    crate::fault::hook(Some(Box::new(move |point| {
        if point == crate::fault::Point::LifecycleBefore {
            before_seen.fetch_add(1, Ordering::SeqCst);
        }
        if point == crate::fault::Point::SentinelClose {
            close_seen.fetch_add(1, Ordering::SeqCst);
            crate::fault::arm_point(
                crate::fault::Point::SentinelClose,
                crate::fault::Fault::Eperm,
            );
        }
    })));
    crate::fault::arm_point(
        crate::fault::Point::LifecycleBefore,
        crate::fault::Fault::Enospc,
    );
    let result = run.register_lifecycle(lease, &plan);
    let close_fired = crate::fault::fired();
    crate::fault::hook(None);
    crate::fault::arm(None);
    let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    // The failing-first implementation can lose the Child handle. Reap
    // only this test's exact recorded child before making assertions.
    reap_test_child(&identity);
    assert!(result.is_err());
    assert_eq!(before_count.load(Ordering::SeqCst), 1);
    assert_eq!(close_count.load(Ordering::SeqCst), 1);
    assert!(close_fired, "sentinel close fault must fire");
    assert_eq!(
        retained,
        Some(identity),
        "the failed close must retain its exact lease"
    );
    assert!(
        cleanup.is_ok(),
        "fixture cleanup must settle the exact owner"
    );
}

fn check_backup_start_fault(point: crate::fault::Point) {
    let home = effigy_test_support::short_tempdir("lb-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "workspace-backup-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner.clone()),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    let starts = Arc::new(AtomicUsize::new(0));
    let observed_starts = Arc::clone(&starts);
    crate::fault::hook(Some(Box::new(move |observed| {
        if observed == crate::fault::Point::WorkspaceBackupAfterSpawn {
            observed_starts.fetch_add(1, Ordering::SeqCst);
        }
    })));
    crate::fault::arm_point(point, crate::fault::Fault::Enospc);
    let enabled = run.enable_custody(&plan);
    let fired = crate::fault::fired();
    crate::fault::arm(None);
    let sentinel = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let partial = run.has_partial_checkout_backup();
    let backup_identity = run.partial_checkout_backup_identity();
    let effect_refused = run.can_effect().is_err();
    let custody_refusal = run.verify_custody().err().map(|error| error.to_string());
    let retry = run.enable_custody(&plan);
    let same_sentinel = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let same_backup = run.partial_checkout_backup_identity();
    let process_effects = run
        .effects()
        .unwrap()
        .iter()
        .filter(|effect| effect.spec.kind == crate::custody::KIND)
        .count();
    let gate = Gate::probe(&cx.operation().observe().unwrap(), &root, &owner);
    let start_count = starts.load(Ordering::SeqCst);
    crate::fault::hook(None);
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    let cleanup_failed = cleanup.is_err();
    assert!(enabled.is_err());
    assert!(fired, "the backup startup fault must fire: {point:?}");
    assert!(sentinel.is_some());
    assert!(partial, "the exact partial backup must remain reachable");
    assert!(
        backup_identity.is_some(),
        "the partial helper identity must be recorded"
    );
    assert!(effect_refused, "partial startup must not allow effects");
    assert!(
        custody_refusal
            .as_deref()
            .is_some_and(|error| error.contains("checkout backup startup remains unsettled")),
        "partial backup must independently refuse custody verification: {custody_refusal:?}"
    );
    assert!(
        retry.is_err(),
        "partial startup must not launch a second helper"
    );
    assert_eq!(same_sentinel, sentinel);
    assert_eq!(same_backup, backup_identity);
    assert_eq!(start_count, 1, "retry must not cross the backup spawn hook");
    assert_eq!(process_effects, 0, "no process effect is journalled");
    assert!(matches!(
        gate,
        Observed::Verified(crate::gate::GateState::Held(_), _)
    ));
    if let Err((error, owner)) = cleanup {
        assert!(
            error
                .to_string()
                .contains("checkout backup cleanup remains unsettled"),
            "partial cleanup must report a named unresolved outcome: {error}"
        );
        assert!(
            owner.has_partial_checkout_backup(),
            "unknown cleanup must retain the same partial backup"
        );
        assert_eq!(owner.partial_checkout_backup_identity(), backup_identity);
        assert_eq!(
            owner.sentinel.as_ref().map(|lease| lease.identity.clone()),
            sentinel,
            "unknown cleanup must retain the same sentinel lease"
        );
        // Fixture teardown only: this is not a production settlement proof.
        drop(owner);
    }
    if point == crate::fault::Point::WorkspaceBackupAfterSpawn {
        assert!(
            cleanup_failed,
            "the pre-config child must exercise unresolved cleanup"
        );
    }
}

#[test]
fn backup_start_fault_retains_partial_owner_with_same_sentinel() {
    for point in [
        crate::fault::Point::WorkspaceBackupAfterSpawn,
        crate::fault::Point::WorkspaceBackupAfterConfig,
        crate::fault::Point::WorkspaceBackupAfterAck,
    ] {
        check_backup_start_fault(point);
    }
}

#[test]
fn after_commit_fault_preserves_same_env_sentinel_for_readback() {
    let home = effigy_test_support::short_tempdir("lf-").unwrap();
    let cx = super::super::tests::context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let checkout = crate::id::CheckoutId::new(crate::id::CheckoutKind::EphemeralClone, [7; 16]);
    let owner = ResourceId::environment(checkout, "workspace-proof").unwrap();
    let mut fake = super::super::tests::Fake::new(home.path());
    let mut run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(owner),
        super::super::tests::opts(1),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let plan = LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Detached,
    };
    crate::fault::arm_point(
        crate::fault::Point::LifecycleAfter,
        crate::fault::Fault::Enospc,
    );
    let enabled = run.enable_custody(&plan);
    let fired = crate::fault::fired();
    crate::fault::arm(None);
    let exact = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let readback = run.inspect_workspace_lifecycle();
    let saved_sentinel: Option<String> = run
        .store
        .conn()
        .query_row(
            "SELECT sentinel_identity FROM runs WHERE run_id=?1",
            [run.id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    run.store
        .conn()
        .execute(
            "UPDATE runs SET sentinel_identity=NULL WHERE run_id=?1",
            [run.id.to_string()],
        )
        .unwrap();
    let mismatch = run.inspect_workspace_lifecycle();
    run.store
        .conn()
        .execute(
            "UPDATE runs SET sentinel_identity=?2 WHERE run_id=?1",
            rusqlite::params![run.id.to_string(), saved_sentinel],
        )
        .unwrap();
    run.store
        .conn()
        .execute(
            "UPDATE runs SET generation=generation+1 WHERE run_id=?1",
            [run.id.to_string()],
        )
        .unwrap();
    let wrong_generation = run.inspect_workspace_lifecycle();
    run.store
        .conn()
        .execute(
            "UPDATE runs SET generation=generation-1 WHERE run_id=?1",
            [run.id.to_string()],
        )
        .unwrap();
    let retained = run.sentinel.as_ref().map(|lease| lease.identity.clone());
    let poisoned = run.can_effect().is_err();
    let cleanup = run.settle_workspace_recoverable(&super::super::tests::cleanup(&cx), &mut fake);
    assert!(enabled.is_err());
    assert!(fired, "the post-commit lifecycle fault must fire");
    assert!(exact.is_some());
    assert_eq!(retained, exact);
    assert!(mismatch.is_err(), "missing sentinel identity must refuse");
    assert!(
        wrong_generation.is_err(),
        "run generation mismatch must refuse"
    );
    assert!(poisoned, "readback alone must not restore effect authority");
    assert!(cleanup.is_ok(), "the same Env Run must remain recoverable");
    assert!(
        readback.is_ok(),
        "same-lease lifecycle readback must be available"
    );
}
#[test]
fn descriptor_closes_once_after_closed_commit_before_gate_release() {
    let home = super::super::tests::home();
    let cx = super::super::tests::context();
    let root = Arc::new(StateRoot::effects(&cx, home.path()).unwrap());
    let mut options = super::super::tests::opts(1);
    options.custody = Some(LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .unwrap(),
        placement: crate::supervise::Placement::Inline,
    });
    let mut fake = super::super::tests::Fake::new(home.path());
    let run = Run::begin(
        &cx,
        &root,
        &super::super::tests::inv(super::super::tests::owner()),
        options,
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let fd = run.sentinel.as_ref().unwrap().writer_fd();
    let count = Arc::new(AtomicUsize::new(0));
    let observed_count = count.clone();
    let observed_root = root.clone();
    let observed_cx = cx.clone();
    crate::fault::hook(Some(Box::new(move |point| {
        if point == crate::fault::Point::SentinelClose {
            // Check before opening the reader, which could reuse this fd number.
            assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EBADF)
            );
            let observe = observed_cx.operation().observe().unwrap();
            assert!(matches!(
                metadata(&observe, &observed_root, super::super::tests::id(1))
                    .verified()
                    .unwrap()
                    .sentinel_status,
                SentinelStatus::Closed
            ));
            assert!(matches!(
                Gate::probe(&observe, &observed_root, &super::super::tests::owner()),
                Observed::Verified(crate::gate::GateState::Held(_), _)
            ));
            observed_count.fetch_add(1, Ordering::SeqCst);
        }
    })));
    run.settle(&super::super::tests::cleanup(&cx), &mut fake)
        .unwrap();
    settle_interrupted(
        &cx,
        &root,
        &super::super::tests::owner(),
        super::super::tests::id(1),
        super::super::tests::next(),
        &super::super::tests::cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    crate::fault::hook(None);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
