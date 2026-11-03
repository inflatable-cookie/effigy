use super::*;

/// One fixture serves the whole read-only git helper surface: status and
/// changed files on a dirty tree, branch/HEAD resolution, the safety and ref
/// helpers on a clean tree, and the dirty-tree refusal.
#[test]
fn execute_rhai_script_exposes_git_status_changed_files_and_safety_helpers() {
    let root = temp_root("git-surface");
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
    std::process::Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:inflatable-cookie/effigy.git",
        ])
        .current_dir(&root)
        .output()
        .expect("git remote add");
    fs::write(root.join("tracked.txt"), "one\n").expect("write tracked");
    std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&root)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&root)
        .output()
        .expect("git commit");

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "git".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        if !git::working_tree_clean() { throw("clean"); }
        git::assert_clean();
        if !git::commit_exists("HEAD") { throw("commit exists"); }
        if git::commit_exists("missing-ref") { throw("missing commit exists"); }
        if git::merge_base("HEAD", "HEAD") != git::rev_parse("HEAD") { throw("merge base"); }
        if !git::is_ancestor("HEAD", "HEAD") { throw("is ancestor"); }
        if !str::contains(git::remote_url(), "github.com") { throw("origin url"); }
        if !str::contains(git::remote_url("origin"), "github.com") { throw("named url"); }
        if git::remote_url("missing") != "" { throw("missing remote"); }
        if git::upstream_branch() != "" { throw("upstream"); }
        let branch = git::current_branch();
        if branch == "" { throw("branch"); }
        let head = git::rev_parse("HEAD");
        if str::contains(head, "\n") || str::trim(head) == "" { throw("rev parse"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");

    fs::write(root.join("tracked.txt"), "two\n").expect("modify tracked");
    let dirty_context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "git".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let dirty_script = r#"
        if git::working_tree_clean() { throw("dirty clean"); }
        let status = git::status();
        if status["clean"] { throw("status clean"); }
        let files = git::changed_files();
        if files[0] != "tracked.txt" { throw("changed files"); }
        git::assert_clean();
    "#;

    let error = execute_rhai_script(&dirty_context, dirty_script, &[], &callbacks())
        .expect_err("dirty tree should fail");
    assert!(error.to_string().contains("git working tree is not clean"));
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
