use super::prelude::*;

#[test]
fn builtin_completion_candidates_text_includes_builtin_and_task_selectors() {
    let _guard = test_lock().lock().expect("lock");
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-text-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_completion_task(root, &["candidates"]).expect("run completion candidates");
    assert!(out.lines().any(|line| line == "help"));
    assert!(out.lines().any(|line| line == "build"));
    assert!(out.lines().any(|line| line == "farmyard/api"));
}

#[test]
fn builtin_completion_bash_script_uses_dynamic_candidates_probe() {
    let _guard = test_lock().lock().expect("lock");
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-bash-dynamic-candidates");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_completion_task(root, &["bash"]).expect("run completion bash");
    assert!(out.contains("effigy completion candidates --prefix \"$cur\""));
}

#[test]
fn builtin_completion_candidates_prefix_requires_value() {
    let _guard = test_lock().lock().expect("lock");
    let _env = with_completion_cache_default();
    let root = temp_workspace("completion-candidates-prefix-missing");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_completion_task(root, &["candidates", "--prefix"])
        .expect_err("completion candidates --prefix should fail without value");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("completion candidates argument --prefix requires a value"));
        }
        other => panic!("unexpected error: {other}"),
    }
}
