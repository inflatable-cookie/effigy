use std::path::Path;

use super::RunnerError;
use effigy_containers::exec::ContainerExecError;
use effigy_core::build_info::StaleLocalInstall;
use effigy_gateway::EndpointStoreRefusal;
/// The endpoint store's own answer survives the conversion into a runner
/// error, so a caller classifies it by type and its text stays what a person
/// already read (effigy#933).
#[test]
fn an_endpoint_store_refusal_keeps_its_typed_kind_and_its_sentence() {
    for (kind, expected) in [
        (
            EndpointStoreRefusal::ObservationUnknown,
            "gateway endpoint store: observation unknown: store unreadable",
        ),
        (
            EndpointStoreRefusal::Refused,
            "gateway endpoint store: revision changed",
        ),
    ] {
        let error = RunnerError::from(effigy_gateway::GatewayError::EndpointStore {
            kind,
            reason: match kind {
                EndpointStoreRefusal::ObservationUnknown => "store unreadable".to_owned(),
                EndpointStoreRefusal::Refused => "revision changed".to_owned(),
            },
        });
        assert!(
            matches!(error, RunnerError::GatewayEndpointStore { kind: carried, .. } if carried == kind),
            "the kind travels with the error"
        );
        assert_eq!(error.to_string(), expected);
        assert_eq!(
            error.category(),
            effigy_kernel::human::Category::Refused,
            "a store refusal is still the request being refused"
        );
    }
}

#[test]
fn task_invocation_path_message_constructors_are_stable() {
    let path = Path::new("/tmp/effigy.toml");
    let cases = [
        (
            "read",
            RunnerError::task_invocation_failed_read(path, "read-failed"),
        ),
        (
            "parse",
            RunnerError::task_invocation_failed_parse(path, "parse-failed"),
        ),
        (
            "write",
            RunnerError::task_invocation_failed_write(path, "write-failed"),
        ),
        (
            "render",
            RunnerError::task_invocation_failed_render(path, "render-failed"),
        ),
    ];
    for (verb, error) in cases {
        assert_eq!(
            error.category(),
            effigy_kernel::human::Category::Refused,
            "{verb}"
        );
        let message = error.to_string();
        // The category and the file it names are the contract; the wording of
        // the sentence around them is not.
        assert!(message.contains(verb), "{verb}: {message}");
        assert!(message.contains("/tmp/effigy.toml"), "{verb}: {message}");
        assert!(
            message.contains(&format!("{verb}-failed")),
            "{verb}: {message}"
        );
    }
}

#[test]
fn container_runtime_state_loss_maps_to_task_invocation() {
    let error = RunnerError::from(ContainerExecError::Failure {
        command: "colima stop".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "time=\"2026-04-20T00:17:15+01:00\" level=warning msg=\"error retrieving runtimes: error retrieving current runtime: empty value\"\n[effigy] command timed out after 15s".to_owned(),
    });

    match error {
        RunnerError::TaskInvocation(message) => {
            assert!(
                message.contains("runtime state is corrupted"),
                "got: {message}"
            );
            assert!(message.contains("colima stop"), "got: {message}");
            assert!(
                message.contains("restart or delete the affected Colima profile"),
                "got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn container_runtime_no_space_maps_to_storage_remediation() {
    let error = RunnerError::from(ContainerExecError::Failure {
        command: "docker compose up".to_owned(),
        code: Some(1),
        stdout: "Loaded image: docker.io/library/demo:latest".to_owned(),
        stderr: "failed to write bundle spec: open /run/containerd/io.containerd.runtime.v2.task/default/demo/config.json: no space left on device".to_owned(),
    });

    match error {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("out of space or inodes"), "got: {message}");
            assert!(
                message.contains("effigy container profile resize"),
                "got: {message}"
            );
            assert!(
                message.contains("effigy container cache prune --global --yes"),
                "got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}
#[test]
fn container_surface_selection_variants_render_stably() {
    use effigy_kernel::human::Category;
    let cases = [
        (
            RunnerError::ContainerSurfaceRegistryMissing,
            Category::Refused,
            "[containers]",
            None,
        ),
        (
            RunnerError::ContainerSurfaceDefaultTargetMissing,
            Category::Refused,
            "[systems].default",
            None,
        ),
        (
            RunnerError::container_surface_not_defined("cache"),
            Category::Refused,
            "container",
            Some("`cache`"),
        ),
        (
            RunnerError::container_surface_not_running("web"),
            Category::Refused,
            "container",
            Some("`web`"),
        ),
    ];
    for (error, category, needle, entity) in cases {
        assert_eq!(error.category(), category);
        let text = error.to_string();
        assert!(text.contains(needle), "{text}");
        if let Some(entity) = entity {
            assert!(text.contains(entity), "{text}");
        }
    }
    // A container that is not running names the command that starts it.
    assert!(RunnerError::container_surface_not_running("web")
        .to_string()
        .contains("effigy container web up"));
}

#[test]
fn an_admission_refusal_carries_the_kernel_category() {
    // effigy#360: a supervisor relays an admission failure with the kernel's
    // own category, so a busy store at managed start is unavailable and never
    // a refusal.
    let unavailable = RunnerError::RunAdmission {
        category: effigy_kernel::human::Category::Unavailable,
        message: "`dev` could not be admitted: the state store could not proceed".into(),
    };
    assert_eq!(
        unavailable.category(),
        effigy_kernel::human::Category::Unavailable
    );
    assert_eq!(
        unavailable.to_string(),
        "`dev` could not be admitted: the state store could not proceed"
    );
    assert_eq!(
        RunnerError::task_invocation("a request failure").category(),
        effigy_kernel::human::Category::Refused
    );
}

#[test]
fn every_runner_failure_is_one_of_the_three_kinds() {
    use effigy_kernel::human::Category;
    // A request, its wiring or a holder refuses; a step, a service or a backend
    // that did not answer is unavailable (F16).
    assert_eq!(
        RunnerError::TaskNotFound {
            name: "dev".into(),
            path: Path::new("/repo/effigy.toml").into(),
        }
        .category(),
        Category::Refused
    );
    assert_eq!(
        RunnerError::TaskCommandFailure {
            command: "nextest".into(),
            code: Some(101),
            stdout: String::new(),
            stderr: "a test failed".into(),
        }
        .category(),
        Category::Unavailable
    );
    assert_eq!(
        RunnerError::TaskManifestRead {
            path: Path::new("/repo/effigy.toml").into(),
            error: std::io::Error::other("the disk answered EIO"),
        }
        .category(),
        Category::Unavailable
    );
    assert_eq!(
        RunnerError::ContainerSurfaceNotRunning {
            container: "web".into()
        }
        .category(),
        Category::Refused
    );
    assert_eq!(
        RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
            scope: "task:dev".into(),
            task: "dev".into(),
            holder_pid: None,
            holder_run: None,
            holder_started_at_epoch_ms: None,
            wait_timeout_ms: None,
            waited_ms: None,
            status_command: None,
            details_json: None,
            remediation: String::new(),
        }))
        .category(),
        Category::Refused
    );
}

#[test]
fn an_unknown_task_name_is_the_only_error_that_names_effigy_tasks() {
    // effigy#360: the invalid-request wording belongs to exactly this case, so
    // a held run, a revoked capability or a broken record never reads as one.
    let not_defined = RunnerError::TaskNotFound {
        name: "deploy".into(),
        path: Path::new("/repo/effigy.toml").into(),
    }
    .to_string();
    for needle in ["task `deploy`", "/repo/effigy.toml", "`effigy tasks`"] {
        assert!(not_defined.contains(needle), "{needle}: {not_defined}");
    }
    let any_catalog = RunnerError::TaskNotFoundAny {
        name: "deploy".into(),
        catalogs: vec!["web".into(), "infra".into()],
    }
    .to_string();
    for needle in ["task `deploy`", "web, infra", "`effigy tasks`"] {
        assert!(any_catalog.contains(needle), "{needle}: {any_catalog}");
    }
    assert!(!any_catalog.contains("not a valid"));
}
#[test]
fn gateway_route_variants_render_stably() {
    use effigy_kernel::human::Category;
    let cases = [
        (
            RunnerError::gateway_route_table("load", "/tmp/routes.json", "invalid json"),
            Category::Unavailable,
            "gateway route ",
            "/tmp/routes.json",
        ),
        (
            RunnerError::gateway_route_registration("register", "client.test", "already exists"),
            Category::Unavailable,
            "gateway route ",
            "`client.test`",
        ),
        (
            RunnerError::gateway_route_shape("validation", "bad port mapping"),
            Category::Refused,
            "gateway route validation failed",
            "bad port mapping",
        ),
        (
            RunnerError::gateway_loopback("registry load", "bad registry"),
            Category::Unavailable,
            "gateway loopback ",
            "bad registry",
        ),
        (
            RunnerError::gateway_runtime_target("validation", "unrelated runtime binding"),
            Category::Unavailable,
            "gateway runtime target validation failed",
            "unrelated runtime binding",
        ),
        (
            RunnerError::gateway_runtime_target("runtime rows", "compose backend unavailable"),
            Category::Unavailable,
            "gateway runtime target runtime rows failed",
            "compose backend unavailable",
        ),
    ];
    for (error, category, category_needle, entity) in cases {
        assert_eq!(error.category(), category);
        let text = error.to_string();
        assert!(text.contains(category_needle), "{text}");
        assert!(text.contains(entity), "{text}");
    }
}

#[test]
fn task_manifest_parse_error_without_stale_install_keeps_exact_message() {
    let error: toml::de::Error = toml::from_str::<toml::Value>("unknown =").unwrap_err();
    let expected = format!(
        "failed to parse {}: {error}",
        Path::new("/tmp/effigy.toml").display()
    );
    let err = RunnerError::TaskManifestParse {
        path: Path::new("/tmp/effigy.toml").to_path_buf(),
        error,
        stale_local_install: None,
    };
    assert_eq!(
        err.to_string(),
        expected,
        "no stale hint may change the plain parse error"
    );
}

#[test]
fn task_manifest_parse_error_with_stale_install_names_recovery() {
    let error: toml::de::Error =
        toml::from_str::<toml::Value>("[docs_policy]\nsources = [\n").unwrap_err();
    let stale = StaleLocalInstall {
        executable: Path::new("/repo/.local-install/bin/effigy").to_path_buf(),
        repo_root: Path::new("/repo").to_path_buf(),
        installed_identity: "v0.12.1+local.abc1234".to_owned(),
        current_identity: "v0.12.1+local.def5678".to_owned(),
    };
    let err = RunnerError::TaskManifestParse {
        path: Path::new("/repo/effigy.toml").to_path_buf(),
        error,
        stale_local_install: Some(Box::new(stale)),
    };
    let rendered = err.to_string();
    assert!(
        rendered.starts_with("failed to parse /repo/effigy.toml:"),
        "original parse error must stay first: {rendered}"
    );
    assert!(
        rendered.contains("repository-local install is behind this checkout"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("/repo/.local-install/bin/effigy records v0.12.1+local.abc1234"),
        "installed identity must be named: {rendered}"
    );
    assert!(
        rendered.contains("/repo is at v0.12.1+local.def5678"),
        "current checkout revision must be named: {rendered}"
    );
    assert!(
        rendered.contains("cargo run --bin effigy -- install:local"),
        "source-build recovery must be named: {rendered}"
    );
}

/// A manifest only a newer binary understands leads with the headline, the
/// parse position and the one recovery: refresh the repository-local install.
/// The path and identities sit in `Details:` instead of the headline
/// (effigy#1090).
#[test]
fn a_stale_install_parse_refusal_leads_with_the_headline_and_the_recovery() {
    let error: toml::de::Error = toml::from_str::<toml::Value>("unknown =").unwrap_err();
    let stale = StaleLocalInstall {
        executable: Path::new("/repo/.local-install/bin/effigy").to_path_buf(),
        repo_root: Path::new("/repo").to_path_buf(),
        installed_identity: "v0.12.1+local.abc1234".to_owned(),
        current_identity: "v0.12.1+local.def5678".to_owned(),
    };
    let err = RunnerError::TaskManifestParse {
        path: Path::new("/repo/effigy.toml").to_path_buf(),
        error,
        stale_local_install: Some(Box::new(stale)),
    };

    let human = err.human_error();
    assert_eq!(human.what_failed(), "Invalid effigy.toml");
    assert!(
        human
            .explanation_lines()
            .iter()
            .any(|line| line.contains("TOML parse error at line")),
        "{human:?}"
    );
    assert_eq!(
        human.next_action_text(),
        Some(
            "Run `effigy install:local` (or `cargo run --bin effigy -- install:local`) from the \
             checkout root"
        )
    );
    let labels: Vec<&str> = human
        .detail_entries()
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(labels, ["manifest", "cause", "installed", "current"]);
    assert!(human
        .detail_entries()
        .iter()
        .any(|(_, value)| value == "v0.12.1+local.abc1234"));

    let rendered = human.render_text();
    assert!(
        rendered.contains("\n\n  Next: Run `effigy install:local`"),
        "{rendered}"
    );
    assert!(rendered.contains("\n\n  Details:\n"), "{rendered}");
    assert!(rendered.contains("manifest:"), "{rendered}");
    assert!(rendered.contains("/repo/effigy.toml"), "{rendered}");
    // `Display` keeps its own sentence for `error.message`.
    assert!(err
        .to_string()
        .starts_with("failed to parse /repo/effigy.toml:"));
}

/// A composition refusal states the problem, moves its own remedy to `Next:`
/// and keeps the manifest path in `Details:` (effigy#1090).
#[test]
fn a_compose_refusal_splits_the_statement_from_its_remedy() {
    let err = RunnerError::TaskManifestCompose {
        path: Path::new("/repo/effigy.toml").to_path_buf(),
        detail: "manifest compose failed in /repo/effigy.toml: `tasks.test` was removed in v0.11; \
                 move the command to a named `[test.suites]` entry"
            .to_owned(),
    };
    let human = err.human_error();
    assert_eq!(human.what_failed(), "Invalid effigy.toml");
    assert_eq!(
        human.explanation_lines(),
        ["`tasks.test` was removed in v0.11"]
    );
    assert_eq!(
        human.next_action_text(),
        Some("move the command to a named `[test.suites]` entry")
    );
    assert_eq!(
        human.detail_entries(),
        [("manifest".to_owned(), "/repo/effigy.toml".to_owned())]
    );
    let rendered = human.render_text();
    assert!(
        !rendered.contains("manifest compose failed in"),
        "the internal prefix stays out of the human view: {rendered}"
    );
    assert!(rendered.contains("Details:"), "{rendered}");
}

/// Task routing refusals name the inventory to run and keep the catalog list
/// out of the headline (effigy#1090).
#[test]
fn task_routing_refusals_name_the_inventory_and_the_prefix() {
    let not_found = RunnerError::TaskNotFound {
        name: "deply".to_owned(),
        path: Path::new("/repo/effigy.toml").to_path_buf(),
    };
    let human = not_found.human_error();
    assert_eq!(human.what_failed(), "Task `deply` is not defined");
    assert_eq!(
        human.next_action_text(),
        Some("Run `effigy tasks` to list available tasks")
    );
    assert!(human.render_text().contains("/repo/effigy.toml"));

    let ambiguous = RunnerError::TaskAmbiguous {
        name: "dev".to_owned(),
        candidates: vec!["web".to_owned(), "infra".to_owned()],
    };
    let human = ambiguous.human_error();
    assert_eq!(human.what_failed(), "Task `dev` matches multiple catalogs");
    assert!(
        human
            .next_action_text()
            .is_some_and(|action| action.contains("`<catalog>:dev`")),
        "{human:?}"
    );
    assert_eq!(
        human.detail_entries(),
        [("matches".to_owned(), "web, infra".to_owned())]
    );
}

/// A container that is not running names the command that starts it; with no
/// machine facts it keeps the compact layout instead of an empty `Details:`
/// block (effigy#1090).
#[test]
fn a_container_that_is_not_running_names_the_command_that_starts_it() {
    let human = RunnerError::ContainerSurfaceNotRunning {
        container: "web".to_owned(),
    }
    .human_error();
    assert_eq!(human.what_failed(), "Container `web` is not running");
    assert_eq!(
        human.next_action_text(),
        Some("Start it with `effigy container web up`")
    );
    let rendered = human.render_text();
    assert!(!rendered.contains("Details:"), "{rendered}");
    assert_eq!(
        rendered,
        "[error] Container `web` is not running\n  \
         next: Start it with `effigy container web up`"
    );
}

/// Gateway refusals keep the phase, path and cause below the headline and name
/// the inspection command (effigy#1090).
#[test]
fn gateway_refusals_keep_the_phase_and_the_cause_below_the_headline() {
    let error = RunnerError::gateway_route_table(
        "load",
        "/home/operator/.effigy/gateway/routes.json",
        "invalid json",
    );
    let human = error.human_error();
    assert_eq!(human.what_failed(), "Gateway route table load failed");
    assert_eq!(
        human.detail_entries()[0],
        ("phase".to_owned(), "load".to_owned())
    );
    assert_eq!(
        human.detail_entries()[1],
        ("cause".to_owned(), "invalid json".to_owned())
    );
    let rendered = human.render_text();
    assert!(
        rendered.contains("Next: Run `effigy gateway status`"),
        "{rendered}"
    );
    assert!(rendered.contains("path:"), "{rendered}");
    assert!(
        rendered.contains("/home/operator/.effigy/gateway/routes.json"),
        "{rendered}"
    );

    let human = RunnerError::GatewayEndpointStore {
        kind: EndpointStoreRefusal::ObservationUnknown,
        reason: "store unreadable".to_owned(),
    }
    .human_error();
    assert_eq!(human.what_failed(), "The gateway endpoint store refused");
    let rendered = human.render_text();
    assert!(
        rendered.contains("store: observation unknown"),
        "{rendered}"
    );
    assert!(rendered.contains("cause: store unreadable"), "{rendered}");
}

#[test]
fn command_json_failure_survives_builtin_port_round_trip() {
    let payload =
        serde_json::json!({"schema":"effigy.task.run.v1","ok":false,"stdout":"diagnostic"});
    let rendered = serde_json::to_string_pretty(&payload).unwrap();
    let error = RunnerError::task_json_failure(
        payload,
        "build",
        9,
        effigy_kernel::outcome::WorkloadOutcome::Failed { code: 9 },
        "diagnostic",
        "error: failed",
    );
    assert_eq!(error.process_exit_code(), 9);
    assert!(error.human_error().render_text().contains("error: failed"));
    assert_eq!(error.json_error_details(), Some(rendered.as_str()));

    let builtin = crate::runner::builtin_ports::runner_to_builtin(error);
    match &builtin {
        effigy_builtin::BuiltinError::TaskJsonFailure(failure) => {
            assert_eq!(failure.rendered(), rendered)
        }
        other => panic!("unexpected builtin error variant: {other:?}"),
    }
    assert_eq!(
        builtin.to_string(),
        "command failed (json output available)"
    );

    let round_tripped = RunnerError::from(builtin);
    match &round_tripped {
        RunnerError::TaskJsonFailure(failure) => assert_eq!(failure.rendered(), rendered),
        other => panic!("unexpected runner error variant: {other:?}"),
    }
    assert_eq!(
        round_tripped.to_string(),
        "command failed (json output available)"
    );
    assert_eq!(round_tripped.json_error_details(), Some(rendered.as_str()));
    assert_eq!(round_tripped.process_exit_code(), 9);
    assert_eq!(
        round_tripped.workload_outcome(),
        effigy_kernel::outcome::WorkloadOutcome::Failed { code: 9 }
    );
    assert!(round_tripped
        .human_error()
        .render_text()
        .contains("error: failed"));
}

#[test]
fn command_failure_renders_the_three_human_facts() {
    let error = RunnerError::TaskCommandFailure {
        command: "npm run build".to_owned(),
        code: Some(2),
        stdout: "compiled 12 modules\n".to_owned(),
        stderr: "\n  error TS2304: Cannot find name 'widget'.\n".to_owned(),
    };
    let human = error.human_error();
    assert_eq!(human.what_failed(), "command `npm run build` failed");
    assert_eq!(
        human.evidence_lines(),
        [
            "exit code 2",
            "stderr: error TS2304: Cannot find name 'widget'.",
            "stdout: compiled 12 modules",
        ],
        "both streams are shown, so the cause cannot hide in the unread one"
    );
    assert!(human
        .next_action_text()
        .is_some_and(|action| action.contains("run the command again")));
    assert_eq!(
        human.render_text(),
        "[error] command `npm run build` failed\n  \
         cause: exit code 2\n         \
         stderr: error TS2304: Cannot find name 'widget'.\n         \
         stdout: compiled 12 modules\n  \
         next: read the output above, fix the cause, then run the command again"
    );
    let json = human.to_json();
    assert_eq!(json["what_failed"], "command `npm run build` failed");
    assert_eq!(
        json["evidence"][1],
        "stderr: error TS2304: Cannot find name 'widget'."
    );
}

/// A lock conflict's human rendering keeps the holder and the commands. The
/// details JSON has no `message`/`cause` key, so lifting it by key used to
/// print only ``task `dev` failed`` (effigy#313).
#[test]
fn a_lock_conflict_renders_its_holder_and_the_next_commands() {
    let error = RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
        scope: "task:dev".to_owned(),
        task: "dev".to_owned(),
        holder_pid: Some(99_290),
        holder_run: Some("agqw465rcnc7zpo22ry62fmtnq".to_owned()),
        holder_started_at_epoch_ms: Some(1),
        wait_timeout_ms: Some(150),
        waited_ms: Some(150),
        status_command: Some("effigy run status agqw465rcnc7zpo22ry62fmtnq".to_owned()),
        details_json: Some(
            r#"{"schema":"effigy.lock-wait.v2","status_command":"effigy run status agqw465rcnc7zpo22ry62fmtnq"}"#
                .to_owned(),
        ),
        remediation: "`dev` is already running (pid 99290 on this host, run \
            agqw465rcnc7zpo22ry62fmtnq, started 1h06m ago, attached to a terminal); \
            waited 150ms of 150ms. Inspect it with `effigy run status \
            agqw465rcnc7zpo22ry62fmtnq` (or list every run with `effigy runs`), wait \
            longer with `effigy --lock-wait-ms 600000 dev`, or stop that run with \
            `effigy run stop agqw465rcnc7zpo22ry62fmtnq` (or press Ctrl-C in its terminal)"
                .to_owned(),
    }));

    let human = error.human_error();
    let rendered = human.render_text();
    assert!(
        rendered.contains("`dev` is already running (pid 99290"),
        "{rendered}"
    );
    assert!(rendered.contains("waited 150ms of 150ms"), "{rendered}");
    assert!(
        rendered.contains("`effigy run status agqw465rcnc7zpo22ry62fmtnq`"),
        "{rendered}"
    );
    assert!(rendered.contains("`effigy run stop "), "{rendered}");
    assert!(rendered.contains("`effigy runs`"), "{rendered}");
    assert!(!rendered.contains("tasks status"), "{rendered}");
    // The structured envelope is unchanged: a machine still reads the schema.
    assert!(error
        .json_error_details()
        .is_some_and(|details| details.contains("effigy.lock-wait.v2")));
}

#[test]
fn a_display_sentence_lifts_its_next_step_out_of_the_evidence() {
    let error = RunnerError::TaskInvocation(
        "container `web` is not running\nnext: run `effigy container web up`".to_owned(),
    );
    let human = error.human_error();
    assert_eq!(human.what_failed(), "container `web` is not running");
    assert!(human.evidence_lines().is_empty());
    assert_eq!(
        human.next_action_text(),
        Some("run `effigy container web up`")
    );
}

#[test]
fn structured_details_name_what_failed_and_the_recovery() {
    let details = serde_json::json!({
        "schema": "effigy.container.activation.v1",
        "cause": "container render: object store is absent or unavailable",
        "message": "Detached activation did not complete.",
        "failure_kind": "invocation_refusal",
        "phase": "detached_activation",
        "readiness": "unverified",
        "stop": "effigy container down",
    });
    let error = RunnerError::legacy_command_failure(details.to_string());
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "container render: object store is absent or unavailable"
    );
    assert_eq!(
        human.evidence_lines()[0],
        "Detached activation did not complete."
    );
    assert!(human
        .evidence_lines()
        .contains(&"phase: detached_activation".to_owned()));
    assert_eq!(human.next_action_text(), Some("effigy container down"));
}

#[test]
fn task_run_details_name_the_task_and_the_failing_output() {
    let details = serde_json::json!({
        "schema": "effigy.task.run.v1",
        "task": "fail",
        "exit_code": 3,
        "failed_step": "1",
        "stderr": "error: widget is undefined\n",
        "stdout": "building widget\n",
    });
    let error = RunnerError::legacy_command_failure(details.to_string());
    let human = error.human_error();
    assert_eq!(human.what_failed(), "task `fail` failed");
    assert_eq!(
        human.evidence_lines(),
        [
            "exit_code: 3",
            "stderr: error: widget is undefined",
            "stdout: building widget"
        ]
    );
    let rendered = human.render_text();
    assert!(rendered.starts_with("[error] task `fail` failed"));
    assert!(rendered.contains("exit_code: 3"));
    assert!(rendered.contains("stderr: error: widget is undefined"));
    assert!(rendered.contains("\n  next: read the output above"));
    assert_eq!(
        human.next_action_text(),
        Some(super::TASK_COMMAND_FAILURE_NEXT)
    );
}

/// A heavy run's refusal keeps the hook's own reason and the next command. Its
/// `effigy.admission.v1` details carry `decision` and `reason` but no
/// `cause`/`message`, so lifting them by key printed only ``task `heavy`
/// failed`` and the operator could not tell a deny from a timeout
/// (effigy#328).
#[test]
fn an_admission_refusal_keeps_the_hook_reason_and_the_next_step() {
    let error = RunnerError::AdmissionRefused {
        detail: "heavy task `heavy` was not admitted (run agqxaxitpsgh4hc2mkxm4pwk4q): \
                 the hook denied admission: host is busy. The task did not run; admission \
                 fails closed\nnext: check the admission hook `EFFIGY_ADMISSION_HOOK`, or \
                 unset it to run without host-wide admission"
            .to_owned(),
        details_json: serde_json::json!({
            "schema": "effigy.admission.v1",
            "schema_version": 1,
            "task": "heavy",
            "run_id": "agqxaxitpsgh4hc2mkxm4pwk4q",
            "class": "heavy",
            "decision": "deny",
            "reason": "the hook denied admission: host is busy",
            "next_action": "check the admission hook `EFFIGY_ADMISSION_HOOK`, or unset it \
                            to run without host-wide admission",
        })
        .to_string(),
    };
    let human = error.human_error();
    assert!(
        human
            .what_failed()
            .contains("denied admission: host is busy"),
        "{}",
        human.what_failed()
    );
    assert!(
        human
            .next_action_text()
            .is_some_and(|action| action.contains("EFFIGY_ADMISSION_HOOK")),
        "{human:?}"
    );
    let rendered = human.render_text();
    assert!(
        rendered.contains("denied admission: host is busy"),
        "{rendered}"
    );
    assert!(!rendered.contains("task `heavy` failed"), "{rendered}");
    // The structured envelope still names the schema and the decision.
    let details = error.json_error_details().expect("admission details");
    assert!(details.contains("effigy.admission.v1"), "{details}");
    assert!(details.contains("\"decision\":\"deny\""), "{details}");
}

/// A failed detached backend reaches the reader as its own `Display` sentence
/// plus the activation advice. That sentence carries the backend's bounded
/// output tail, whose last line is the cause, so folding must keep the end of
/// it: the fatal line used to be dropped and only the first lines survived
/// (effigy#364).
#[test]
fn a_long_backend_failure_keeps_the_fatal_line_it_ended_with() {
    let mut stderr = String::from(
        "[effigy] detached backend failed (exit code 1)\n[effigy] backend output tail:\n",
    );
    for index in 0..58 {
        stderr.push_str(&format!("step {index}: building image\n"));
    }
    stderr.push_str("level=fatal msg=\"X\"\n");
    let failure = RunnerError::from(ContainerExecError::Failure {
        command: "docker compose up".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr,
    });
    let activation = RunnerError::task_invocation(format!(
        "{failure}\ndetached activation unavailable: readiness and exposure are unverified."
    ));
    let human = activation.human_error();
    assert!(
        human.render_text().contains("level=fatal msg=\"X\""),
        "{}",
        human.render_text()
    );
    assert!(human.evidence_lines().len() <= effigy_ui::human::MAX_EVIDENCE_LINES);
}

/// A Compose launch refusal is one long line: the command it would have run,
/// then why it refused. The human view keeps both ends, so the reason survives
/// the bound instead of only reaching `--json` (effigy#1003).
#[test]
fn a_long_launch_refusal_keeps_its_cause_in_the_human_output() {
    let objects = format!("/Users/operator/.effigy/state/objects/{}", "a".repeat(420));
    let command = format!(
        "colima nerdctl --profile effigy -- compose -f {objects} up -d --build --remove-orphans"
    );
    let failure = RunnerError::from(ContainerExecError::Launch {
        command,
        error: std::io::Error::other("VM Compose interpolation requires retained kernel custody"),
    });
    let human = failure.human_error();
    let rendered = human.render_text();
    // The evidence budget wraps one long line at word boundaries rather than
    // dropping its tail (effigy#1003), so the reason is present in the text but
    // may straddle a wrap. Collapsing whitespace is what "the reason survives"
    // means here; the head check below proves the wrap kept both ends.
    let collapsed = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        collapsed.contains("VM Compose interpolation requires retained kernel custody"),
        "{rendered}"
    );
    assert!(
        collapsed.contains("colima nerdctl --profile effigy"),
        "the head still names the command: {rendered}"
    );
}

#[test]
fn text_command_payload_carries_its_next_action_to_the_json_form() {
    let error = RunnerError::legacy_command_failure(
        "qa group `ci` needs a planner\nscope: 3 files\nnext: run `effigy qa:plan`\n".to_owned(),
    );
    let human = error.human_error();
    assert_eq!(human.what_failed(), "qa group `ci` needs a planner");
    assert_eq!(human.next_action_text(), Some("run `effigy qa:plan`"));
    assert_eq!(
        human.to_json()["next_action"],
        "run `effigy qa:plan`",
        "the JSON form must name the action the text form names"
    );
    assert!(human.render_text().contains("next: run `effigy qa:plan`"));
}

#[test]
fn a_nested_envelope_failure_carries_its_next_action_to_the_json_form() {
    // A command that reports its own failure in the standard `--json` envelope
    // nests the human block under `error.human`. The outer form used to reduce
    // that payload to the generic display sentence, so `next_action` was null
    // while the payload named the step.
    let payload = serde_json::json!({
        "ok": false,
        "error": {
            "kind": "unavailable",
            "type": "RunnerError",
            "message": "command `effigy test:focused` failed",
            "details": { "exit_code": 3 },
            "human": {
                "what_failed": "step `lint` failed",
                "evidence": ["exit code 3", "token=secret"],
                "next_action": "run `effigy lint:feedback`",
            },
        },
    })
    .to_string();
    let error = RunnerError::legacy_command_failure(payload);
    let human = error.human_error();
    assert_eq!(human.what_failed(), "step `lint` failed");
    assert_eq!(human.evidence_lines()[0], "exit code 3");
    assert!(!human.render_text().contains("secret"));
    assert_eq!(
        human.to_json()["next_action"],
        "run `effigy lint:feedback`",
        "the outer JSON form carries the action the payload names"
    );
    assert!(human
        .render_text()
        .contains("next: run `effigy lint:feedback`"));
}

#[test]
fn command_payload_without_an_action_keeps_a_valid_null() {
    let error = RunnerError::legacy_command_failure(
        serde_json::json!({ "schema": "effigy.run.v1", "message": "the lock is held", "state": "blocked" }).to_string(),
    );
    let json = error.human_error().to_json();
    assert_eq!(json["what_failed"], "the lock is held");
    assert!(json["next_action"].is_null());
}

/// A missing capability keeps its own class and sentence. It used to arrive as
/// a command failure against a command nobody ran, which told a person to read
/// the output of `container status` instead of fixing the engine.
#[test]
fn a_missing_capability_is_reported_as_the_environment_it_lacks() {
    let error = RunnerError::from(effigy_runtime::EffigyRuntimeError::unavailable(
        effigy_runtime::Capability::ContainerRuntime,
        "engine not found",
    ));
    assert!(
        matches!(error, RunnerError::EnvironmentUnavailable { .. }),
        "{error:?}"
    );
    assert_eq!(
        error.category(),
        effigy_kernel::human::Category::Unavailable
    );
    let rendered = error.to_string();
    assert_eq!(
        rendered, "the container runtime is unavailable: engine not found",
        "the sentence names the environment, not a fabricated command"
    );
    assert!(!rendered.contains("container status"));
}

/// The tasks crate's failure classes keep their runner class: a refused
/// selector request is a refused invocation, while a renderer refusal is a UI
/// failure and never masquerades as bad user input. Before the typed
/// conversion both arrived as `TaskInvocation` strings.
#[test]
fn tasks_crate_failures_keep_their_class_at_the_runner_edge() {
    let refused = RunnerError::from(effigy_tasks::EffigyTasksError::message(
        "qa-group selector is empty",
    ));
    assert!(
        matches!(&refused, RunnerError::TaskInvocation(message) if message == "qa-group selector is empty"),
        "a refused request stays an invocation refusal: {refused:?}"
    );
    assert_eq!(refused.to_string(), "qa-group selector is empty");

    let render = RunnerError::from(effigy_tasks::EffigyTasksError::Render(
        effigy_ui::UiError::Encoding("failed to encode json: refused".to_owned()),
    ));
    assert!(
        matches!(&render, RunnerError::Ui(message) if message == "failed to encode json: refused"),
        "a renderer refusal stays a UI failure: {render:?}"
    );
    assert!(
        !matches!(render, RunnerError::TaskInvocation(_)),
        "a renderer refusal must not be reported as bad user input"
    );
}

/// effigy#1014: a guest adapter failure is classified by the call and by what
/// the host can actually observe. A collapsed guest exit is reported as the
/// status the host saw, never as a semantic cause; a host file operation keeps
/// its own `ErrorKind`. Neither sentence carries the adapter's message.
#[test]
fn workspace_guest_failures_are_classified_by_what_the_host_observed() {
    use super::WorkspaceGuestFailure;
    use std::os::unix::process::ExitStatusExt;

    // A collapsed guest exit is only the status: Colima/nerdctl reports every
    // non-zero guest exit as host exit 1, so nothing more may be claimed.
    let collapsed = WorkspaceGuestFailure::from_status(std::process::ExitStatus::from_raw(1 << 8));
    assert_eq!(collapsed, WorkspaceGuestFailure::Exit(Some(1)));
    assert_eq!(collapsed.sentence(), "the guest command failed (exit 1)");

    // Even the exit conventions that would look semantic are only a status.
    for raw in [126 << 8, 127 << 8, 130 << 8, libc::SIGTERM] {
        let status = std::process::ExitStatus::from_raw(raw);
        let class = WorkspaceGuestFailure::from_status(status);
        assert_eq!(class, WorkspaceGuestFailure::Exit(status.code()));
        assert!(
            class.sentence().starts_with("the guest command failed"),
            "a collapsed status is never interpreted: {}",
            class.sentence()
        );
    }

    // A host file operation keeps its own kind, and each kind has one fixed
    // sentence.
    for (kind, sentence) in [
        (std::io::ErrorKind::NotFound, "the host has no such path"),
        (
            std::io::ErrorKind::PermissionDenied,
            "the host refused permission",
        ),
        (
            std::io::ErrorKind::AlreadyExists,
            "the host already holds another object at that path",
        ),
        (
            std::io::ErrorKind::ReadOnlyFilesystem,
            "the host filesystem refused the write",
        ),
        (
            std::io::ErrorKind::TimedOut,
            "the host file operation ran out of time",
        ),
        (
            std::io::ErrorKind::Interrupted,
            "the host file operation was interrupted",
        ),
        (
            std::io::ErrorKind::WouldBlock,
            "the host file operation failed",
        ),
    ] {
        let class = WorkspaceGuestFailure::from_io(kind);
        assert_eq!(class, WorkspaceGuestFailure::HostIo(kind));
        assert_eq!(class.sentence(), sentence, "{kind:?}");
    }

    // Every class sentence is fixed vocabulary, never an adapter message.
    for class in [
        WorkspaceGuestFailure::HostIo(std::io::ErrorKind::PermissionDenied),
        WorkspaceGuestFailure::TimedOut,
        WorkspaceGuestFailure::Exit(Some(1)),
        WorkspaceGuestFailure::Exit(None),
        WorkspaceGuestFailure::Unavailable,
        WorkspaceGuestFailure::NoDigestTool,
    ] {
        assert!(!class.sentence().contains("adversarial"), "{class:?}");
    }
    assert_eq!(
        WorkspaceGuestFailure::Exit(None).sentence(),
        "the guest command failed (status unavailable)"
    );
    assert_eq!(
        WorkspaceGuestFailure::TimedOut.sentence(),
        "the guest command ran out of time"
    );
    assert_eq!(
        WorkspaceGuestFailure::Unavailable.sentence(),
        "the guest command could not be started or read"
    );
    assert_eq!(
        WorkspaceGuestFailure::NoDigestTool.sentence(),
        "the guest image has no SHA-256 utility"
    );

    // Display keeps the adapter's own sentence for a person in the container;
    // the host diagnostic reads the class.
    let error = RunnerError::WorkspaceGuest {
        operation: "publishing a guest file",
        kind: WorkspaceGuestFailure::Exit(Some(1)),
        message: "cannot publish `x`: adversarial-secret-backend-diagnostic".into(),
    };
    assert!(error
        .to_string()
        .contains("adversarial-secret-backend-diagnostic"));
    assert!(matches!(
        error.category(),
        effigy_kernel::human::Category::Unavailable
    ));
}

#[test]
fn readiness_failure_human_rendering_avoids_duplicate_cause_empty_streams_and_doubled_chain() {
    let inner_probe = ContainerExecError::Failure {
        command: "Colima containerd readiness probe".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "[effigy] command timed out after 375ms: owned capture child terminated and reaped; stdout/stderr EOF verified".to_owned(),
    };
    let inner_rendered = inner_probe.to_string();
    assert!(
        !inner_rendered.contains("stdout:"),
        "empty stdout must be omitted: {inner_rendered}"
    );

    let readiness = RunnerError::from(ContainerExecError::Failure {
        command: "Colima runtime readiness".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!(
            "Colima containerd and BuildKit did not become ready within 90s; last runtime probe: {inner_rendered}"
        ),
    });
    let wrapped = RunnerError::wrap_detached_activation(Some("web".to_owned()), readiness);
    let rendered = wrapped.human_error().render_text();
    assert!(
        !rendered.contains("detached activation unavailable"),
        "the wrapper states its own next step, not a stacked paragraph: {rendered}"
    );

    assert!(
        !rendered.contains("cause: cause:"),
        "must not duplicate cause prefix: {rendered}"
    );
    assert!(
        !rendered.contains("[error] [error]"),
        "must not duplicate [error] prefix: {rendered}"
    );
    assert!(
        !rendered.contains("stdout:"),
        "must not include empty stdout header: {rendered}"
    );
    assert!(
        !rendered.contains("after 0s"),
        "must not truncate sub-second timeout to 0s: {rendered}"
    );
    assert_eq!(
        rendered
            .matches("Colima containerd readiness probe failed")
            .count(),
        1,
        "must not double-print the readiness probe error chain: {rendered}"
    );
    assert!(
        rendered.contains("within 90s") && rendered.contains("after 375ms"),
        "keeps readiness window and sub-second probe detail: {rendered}"
    );
}

#[test]
fn container_wait_refused_renders_short_headline_next_and_details() {
    let error = RunnerError::container_wait_refused(
        "web",
        "environment `web` did not become ready within 60s; last container output: nginx failed"
            .to_owned(),
        serde_json::json!({
            "failure_kind": "container_readiness",
            "waiting_on": [{
                "kind": "service",
                "service": "app",
                "problem": "app is unhealthy after 4 failed checks",
            }],
        }),
    );

    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "Container `web` did not become ready in time"
    );
    assert_eq!(
        human.explanation_lines(),
        [
            "environment `web` did not become ready within 60s",
            "Waiting on: app is unhealthy after 4 failed checks",
        ]
    );
    assert_eq!(
        human.next_action_text(),
        Some("Run `effigy container web logs` to inspect startup output")
    );
    assert_eq!(
        human.detail_entries(),
        [
            ("container".to_owned(), "web".to_owned()),
            ("failure_kind".to_owned(), "container_readiness".to_owned()),
            (
                "cause".to_owned(),
                "environment `web` did not become ready within 60s; last container output: nginx failed".to_owned(),
            ),
        ]
    );
    let rendered = human.render_text();
    assert!(rendered.starts_with("[error] Container `web` did not become ready in time\n\n"));
    assert!(
        rendered.contains("\n\n  Next: Run `effigy container web logs` to inspect startup output")
    );
    assert!(rendered.contains("\n\n  Details:\n"));
}

#[test]
fn managed_host_readiness_renders_with_and_without_observed_status() {
    let with_status = RunnerError::ManagedHostReadiness {
        process: "vite".to_owned(),
        diagnostic: "managed listener answered /health with HTTP 502".to_owned(),
        observed_status: Some(502),
    };
    let human = with_status.human_error();
    assert_eq!(
        human.what_failed(),
        "Host process `vite` did not become ready"
    );
    assert_eq!(
        human.explanation_lines(),
        ["The readiness check received HTTP 502 before the process stopped."]
    );
    assert_eq!(
        human.next_action_text(),
        Some("Check `vite` output or run it directly to see why it failed")
    );
    assert_eq!(
        human.detail_entries(),
        [
            ("process".to_owned(), "vite".to_owned()),
            ("http_status".to_owned(), "502".to_owned()),
            (
                "cause".to_owned(),
                "managed listener answered /health with HTTP 502".to_owned()
            ),
        ]
    );

    let without_status = RunnerError::ManagedHostReadiness {
        process: "worker".to_owned(),
        diagnostic: "child exited with status 1".to_owned(),
        observed_status: None,
    };
    let human = without_status.human_error();
    assert_eq!(
        human.explanation_lines(),
        ["The process failed before its readiness check passed."]
    );
    assert_eq!(
        human.detail_entries(),
        [
            ("process".to_owned(), "worker".to_owned()),
            ("cause".to_owned(), "child exited with status 1".to_owned()),
        ]
    );
}

#[test]
fn managed_listener_exposure_renders_partial_routes_and_withheld_details() {
    let error = RunnerError::ManagedListenerExposure {
        environment: "web".to_owned(),
        exposed: vec!["web.test -> 127.0.0.1:8080".to_owned()],
        failures: vec![crate::runner::host_process::ManagedHostListenerFailure {
            name: "front".to_owned(),
            status: crate::runner::host_process::HostLifecycleStatus::Failed,
            declared_route_domain: "front.test".to_owned(),
            route_domain: Some("front.test".to_owned()),
            observed_status: Some(500),
            detail: "readiness check failed".to_owned(),
            output_tail: Some("error: port in use".to_owned()),
            log: "/repo/.effigy/runtime/front.log".to_owned(),
        }],
    };
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "Some routes for `web` could not be published"
    );
    assert_eq!(
        human.explanation_lines(),
        ["1 route(s) were withheld while 1 published."]
    );
    assert_eq!(
        human.next_action_text(),
        Some("Fix the failing host listener(s) listed below and re-run `effigy dev`")
    );
    assert_eq!(
        human.detail_entries(),
        [
            ("environment".to_owned(), "web".to_owned()),
            ("published".to_owned(), "web.test -> 127.0.0.1:8080".to_owned()),
            (
                "withheld".to_owned(),
                "host listener `front` kept route front.test withheld: readiness check failed (observed HTTP 500)".to_owned(),
            ),
            ("output".to_owned(), "error: port in use".to_owned()),
        ]
    );
}

#[test]
fn workspace_session_cleanup_renders_headline_next_and_cleanup_detail() {
    let error = RunnerError::workspace_session_cleanup(
        "shell exited with status 1",
        "container stop timed out",
    );
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "Workspace session ended with a cleanup error"
    );
    assert_eq!(human.explanation_lines(), ["shell exited with status 1"]);
    assert_eq!(
        human.next_action_text(),
        Some("Run `effigy container status` to check whether the workspace container is still running")
    );
    assert_eq!(
        human.detail_entries(),
        [("cleanup".to_owned(), "container stop timed out".to_owned())]
    );
}

#[test]
fn structured_manifest_compose_promotes_headline_preserves_details_and_dedups_manifest() {
    let err = RunnerError::from(effigy_manifest::ManifestError::Compose {
        path: Path::new("/var/www/brains/effigy.toml").to_path_buf(),
        detail: "This project's bundle needs to be synced on the host\n\n  The workspace container cannot fetch remote bundle sources.\n\n  Next: Exit the container and run `effigy bundle sync` on the host\n\n  Details:\n    manifest: /var/www/brains/effigy.toml\n    source:   git"
            .to_owned(),
    });
    assert!(matches!(err, RunnerError::TaskManifestCompose { .. }));
    let human = err.human_error();
    assert_eq!(
        human.what_failed(),
        "This project's bundle needs to be synced on the host"
    );
    assert_eq!(
        human.explanation_lines(),
        ["The workspace container cannot fetch remote bundle sources."]
    );
    assert_eq!(
        human.next_action_text(),
        Some("Exit the container and run `effigy bundle sync` on the host")
    );
    assert_eq!(
        human.detail_entries(),
        [
            (
                "manifest".to_owned(),
                "/var/www/brains/effigy.toml".to_owned()
            ),
            ("source".to_owned(), "git".to_owned()),
        ]
    );
    let rendered = human.render_text();
    assert_eq!(rendered.matches("manifest:").count(), 1, "{rendered}");

    // Without an existing manifest row in Details:, manifest_compose adds it once.
    let without_manifest = RunnerError::TaskManifestCompose {
        path: Path::new("/repo/effigy.toml").to_path_buf(),
        detail: "This project's bundle needs to be synced on the host\n\n  Next: Run `effigy bundle sync` on the host"
            .to_owned(),
    };
    let human = without_manifest.human_error();
    assert_eq!(
        human.detail_entries(),
        [("manifest".to_owned(), "/repo/effigy.toml".to_owned())]
    );
}

#[test]
fn json_command_envelope_remains_backward_compatible_with_structured_human_errors() {
    let error = RunnerError::container_wait_refused(
        "web",
        "environment `web` did not become ready within 60s".to_owned(),
        serde_json::json!({
            "failure_kind": "container_readiness",
            "waiting_on": [{ "kind": "service", "service": "app", "problem": "app is starting" }],
        }),
    );
    let details = error
        .json_error_details()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    let envelope = crate::cli::output::build_json_envelope_error_with_human(
        "container",
        "up",
        "RunnerError",
        "refused",
        &error.to_string(),
        details,
        error.human_error().to_json(),
    );

    assert_eq!(envelope["schema"], "effigy.command.v1");
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["kind"], "refused");
    assert_eq!(envelope["error"]["type"], "RunnerError");
    assert_eq!(
        envelope["error"]["message"],
        "environment `web` did not become ready within 60s"
    );
    assert_eq!(
        envelope["error"]["details"]["failure_kind"],
        "container_readiness"
    );
    assert_eq!(
        envelope["error"]["human"]["what_failed"],
        "Container `web` did not become ready in time"
    );
    assert_eq!(
        envelope["error"]["human"]["next_action"],
        "Run `effigy container web logs` to inspect startup output"
    );
    assert_eq!(
        envelope["error"]["human"]["details"][0],
        serde_json::json!({"label": "container", "value": "web"})
    );
}

#[test]
fn engine_absence_container_failures_render_plain_headline_and_next() {
    let docker = RunnerError::from(ContainerExecError::Failure {
        command: "docker ps".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr: "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
    });
    let human = docker.human_error();
    assert_eq!(human.what_failed(), "Docker daemon is not running");
    assert_eq!(
        human.next_action_text(),
        Some("start Docker Desktop or the Docker daemon, then retry")
    );

    let colima = RunnerError::from(ContainerExecError::Failure {
        command: "colima status".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr: "level=fatal msg=\"colima [profile=effigy] is not running\"".to_owned(),
    });
    let human = colima.human_error();
    assert_eq!(
        human.what_failed(),
        "Colima profile `effigy` is not running"
    );
    assert_eq!(
        human.next_action_text(),
        Some("start it with `colima start --profile effigy` or run `effigy container up`")
    );
}

/// A detached activation that timed out in a runtime command renders one
/// headline, one explanation, one `Next:` and a `Details:` block. The timeout
/// is named plainly, and no stacked paragraph or generic next step survives
/// (effigy#1105).
#[test]
fn detached_container_timeout_renders_one_clean_error() {
    let timeout = RunnerError::TaskCommandFailure {
        command: "runtime ps".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "[effigy] command timed out".to_owned(),
    };
    let error = RunnerError::wrap_detached_activation(Some("web".to_owned()), timeout);
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "Container `web` did not finish starting"
    );
    assert_eq!(
        human.next_action_text(),
        Some(
            "Run `effigy container up --detach` to finish setup, or `effigy container down` to stop it"
        )
    );
    let rendered = human.render_text();
    assert!(
        rendered.contains("command `runtime ps` timed out"),
        "{rendered}"
    );
    assert!(rendered.contains("Details:"), "{rendered}");
    assert_eq!(
        rendered.matches("Compose left the stack").count(),
        1,
        "{rendered}"
    );
    assert_eq!(rendered.matches("Next:").count(), 1, "{rendered}");
    for stale in [
        "a signal stopped it",
        "[effigy] command timed out",
        "stays as Compose left it",
        "detached activation unavailable",
        "read the output above",
        "cause: cause:",
    ] {
        assert!(!rendered.contains(stale), "`{stale}` leaked: {rendered}");
    }
}

/// Dependency drift keeps its own headline and remediation through the
/// detached activation wrapper instead of inheriting Compose's generic advice.
#[test]
fn detached_activation_host_dependency_drift_renders_its_remediation_directly() {
    let error = RunnerError::wrap_detached_activation(
        Some("web".to_owned()),
        RunnerError::HostDependencyDrift {
            detail: "JavaScript dependencies for `apps/web` disagree with `bun.lock`: `praxis` is installed at 0.6.0 but the lock resolves 0.7.1. Effigy refused to launch this environment's managed host processes because a stale install can fail after launch.".to_owned(),
            remediation: "Run `bun install --frozen-lockfile` or `effigy bootstrap`, then rerun the container start.".to_owned(),
        },
    );
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "Host JavaScript dependencies have drifted"
    );
    assert_eq!(
        human.next_action_text(),
        Some("Run `bun install --frozen-lockfile` or `effigy bootstrap`, then rerun the container start.")
    );
    let rendered = human.render_text();
    assert!(rendered.contains("`praxis`"), "{rendered}");
    assert!(rendered.contains("`bun.lock`"), "{rendered}");
    assert!(
        !rendered.contains("Backing containers were started and left running."),
        "a wrapper without lifecycle-stage context must not claim a stack was started: {rendered}"
    );
    assert!(!rendered.contains("Container `web` did not finish starting"), "{rendered}");
    assert!(!rendered.contains("Compose left the stack"), "{rendered}");
    assert!(
        !rendered.contains("Run `effigy container up --detach` to finish setup"),
        "{rendered}"
    );
}

/// A timeout the command produced output before is still a timeout. The
/// output stays as bounded evidence, and only the marker line goes.
#[test]
fn timed_out_command_keeps_its_output_and_drops_the_marker() {
    let human = RunnerError::TaskCommandFailure {
        command: "docker compose up".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "pulling web\n[effigy] command timed out after 15s: owned capture child terminated and reaped; stdout/stderr EOF verified".to_owned(),
    }
    .human_error();
    assert_eq!(human.what_failed(), "command `docker compose up` timed out");
    let evidence = human.evidence_lines().join("\n");
    assert!(evidence.contains("pulling web"), "{evidence}");
    assert!(
        !evidence.contains("[effigy] command timed out"),
        "{evidence}"
    );
    assert!(!evidence.contains("reaped"), "{evidence}");
}

/// A timeout that names a retained output file keeps the path, because a
/// person can open that file.
#[test]
fn timed_out_command_keeps_the_retained_output_path() {
    let human = RunnerError::TaskCommandFailure {
        command: "docker ps".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "[effigy] command timed out; output retained at /tmp/effigy/run.log".to_owned(),
    }
    .human_error();
    let evidence = human.evidence_lines().join("\n");
    assert!(
        evidence.contains("output retained at /tmp/effigy/run.log"),
        "{evidence}"
    );
}

/// A command that died on a signal, without Effigy's deadline, keeps the
/// signal sentence. Only the timeout marker changes the wording.
#[test]
fn signal_end_without_a_deadline_keeps_the_signal_sentence() {
    let human = RunnerError::TaskCommandFailure {
        command: "docker ps".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "killed by the host".to_owned(),
    }
    .human_error();
    assert_eq!(human.what_failed(), "command `docker ps` failed");
    assert!(
        human
            .evidence_lines()
            .contains(&"a signal stopped it before it reported an exit code".to_owned()),
        "{:?}",
        human.evidence_lines()
    );
}

/// A detached failure with no typed cause still renders one headline and one
/// `Next:`, and an unnamed container is named as such.
#[test]
fn detached_activation_failure_without_a_name_renders_one_paragraph() {
    let inner = RunnerError::task_invocation(
        "gateway domain `api.app.test` is claimed by a container route from checkout `/gone/worktree`",
    );
    let error = RunnerError::wrap_detached_activation(None, inner);
    let human = error.human_error();
    assert_eq!(human.what_failed(), "Container did not finish starting");
    let rendered = human.render_text();
    assert_eq!(rendered.matches("Next:").count(), 1, "{rendered}");
    assert!(rendered.contains("/gone/worktree"), "{rendered}");
    assert!(!rendered.contains("stays as Compose"), "{rendered}");
}

/// Wrapping an already-wrapped failure keeps the first wrapper, so the
/// activation never states the same paragraph twice.
#[test]
fn detached_activation_wrapping_is_idempotent() {
    let once = RunnerError::wrap_detached_activation(
        Some("web".to_owned()),
        RunnerError::task_invocation("boom"),
    );
    let twice = RunnerError::wrap_detached_activation(Some("api".to_owned()), once);
    let RunnerError::DetachedActivationFailure { container, cause } = &twice else {
        panic!("a wrapped failure stays wrapped: {twice:?}");
    };
    assert_eq!(container.as_deref(), Some("web"));
    assert!(
        !matches!(
            cause.as_ref(),
            RunnerError::DetachedActivationFailure { .. }
        ),
        "the wrapper never nests: {twice:?}"
    );
    assert_eq!(twice.to_string(), "boom");
}

/// A long backend output keeps its last line, the one that says why it
/// stopped, through the detached wrapper (effigy#364).
#[test]
fn detached_activation_keeps_the_fatal_tail_of_long_backend_output() {
    let mut stderr = String::new();
    for index in 0..200 {
        stderr.push_str(&format!("progress step {index}\n"));
    }
    stderr.push_str("level=fatal msg=\"runtime socket closed before the container started\"");
    let error = RunnerError::wrap_detached_activation(
        Some("web".to_owned()),
        RunnerError::TaskCommandFailure {
            command: "docker compose up".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr,
        },
    );
    let rendered = error.human_error().render_text();
    assert!(
        rendered.contains("runtime socket closed before the container started"),
        "{rendered}"
    );
    assert!(!rendered.contains("read the output above"), "{rendered}");
}

#[test]
fn predecessor_unsettled_names_acknowledge_and_detached_failure_promotes_it() {
    let run_id = "01JABCDEF1234567890ABCDEF";
    let error = RunnerError::PredecessorUnsettled {
        run: run_id.to_owned(),
        effects: 2,
        context: Some("container environment"),
    };
    let display = error.to_string();
    assert!(
        display.contains(&format!("effigy run acknowledge {run_id}")),
        "{display}"
    );
    assert!(!display.contains("stop what holds it"), "{display}");
    let human = error.human_error();
    assert!(
        human
            .next_action_text()
            .is_some_and(|next| next.contains(&format!("effigy run acknowledge {run_id}"))),
        "{:?}",
        human.next_action_text()
    );

    let wrapped = RunnerError::wrap_detached_activation(
        Some("web".to_owned()),
        RunnerError::from(
            effigy_containers::ContainerPolicyError::PredecessorUnsettled {
                run: run_id.to_owned(),
                effects: 1,
            },
        ),
    );
    let rendered = wrapped.human_error().render_text();
    assert!(
        rendered.contains(&format!("effigy run acknowledge {run_id}")),
        "{rendered}"
    );
    assert!(!rendered.contains("stop what holds it"), "{rendered}");
    assert!(
        !rendered.contains("Run effigy container up --detach"),
        "{rendered}"
    );
}
