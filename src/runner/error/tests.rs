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

#[test]
fn command_json_failure_survives_builtin_port_round_trip() {
    let rendered = r#"{"schema":"effigy.task.run.v1","ok":false,"stdout":"diagnostic"}"#;
    let error = RunnerError::CommandJsonFailure {
        rendered: rendered.to_owned(),
    };

    let builtin = crate::runner::builtin_ports::runner_to_builtin(error);
    match &builtin {
        effigy_builtin::BuiltinError::CommandJsonFailure { rendered: carried } => {
            assert_eq!(carried, rendered);
        }
        other => panic!("unexpected builtin error variant: {other:?}"),
    }
    assert_eq!(
        builtin.to_string(),
        "command failed (json output available)"
    );

    let round_tripped = RunnerError::from(builtin);
    match &round_tripped {
        RunnerError::CommandJsonFailure { rendered: carried } => {
            assert_eq!(carried, rendered);
        }
        other => panic!("unexpected runner error variant: {other:?}"),
    }
    assert_eq!(
        round_tripped.to_string(),
        "command failed (json output available)"
    );
    assert_eq!(round_tripped.json_error_details(), Some(rendered));
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
    let error = RunnerError::CommandJsonFailure {
        rendered: details.to_string(),
    };
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
    let error = RunnerError::CommandJsonFailure {
        rendered: details.to_string(),
    };
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

#[test]
fn retained_step_capture_is_complete_private_and_named_in_text_and_json() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("scratch");
    let run = effigy_kernel::id::RunId::new(0, [7; 10]).expect("run id");
    crate::runner::scratch::with_test_scope(root.path(), || {
        let stdout = format!("{}\nFAIL example::broken\nthread 'broken' panicked at lib.rs:1:1:\nassertion failed: useful cause\n{}", "progress\n".repeat(300), "PASS other\n".repeat(300));
        let retained =
            crate::runner::scratch::retain_step_output(&run, &stdout, "token=private", false);
        let path = std::path::Path::new(
            retained
                .strip_prefix("full failed-step output: ")
                .expect("retained path"),
        );
        assert!(path.starts_with(root.path()));
        let capture = std::fs::read_to_string(path).expect("full output exists");
        assert!(capture.contains(&stdout));
        assert!(!capture.contains("private"));
        assert_eq!(
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let stderr = format!("{}\n{retained}", "progress\n".repeat(30));
        for error in [
            RunnerError::TaskCommandFailure { command: "test step".into(), code: Some(100), stdout: stdout.clone(), stderr: stderr.clone() },
            RunnerError::CommandJsonFailure { rendered: serde_json::json!({"task":"test", "stdout":stdout, "stderr":stderr, "exit_code":100}).to_string() },
        ] {
            let human = error.human_error();
            let rendered = human.render_text();
            assert!(rendered.contains(&retained), "{rendered}");
            assert!(rendered.contains("assertion failed: useful cause"), "{rendered}");
            assert!(human.to_json()["evidence"].as_array().expect("evidence").len() <= effigy_ui::human::MAX_EVIDENCE_LINES);
        }
    });
}

/// A relayed step's record is written where a run-scratch teardown cannot
/// remove it, keeps no credential, and still names the panic in the summary
/// the operator reads (effigy#937, QA b0850d50).
#[test]
fn a_relayed_step_record_outlives_the_run_scratch_and_names_the_panic() {
    let scratch = tempfile::tempdir().expect("scratch root");
    let run_scratch = tempfile::tempdir().expect("run scratch");
    let run = effigy_kernel::id::RunId::new(0, [9; 10]).expect("run id");
    let stdout = "PASS fine\nFAIL effigy::cli_output_tests cohort\n".to_owned();
    let stderr = format!(
        "{}
thread 'cohort' panicked at crates/effigy/src/lib.rs:12:5:
assertion `left == right` failed: shared run stayed up
token=supersecret\n",
        "progress\n".repeat(400)
    );

    let retained =
        crate::runner::scratch::with_test_scratch(Some(scratch.path().to_path_buf()), true, || {
            crate::runner::scratch::with_test_scope(run_scratch.path(), || {
                crate::runner::scratch::retain_step_output(&run, &stdout, &stderr, true)
            })
        });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    );
    assert!(
        path.starts_with(
            scratch
                .path()
                .join("effigy/failed-steps")
                .join(run.to_string())
        ),
        "the record lives on the operator's root, not in the run's temp: {}",
        path.display()
    );
    // The harness removes the run's own temporary directory at teardown.
    std::fs::remove_dir_all(run_scratch.path()).expect("run scratch removed");
    let record = std::fs::read_to_string(path).expect("the record survived the teardown");
    assert!(
        record.contains("assertion `left == right` failed"),
        "{record}"
    );
    assert!(!record.contains("supersecret"), "{record}");

    // The failure's own summary carries the panic line and the record's path.
    let human = RunnerError::TaskCommandFailure {
        command: "test -- --no-fail-fast".into(),
        code: Some(100),
        stdout,
        stderr: format!("error: test run failed\n{retained}"),
    }
    .human_error();
    let rendered = human.render_text();
    assert!(
        rendered.contains("thread 'cohort' panicked at crates/effigy/src/lib.rs:12:5:"),
        "{rendered}"
    );
    assert!(
        rendered.contains(path.to_string_lossy().as_ref()),
        "{rendered}"
    );
    assert!(!rendered.contains("supersecret"), "{rendered}");
    assert!(human.evidence_lines().len() <= effigy_ui::human::MAX_EVIDENCE_LINES);
}

/// The record directory is a retained tree like any other: a later sweep
/// retires it a day after the failure and never while it is fresh.
#[test]
fn a_failed_step_record_is_past_its_life_a_day_after_the_failure() {
    let scratch = tempfile::tempdir().expect("scratch root");
    let root = scratch.path().to_path_buf();
    let run = effigy_kernel::id::RunId::new(0, [11; 10]).expect("run id");
    let retained = crate::runner::scratch::with_test_scratch(Some(root.clone()), true, || {
        crate::runner::scratch::retain_step_output(&run, "out", "err", false)
    });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    )
    .to_path_buf();
    let fresh = crate::runner::scratch::sweep_at(&root, now_ms(), std::time::Duration::MAX);
    assert!(path.exists(), "a fresh record is never swept: {fresh:?}");
    assert_eq!(fresh.removed, 0, "{fresh:?}");

    let later = now_ms() + 25 * 60 * 60 * 1000;
    let report = crate::runner::scratch::sweep_at(&root, later, std::time::Duration::MAX);
    assert_eq!(report.removed, 1, "{report:?}");
    assert!(!path.exists(), "the sweep retired the record's directory");
    assert!(
        path.parent()
            .expect("record dir")
            .parent()
            .expect("failed-steps")
            .parent()
            .expect("namespace")
            .exists(),
        "the sweep removes the record's run directory, not the namespace"
    );
}

/// A configured root the operator already named `effigy` shows the namespace
/// segment twice. That layout is the intended one (effigy#946): the `effigy/`
/// child is the namespace the sweep descends into, so a record left directly
/// under such a root would sit beside it and never be retired.
#[test]
fn a_record_under_a_root_named_effigy_stays_in_the_swept_namespace() {
    let owner = tempfile::tempdir().expect("owner");
    let root = owner.path().join("scratch").join("effigy");
    std::fs::create_dir_all(&root).expect("configured root");
    let run = effigy_kernel::id::RunId::new(0, [12; 10]).expect("run id");
    let retained = crate::runner::scratch::with_test_scratch(Some(root.clone()), true, || {
        crate::runner::scratch::retain_step_output(&run, "out", "err", false)
    });
    let path = std::path::Path::new(
        retained
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("full failed-step output: "))
            .expect("retained path"),
    )
    .to_path_buf();
    let expected = root.join("effigy/failed-steps").join(run.to_string());
    assert!(
        path.starts_with(&expected),
        "the record stays under the namespace the sweep reaches: {}",
        path.display()
    );
    let later = now_ms() + 25 * 60 * 60 * 1000;
    let report = crate::runner::scratch::sweep_at(&root, later, std::time::Duration::MAX);
    assert_eq!(report.removed, 1, "{report:?}");
    assert!(!path.exists(), "a later sweep retires the record");
}

/// Now, in milliseconds since the Unix epoch, as the sweep takes it.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[test]
fn text_command_payload_carries_its_next_action_to_the_json_form() {
    let error = RunnerError::CommandJsonFailure {
        rendered: "qa group `ci` needs a planner\nscope: 3 files\nnext: run `effigy qa:plan`\n"
            .to_owned(),
    };
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
                "evidence": ["exit code 3"],
                "next_action": "run `effigy lint:feedback`",
            },
        },
    })
    .to_string();
    let error = RunnerError::CommandJsonFailure { rendered: payload };
    let human = error.human_error();
    assert_eq!(human.what_failed(), "step `lint` failed");
    assert_eq!(human.evidence_lines(), ["exit code 3"]);
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
    let error = RunnerError::CommandJsonFailure {
        rendered: serde_json::json!({ "message": "the lock is held", "state": "blocked" })
            .to_string(),
    };
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
