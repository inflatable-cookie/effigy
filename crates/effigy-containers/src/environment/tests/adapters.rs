//! Strict-fake proofs for both backend adapters (§12.2): fake `colima` and
//! `docker` executables record every invocation. The selected adapter issues
//! exactly its own inventory calls, and the unselected backend sees none. The
//! apply and the stop are not adapter work: the caller passes them to the
//! kernel as one effect.

use super::*;
use std::os::unix::fs::PermissionsExt;

const ID: &str = "87c434af41df6f447bfb13218637f97b211098ffe97573a00c766c56f5008cdd";

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
        let env = crate::test_env_lock();
        let dir = home();
        let bin = dir.path().join("bin");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let inspect = dir.path().join("inspect.json");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/environment/underlay-workspace.inspect.json"
            ),
            &inspect,
        )
        .unwrap();
        for tool in ["colima", "docker"] {
            let script = bin.join(tool);
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}/{tool}'\ncase \" $* \" in\n  *' ps '*) echo {ID} ;;\n  *' inspect '*) cat '{}' ;;\nesac\nexit 0\n",
                    logs.display(),
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
