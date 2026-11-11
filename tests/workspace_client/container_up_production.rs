//! Actual CLI -> detached production orchestration, with only external engine
//! and guest I/O doubled. This never qualifies a native backend.
//!
//! Ordinary production retains its pre-apply refusal. The uninstalled
//! qualification feature exercises the real ownership and commit path, failure
//! recovery and secret delivery. Only external engine/guest I/O is doubled;
//! these controls never qualify a native backend or consumer.
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use effigy_kernel::observe::Observed;

struct BackendFixture {
    fixture: effigy_test_support::Fixture,
    project: PathBuf,
    home: PathBuf,
    config: PathBuf,
    path: std::ffi::OsString,
}

/// A workspace declaration that names its own container: the explicit
/// workspace shape the guarded activation is requested for.
const WORKSPACE_DECLARATION: &str = r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
profile = "default"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = "dev"
home = "/home/dev"
mounts = ["./prepared:/workspace/prepared:ro"]
"#;

/// The exact declaration a legacy bundle export writes (the bundles/workspace-app
/// `export.toml`, and the shape the Acowtancy manifest composes): the system
/// selects its container at the system level, keeps
/// `working_dir`/`user`/`home`/`mounts` there, and leaves the
/// `workspaces.<name>` table empty, while `[containers].default` names the same
/// container. The bundle also ships the workspace service and its
/// `isolated_dirs` from the catalog; that belongs to the manifest fixtures, not
/// to this routing proof.
const LEGACY_BUNDLE_EXPORT: &str = r#"
[containers]
default = "stack"
[containers.stack]
compose_file = "compose.yml"
primary_service = "app"
profile = "default"
startup = "detached"
[systems]
default = "dev"
[systems.dev]
container = "stack"
default_workspace = "app"
working_dir = "/workspace-root/acowtancy"
user = "dev"
home = "/home/dev"
mounts = []
[systems.dev.workspaces.app]
"#;

impl BackendFixture {
    fn new() -> Self {
        Self::with_manifest("container-production-fake", WORKSPACE_DECLARATION)
    }

    fn with_manifest(label: &str, manifest: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let fixture = effigy_test_support::Fixture::new(label).expect("fixture");
        let project = fixture.path().join("project");
        let home = fixture.path().join("home");
        let bin = fixture.path().join("bin");
        for directory in [&project.join(".git"), &home, &bin] {
            std::fs::create_dir_all(directory).expect("fixture directory");
        }
        std::fs::write(
            project.join(".git/config"),
            "[effigy]\n runtimeScope = ephemeral\n",
        )
        .expect("scoped fixture checkout");
        std::fs::write(project.join("effigy.toml"), manifest).expect("manifest");
        std::fs::write(
            project.join("compose.yml"),
            "services:\n  app:\n    image: example:one\n",
        )
        .expect("compose");
        // Arbitrary bind sources are user-owned; the ordinary Prepare creates
        // its generated Compose artifacts and checkout token, not these inputs.
        std::fs::create_dir(project.join("prepared")).expect("user-owned bind input");
        let (machine, architecture, elf_machine, interpreter, triple) = match std::env::consts::ARCH
        {
            "aarch64" => (
                "aarch64",
                "arm64",
                183u16,
                "/lib/ld-linux-aarch64.so.1",
                "aarch64-unknown-linux-gnu",
            ),
            "x86_64" => (
                "x86_64",
                "amd64",
                62u16,
                "/lib64/ld-linux-x86-64.so.2",
                "x86_64-unknown-linux-gnu",
            ),
            _ => panic!("fixture requires pinned host architecture"),
        };
        let mut elf = vec![0; 512];
        elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        elf[18..20].copy_from_slice(&elf_machine.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());
        elf[64..68].copy_from_slice(&3u32.to_le_bytes());
        elf[72..80].copy_from_slice(&128u64.to_le_bytes());
        elf[96..104].copy_from_slice(&((interpreter.len() + 1) as u64).to_le_bytes());
        elf[128..128 + interpreter.len()].copy_from_slice(interpreter.as_bytes());
        let artifact = fixture.path().join("artifact");
        std::fs::write(&artifact, &elf).expect("fixture ELF bytes");
        std::fs::write(
            artifact.with_extension("active-version"),
            effigy_core::build_info::active_version(),
        )
        .expect("producer version");
        // Match the actual producer-owned sidecar paths; this is a fixture
        // artifact receipt, never a fabricated kernel creation receipt.
        std::fs::write(
            format!("{}.receipt", artifact.display()),
            format!(
                "release_triple={triple}\nsha256={}\n",
                effigy_kernel::id::Digest::of(&elf).hex()
            ),
        )
        .expect("producer receipt");
        let config = fixture.path().join("fake.json");
        std::fs::write(&config, serde_json::to_vec(&serde_json::json!({
            "artifact": artifact, "applied":fixture.path().join("applied"), "export":fixture.path().join("guest-files.json"),
            "container_id":"a1".repeat(32), "boot_id":"3a7c86df-0045-472e-b065-c1295816ab22",
            "machine":machine, "architecture":architecture,"libc":"gnu",
        })).expect("configuration")).expect("fake configuration");
        for program in ["docker", "colima", "nerdctl"] {
            let file = bin.join(program);
            std::fs::write(&file, include_bytes!("fixtures/container_up_engine.py"))
                .expect("fake program");
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).expect("PATH");
        Self {
            fixture,
            project,
            home,
            config,
            path,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("DOCKER_CONTEXT", "default")
            .env("EFFIGY_WORKSPACE_ACTIVATION_FAKE", &self.config);
        command
    }

    /// The ordinary Compose invocation's environment. The pinned guest
    /// artifact is produced by the separate Linux artifact task, so the
    /// handoff skip hook keeps this proof to routing and one apply.
    fn compose_command(&self) -> Command {
        let mut command = self.command();
        command.env("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", "1");
        command
    }

    /// The explicit system/workspace selection (`--system <NAME>`): the
    /// invocation that asks for the guarded workspace activation.
    fn workspace_command(&self) -> Command {
        let mut command = self.command();
        command.args(["container", "up", "--system", "dev"]);
        command
    }

    fn request_stop(&self) {
        let db = self.project.join(".effigy/state/kernel.db");
        let Ok(connection) =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return;
        };
        let Ok(mut statement) =
            connection.prepare("SELECT run_id FROM runs WHERE phase <> 'closed'")
        else {
            return;
        };
        let Ok(ids) = statement.query_map([], |row| row.get::<_, String>(0)) else {
            return;
        };
        let ids: Vec<_> = ids.flatten().collect();
        let cx = super::super::effects();
        let observe = super::super::observation();
        let root = super::super::state_root(&cx, &self.home, &self.project);
        for (index, id) in ids.into_iter().enumerate() {
            let Ok(id) = id.parse::<effigy_kernel::id::RunId>() else {
                continue;
            };
            let Some(summary) = super::super::read_summary(&observe, &root, id) else {
                continue;
            };
            let Some(invocation) = summary.invocation else {
                continue;
            };
            if invocation.spec().task.task() != "workspace-serving" {
                continue;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_millis() as u64;
            let request = effigy_kernel::run::Request {
                id: effigy_kernel::id::RunId::new(now, [index as u8 + 41; 10]).expect("request ID"),
                run: id,
                expected: effigy_kernel::run::Fence {
                    resource: invocation.spec().owner.clone(),
                    generation: summary.generation,
                },
                action: effigy_kernel::run::RequestAction::Stop,
            };
            let _ = effigy_kernel::run::request(&cx, &root, &request);
        }
    }

    fn assert_settled(&self) {
        self.request_stop();
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let connection = rusqlite::Connection::open_with_flags(
                self.project.join(".effigy/state/kernel.db"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("fixture journal");
            let open: i64 = connection.query_row("SELECT count(*) FROM runs WHERE phase <> 'closed' OR cleanup_outcome <> 'complete' OR cleanup_outcome IS NULL", [], |row| row.get(0)).expect("exact run closure");
            if open == 0 {
                return;
            }
            assert!(
                Instant::now() < until,
                "original fixture owners did not settle completely"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.config.with_extension("calls")).unwrap_or_default()
    }
}

fn assert_no_durable_error_bytes(root: &std::path::Path) {
    let secret = b"adversarial-secret-backend-diagnostic";
    if !root.exists() {
        return;
    }
    let metadata = std::fs::symlink_metadata(root).expect("private fixture metadata");
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(root).expect("private fixture state") {
            assert_no_durable_error_bytes(&entry.expect("entry").path());
        }
    } else if metadata.is_file() {
        let bytes = std::fs::read(root).expect("durable fixture bytes");
        assert!(
            !bytes.windows(secret.len()).any(|window| window == secret),
            "secret-bearing error reached durable state"
        );
    }
}

impl Drop for BackendFixture {
    fn drop(&mut self) {
        // Only processes recorded in this private fixture's own journal are
        // eligible. A reused PID never qualifies: compare the full identity.
        let db = self.project.join(".effigy/state/kernel.db");
        let Ok(connection) =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return;
        };
        let Ok(mut statement) = connection
            .prepare("SELECT owner_identity, supervisor_identity, sentinel_identity FROM runs")
        else {
            return;
        };
        let Ok(rows) = statement.query_map([], |row| {
            Ok([
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ])
        }) else {
            return;
        };
        let mut identities = Vec::new();
        for row in rows.flatten() {
            for raw in row.into_iter().flatten() {
                if let Ok(identity) =
                    serde_json::from_str::<effigy_kernel::ident::ProcessIdentity>(&raw)
                {
                    if identity.pid != std::process::id() && !identities.contains(&identity) {
                        identities.push(identity);
                    }
                }
            }
        }
        drop(statement);
        drop(connection);
        // Teardown is not settlement evidence. Request stop first; force only
        // for a failing fixture and record it explicitly so positives reject it.
        self.request_stop();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let live: Vec<_> = identities.iter().filter(|id| matches!(effigy_kernel::ident::process_identity(id.pid),Observed::Verified(ref current,_) if current == *id)).collect();
            if live.is_empty() {
                break;
            }
            if Instant::now() >= until {
                for id in live {
                    let _ = Command::new("kill")
                        .args(["-TERM", &id.pid.to_string()])
                        .status();
                }
                std::fs::write(
                    self.fixture.path().join("forced-fixture-cleanup"),
                    b"forced; not settlement proof",
                )
                .ok();
                let term_until = Instant::now() + Duration::from_secs(2);
                while Instant::now() < term_until {
                    if identities.iter().all(|id| !matches!(effigy_kernel::ident::process_identity(id.pid), Observed::Verified(ref current,_) if current == id)) { return; }
                    std::thread::sleep(Duration::from_millis(20));
                }
                for id in &identities {
                    if matches!(effigy_kernel::ident::process_identity(id.pid), Observed::Verified(ref current,_) if current == id)
                    {
                        let status = Command::new("kill")
                            .args(["-KILL", &id.pid.to_string()])
                            .status();
                        eprintln!("failing fixture forced exact PID {} cleanup: {status:?}; not settlement evidence", id.pid);
                    }
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    if root.exists() {
        for entry in std::fs::read_dir(root).expect("snapshot") {
            let entry = entry.expect("entry");
            let kind = entry.file_type().expect("type");
            if kind.is_dir() {
                files.extend(snapshot(&entry.path()));
            } else if kind.is_file() {
                files.insert(entry.path(), std::fs::read(entry.path()).expect("file"));
            }
        }
    }
    files
}

/// The legacy bundle export shape is an ordinary Compose invocation: nothing
/// in it asks for the guarded workspace activation, so plain `container up`
/// prepares and applies the stack through the ordinary reconciler exactly
/// once instead of refusing on the private-preparation capability (effigy#535).
#[test]
fn legacy_bundle_default_workspace_plain_up_applies_once_through_ordinary_compose() {
    let fixture = BackendFixture::with_manifest("container-export-shape", LEGACY_BUNDLE_EXPORT);
    let output = fixture
        .compose_command()
        .args(["container", "up", "--detach", "--json"])
        .output()
        .expect("actual CLI");
    let diagnostics = format!(
        "{}{}\nengine calls:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        fixture.calls()
    );
    assert!(output.status.success(), "{diagnostics}");
    assert!(
        !diagnostics.contains("workspace private preparation"),
        "an ordinary Compose invocation must not name the private workspace cut: {diagnostics}"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("container JSON report");
    assert_eq!(report["ok"], true, "{diagnostics}");
    assert!(
        fixture.fixture.path().join("applied").exists(),
        "the ordinary path applies the stack through the inert backend"
    );
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        1,
        "exactly one ordinary Compose apply: {diagnostics}"
    );
}

/// A hybrid environment keeps the ordinary path too: the workspace another
/// container declares does not classify this container, and the explicit
/// system selection that would request the guarded activation still refuses.
#[test]
fn hybrid_container_plain_up_stays_ordinary_while_explicit_workspace_refuses() {
    let fixture = BackendFixture::with_manifest(
        "container-export-shape-hybrid",
        &format!("{LEGACY_BUNDLE_EXPORT}\n[containers.hybrid]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\nprofile = \"default\"\nworking_dir = \"/tmp\"\n"),
    );
    let output = fixture
        .compose_command()
        .args(["container", "hybrid", "up", "--detach", "--json"])
        .output()
        .expect("actual CLI");
    let diagnostics = format!(
        "{}{}\nengine calls:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        fixture.calls()
    );
    assert!(output.status.success(), "{diagnostics}");
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        1,
        "exactly one ordinary Compose apply: {diagnostics}"
    );
    let before_calls = fixture.calls();
    let refusal = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!refusal.status.success());
    let public = format!(
        "{}{}",
        String::from_utf8_lossy(&refusal.stdout),
        String::from_utf8_lossy(&refusal.stderr)
    );
    assert!(
        public.contains("workspace private preparation output is unavailable"),
        "{public}"
    );
    assert_eq!(
        fixture.calls(),
        before_calls,
        "no backend call for the refusal"
    );
}

/// A malformed declaration keeps the actionable manifest error on the ordinary
/// invocation; only the explicit workspace request crosses the guarded
/// prevalidation and its redacted refusal.
#[test]
fn malformed_workspace_declaration_keeps_ordinary_error_and_workspace_refusal() {
    let fixture = BackendFixture::with_manifest(
        "container-export-shape-malformed",
        r#"
[containers]
default = "stack"
[containers.stack]
compose_file = "compose.yml"
primary_service = "app"
profile = "default"
[systems]
default = "dev"
[systems.dev]
container = "stack"
default_workspace = "app"
working_dir = "/workspace-root/acowtancy"
user = "dev"
home = "/home/dev"
[systems.dev.workspaces.app]
working_dir = "/app"
[systems.dev.workspaces.other]
working_dir = "/other"
"#,
    );
    let output = fixture
        .compose_command()
        .args(["container", "up", "--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(
        !output.status.success(),
        "conflicting workspaces must refuse"
    );
    let public = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        public.contains("conflicting operator workspaces"),
        "the ordinary invocation keeps the actionable declaration error: {public}"
    );
    assert!(
        !public.contains("workspace private preparation"),
        "{public}"
    );
    assert!(!fixture.fixture.path().join("applied").exists());
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        0
    );
    let refusal = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!refusal.status.success());
    let refusal_public = format!(
        "{}{}",
        String::from_utf8_lossy(&refusal.stdout),
        String::from_utf8_lossy(&refusal.stderr)
    );
    assert!(
        refusal_public.contains("workspace private preparation output is unavailable")
            || refusal_public.contains("workspace checkout/backend prevalidation unavailable"),
        "{refusal_public}"
    );
    assert!(!fixture.fixture.path().join("applied").exists());
}

#[test]
fn actual_cli_fake_backend_refuses_before_workspace_effects() {
    let fixture = BackendFixture::new();
    let before_project = snapshot(&fixture.project);
    let before_home = snapshot(&fixture.home);
    let before_calls = fixture.calls();
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!output.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("container JSON refusal");
    assert_eq!(report["ok"], false);
    assert_eq!(report["error"]["type"], "RunnerError");
    assert!(report
        .to_string()
        .contains("workspace private preparation output is unavailable"));
    assert_eq!(
        snapshot(&fixture.project),
        before_project,
        "no project file changes"
    );
    assert_eq!(snapshot(&fixture.home), before_home, "no home file changes");
    assert_eq!(fixture.calls(), before_calls, "no backend calls");
    assert!(!fixture.project.join(".effigy").exists());
    assert!(!fixture.project.join(".git/effigy-runtime-scope").exists());
    assert!(!fixture.fixture.path().join("applied").exists());
}

// The deferred tests below retain their original assertions for #533/#531.
// These controls exercise the same fault inputs at the current refusal boundary.
fn assert_fault_refused_before_effects(flags: &[&str]) {
    let fixture = BackendFixture::new();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.config).expect("configuration"))
            .expect("config JSON");
    for flag in flags {
        config[*flag] = true.into();
    }
    std::fs::write(
        &fixture.config,
        serde_json::to_vec(&config).expect("fault input"),
    )
    .expect("fault input");
    let project_before = snapshot(&fixture.project);
    let home_before = snapshot(&fixture.home);
    let calls_before = fixture.calls();
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!output.status.success());
    let public = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        public.contains("workspace private preparation output is unavailable"),
        "{public}"
    );
    assert!(!public.contains("adversarial-secret-backend-diagnostic"));
    assert_eq!(
        snapshot(&fixture.project),
        project_before,
        "project unchanged"
    );
    assert_eq!(snapshot(&fixture.home), home_before, "home unchanged");
    assert_eq!(fixture.calls(), calls_before, "no backend calls");
    assert!(!fixture.project.join(".effigy").exists());
    assert!(!fixture.fixture.path().join("applied").exists());
}

#[test]
fn cut_refuses_before_selected_handoff_faults() {
    assert_fault_refused_before_effects(&["fail_handoff", "fail_adoption_cleanup"]);
}

#[test]
fn cut_refuses_before_late_revalidation_fault() {
    assert_fault_refused_before_effects(&["delay_final_revalidation"]);
}

#[test]
fn cut_refuses_before_publication_fault() {
    assert_fault_refused_before_effects(&["fail_publication"]);
}

#[test]
fn cut_refuses_before_binding_staging_and_liveness_faults() {
    for case in ["fail_binding", "fail_binary_staging", "fail_liveness"] {
        assert_fault_refused_before_effects(&[case]);
    }
}

#[test]
#[cfg_attr(
    not(feature = "workspace-private-qualification"),
    ignore = "production private apply remains refused; qualified route owns this proof"
)]
fn actual_selected_handoff_failure_retains_owner_until_fresh_cleanup_recovery() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.config).expect("config"))
            .expect("config JSON");
    config["fail_handoff"] = true.into();
    config["fail_adoption_cleanup"] = true.into();
    std::fs::write(
        &fixture.config,
        serde_json::to_vec(&config).expect("fault configuration"),
    )
    .expect("fault input");
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(
        !output.status.success(),
        "a failed handoff must not acknowledge Serving"
    );
    let message = String::from_utf8_lossy(&output.stdout);
    assert!(
        message.contains("workspace selected handoff refused"),
        "{message}"
    );
    assert!(message.contains("cleanup failed"), "{message}");
    assert!(
        message.contains("exact selected owner retained"),
        "{message}"
    );
    // The original owning CLI pipe closes after its refusal. That single
    // owner-loss transition must retry the same owner with a fresh budget.
    fixture.assert_settled();
    let connection = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let sessions: i64 = connection
        .query_row("SELECT count(*) FROM workspace_session_runs", [], |row| {
            row.get(0)
        })
        .expect("session owners");
    assert_eq!(
        sessions, 1,
        "one original SessionSupervisor, no replacement owner"
    );
    let files: std::collections::BTreeMap<String, Vec<u8>> = serde_json::from_slice(
        &std::fs::read(fixture.fixture.path().join("guest-files.json")).expect("staged guest data"),
    )
    .expect("guest map");
    let document = files
        .values()
        .filter_map(|bytes| {
            serde_json::from_slice::<effigy_kernel::workspace_session::WorkspaceContextDocument>(
                bytes,
            )
            .ok()
        })
        .next()
        .expect("actual selected document");
    assert!(
        message.contains(&document.session.run_id),
        "the refusal must retain the exact published session ID"
    );
    let port: u16 = document
        .locator
        .endpoint
        .strip_prefix("effigy-host:")
        .expect("locator")
        .parse()
        .expect("port");
    assert!(
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(200)
        )
        .is_err(),
        "retired listener must be unavailable"
    );
    assert!(
        !fixture
            .fixture
            .path()
            .join("forced-fixture-cleanup")
            .exists(),
        "forced teardown cannot be recovery evidence"
    );
}

fn guarded_effect_failure_retains_exact_gate_free_custody(case: &str) {
    let fixture = BackendFixture::new();
    // Keep the two-second revalidation budget free of host process startup.
    // Publication exercises Unknown host effects; revalidation exercises the
    // known-absent host step alongside Unknown workspace permissions.
    let host_effect_declared = case == "fail_publication";
    if host_effect_declared {
        let manifest = fixture.project.join("effigy.toml");
        let mut declaration = std::fs::read_to_string(&manifest).expect("manifest");
        declaration.push_str(
            "\n[[containers.web.host_processes]]\nname = 'unsettled'\nrun = 'true'\nrestart = 'never'\n",
        );
        std::fs::write(manifest, declaration).expect("declared host effect");
    }
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.config).expect("configuration"))
            .expect("JSON");
    config[case] = true.into();
    std::fs::write(
        &fixture.config,
        serde_json::to_vec(&config).expect("fault input"),
    )
    .expect("fault");
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(
        !output.status.success(),
        "failure must not acknowledge Serving"
    );
    let public = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        public.contains("integration effects unsettled; re-run up"),
        "{public}"
    );
    assert!(!public.contains("cleanup settled"), "{public}");
    assert!(
        !public.contains("adversarial-secret-backend-diagnostic"),
        "{public}"
    );
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        1,
        "single apply"
    );
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let trace = std::fs::read_to_string(fixture.fixture.path().join("guest-files.unwind"))
            .unwrap_or_default();
        if trace.lines().count() >= 5 {
            assert!(
                trace.starts_with("HostIntegrations\nRoutes\nSecrets\nPermissions\nReadiness\n"),
                "reverse canonical order: {trace}"
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "ordinary reverse unwind was not attempted: {trace}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    if case == "delay_final_revalidation" {
        assert!(
            fixture
                .fixture
                .path()
                .join("guest-files.revalidation")
                .exists(),
            "delay was inside final revalidation"
        );
    }
    let connection = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let diagnostics: Vec<String> = connection
        .prepare("SELECT error FROM events WHERE error IS NOT NULL")
        .expect("error journal")
        .query_map([], |r| r.get(0))
        .expect("diagnostics")
        .collect::<Result<_, _>>()
        .expect("error rows");
    assert!(
        diagnostics
            .iter()
            .all(|text| !text.contains("adversarial-secret-backend-diagnostic")),
        "backend error bytes leaked into durable diagnostics"
    );
    let (state, environment, session): (String, String, String) = connection
        .query_row(
            "SELECT state,environment_run,session_run FROM workspace_activation_reservations WHERE session_run IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("original reservation");
    assert_eq!(state, "reserved", "no publication commit or cancellation");
    for id in [&environment, &session] {
        let (phase, closed): (String, Option<i64>) = connection
            .query_row(
                "SELECT phase,closed_at FROM runs WHERE run_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("exact owner");
        assert_ne!(phase, "closed", "unsettled effects must retain exact owner");
        assert_eq!(closed, None);
    }
    let records: Vec<String> = connection.prepare("SELECT target FROM events WHERE run_id=?1 AND kind='kernel:workspace-integration-step'").expect("steps").query_map([&session], |row| row.get(0)).expect("journal rows").collect::<Result<_,_>>().expect("records");
    let records: Vec<effigy_kernel::run::EffectSpec> = records
        .iter()
        .map(|r| serde_json::from_str(r).expect("typed effect"))
        .collect();
    assert_eq!(
        records
            .iter()
            .filter(|r| r.payload["state"] == "completed")
            .count(),
        5,
        "all five effects completed before failure"
    );
    assert!(
        records.iter().all(|r| {
            let unknown = r.payload["step"] == "permissions"
                || (host_effect_declared && r.payload["step"] == "host_integrations");
            r.payload["state"] != "cleared" || !unknown
        }),
        "Unknown mutations must retain their journal markers"
    );
    if !host_effect_declared {
        assert!(
            records.iter().any(
                |r| r.payload["state"] == "cleared" && r.payload["step"] == "host_integrations"
            ),
            "undeclared host integration is known absent"
        );
    }
    assert_no_durable_error_bytes(&fixture.project.join(".effigy"));
    assert_no_durable_error_bytes(&fixture.home);
    let cx = super::super::effects();
    let observe = super::super::observation();
    let root = super::super::state_root(&cx, &fixture.home, &fixture.project);
    let summary = super::super::read_summary(&observe, &root, environment.parse().expect("Env ID"))
        .expect("Env summary");
    let resource = &summary
        .invocation
        .expect("actual admitted invocation")
        .spec()
        .owner
        .clone();
    assert_eq!(resource.kind(), effigy_kernel::id::ResourceKind::Env);
    let gate = effigy_kernel::gate::Gate::acquire(
        &cx,
        &root,
        std::slice::from_ref(resource),
        effigy_kernel::gate::Wait::FailFast,
    )
    .expect("cleanup unwind runs outside Env gate");
    drop(gate);
    // Exact Unknown custody is deliberately unsettled. Fixture teardown stops
    // recorded PIDs; forced teardown is NOT cleanup or convergence evidence.
}

#[test]
#[cfg_attr(
    not(feature = "workspace-private-qualification"),
    ignore = "production private apply remains refused; qualified route owns this proof"
)]
fn actual_late_revalidation_failure_unwinds_outside_env_and_retains_unknown() {
    guarded_effect_failure_retains_exact_gate_free_custody("delay_final_revalidation");
}

#[test]
#[cfg_attr(
    not(feature = "workspace-private-qualification"),
    ignore = "production private apply remains refused; qualified route owns this proof"
)]
fn actual_phase_b_publication_failure_unwinds_outside_env_and_retains_unknown() {
    guarded_effect_failure_retains_exact_gate_free_custody("fail_publication");
}

#[test]
#[cfg_attr(
    not(feature = "workspace-private-qualification"),
    ignore = "production private apply remains refused; qualified route owns this proof"
)]
fn actual_binding_staging_and_liveness_errors_are_private_at_all_diagnostic_sinks() {
    for case in ["fail_binding", "fail_binary_staging", "fail_liveness"] {
        let fixture = BackendFixture::new();
        effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
            .expect("prepared identity");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fixture.config).expect("config")).expect("JSON");
        config[case] = true.into();
        std::fs::write(
            &fixture.config,
            serde_json::to_vec(&config).expect("fault input"),
        )
        .expect("fault");
        let output = fixture
            .workspace_command()
            .args(["--detach", "--json"])
            .output()
            .expect("actual CLI");
        assert!(!output.status.success(), "{case}: no Serving");
        for sink in [
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            std::fs::read_to_string(fixture.config.with_extension("failure")).unwrap_or_default(),
        ] {
            assert!(
                !sink.contains("adversarial-secret-backend-diagnostic"),
                "{case}: secret-bearing error escaped"
            );
        }
        let connection = rusqlite::Connection::open_with_flags(
            fixture.project.join(".effigy/state/kernel.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("journal");
        let diagnostics: Vec<String> = connection
            .prepare("SELECT error FROM events WHERE error IS NOT NULL")
            .expect("errors")
            .query_map([], |r| r.get(0))
            .expect("rows")
            .collect::<Result<_, _>>()
            .expect("diagnostics");
        assert!(
            diagnostics
                .iter()
                .all(|s| !s.contains("adversarial-secret-backend-diagnostic")),
            "{case}: durable error leak"
        );
        assert_no_durable_error_bytes(&fixture.project.join(".effigy"));
        assert_no_durable_error_bytes(&fixture.home);
        let classifications = std::fs::read_to_string(fixture.config.with_extension("failure"))
            .expect("fixed stage log");
        assert!(
            classifications.contains(if case == "fail_binding" {
                "bind/reacquire"
            } else {
                "publication/selection"
            }),
            "the injected {case} path was exercised: {classifications}"
        );
        // Unknown custody may intentionally require forced exact-PID fixture
        // teardown. No settlement/recovery claim follows from this assertion.
    }
}

#[test]
#[cfg_attr(
    not(feature = "workspace-private-qualification"),
    ignore = "production private apply remains refused; qualified route owns this proof"
)]
fn a_settled_activation_cleanup_never_hides_owned_residue() {
    // effigy#901: custody settling is not resource removal. The reported
    // sentence may claim "settled" only when the owned project's inventory is
    // verifiably empty; otherwise it names the residue and the one recovery.
    for configured in [
        None,
        Some(serde_json::json!(["checkout-dev-ec-746e15dc40d7-app-1"])),
    ] {
        let fixture = BackendFixture::new();
        effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
            .expect("prepared identity");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fixture.config).expect("config")).expect("JSON");
        config["fail_binding"] = true.into();
        if let Some(residue) = &configured {
            config["residue"] = residue.clone();
        }
        std::fs::write(
            &fixture.config,
            serde_json::to_vec(&config).expect("fault input"),
        )
        .expect("fault");
        let output = fixture
            .workspace_command()
            .args(["--detach", "--json"])
            .output()
            .expect("actual CLI");
        assert!(
            !output.status.success(),
            "no Serving on a failed activation"
        );
        let public = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            public.contains("workspace activation failed during bind/reacquire"),
            "{public}"
        );
        match configured {
            None => assert!(
                public.contains("exact owner cleanup settled"),
                "a verified empty owned inventory is settled: {public}"
            ),
            Some(_) => {
                assert!(!public.contains("cleanup settled"), "{public}");
                assert!(
                    public.contains(
                        "1 owned container(s) remain: checkout-dev-ec-746e15dc40d7-app-1"
                    ),
                    "{public}"
                );
                assert!(public.contains("effigy container down"), "{public}");
            }
        }
        fixture.assert_settled();
    }
}

#[test]
fn actual_cli_refuses_unqualified_private_preparation_before_any_prepare_or_apply() {
    let fixture = BackendFixture::new();
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!output.status.success());
    let public = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        public.contains("workspace private preparation output is unavailable"),
        "{public}"
    );
    assert!(
        !fixture.project.join(".git/effigy-checkout-id").exists(),
        "no checkout token minted"
    );
    assert!(
        !fixture.project.join(".effigy").exists(),
        "no generated/render/journal/log state"
    );
    assert!(
        !fixture.fixture.path().join("applied").exists(),
        "no compose apply"
    );
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        0
    );
}

#[test]
fn actual_detached_entry_refuses_valid_prepared_input_before_admission_or_plan() {
    use effigy_containers::environment::{BackendKind, BackendReason, BackendSelection};
    use std::io::Write;
    use std::process::Stdio;
    let fixture = BackendFixture::new();
    let token = effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("ordinary token setup");
    let policy = effigy_containers::load_container_policy(&fixture.project, Some("web"))
        .expect("actual in-memory prepared policy");
    let selected = BackendSelection {
        kind: BackendKind::DockerEngine,
        engine: effigy_containers::BackendId::docker_compose(),
        context: "default".into(),
        reason: BackendReason::ManifestDriver("docker".into()),
    };
    let declaration = effigy::runner::workspace_prepared_declaration_fixture(&policy, &selected)
        .expect("actual file/source fingerprints");
    let checkout: Vec<u8> = token
        .as_bytes()
        .chunks_exact(2)
        .map(|bytes| {
            u8::from_str_radix(std::str::from_utf8(bytes).expect("hex"), 16).expect("token byte")
        })
        .collect();
    let before_project = snapshot(&fixture.project);
    let before_home = snapshot(&fixture.home);
    let before_calls = fixture.calls();
    let request = serde_json::json!({"schema":"effigy.workspace-serving.v1","state_home":fixture.home,"project_path":fixture.project,"checkout":checkout,"environment":"web","profile":"default","activation":{"kind":"ContainerUp","request":{"environment":"web","retention":"persistent","budget_ms":60000,"post_prep":declaration}}});
    let mut command = fixture.command();
    command
        .arg("__workspace-serving-fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("real detached front end");
    child
        .stdin
        .take()
        .expect("request pipe")
        .write_all(request.to_string().as_bytes())
        .expect("payload");
    let output = child.wait_with_output().expect("detached refusal");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "no Serving ack");
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostic.contains("workspace private preparation output is unavailable"),
        "{diagnostic}"
    );
    assert!(
        !diagnostic.contains("supervisor_run="),
        "no host run admitted"
    );
    assert_eq!(
        snapshot(&fixture.project),
        before_project,
        "no new token/journal/plan/secret/log files"
    );
    assert_eq!(snapshot(&fixture.home), before_home, "no home state files");
    assert_eq!(
        fixture.calls(),
        before_calls,
        "no engine observation or effect"
    );
}

/// Real CLI, real Env/helper/file custody, doubled backend and guest only.
#[cfg(feature = "workspace-private-qualification")]
#[test]
fn actual_cli_fake_backend_reaches_committed_workspace() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "{diagnostics}\nfixed stage: {}\nengine calls: {}",
        std::fs::read_to_string(fixture.config.with_extension("failure")).unwrap_or_default(),
        fixture.calls()
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON success");
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        1,
        "one actual apply: {}",
        fixture.calls()
    );
    let connection = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let selected: i64 = connection
        .query_row(
            "SELECT count(*) FROM workspace_activation_reservations WHERE state='committed'",
            [],
            |row| row.get(0),
        )
        .expect("actual commit");
    assert_eq!(selected, 1, "actual committed workspace reservation");
    let selected_run: String = connection
        .query_row(
            "SELECT session_run FROM workspace_selection_obligations WHERE state='switched'",
            [],
            |row| row.get(0),
        )
        .expect("durable switched selection");
    let phase: String = connection
        .query_row(
            "SELECT phase FROM runs WHERE run_id=?1",
            [&selected_run],
            |row| row.get(0),
        )
        .expect("selected live owner");
    assert_ne!(
        phase, "closed",
        "Serving acknowledges a live selected owner"
    );
    let files: std::collections::BTreeMap<String, Vec<u8>> = serde_json::from_slice(
        &std::fs::read(fixture.config.parent().unwrap().join("guest-files.json"))
            .expect("actual guest writes"),
    )
    .expect("guest map");
    let documents: Vec<_> = files
        .values()
        .filter_map(|bytes| {
            serde_json::from_slice::<effigy_kernel::workspace_session::WorkspaceContextDocument>(
                bytes,
            )
            .ok()
        })
        .collect();
    assert_eq!(documents.len(), 1, "one complete v2 publication");
    assert_eq!(
        documents[0].session.run_id, selected_run,
        "guest publication names the actually selected owner"
    );
    drop(connection);
    fixture.assert_settled();
    assert_no_durable_error_bytes(&fixture.project);
    assert_no_durable_error_bytes(&fixture.home);
    assert!(
        !fixture
            .fixture
            .path()
            .join("forced-fixture-cleanup")
            .exists(),
        "normal cleanup required"
    );
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn qualification_private_preparation_failure_discards_all_backend_diagnostics() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    std::fs::write(fixture.config.with_extension("prepare-failure"), b"fail")
        .expect("private fixture fault");
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("actual CLI");
    assert!(!output.status.success());
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.contains("stage=child-preparation"),
        "{diagnostics}"
    );
    assert!(!diagnostics.contains("adversarial-secret-backend-diagnostic"));
    assert!(
        fixture
            .config
            .with_extension("prepare-failure-observed")
            .exists(),
        "real failing backend boundary crossed"
    );
    assert!(
        !fixture.calls().contains("\"up\""),
        "preparation failure prevents apply"
    );
    fixture.assert_settled();
    assert_no_durable_error_bytes(&fixture.project);
    assert_no_durable_error_bytes(&fixture.home);
    assert!(!fixture
        .fixture
        .path()
        .join("forced-fixture-cleanup")
        .exists());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn qualification_preparation_entry_refuses_a_forged_process_receipt() {
    let fixture = BackendFixture::new();
    let cx = super::super::effects();
    let root = super::super::state_root(&cx, &fixture.home, &fixture.project);
    drop(effigy_kernel::store::Store::open(&cx, &root).expect("empty real journal"));
    let intent = serde_json::json!({
        "directory": effigy_kernel::fs::DirFd::open(&fixture.project).unwrap().identity().unwrap(),
        "checkout": effigy_kernel::id::CheckoutId::new(effigy_kernel::id::CheckoutKind::EphemeralClone, [44;16]),
        "environment": "web", "backend_kind": effigy_containers::environment::BackendKind::DockerDesktop,
        "engine":"docker", "profile":"default", "render":null,
    });
    let before_project = snapshot(&fixture.project);
    let before_home = snapshot(&fixture.home);
    let output = fixture
        .command()
        .arg("__kernel-workspace-private-prepare")
        .arg(intent.to_string())
        .env(
            "EFFIGY_CUSTODY_NONCE",
            format!("{}:1", effigy_kernel::id::RunId::new(1, [44; 10]).unwrap()),
        )
        .output()
        .expect("direct forged child");
    assert_eq!(
        output.status.code(),
        Some(66),
        "real custody authentication refused"
    );
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert_eq!(
        snapshot(&fixture.project),
        before_project,
        "no effects on unauthenticated entry"
    );
    assert_eq!(snapshot(&fixture.home), before_home);
    assert!(fixture.calls().is_empty());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn qualification_preparation_receipt_cannot_authorize_a_different_intent() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let output = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .env("EFFIGY_TEST_PREPARATION_INTENT_FAULT", "1")
        .output()
        .expect("actual qualified spawn");
    assert!(!output.status.success());
    let diagnostics = String::from_utf8_lossy(&output.stdout);
    assert!(diagnostics.contains("stage=child-custody"), "{diagnostics}");
    assert!(
        fixture.calls().is_empty(),
        "mismatched intent gets no backend I/O"
    );
    fixture.assert_settled();
    assert_no_durable_error_bytes(&fixture.project);
    assert_no_durable_error_bytes(&fixture.home);
    assert!(!fixture
        .fixture
        .path()
        .join("forced-fixture-cleanup")
        .exists());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn actual_cli_down_then_fresh_up_retires_selected_session_and_commits_a_new_one() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let up = || {
        let output = fixture
            .workspace_command()
            .args(["--detach", "--json"])
            .output()
            .expect("real up");
        assert!(
            output.status.success(),
            "{}{} stages={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            std::fs::read_to_string(fixture.config.with_extension("failure")).unwrap_or_default()
        );
        let db = rusqlite::Connection::open_with_flags(
            fixture.project.join(".effigy/state/kernel.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("journal");
        db.query_row("SELECT session_run,environment_generation FROM workspace_selection_obligations WHERE state='switched' ORDER BY environment_generation DESC LIMIT 1", [], |row| Ok((row.get::<_,String>(0)?, row.get::<_,i64>(1)?))).expect("selected owner")
    };
    let first = up();
    let down = fixture
        .command()
        .args(["container", "web", "down", "--json"])
        .output()
        .expect("real down");
    assert!(
        down.status.success(),
        "{}{}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    let db = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let closed: bool = db
        .query_row(
            "SELECT phase='closed' AND cleanup_outcome='complete' FROM runs WHERE run_id=?1",
            [&first.0],
            |row| row.get(0),
        )
        .expect("original session retirement");
    assert!(
        closed,
        "down must retire the actual selected session before it acknowledges"
    );
    drop(db);
    assert!(
        !fixture.fixture.path().join("applied").exists(),
        "real backend down ran"
    );
    let second = up();
    assert_ne!(second.0, first.0, "fresh actual session owner");
    assert!(second.1 > first.1, "fresh Env generation");
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.contains("\"up\""))
            .count(),
        2
    );
    fixture.assert_settled();
    assert_no_durable_error_bytes(&fixture.project);
    assert_no_durable_error_bytes(&fixture.home);
    assert!(!fixture
        .fixture
        .path()
        .join("forced-fixture-cleanup")
        .exists());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn actual_cli_crash_recovery_refuses_reuse_then_down_and_fresh_up() {
    let fixture = BackendFixture::new();
    effigy_core::worktree_scope::prepare_checkout_token(&fixture.project)
        .expect("prepared identity");
    let first = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("first up");
    assert!(first.status.success(), "first up");
    let db = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let original: String = db
        .query_row(
            "SELECT session_run FROM workspace_selection_obligations WHERE state='switched'",
            [],
            |row| row.get(0),
        )
        .expect("original session");
    let supervisor: String = db
        .query_row(
            "SELECT supervisor FROM workspace_session_custody WHERE run_id=?1",
            [&original],
            |row| row.get(0),
        )
        .expect("actual supervisor");
    let raw: String = db
        .query_row(
            "SELECT owner_identity FROM runs WHERE run_id=?1",
            [&supervisor],
            |row| row.get(0),
        )
        .expect("supervisor birth");
    let identity: effigy_kernel::ident::ProcessIdentity =
        serde_json::from_str(&raw).expect("full identity");
    assert!(
        matches!(effigy_kernel::ident::process_identity(identity.pid), Observed::Verified(ref current,_) if *current == identity)
    );
    let fault = Command::new(env!("CARGO_BIN_EXE_workspace-qualification-fixture"))
        .arg("crash")
        .arg(&fixture.project)
        .arg(&supervisor)
        .env("HOME", &fixture.home)
        .output()
        .expect("exact kernel-verified fixture fault");
    assert!(
        fault.status.success(),
        "{}",
        String::from_utf8_lossy(&fault.stderr)
    );
    drop(db);
    let refused = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("uncertain reuse");
    assert!(
        !refused.status.success(),
        "unproved existing-target artifact selection must refuse"
    );
    let db = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let selected: String = db.query_row("SELECT session_run FROM workspace_selection_obligations WHERE state='switched' ORDER BY environment_generation DESC LIMIT 1", [], |row| row.get(0)).expect("selection unchanged by refusal");
    assert_eq!(selected, original, "no premature selector switch");
    drop(db);
    let down = fixture
        .command()
        .args(["container", "web", "down", "--json"])
        .output()
        .expect("explicit recovery down");
    assert!(
        down.status.success(),
        "{}{}",
        String::from_utf8_lossy(&down.stdout),
        String::from_utf8_lossy(&down.stderr)
    );
    let next = fixture
        .workspace_command()
        .args(["--detach", "--json"])
        .output()
        .expect("fresh up after down");
    assert!(
        next.status.success(),
        "{}{}",
        String::from_utf8_lossy(&next.stdout),
        String::from_utf8_lossy(&next.stderr)
    );
    let db = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let successor: String = db.query_row("SELECT session_run FROM workspace_selection_obligations WHERE state='switched' ORDER BY environment_generation DESC LIMIT 1", [], |row| row.get(0)).expect("fresh session");
    assert_ne!(successor, original);
    let closed: bool = db
        .query_row(
            "SELECT phase='closed' AND cleanup_outcome='complete' FROM runs WHERE run_id=?1",
            [&original],
            |row| row.get(0),
        )
        .expect("original closure");
    assert!(
        closed,
        "actual recovery closes the original session before success"
    );
    drop(db);
    fixture.assert_settled();
    assert_no_durable_error_bytes(&fixture.project);
    assert!(!fixture
        .fixture
        .path()
        .join("forced-fixture-cleanup")
        .exists());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn actual_cli_secret_interpolation_is_owned_through_commit_and_never_reaches_diagnostics() {
    secret_interpolation_cli(false);
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn actual_cli_secret_interpolation_vm_file_is_transferred_to_the_same_env_owner() {
    secret_interpolation_cli(true);
}

#[cfg(feature = "workspace-private-qualification")]
fn secret_interpolation_cli(vm: bool) {
    let manifest = format!("{WORKSPACE_DECLARATION}\n[secrets]\nbackend = \"effigy-vault\"\n[secrets.vault]\npath = \".effigy/secrets/local.vault\"\n[secrets.keys.probe_token]\nrequired = true\ntargets = [\"containers\"]\n");
    let fixture = BackendFixture::with_manifest("qualified-interpolation", &manifest);
    let prepared = fixture
        .command()
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
        .collect::<Vec<_>>();
    let setup = Command::new(env!("CARGO_BIN_EXE_workspace-qualification-fixture"))
        .args(["prepare"])
        .arg(&fixture.project)
        .envs(prepared)
        .output()
        .expect("real isolated vault producer");
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    std::fs::write(
        fixture.config.with_extension("expect-interpolation"),
        b"required",
    )
    .unwrap();
    let mut command = fixture.workspace_command();
    if vm {
        let temp = fixture.fixture.path().join("tmp");
        std::fs::create_dir(&temp).unwrap();
        command
            .env("EFFIGY_COMPOSE_BACKEND", "containerd")
            .env("TMPDIR", temp);
    }
    let output = command
        .args(["--detach", "--json"])
        .output()
        .expect("actual secret-bearing up");
    assert!(
        output.status.success(),
        "{}{} stages={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        std::fs::read_to_string(fixture.config.with_extension("failure")).unwrap_or_default()
    );
    let canary = b"531-private-compose-canary";
    assert!(!output
        .stdout
        .windows(canary.len())
        .any(|bytes| bytes == canary));
    assert!(!output
        .stderr
        .windows(canary.len())
        .any(|bytes| bytes == canary));
    let original = std::fs::read_to_string(fixture.config.with_extension("interpolation-observed"))
        .expect("backend read exact interpolation file");
    assert_eq!(
        original == "host-environment",
        !vm,
        "actual backend delivery kind"
    );
    assert!(
        !std::path::Path::new(&original).exists(),
        "verified closure cleans the exact file before commit finishes"
    );
    let db = rusqlite::Connection::open_with_flags(
        fixture.project.join(".effigy/state/kernel.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("journal");
    let combined: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE receipt IS NOT NULL AND json_type(receipt,'$.interpolation_v1.discard_v1') IS NOT NULL)", [], |row| row.get(0)).expect("durable combined receipt");
    assert_eq!(combined, vm, "VM apply must own the exact interpolation file; host Docker delivers through its environment");
    drop(db);
    fixture.assert_settled();
    fn scan(path: &std::path::Path, canary: &[u8]) {
        if path.is_symlink() {
            return;
        }
        if path.is_dir() {
            for child in std::fs::read_dir(path).unwrap() {
                scan(&child.unwrap().path(), canary);
            }
        } else if path.is_file() {
            assert!(
                !std::fs::read(path)
                    .unwrap()
                    .windows(canary.len())
                    .any(|bytes| bytes == canary),
                "secret leaked into durable fixture state"
            );
        }
    }
    scan(&fixture.project, canary);
    scan(&fixture.home, canary);
    assert!(!fixture
        .fixture
        .path()
        .join("forced-fixture-cleanup")
        .exists());
}

#[cfg(feature = "workspace-private-qualification")]
#[test]
fn qualification_preparation_never_starts_or_repairs_a_stopped_or_unknown_vm() {
    for fault in ["runtime-stopped", "runtime-status-unknown"] {
        let fixture = BackendFixture::new();
        effigy_core::worktree_scope::prepare_checkout_token(&fixture.project).unwrap();
        let temp = fixture.fixture.path().join("tmp");
        std::fs::create_dir(&temp).unwrap();
        std::fs::write(fixture.config.with_extension(fault), b"required refusal").unwrap();
        let output = fixture
            .workspace_command()
            .args(["--detach", "--json"])
            .env("EFFIGY_COMPOSE_BACKEND", "containerd")
            .env("TMPDIR", temp)
            .output()
            .expect("qualified entry");
        assert!(
            !output.status.success(),
            "stopped/unknown backend must refuse"
        );
        let calls = fixture
            .calls()
            .lines()
            .map(|line| serde_json::from_str::<Vec<String>>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(
            calls
                .iter()
                .any(|args| args.iter().any(|arg| arg == "status")),
            "real readiness boundary crossed"
        );
        assert!(
            !calls.iter().flatten().any(|arg| [
                "start",
                "stop",
                "restart",
                "delete",
                "rm",
                "up",
                "down",
                "systemctl"
            ]
            .contains(&arg.as_str())),
            "qualification may not mutate the VM/backend on unavailable readiness"
        );
        fixture.assert_settled();
        assert!(!fixture
            .fixture
            .path()
            .join("forced-fixture-cleanup")
            .exists());
    }
}
