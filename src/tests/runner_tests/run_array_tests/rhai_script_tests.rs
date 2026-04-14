use super::prelude::{
    assert_file_text_equals, assert_invocation_error_contains, fs, temp_workspace, write_manifest,
};
use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use crate::TaskInvocation;

#[test]
fn run_manifest_task_run_array_supports_inline_rhai_steps() {
    let root = temp_workspace("run-array-inline-rhai-step");
    let marker = root.join("marker.txt");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks.capture]
run = [{{ rhai = 'let payload = json_parse("{{\"kind\":\"inline\"}}"); write_file("{}", payload["kind"] + "|" + args[0]);' }}]
"#,
            marker.display()
        ),
    );

    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "capture".to_owned(),
            args: vec!["hello-world".to_owned()],
        },
        root.clone(),
    )
    .expect("inline rhai task should run");

    assert_file_text_equals(&marker, "inline|hello-world");
}

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
run = [{ rhai_file = "scripts/rhai/validate.rhai" }]
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
    assert_invocation_error_contains(
        error,
        &["define exactly one of `run`, `task`, `rhai`, or `rhai_file`"],
    );
}
