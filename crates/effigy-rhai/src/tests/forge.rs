use super::*;

#[test]
fn execute_rhai_script_detects_github_forge_from_origin_remote() {
    let root = temp_root("forge-provider");
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .expect("git init");
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

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "forge".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        if forge::provider() != "github" { throw("provider"); }
        if forge::provider(#{ provider: "github" }) != "github" { throw("explicit provider"); }
        let status = forge::status();
        if status["provider"] != "github" { throw("status provider"); }
        if status["adapter"] != "gh" { throw("adapter"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn rhai_surface_registry_lists_forge_module() {
    let surface = crate::surface::rhai_surface_json();
    assert!(surface["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .any(|module| module.as_str() == Some("forge")));
    assert!(surface["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .any(|function| function["module"] == "forge" && function["name"] == "pr_create"));
}
