use super::*;

#[test]
fn execute_rhai_script_exposes_git_status_and_changed_files() {
    let root = temp_root("git-status");
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .expect("git init");
    fs::write(root.join("tracked.txt"), "one\n").expect("write tracked");
    std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&root)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .env("GIT_AUTHOR_NAME", "Effigy")
        .env("GIT_AUTHOR_EMAIL", "effigy@example.test")
        .env("GIT_COMMITTER_NAME", "Effigy")
        .env("GIT_COMMITTER_EMAIL", "effigy@example.test")
        .current_dir(&root)
        .output()
        .expect("git commit");
    fs::write(root.join("tracked.txt"), "two\n").expect("modify tracked");

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "git".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        let status = git::status();
        if status["clean"] { throw("status clean"); }
        let files = git::changed_files();
        if files[0] != "tracked.txt" { throw("changed files"); }
        let branch = git::current_branch();
        if branch == "" { throw("branch"); }
        let head = git::rev_parse("HEAD");
        if str::contains(head, "\n") || str::trim(head) == "" { throw("rev parse"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_stage_and_commit_with_git_helpers() {
    let root = temp_root("git-commit");
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.name", "Effigy"])
        .current_dir(&root)
        .output()
        .expect("git config user.name");
    std::process::Command::new("git")
        .args(["config", "user.email", "effigy@example.test"])
        .current_dir(&root)
        .output()
        .expect("git config user.email");

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "git".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        fs::write_file("script.txt", "hello\n");
        let add = git::add(["script.txt"]);
        if !add["success"] { throw(add["stderr"]); }
        let commit = git::commit("script commit");
        if !commit["success"] { throw(commit["stderr"]); }
        if git::rev_parse("HEAD") == "" { throw("missing head"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn rhai_surface_registry_lists_git_module() {
    let surface = crate::surface::rhai_surface_json();
    assert!(surface["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("git")));
    assert!(surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "git" && function["name"] == "commit"));
}
