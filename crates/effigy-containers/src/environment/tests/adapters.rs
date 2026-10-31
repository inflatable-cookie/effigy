//! Strict-fake proofs for both backend adapters (§12.2): fake `colima` and
//! `docker` executables record every invocation. The selected adapter issues
//! exactly its own inventory calls, and the unselected backend sees none. The
//! apply and the stop are not adapter work: the caller passes them to the
//! kernel as one effect.

use super::*;
use std::os::unix::fs::PermissionsExt;

const ID: &str = "87c434af41df6f447bfb13218637f97b211098ffe97573a00c766c56f5008cdd";
const OTHER_ID: &str = "5d0c7e9a2b1f4c6d8e0a3b5c7d9e1f2a4b6c8d0e2f4a6b8c0d2e4f6a8b0c2d4e";

/// The container the fake backend reports, exactly as `docker inspect` writes
/// it.
fn fixture_inspect() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/environment/underlay-workspace.inspect.json"
    );
    std::fs::read_to_string(path).expect("inspect fixture")
}

struct FakeTools {
    _dir: effigy_test_support::Fixture,
    bin: PathBuf,
    logs: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    // Held until `Drop` restores PATH: shared-process test runs share it.
    _env: std::sync::MutexGuard<'static, ()>,
}

impl FakeTools {
    fn install() -> Self {
        Self::install_answering(&format!("{ID}\n"), &fixture_inspect())
    }

    /// The same fake tools, answering the listing and the inspect with exactly
    /// these bytes: a proof can then hand the adapter a short, duplicate or
    /// incomplete answer and see it refuse.
    fn install_answering(listing: &str, inspect_json: &str) -> Self {
        let env = crate::test_env_lock();
        let dir = home();
        let bin = dir.path().join("bin");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let inspect = dir.path().join("inspect.json");
        std::fs::write(&inspect, inspect_json).unwrap();
        let listed = dir.path().join("listing.txt");
        std::fs::write(&listed, listing).unwrap();
        for tool in ["colima", "docker"] {
            let script = bin.join(tool);
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}/{tool}'\ncase \" $* \" in\n  *' ps '*) cat '{}' ;;\n  *' inspect '*) cat '{}' ;;\nesac\nexit 0\n",
                    logs.display(),
                    listed.display(),
                    inspect.display(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let previous_path = std::env::var_os("PATH");
        let mut path = std::ffi::OsString::from(&bin);
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            _dir: dir,
            bin,
            logs,
            previous_path,
            _env: env,
        }
    }

    fn calls(&self, tool: &str) -> Vec<String> {
        std::fs::read_to_string(self.logs.join(tool))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for FakeTools {
    fn drop(&mut self) {
        let _ = &self.bin;
        match self.previous_path.take() {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

fn exercise(selection: BackendSelection, tools: &FakeTools) -> Vec<Workload> {
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let mut adapter = selection.adapter();
    match adapter.inventory(&workdir, PROJECT) {
        Observed::Verified(rows, _) => rows,
        other => panic!("inventory: {other:?}"),
    }
}

#[test]
fn colima_adapter_targets_only_its_profile() {
    let tools = FakeTools::install();
    let rows = exercise(selection(), &tools);
    assert_eq!(
        tools.calls("colima"),
        [
            format!(
                "nerdctl --profile fixture -- ps --all --no-trunc --filter label=com.docker.compose.project={PROJECT} --format {{{{.ID}}}}"
            ),
            format!("nerdctl --profile fixture -- inspect {ID}"),
        ]
    );
    assert!(
        tools.calls("docker").is_empty(),
        "unselected backend was called"
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, ID);
    assert!(rows[0].running);
    assert_eq!(
        rows[0].labels.get("com.effigy.scope").map(String::as_str),
        Some("4677bfcb81da18054ef32e71aa3f3c29")
    );
}

#[test]
fn docker_adapter_targets_only_the_docker_cli() {
    let tools = FakeTools::install();
    let rows = exercise(docker_selection(), &tools);
    assert_eq!(
        tools.calls("docker"),
        [
            format!(
                "ps --all --no-trunc --filter label=com.docker.compose.project={PROJECT} --format {{{{.ID}}}}"
            ),
            format!("inspect {ID}"),
        ]
    );
    assert!(
        tools.calls("colima").is_empty(),
        "unselected backend was called"
    );
    assert_eq!(rows.len(), 1);
}

#[test]
fn selection_reasons_follow_policy_precedence() {
    let _env = crate::test_env_lock();
    let home = home();
    let previous = (
        std::env::var_os("HOME"),
        std::env::var_os("EFFIGY_COMPOSE_BACKEND"),
    );
    std::env::set_var("HOME", home.path());
    std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
    let policy = super::policy(home.path());
    // The engine and precedence are `crate::backend::exact_backend`'s; this
    // seam adds the context and the reason it shows.
    let selected = select_backend(home.path(), &policy);
    assert_eq!(selected.kind, BackendKind::Colima);
    assert_eq!(selected.context, policy.profile);
    assert_eq!(
        selected.reason,
        BackendReason::ManifestDriver("colima".into())
    );
    // A record the store does not hold grants nothing: it is used only when
    // the exact backend names the record as its source.
    let mut recorded = selected.clone();
    recorded.context = "recorded".into();
    recorded.reason = BackendReason::Recorded("user-preference".into());
    assert_eq!(
        effective_backend(home.path(), &policy, Some(&recorded)),
        selected
    );
    // An explicit backend for this invocation outranks every other source.
    std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");
    let explicit = effective_backend(home.path(), &policy, Some(&recorded));
    assert_eq!(explicit.engine, crate::BackendId::docker_compose());
    assert_eq!(
        explicit.reason,
        BackendReason::EnvironmentVariable("docker".into())
    );
    assert!(explicit
        .describe()
        .contains("EFFIGY_COMPOSE_BACKEND=docker"));
    let (home, backend) = previous;
    match home {
        Some(home) => std::env::set_var("HOME", home),
        None => std::env::remove_var("HOME"),
    }
    match backend {
        Some(backend) => std::env::set_var("EFFIGY_COMPOSE_BACKEND", backend),
        None => std::env::remove_var("EFFIGY_COMPOSE_BACKEND"),
    }
}

/// effigy#508: neither adapter can name the run that created a container, and
/// neither pretends it can. The attempt issues no backend command at all —
/// "observe the project and infer" is exactly the receipt this seam refuses to
/// mint — and the reason names the engine and the missing API.
#[test]
fn no_adapter_mints_a_creator_receipt_from_observation() {
    let tools = FakeTools::install();
    for (mut adapter, tool) in [
        (selection().adapter(), "colima"),
        (docker_selection().adapter(), "docker"),
    ] {
        match adapter.created_by(PROJECT, ID) {
            Observed::Unknown(UnknownReason::NoBackendRecord(detail)) => {
                assert!(
                    detail.contains("keeps no record of which client created a container"),
                    "{detail}"
                );
                assert!(
                    detail.contains(adapter.selection().kind.label()),
                    "{detail}"
                );
                assert!(detail.contains(ID), "{detail}");
                assert!(detail.contains(PROJECT), "{detail}");
            }
            other => panic!("{tool} adapter answered a creator question: {other:?}"),
        }
    }
    assert!(
        tools.calls("colima").is_empty() && tools.calls("docker").is_empty(),
        "a creator question reached the backend: {:?} {:?}",
        tools.calls("colima"),
        tools.calls("docker")
    );
}

fn docker_selection() -> BackendSelection {
    BackendSelection {
        kind: if cfg!(target_os = "macos") {
            BackendKind::DockerDesktop
        } else {
            BackendKind::DockerEngine
        },
        engine: crate::BackendId::docker_compose(),
        context: "default".into(),
        reason: BackendReason::UserPreference,
    }
}

/// effigy#508: the creation window's bounded inventory shares one absolute
/// deadline. An expired deadline refuses before the read spawns anything, so a
/// hung runtime can never hold the environment gate open.
#[test]
fn an_expired_window_deadline_refuses_the_inventory_without_reading() {
    let tools = FakeTools::install();
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
    for mut adapter in [selection().adapter(), docker_selection().adapter()] {
        match adapter.inventory_until(&workdir, PROJECT, deadline) {
            Observed::Unknown(UnknownReason::TimedOut { phase }) => {
                assert_eq!(
                    phase, "list project",
                    "the phase names the read that refused"
                )
            }
            other => panic!("an expired window must time out, not {other:?}"),
        }
    }
    assert!(
        tools.calls("colima").is_empty() && tools.calls("docker").is_empty(),
        "an expired deadline spawned a backend read: {:?} {:?}",
        tools.calls("colima"),
        tools.calls("docker")
    );
}

/// The bounded inventory issues exactly the unbounded one's calls, so the
/// window reads the same exact identity the rest of the seam does.
#[test]
fn a_bounded_inventory_reads_the_same_exact_identity() {
    let tools = FakeTools::install();
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut adapter = selection().adapter();
    assert!(matches!(
        adapter.inventory_until(&workdir, PROJECT, deadline),
        Observed::Verified(rows, _) if rows.len() == 1 && rows[0].id == ID
    ));
    assert_eq!(
        tools.calls("colima"),
        [
            format!(
                "nerdctl --profile fixture -- ps --all --no-trunc --filter label=com.docker.compose.project={PROJECT} --format {{{{.ID}}}}"
            ),
            format!("nerdctl --profile fixture -- inspect {ID}"),
        ]
    );
}

/// effigy#508: an inventory is exact or it is nothing. Nothing the listing
/// says may disagree with what the inspect answers, in either direction.
#[test]
fn an_inventory_that_is_not_exactly_the_listing_refuses() {
    // The fixture row, and a second real row this proof lifts from it.
    let one: Vec<serde_json::Value> = serde_json::from_str(&fixture_inspect()).expect("fixture");
    let mut other = one[0].clone();
    other["Id"] = serde_json::Value::String(OTHER_ID.to_owned());
    other["Name"] = serde_json::Value::String("/acme-stack-db-1".to_owned());
    let rows = |rows: Vec<serde_json::Value>| serde_json::to_string(&rows).expect("json");
    let single = rows(vec![one[0].clone()]);
    let only_other = rows(vec![other.clone()]);
    let doubled = rows(vec![one[0].clone(), one[0].clone()]);

    let cases: [(&str, &str, &str, &str); 6] = [
        (
            "a listing that is not a full id",
            "87c434af41df\n",
            &single,
            "not a full container id",
        ),
        (
            "a listing that names one container twice",
            &format!("{ID}\n{ID}\n"),
            &single,
            "twice",
        ),
        (
            "an inspect that omits a listed container",
            &format!("{ID}\n{OTHER_ID}\n"),
            &single,
            "answered 1 containers for 2 listed ids",
        ),
        (
            "an inspect that answers a container the listing did not name",
            &format!("{ID}\n"),
            &only_other,
            "which the listing did not name",
        ),
        (
            "an inspect that answers one container twice",
            &format!("{ID}\n{OTHER_ID}\n"),
            &doubled,
            "twice",
        ),
        (
            "an inspect that reports a non-full id",
            &format!("{ID}\n"),
            r#"[{"Id":"87c434af41df","Name":"/acme-stack-workspace-1","Image":"sha256:aa","Created":"2999-01-01T00:00:00Z","State":{"Running":true},"Config":{"Image":"rust:1.97","Labels":{}}}]"#,
            "not a full container id",
        ),
    ];
    for (case, listing, inspect, expected) in cases {
        let tools = FakeTools::install_answering(listing, inspect);
        let workdir = tools.logs.parent().unwrap().to_path_buf();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut adapter = selection().adapter();
        match adapter.inventory_until(&workdir, PROJECT, deadline) {
            Observed::Unknown(UnknownReason::Inconsistent(detail)) => {
                assert!(detail.contains(expected), "{case}: {detail}")
            }
            other => panic!("{case} was accepted as an inventory: {other:?}"),
        }
    }
}

/// The exact inventory still verifies when the listing and the inspect agree,
/// and the rows carry the image reference and creation stamp the window binds.
#[test]
fn an_exact_inventory_carries_the_image_reference_and_stamp() {
    let tools = FakeTools::install();
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut adapter = selection().adapter();
    match adapter.inventory_until(&workdir, PROJECT, deadline) {
        Observed::Verified(rows, _) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, ID);
            assert_eq!(rows[0].image_reference, "rust:1.97");
            assert_eq!(rows[0].created, "2026-10-30T22:14:03.216453829Z");
            assert!(!rows[0].image.is_empty());
        }
        other => panic!("an exact inventory was refused: {other:?}"),
    }
}
