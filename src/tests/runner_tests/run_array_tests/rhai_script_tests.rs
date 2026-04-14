use super::prelude::{
    assert_file_text_equals, assert_invocation_error_contains, fs, temp_workspace, write_manifest,
};
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::TaskInvocation;

#[test]
fn run_manifest_task_run_array_supports_file_backed_rhai_steps() {
    let root = temp_workspace("run-array-file-rhai-step");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/validate.rhai"),
        r#"
let process = run_process("sh", ["-lc", "printf process-ok"]);
run_task("capture", []);
write_file("process.txt", process["stdout"]);
write_file("nested.txt", read_file("nested-source.txt"));
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.capture]
run = "printf nested-ok > nested-source.txt"

[tasks.validate]
run = [{ rhai = "scripts/rhai/validate.rhai" }]
"#,
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root.clone(),
    )
    .expect("file-backed rhai task should run");

    assert_file_text_equals(&root.join("process.txt"), "process-ok");
    assert_file_text_equals(&root.join("nested.txt"), "nested-ok");
}

#[test]
fn run_manifest_task_run_array_rejects_conflicting_rhai_step_keys() {
    let root = temp_workspace("run-array-invalid-rhai-step");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ run = "printf invalid", rhai = "print(\"nope\")" }]
"#,
    );

    let error = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect_err("conflicting rhai step should fail");
    assert_invocation_error_contains(error, &["define exactly one of `run`, `task`, or `rhai`"]);
}

#[test]
fn run_manifest_task_run_array_rhai_steps_support_args_and_runtime_helpers() {
    let root = temp_workspace("run-array-rhai-runtime-helpers");
    fs::create_dir_all(root.join("scripts/rhai")).expect("mkdir rhai script dir");
    fs::write(
        root.join("scripts/rhai/helpers.rhai"),
        r#"
let stamp = now_utc();
let scratch = make_temp_dir("rhai-runtime-helper");
let pid = process_id().to_string();
write_file("stamp.txt", stamp);
write_file("arg.txt", args[0].to_string());
write_file("scratch.txt", scratch);
write_lines("lines.txt", ["one", "two"]);
append_file("append.txt", "alpha\n");
append_file("append.txt", "beta\n");
write_file("pid.txt", pid);
if !path_exists(scratch) {
    throw "scratch dir was not created";
}
remove_path(scratch);
"#,
    )
    .expect("write rhai script");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.validate]
run = [{ rhai = "scripts/rhai/helpers.rhai" }]
"#,
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".to_owned(),
            args: vec!["helper-ok".to_owned()],
        },
        root.clone(),
    )
    .expect("rhai helper task should run");

    let stamp = fs::read_to_string(root.join("stamp.txt")).expect("read stamp");
    assert!(stamp.ends_with('Z'), "expected UTC timestamp: {stamp}");
    assert_file_text_equals(&root.join("arg.txt"), "helper-ok");
    assert_file_text_equals(&root.join("lines.txt"), "one\ntwo\n");
    assert_file_text_equals(&root.join("append.txt"), "alpha\nbeta\n");
    let pid = fs::read_to_string(root.join("pid.txt")).expect("read pid");
    assert!(
        pid.trim().parse::<u32>().is_ok(),
        "expected numeric pid output: {pid}"
    );
    let scratch = fs::read_to_string(root.join("scratch.txt")).expect("read scratch dir");
    assert!(
        !std::path::Path::new(scratch.trim()).exists(),
        "scratch dir should be removed: {scratch}"
    );
}
