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

#[test]
fn actual_selected_handoff_failure_retains_owner_until_fresh_cleanup_recovery() {
    let fixture = BackendFixture::new();
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
        .command()
        .args(["container", "up", "--detach", "--json"])
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
        .command()
        .args(["container", "up", "--detach", "--json"])
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
            "SELECT state,environment_run,session_run FROM workspace_activation_reservations",
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
        records.iter().all(|r| r.payload["state"] != "cleared"
            || !matches!(
                r.payload["step"].as_str(),
                Some("permissions" | "host_integrations")
            )),
        "Unknown mutations must retain their journal markers"
    );
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
fn actual_late_revalidation_failure_unwinds_outside_env_and_retains_unknown() {
    guarded_effect_failure_retains_exact_gate_free_custody("delay_final_revalidation");
}

#[test]
fn actual_phase_b_publication_failure_unwinds_outside_env_and_retains_unknown() {
    guarded_effect_failure_retains_exact_gate_free_custody("fail_publication");
}

#[test]
fn actual_binding_staging_and_liveness_errors_are_private_at_all_diagnostic_sinks() {
    for case in ["fail_binding", "fail_binary_staging", "fail_liveness"] {
        let fixture = BackendFixture::new();
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fixture.config).expect("config")).expect("JSON");
        config[case] = true.into();
        std::fs::write(
            &fixture.config,
            serde_json::to_vec(&config).expect("fault input"),
        )
        .expect("fault");
        let output = fixture
            .command()
            .args(["container", "up", "--detach", "--json"])
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
