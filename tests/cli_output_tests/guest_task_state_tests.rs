//! A workspace guest runs an ordinary container-routed task beside its own
//! guest-local task state, while the checkout's host-state mask stays
//! inaccessible and untouched (effigy#1037).
//!
//! The fixture is the reported shape without any consumer: a checkout whose
//! `.effigy/state` is the read-only host-state mask, a container-routed task
//! (the harmless equivalent of `sync:db` / `sync:neon`), and the workspace
//! handoff marker Effigy's own compose exec sets. The task's own command proves
//! the placement, its working directory, the arguments delivered to it, its
//! output and its exit status; the mask's inode, mode and modification time
//! prove nothing in the guest wrote, adopted or repaired host state.
//!
//! The fixture declares a container target but no backend, so a placement that
//! tried to route back to the host's runtime would fail to load a primary
//! service instead of running the command: the workload only runs if the guest
//! admitted and executed it locally.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_test_support::process::FixtureCommandExt;
use effigy_test_support::Fixture;

const MANIFEST: &str = r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[tasks]
[tasks."fixture:sync-db"]
run_in = "container"
run = "printf 'fixture-sync-db handoff=%s cwd=%s argv=[%s][%s]\n' \"$EFFIGY_INTERNAL_CONTAINER_HANDOFF\" \"$(pwd -P)\" {args}"

[tasks."fixture:sync-neon"]
run_in = "container"
run = "printf 'fixture-sync-neon handoff=%s cwd=%s argv=[%s][%s]\n' \"$EFFIGY_INTERNAL_CONTAINER_HANDOFF\" \"$(pwd -P)\" {args}; exit 7"
"#;

/// One workspace guest fixture: the mounted checkout with its host-state mask,
/// and the container's own HOME (the guest's writable home, never the host's).
struct GuestFixture {
    checkout: PathBuf,
    home: PathBuf,
    mask: PathBuf,
    /// Restores the mask's own read/traverse before this fixture's tree is
    /// removed; the test's assertions run while the mask is still shut.
    _restore_mask: MaskMode,
}

/// Reopen the fixture's mask only for teardown.
struct MaskMode(PathBuf);

impl Drop for MaskMode {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
    }
}

impl GuestFixture {
    fn new(name: &str) -> Self {
        let root = Fixture::thread_lifetime(&format!("effigy-guest-task-state-{name}-"))
            .expect("owned guest fixture");
        let checkout = root.join("checkout");
        let home = root.join("home");
        fs::create_dir_all(checkout.join(".git")).expect("checkout git directory");
        fs::create_dir_all(&home).expect("guest home");
        fs::write(checkout.join("effigy.toml"), MANIFEST).expect("write manifest");
        let mask = checkout.join(".effigy/state");
        fs::create_dir_all(&mask).expect("host-state mask directory");
        // The container's mask is a foreign-owned volume the workspace user may
        // read but never write. This fixture owns its checkout, and the kernel
        // adopts an *owned* read-only state root by tightening it to 0700, which
        // a foreign-owned mask never is. Denying the owner read/traverse is
        // therefore the local equivalent: the path is unreadable to the guest,
        // so any attempt to open it is refused exactly as in the container.
        fs::set_permissions(&mask, fs::Permissions::from_mode(0o000)).expect("mask mode");
        Self {
            checkout,
            home,
            mask: mask.clone(),
            _restore_mask: MaskMode(mask),
        }
    }

    /// The mask exactly as the workspace left it.
    fn mask_before(&self) -> fs::Metadata {
        fs::symlink_metadata(&self.mask).expect("mask metadata")
    }

    /// Run the nested guest Effigy from the checkout, as the workspace user.
    fn run(&self, args: &[&str]) -> Output {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_effigy"));
        command.current_dir(&self.checkout).args(args);
        // The guest is its own process: no Effigy marker from the worker that
        // runs this suite leaks into it, and the one marker the handoff sets is
        // the only placement fact it sees.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("EFFIGY_") {
                command.env_remove(&key);
            }
        }
        command
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .env("EFFIGY_INTERNAL_CONTAINER_HANDOFF", "1")
            .output_owned()
            .expect("run the nested guest task")
    }

    /// The one guest-local journal the checkout's task runs live in.
    fn guest_state_root(&self) -> PathBuf {
        let root = self.home.join(".effigy/guest-checkouts");
        let entries = fs::read_dir(&root)
            .expect("the guest keeps task state beside its own HOME")
            .collect::<Result<Vec<_>, _>>()
            .expect("guest state entries");
        assert_eq!(entries.len(), 1, "one checkout owns one guest journal");
        entries[0].path()
    }

    fn assert_mask_untouched(&self, before: &fs::Metadata, task: &str) {
        let after = fs::symlink_metadata(&self.mask).expect("mask metadata after the run");
        assert_eq!(after.ino(), before.ino(), "`{task}` replaced the mask");
        assert_eq!(
            after.mode() & 0o7777,
            before.mode() & 0o7777,
            "`{task}` changed the mask mode"
        );
        assert_eq!(
            after.mtime(),
            before.mtime(),
            "`{task}` wrote inside the mask"
        );
        assert_eq!(
            fs::read_dir(&self.mask)
                .expect_err("the mask is still inaccessible")
                .kind(),
            std::io::ErrorKind::PermissionDenied,
            "`{task}` made the mask readable"
        );
        let effigy = self.checkout.join(".effigy");
        let entries = fs::read_dir(&effigy)
            .expect("checkout .effigy")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("state")],
            "the guest wrote checkout state beside the mask"
        );
    }
}

/// The workspace user this fixture proves: the mask is a foreign-owned volume,
/// so the proof needs a genuinely unprivileged process.
fn assert_workspace_user() -> bool {
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    if uid == 0 {
        eprintln!(
            "skipped: this fixture proves the guest placement as a non-root workspace user, and \
             this suite runs as uid 0"
        );
        return false;
    }
    true
}

fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .expect("canonical fixture path")
        .display()
        .to_string()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

/// A container-routed task runs in the guest, locally, with its working
/// directory, arguments, output and exit status intact — and the masked host
/// state root is never opened.
#[test]
fn guest_runs_a_container_routed_task_beside_its_own_state() {
    if !assert_workspace_user() {
        return;
    }
    let fixture = GuestFixture::new("sync-db");
    let before = fixture.mask_before();
    let output = fixture.run(&["fixture:sync-db", "alpha", "beta"]);
    assert!(
        output.status.success(),
        "guest task failed: status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = stdout(&output);
    assert!(
        rendered.contains(&format!(
            "fixture-sync-db handoff=1 cwd={} argv=[alpha][beta]",
            canonical(&fixture.checkout)
        )),
        "the task ran in the guest with its own cwd and arguments: {rendered}"
    );
    fixture.assert_mask_untouched(&before, "fixture:sync-db");

    // The run's journal is the guest's own root, private and nowhere near the
    // mounted checkout.
    let state = fixture.guest_state_root();
    assert!(state.starts_with(fixture.home.join(".effigy/guest-checkouts")));
    let journal = state.join(".effigy/state");
    let metadata = fs::symlink_metadata(&journal).expect("guest journal state root");
    assert_eq!(
        metadata.mode() & 0o7777,
        0o700,
        "the guest journal is private"
    );
    // SAFETY: geteuid has no preconditions.
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
}

/// The guest keeps the wrapped command's exit status, so a failing task is
/// still a failing task and the mask is still untouched.
#[test]
fn guest_preserves_the_task_exit_status() {
    if !assert_workspace_user() {
        return;
    }
    let fixture = GuestFixture::new("sync-neon");
    let before = fixture.mask_before();
    let output = fixture.run(&["fixture:sync-neon", "gamma", "delta"]);
    assert_eq!(
        output.status.code(),
        Some(7),
        "the task's own exit status survives: status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = stdout(&output);
    assert!(
        rendered.contains(&format!(
            "fixture-sync-neon handoff=1 cwd={} argv=[gamma][delta]",
            canonical(&fixture.checkout)
        )),
        "the failing task still ran in the guest with its own cwd and arguments: {rendered}"
    );
    fixture.assert_mask_untouched(&before, "fixture:sync-neon");
}
