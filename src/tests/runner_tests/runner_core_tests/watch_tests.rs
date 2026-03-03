use super::prelude::*;

#[test]
fn run_manifest_task_builtin_watch_help_renders_topic() {
    let root = temp_workspace("builtin-watch-help");
    write_empty_manifest(&root);

    let out = run_builtin_ok(root, "watch", &["--help"]);
    assert_contains_all(
        &out,
        &[
            "watch Help",
            "--owner <effigy|external>",
            "--debounce-ms <MS>",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_watch_validates_owner_and_arguments() {
    let cases = [
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-required-legacy",
            command: "watch",
            args: &[],
            manifest: "",
            expected: &["--owner <effigy|external>` is required"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-unknown-arg",
            command: "watch",
            args: &["--wat"],
            manifest: "",
            expected: &["unknown argument(s) for built-in `watch`: --wat"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-required",
            command: "watch",
            args: &["build", "--once"],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["--owner <effigy|external>` is required"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-owner-external",
            command: "watch",
            args: &["--owner", "external", "build", "--once"],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["watch owner `external`", "Run the task directly"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-missing-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs"],
            manifest: "",
            expected: &["`--max-runs` requires a numeric value"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-invalid-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs", "nope"],
            manifest: "",
            expected: &["invalid `--max-runs` value `nope`"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-zero-max-runs-value",
            command: "watch",
            args: &["--owner", "effigy", "--max-runs", "0"],
            manifest: "",
            expected: &["`--max-runs` must be greater than zero"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-missing-debounce-value",
            command: "watch",
            args: &["--owner", "effigy", "--debounce-ms"],
            manifest: "",
            expected: &["`--debounce-ms` requires a numeric value"],
        },
        BuiltinErrorCase {
            workspace: "builtin-watch-invalid-debounce-value",
            command: "watch",
            args: &["--owner", "effigy", "--debounce-ms", "nope"],
            manifest: "",
            expected: &["invalid `--debounce-ms` value `nope`"],
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_root_manifest(&root, case.manifest);
        let err = run_builtin_err(root, case.command, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}

#[test]
fn run_manifest_task_builtin_watch_once_executes_target_task() {
    let root = temp_workspace("builtin-watch-once-exec");
    let marker = root.join("watch-once.log");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.build]\nrun = \"printf watched > '{}'\"\n",
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "watch", &["--owner", "effigy", "--once", "build"]);
    assert_contains_all(&out, &["watch complete after 1 run(s)."]);
    assert!(marker.exists(), "watch --once should execute the target");
}

#[test]
fn run_manifest_task_builtin_watch_rejects_concurrent_watch_owner_for_same_target() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-watch-lock-conflict");
    write_build_task_manifest(&root, "sleep 2");

    let root_for_thread = root.clone();
    let join = thread::spawn(move || {
        run_task(
            &root_for_thread,
            "watch",
            &["--owner", "effigy", "--once", "build"],
        )
    });

    let watch_lock = root.join(".effigy/locks/task-watch-build.lock");
    wait_for_path_exists(
        &watch_lock,
        Duration::from_secs(5),
        "watch lock for owner=effigy target=build",
    );

    let err = run_task(&root, "watch", &["--owner", "effigy", "--once", "build"])
        .expect_err("second watch owner should conflict on watch scope lock");
    assert_lock_conflict(err, "task:watch:build", "effigy unlock task:watch:build");

    let first = join.join().expect("thread join");
    first.expect("first watch should complete");
}
