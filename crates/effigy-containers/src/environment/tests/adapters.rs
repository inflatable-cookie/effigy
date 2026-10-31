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
    let selection = BackendSelection {
        kind: if cfg!(target_os = "macos") {
            BackendKind::DockerDesktop
        } else {
            BackendKind::DockerEngine
        },
        engine: crate::BackendId::docker_compose(),
        context: "default".into(),
        reason: BackendReason::UserPreference,
    };
    let rows = exercise(selection, &tools);
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

/// A fake engine whose `inspect` answers by name and whose `image inspect`
/// answers one exact image id, so the create read-back can be exercised on both
/// backend paths without a running engine.
struct CreationTools {
    _dir: effigy_test_support::Fixture,
    inspect: PathBuf,
    image: PathBuf,
    logs: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    _env: std::sync::MutexGuard<'static, ()>,
}

impl CreationTools {
    fn install() -> Self {
        let env = crate::test_env_lock();
        let dir = home();
        let bin = dir.path().join("bin");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let inspect = dir.path().join("created.inspect.json");
        std::fs::write(
            &inspect,
            format!(
                r#"[{{
                  "Id": "{ID}",
                  "Name": "/{project}-app-1",
                  "Image": "sha256:{image}",
                  "State": {{"Running": true}},
                  "Config": {{
                    "Image": "fixture:latest",
                    "Labels": {{
                      "com.docker.compose.project": "{project}",
                      "com.docker.compose.service": "app",
                      "com.docker.compose.oneoff": "False",
                      "com.effigy.managed": "true",
                      "com.effigy.project": "{project}",
                      "com.effigy.scope": "4677bfcb81da18054ef32e71aa3f3c29",
                      "com.effigy.checkout": "/checkout"
                    }}
                  }}
                }}]"#,
                project = PROJECT,
                image = "c".repeat(64),
            ),
        )
        .unwrap();
        let image = dir.path().join("image.inspect.json");
        std::fs::write(
            &image,
            format!(r#"[{{"Id": "sha256:{}"}}]"#, "c".repeat(64)),
        )
        .unwrap();
        for tool in ["colima", "docker"] {
            let script = bin.join(tool);
            std::fs::write(
                &script,
                format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}/{tool}'\ncase \" $* \" in\n  *' image inspect '*) cat '{}' ;;\n  *' inspect '*) cat '{}' ;;\nesac\nexit 0\n",
                    logs.display(),
                    image.display(),
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
            inspect,
            image,
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

impl Drop for CreationTools {
    fn drop(&mut self) {
        match self.previous_path.take() {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        let _ = (&self.inspect, &self.image);
    }
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

/// The created container is read back by the exact name the create report
/// printed, on the selected backend only, and the image id it runs is resolved
/// on that same backend.
#[test]
fn a_created_container_is_read_back_by_name_on_both_backends() {
    for (label, selection, tool) in [
        ("colima", selection(), "colima"),
        ("docker", docker_selection(), "docker"),
    ] {
        let tools = CreationTools::install();
        let workdir = tools.logs.parent().unwrap().to_path_buf();
        let name = format!("{PROJECT}-app-1");
        let mut adapter = selection.adapter();
        let rows = match adapter.containers_named(&workdir, PROJECT, std::slice::from_ref(&name)) {
            Observed::Verified(rows, _) => rows,
            other => panic!("{label}: named read: {other:?}"),
        };
        assert_eq!(rows.len(), 1, "{label}");
        assert_eq!(rows[0].id, ID, "{label}");
        assert_eq!(rows[0].name, name, "{label}");
        assert_eq!(rows[0].image, format!("sha256:{}", "c".repeat(64)));
        assert_eq!(rows[0].image_reference, "fixture:latest");
        let resolved = match adapter.image_id(&workdir, "fixture:latest") {
            Observed::Verified(id, _) => id,
            other => panic!("{label}: image read: {other:?}"),
        };
        assert_eq!(resolved, format!("sha256:{}", "c".repeat(64)));
        let calls = tools.calls(tool);
        assert!(
            calls
                .iter()
                .any(|call| call.contains(&format!("inspect {name}"))),
            "{label}: the read names the created container: {calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|call| call.contains("image inspect fixture:latest")),
            "{label}: the image id is read on the same backend: {calls:?}"
        );
        let other_tool = if tool == "docker" { "colima" } else { "docker" };
        assert!(
            tools.calls(other_tool).is_empty(),
            "{label}: the unselected backend was called"
        );
    }
}

/// An inspect that answers a name the report never printed is not a partial
/// success: the whole read-back refuses, so no id from it can be created.
#[test]
fn a_named_read_refuses_an_answer_for_another_container() {
    let tools = CreationTools::install();
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let mut adapter = selection().adapter();
    match adapter.containers_named(&workdir, PROJECT, &[format!("{PROJECT}-cache-1")]) {
        Observed::Unknown(UnknownReason::Inconsistent(reason)) => assert!(
            reason.contains("did not name") || reason.contains("not `"),
            "{reason}"
        ),
        other => panic!("expected a refusal for a foreign answer, got {other:?}"),
    }
}

/// An image the engine reports only as a mutable reference is not an image id,
/// so it is never accepted as one.
#[test]
fn an_image_read_that_is_only_a_reference_is_refused() {
    let tools = CreationTools::install();
    std::fs::write(&tools.image, br#"[{"Id": "fixture:latest"}]"#).unwrap();
    let workdir = tools.logs.parent().unwrap().to_path_buf();
    let mut adapter = selection().adapter();
    assert!(
        matches!(
            adapter.image_id(&workdir, "fixture:latest"),
            Observed::Unknown(_)
        ),
        "a reference is not an image id"
    );
}
