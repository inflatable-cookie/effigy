//! Actual CLI -> detached production orchestration, with only external engine
//! and guest I/O doubled. This never qualifies a native backend.
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

impl BackendFixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let fixture =
            effigy_test_support::Fixture::new("container-production-fake").expect("fixture");
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
        std::fs::write(
            project.join("effigy.toml"),
            r#"
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
"#,
        )
        .expect("manifest");
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

#[test]
fn actual_cli_fake_backend_reaches_committed_workspace() {
    let fixture = BackendFixture::new();
    let output = fixture
        .command()
        .args(["container", "up", "--detach", "--json"])
        .output()
        .expect("actual CLI");
    let diagnostics = format!(
        "{}\n{}\nengine calls:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}\nprivate fixture diagnostic: {}",
            fixture
                .calls()
                .lines()
                .rev()
                .take(18)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"),
            std::fs::read_to_string(fixture.config.with_extension("failure")).unwrap_or_default()
        )
    );
    assert!(output.status.success(), "{diagnostics}");
    assert!(
        fixture.project.join(".git/effigy-runtime-scope").is_file(),
        "ordinary checkout identity preparation"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("normal container JSON report");
    assert_eq!(report["schema"], "effigy.command.v1");
    assert_eq!(report["ok"], true);
    assert!(report["result"].is_object());
    assert!(
        fixture.fixture.path().join("applied").exists(),
        "one actual gated apply through fake engine"
    );
    let applies = fixture
        .calls()
        .lines()
        .filter(|line| line.contains("\"up\""))
        .count();
    assert_eq!(applies, 1, "one apply");
    // This test process is a third process: the initiating CLI above has
    // exited, and it never received a run/lease owner or a preissued grant.
    let public_output = format!(
        "{}{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        fixture.calls()
    );
    let probe = effigy::workspace_serving_fixture::probe_production_guest(
        &fixture.fixture.path().join("guest-files.json"),
        &public_output,
    )
    .expect("third process issued-capability Observe");
    assert_eq!(probe.response["schema"], "effigy.runs.v1");
    assert_eq!(probe.response["observed"], true);
    assert!(probe.response["runs"].is_array());
    assert!(
        probe.refused_unissued,
        "unissued capability must be refused"
    );
    assert!(
        probe.wrong_pin_refused,
        "wrong pin must fail before application frames"
    );
    assert!(
        probe.no_credential_echo,
        "startup/response must not echo credentials"
    );
    fixture.assert_settled();
}
