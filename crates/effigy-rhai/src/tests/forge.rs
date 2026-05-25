use super::*;
use std::os::unix::fs::PermissionsExt;

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
fn execute_rhai_script_routes_pull_request_helpers_through_gh_adapter() {
    let root = temp_root("forge-gh");
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

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let log_path = root.join("gh.log");
    let fake_gh = bin_dir.join("gh");
    fs::write(
        &fake_gh,
        format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
case "$1 $2" in
  "pr view")
    printf '{{"number":17,"title":"Viewed","state":"OPEN","url":"https://example.test/pr/17"}}\n'
    ;;
  "pr list")
    printf '[{{"number":17,"title":"Listed","state":"OPEN","url":"https://example.test/pr/17"}}]\n'
    ;;
  "pr create")
    printf 'https://example.test/pr/new\n'
    ;;
  "pr checkout")
    printf 'checked out %s\n' "$3"
    ;;
  *)
    exit 0
    ;;
esac
"#,
            log_path.display()
        ),
    )
    .expect("write fake gh");
    let mut permissions = fs::metadata(&fake_gh)
        .expect("fake gh metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_gh, permissions).expect("chmod fake gh");

    let old_path = std::env::var("PATH").ok().unwrap_or_default();
    let _env = ScopedTestEnv::set_many(&[("PATH", format!("{}:{old_path}", bin_dir.display()))]);

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "forge".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        let viewed = forge::pr_view(#{ number: 17, fields: "number,title,state,url" });
        if viewed["number"] != 17 { throw("view number"); }
        if viewed["title"] != "Viewed" { throw("view title"); }

        let listed = forge::pr_list(#{
            state: "open",
            base: "main",
            head: "feature",
            author: "@me",
            search: "review-requested:@me",
            limit: 3,
            fields: "number,title,state,url",
        });
        if listed.len() != 1 { throw("list length"); }
        if listed[0]["title"] != "Listed" { throw("list title"); }

        let created = forge::pr_create(#{
            title: "Add thing",
            body: "Body text",
            base: "main",
            head: "feature",
            draft: true,
        });
        if !created["success"] { throw(created["stderr"]); }
        if !str::contains(created["stdout"], "/pr/new") { throw("create stdout"); }

        let checkout = forge::pr_checkout(17);
        if !checkout["success"] { throw(checkout["stderr"]); }
        if !str::contains(checkout["stdout"], "17") { throw("checkout stdout"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");

    let log = fs::read_to_string(log_path).expect("read gh log");
    assert!(
        log.contains("pr view 17 --json number,title,state,url"),
        "{log}"
    );
    assert!(
        log.contains(
            "pr list --json number,title,state,url --state open --base main --head feature --author @me --search review-requested:@me --limit 3"
        ),
        "{log}"
    );
    assert!(
        log.contains(
            "pr create --title Add thing --body Body text --base main --head feature --draft"
        ),
        "{log}"
    );
    assert!(log.contains("pr checkout 17"), "{log}");
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
