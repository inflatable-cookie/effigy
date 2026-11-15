//! Retained custody for a value-bearing VM Compose launch (effigy#1003).
//!
//! These proofs drive the production launchers with a fake `colima` and the
//! real obligations around them: an Env-owned kernel Run that admitted without
//! custody, a checkout the VM would read the interpolation file from, and the
//! trusted record each launch keeps.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::{CancelToken, CleanupContext, EffectClass, EffectContext, Placement};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
    Route, RunId, Surface, TaskId,
};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::outcome::{CleanupOutcome, Invocation};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, Run, SettlementAdapter};

use super::retained::{self, test_executable, RetainedComposeOwner};

/// A fixture value. It exists only in these tests and in the fake child.
const SENTINEL: &str = "effigy-fixture-retained-51c7";

/// A `colima` fake that records the interpolation file it was handed, prints
/// its contents to stdout and exits with `status`.
fn fake_colima(root: &Path, name: &str, status: i32) -> PathBuf {
    let bin = root.join(name);
    fs::create_dir_all(&bin).expect("fake bin dir");
    let log = root.join(format!("{name}.observed"));
    let script = format!(
        "#!/bin/sh\n\
         f=\"\"\n\
         prev=\"\"\n\
         for a in \"$@\"; do\n\
           if [ \"$prev\" = \"--env-file\" ]; then f=\"$a\"; fi\n\
           prev=\"$a\"\n\
         done\n\
         if [ -n \"$f\" ]; then\n\
           echo \"env-file=$f\" >> '{log}'\n\
           {{ ls -ld \"$(dirname \"$f\")\"; ls -l \"$f\"; cat \"$f\"; }} >> '{log}'\n\
           echo \"read $(basename \"$f\")\"\n\
           cat \"$f\"\n\
         fi\n\
         echo 'creating app'\n\
         echo 'starting app'\n\
         exit {status}\n",
        log = log.display()
    );
    let path = bin.join("colima");
    fs::write(&path, script).expect("fake colima");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("fake colima mode");
    path
}

/// A `colima` fake that stays alive past a short launch deadline, so the end of
/// the invocation cannot be proven while the child is running.
fn hanging_colima(root: &Path, name: &str, seconds: u64) -> PathBuf {
    let bin = root.join(name);
    fs::create_dir_all(&bin).expect("fake bin dir");
    let script = format!("#!/bin/sh\nsleep {seconds}\n");
    let path = bin.join("colima");
    fs::write(&path, script).expect("hanging colima");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("hanging colima mode");
    path
}

fn compose_args() -> Vec<OsString> {
    ["compose", "-f", "compose.yml", "up", "-d", "-p", "stack"]
        .into_iter()
        .map(OsString::from)
        .collect()
}

fn secret_env() -> Vec<(String, OsString)> {
    vec![(
        "DATABASE_URL".to_owned(),
        OsString::from(format!("postgres://fixture/{SENTINEL}")),
    )]
}

/// A binary that speaks the kernel helper protocol. `effigy test:focused`
/// exports this checkout's CLI; an operator may name a binary explicitly.
fn helper_binary() -> PathBuf {
    for key in ["EFFIGY_EXECUTABLE", "EFFIGY_KERNEL_CUSTODY_FIXTURE"] {
        if let Some(path) = std::env::var_os(key) {
            let path = PathBuf::from(path);
            assert!(path.is_file(), "{key} names no binary: {}", path.display());
            return path;
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    for target in std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(workspace.join("target")))
    {
        for name in ["effigy", "kernel-custody-fixture"] {
            let candidate = target.join("debug").join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!(
        "no helper binary: run `oxide build --bin effigy`, or export EFFIGY_EXECUTABLE as the absolute path to one"
    )
}

fn with_helper<T>(run: impl FnOnce() -> T) -> T {
    test_executable::set(&helper_binary());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    test_executable::clear();
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// Only the kernel owns the fixture's effects; anything else is unexpected and
/// never silently clean.
struct NoExternalEffects;

impl SettlementAdapter for NoExternalEffects {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected Compose fixture effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected Compose fixture effect".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected Compose fixture effect".into(),
        ))
    }
}

/// Run one fixture against the Env-owned Run a bring-up holds, then settle that
/// run exactly as the environment transition would.
///
/// The Run is admitted *without* custody, which is the production shape: the
/// launch itself has to enable it before the spawn barrier opens.
fn harness<T>(
    body: impl FnOnce(&mut Run<'static>, &EffectContext, &Path, &Path) -> T,
) -> (T, CleanupOutcome) {
    let (value, cleanup) = harness_settling(body);
    (value, cleanup.expect("retained custody settlement"))
}

/// [`harness`] that leaves the settlement result to the caller, so a proof can
/// ask what the kernel proves about a launch whose end was unproven.
fn harness_settling<T>(
    body: impl FnOnce(&mut Run<'static>, &EffectContext, &Path, &Path) -> T,
) -> (T, Result<CleanupOutcome, String>) {
    let repo = tempfile::tempdir().expect("fixture checkout");
    let state = effigy_test_support::short_tempdir("rc-").expect("private state home");
    let effigy_home = state.path().join(".effigy");
    fs::create_dir_all(&effigy_home).expect("Effigy home");
    effigy_core::worktree_scope::prepare_checkout_token(repo.path()).expect("checkout identity");
    let operation = crate::environment::operation(
        EffectClass::Mutate,
        CancelToken::default(),
        Duration::from_secs(180),
    );
    let cx = operation.effects().expect("effects capability");
    // The Run borrows this root for the rest of the fixture.
    let root: &'static StateRoot = Box::leak(Box::new(
        StateRoot::effects(&cx, state.path()).expect("state root"),
    ));
    let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]);
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout, "", Surface::Published, "container-up", None)
            .expect("task identity"),
        route: Route::Task,
        argv: vec![],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: ResourceId::environment(checkout, "stack").expect("environment identity"),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("invocation");
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64;
    let admission = cx
        .operation()
        .cleanup(Duration::from_secs(30))
        .expect("admission window");
    let mut adapter = NoExternalEffects;
    let mut run = Run::begin(
        &cx,
        root,
        &invocation,
        BeginOpts {
            id: RunId::new(milliseconds, [3_u8; 10]).expect("run identity"),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: Invocation::validated(
                vec!["effigy".into(), "container".into(), "up".into()],
                |_| true,
            )
            .expect("recovery command"),
            custody: None,
        },
        &admission,
        &mut adapter,
    )
    .expect("environment run admission");
    let value = crate::with_test_effigy_home(&effigy_home, || {
        body(&mut run, &cx, repo.path(), state.path())
    });
    let settlement = cx
        .operation()
        .cleanup(Duration::from_secs(60))
        .expect("settlement window");
    let settled = match run.settle(&settlement, &mut adapter) {
        Ok(outcome) => Ok(outcome.cleanup()),
        Err(error) => Err(error.to_string()),
    };
    (value, settled)
}

/// The interpolation files a live or finished launch left, excluding the
/// custody journals beside them.
fn compose_env_files(state: &Path) -> Vec<PathBuf> {
    fs::read_dir(state.join(".effigy").join("compose-env"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("compose-") && name.ends_with(".env"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// A host backend and a value-free VM invocation keep their current path: no
/// custody, no interpolation file, no refusal.
#[test]
fn a_host_backend_and_a_value_free_vm_invocation_need_no_custody() {
    use crate::policy::render::vm_interpolation_required;
    let env = secret_env();
    assert!(!vm_interpolation_required(
        std::ffi::OsStr::new("/usr/local/bin/docker"),
        &env
    ));
    assert!(!vm_interpolation_required(
        std::ffi::OsStr::new("/opt/homebrew/bin/colima"),
        &[]
    ));
    assert!(vm_interpolation_required(
        std::ffi::OsStr::new("/opt/homebrew/bin/colima"),
        &env
    ));

    let root = tempfile::tempdir().expect("fixture root");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let program = fake_colima(root.path(), "direct", 0);
    let output = crate::with_test_effigy_home(&home, || {
        crate::exec::process::run_command_capture_os_with_env(
            root.path(),
            &program.to_string_lossy(),
            &compose_args(),
            "value-free compose",
            &[],
        )
    })
    .expect("a value-free invocation is unchanged");
    assert!(output.status.success());
    assert!(
        !root.path().join("direct.observed").exists(),
        "a value-free invocation names no interpolation file"
    );
}

/// A secret-bearing VM capture runs under the retained Run, the child reads the
/// value, and its captured output carries no secret.
#[test]
fn a_secret_bearing_vm_capture_runs_under_retained_custody() {
    let ((), cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "capture", 0);
        let observed = repo.join("capture.observed");
        let env = secret_env();
        let output = with_helper(|| {
            retained::capture(
                repo,
                program.as_os_str(),
                &compose_args(),
                "retained capture",
                &env,
                Instant::now() + Duration::from_secs(60),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect("retained capture");
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains(SENTINEL),
            "captured output must be redacted: {stdout}"
        );
        assert!(stdout.contains("[REDACTED]"), "{stdout}");
        let record = fs::read_to_string(&observed).expect("the child ran");
        assert!(
            record.contains(&format!("DATABASE_URL=postgres://fixture/{SENTINEL}")),
            "the VM reader saw the value: {record}"
        );
        assert_eq!(
            compose_env_files(state).len(),
            1,
            "the file is still held until the group closes"
        );
    });
    assert_eq!(cleanup, CleanupOutcome::Complete);
}

/// A secret-bearing detached launch keeps its retained backend record, relays
/// every complete line and never writes the value into it.
#[test]
fn a_secret_bearing_vm_detached_launch_keeps_its_record_redacted() {
    let ((), cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "detached", 0);
        let env = secret_env();
        let mut lines = Vec::new();
        let mut on_line = |line: &str| lines.push(line.to_owned());
        let output = with_helper(|| {
            retained::detached(
                repo,
                program.as_os_str(),
                &compose_args(),
                "retained detached",
                &env,
                None,
                &mut on_line,
                Instant::now() + Duration::from_secs(60),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect("retained detached launch");
        assert!(output.status.success(), "{output:?}");
        assert!(
            lines.iter().any(|line| line == "starting app"),
            "the runtime's own lines reach the caller: {lines:?}"
        );
        let record_path = crate::exec::retained_backend_log_path(repo, "stack");
        let record = fs::read_to_string(&record_path).expect("retained backend record");
        assert!(record.contains("creating app"), "{record}");
        assert!(
            !record.contains(SENTINEL),
            "the record holds no secret value: {record}"
        );
        assert_eq!(compose_env_files(state).len(), 1);
    });
    assert_eq!(cleanup, CleanupOutcome::Complete);
}

/// A failed Compose run still reports its own failure and leaves no secret.
#[test]
fn a_failed_retained_detached_launch_reports_the_backend_failure() {
    let ((), cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "failing", 7);
        let env = secret_env();
        let error = with_helper(|| {
            retained::detached(
                repo,
                program.as_os_str(),
                &compose_args(),
                "retained failing",
                &env,
                None,
                &mut |_| {},
                Instant::now() + Duration::from_secs(60),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect_err("a failing Compose run is reported");
        let message = error.to_string();
        assert!(message.contains("failed"), "{message}");
        assert!(!message.contains(SENTINEL), "{message}");
        assert_eq!(compose_env_files(state).len(), 1);
    });
    assert_eq!(cleanup, CleanupOutcome::Complete);
}

/// A launch whose custody cannot be admitted refuses before spawn and names the
/// invocation instead of starting the child without custody.
#[test]
fn a_refused_custody_admission_names_the_cause_before_any_spawn() {
    let ((), cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "refused", 0);
        let observed = repo.join("refused.observed");
        let broken = repo.join("not-a-helper");
        fs::write(&broken, "#!/bin/sh\nexit 1\n").expect("broken helper");
        fs::set_permissions(&broken, fs::Permissions::from_mode(0o755)).expect("mode");
        test_executable::set(&broken);
        let env = secret_env();
        let error = retained::capture(
            repo,
            program.as_os_str(),
            &compose_args(),
            "retained refusal",
            &env,
            Instant::now() + Duration::from_secs(30),
            &mut RetainedComposeOwner::new(run, cx),
        )
        .expect_err("custody that cannot be admitted refuses");
        test_executable::clear();
        let message = error.to_string();
        assert!(
            message.contains("retained refusal"),
            "the refusal names the invocation: {message}"
        );
        assert!(
            !observed.exists(),
            "no child runs without retained custody: {message}"
        );
        assert!(
            compose_env_files(state).is_empty(),
            "the refused launch leaves no interpolation file behind"
        );
    });
    assert_eq!(cleanup, CleanupOutcome::Complete);
}

/// The VM reads the interpolation file itself: a profile that does not share
/// the state home refuses before spawn instead of failing inside the VM with
/// `couldn't find env file`.
#[test]
fn a_state_home_the_profile_does_not_share_refuses_before_spawn() {
    let _lock = crate::test_env_lock();
    let profile_home = tempfile::tempdir().expect("private Colima home");
    let svm_share = tempfile::tempdir().expect("unrelated share");
    let profile = profile_home.path().join("rc1003");
    fs::create_dir_all(&profile).expect("profile directory");
    fs::write(
        profile.join("colima.yaml"),
        format!(
            "mounts:\n  - location: {}\n    writable: true\n",
            svm_share.path().display()
        ),
    )
    .expect("profile config");
    let previous = std::env::var_os("COLIMA_HOME");
    std::env::set_var("COLIMA_HOME", profile_home.path());
    let ((), cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "unshared-state", 0);
        let observed = repo.join("unshared-state.observed");
        let env = secret_env();
        let args = [
            "nerdctl",
            "--profile",
            "rc1003",
            "--",
            "compose",
            "-f",
            "compose.yml",
            "up",
            "-d",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        let error = with_helper(|| {
            retained::capture(
                repo,
                program.as_os_str(),
                &args,
                "unshared state home",
                &env,
                Instant::now() + Duration::from_secs(30),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect_err("an unreadable state home refuses");
        let message = error.to_string();
        assert!(message.contains("does not share"), "{message}");
        assert!(message.contains("rc1003"), "{message}");
        assert!(
            message.contains(&state.join(".effigy").display().to_string()),
            "the refusal names the unshared directory: {message}"
        );
        assert!(!observed.exists(), "no child runs: {message}");
        assert!(
            compose_env_files(state).is_empty(),
            "the refused launch leaves no interpolation file"
        );
    });
    match previous {
        Some(value) => std::env::set_var("COLIMA_HOME", value),
        None => std::env::remove_var("COLIMA_HOME"),
    }
    assert_eq!(cleanup, CleanupOutcome::Complete);
}

/// A launch whose end cannot be proven retains its interpolation file: the
/// kernel, not the caller, decides when the group has closed (brief Proof:
/// "retained on Unknown and removed only after verified closure").
#[test]
fn an_unproven_end_retains_the_file_until_verified_closure() {
    let (retained_path, cleanup) = harness_settling(|run, cx, repo, state| {
        let program = hanging_colima(repo, "hanging", 30);
        let env = secret_env();
        let error = with_helper(|| {
            retained::capture(
                repo,
                program.as_os_str(),
                &compose_args(),
                "unproven end",
                &env,
                // The launch's own deadline passes while the child is alive.
                Instant::now() + Duration::from_millis(400),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect_err("a launch whose end is unproven is reported");
        let message = error.to_string();
        assert!(
            message.contains("end is unproven"),
            "the report names the unproven end: {message}"
        );
        assert!(
            message.contains("stays in kernel custody"),
            "the report says the file is retained: {message}"
        );
        let files = compose_env_files(state);
        assert_eq!(
            files.len(),
            1,
            "the file is retained while its group's closure is unproven: {files:?}"
        );
        // The custody journal beside it names the exact invocation that owns it.
        let journals: Vec<_> = fs::read_dir(state.join(".effigy").join("compose-env"))
            .expect("compose-env directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".custody-"))
            .collect();
        assert_eq!(
            journals.len(),
            1,
            "one binding journal per invocation: {journals:?}"
        );
        files[0].clone()
    });
    let cleanup = cleanup.expect("a proved closure settles the retained launch");
    assert_eq!(cleanup, CleanupOutcome::Complete);
    assert!(
        !retained_path.exists(),
        "verified closure removes the interpolation file"
    );
}

/// Settlement removes the file only after verified closure, and no state-home
/// file keeps the value.
#[test]
fn settlement_removes_the_file_and_persists_no_secret() {
    let (state_path, cleanup) = harness(|run, cx, repo, state| {
        let program = fake_colima(repo, "owner", 0);
        let env = secret_env();
        with_helper(|| {
            retained::capture(
                repo,
                program.as_os_str(),
                &compose_args(),
                "retained owner",
                &env,
                Instant::now() + Duration::from_secs(60),
                &mut RetainedComposeOwner::new(run, cx),
            )
        })
        .expect("retained capture");
        state.to_path_buf()
    });
    assert_eq!(cleanup, CleanupOutcome::Complete);
    assert!(
        compose_env_files(&state_path).is_empty(),
        "verified closure removes the interpolation file"
    );
    for entry in walk(&state_path) {
        let bytes = fs::read(&entry).unwrap_or_default();
        assert!(
            !bytes
                .windows(SENTINEL.len())
                .any(|window| window == SENTINEL.as_bytes()),
            "the sentinel persists in {}",
            entry.display()
        );
    }
}
