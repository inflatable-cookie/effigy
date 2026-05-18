use super::*;

#[test]
fn execute_rhai_script_exposes_runtime_context_helper() {
    let root = temp_root("runtime-context");
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctx\"\n").expect("manifest");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("nested");
    let runtime_context = effigy_context::EffigyRuntimeContext::builder()
        .cwd_override(Some(nested.clone()))
        .repo_override(Some(root.clone()))
        .captured_env(effigy_context::CapturedEnv {
            container_handoff: Some("1".into()),
            ..effigy_context::CapturedEnv::default()
        })
        .capture()
        .expect("runtime context");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
            let ctx = runtime::context();
            if ctx["invocation_cwd"] != "{}" {{ throw("invocation cwd"); }}
            if ctx["command_root"] != "{}" {{ throw("command root"); }}
            if ctx["repo_override"] != "{}" {{ throw("repo override"); }}
            if ctx["invocation_mode"] != "container_handoff" {{ throw("mode"); }}
            if !ctx["inside_container_handoff"] {{ throw("handoff"); }}
            if ctx["host"]["os"] == "" {{ throw("host os"); }}
            if ctx["host"]["arch"] == "" {{ throw("host arch"); }}
        "#,
        runtime_context.invocation_cwd().display(),
        runtime_context.command_root().display(),
        runtime_context
            .repo_override()
            .expect("repo override")
            .display(),
    );

    execute_rhai_script_with_runtime_context(
        &context,
        Some(&runtime_context),
        &script,
        &[],
        &callbacks(),
    )
    .expect("execute");
}

#[test]
fn execute_rhai_script_resolves_imports_from_catalog_root() {
    let invocation_root = temp_root("rhai-import-invocation");
    let catalog_root = temp_root("rhai-import-catalog");
    let scripts = catalog_root.join("scripts/tasks");
    fs::create_dir_all(&scripts).expect("scripts");
    fs::write(scripts.join("helper.rhai"), "fn value() { \"ok\" }").expect("helper");
    let context = ScriptContext {
        cwd: invocation_root.clone(),
        repo_root: invocation_root.clone(),
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
            import "scripts/tasks/helper.rhai" as helper;
            if helper::value() != "ok" {{ throw("import"); }}
            if catalog_root != "{}" {{ throw("catalog root"); }}
            if invocation_cwd != "{}" {{ throw("invocation cwd"); }}
        "#,
        catalog_root.display(),
        invocation_root.display(),
    );

    unsafe {
        std::env::set_var(EFFIGY_RHAI_CATALOG_ROOT, &catalog_root);
        std::env::set_var(EFFIGY_RHAI_INVOCATION_CWD, &invocation_root);
    }
    let result = execute_rhai_script(&context, &script, &[], &callbacks());
    unsafe {
        std::env::remove_var(EFFIGY_RHAI_CATALOG_ROOT);
        std::env::remove_var(EFFIGY_RHAI_INVOCATION_CWD);
    }
    result.expect("execute");
}

#[test]
fn execute_rhai_script_exposes_state_capture_context_helpers() {
    let root = temp_root("state-capture-context");
    let context_path = root.join(".effigy/state/capture-context/acowtancy-uat/new-content.json");
    fs::create_dir_all(context_path.parent().expect("context dir")).expect("context dir");
    fs::write(
        &context_path,
        r#"{
  "schema": "effigy.state-stack.capture-context.v1",
  "schema_version": 1,
  "stack_name": "acowtancy-uat",
  "capture_role": "uat-capture",
  "source_environment": "uat",
  "key": "new-content",
  "source": ".effigy/state/captures/new-content.json",
  "destination_ref": "oci://ghcr.io/acowtancy/state:new-content"
}
"#,
    )
    .expect("context");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "state:capture-new-content".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let _env = ScopedTestEnv::set_many(&[
        (
            "EFFIGY_STATE_CAPTURE_CONTEXT",
            ".effigy/state/capture-context/acowtancy-uat/new-content.json".to_owned(),
        ),
        (
            "EFFIGY_STATE_CAPTURE_SOURCE",
            ".effigy/state/captures/new-content.json".to_owned(),
        ),
        (
            "EFFIGY_STATE_CAPTURE_DESTINATION_REF",
            "oci://ghcr.io/acowtancy/state:new-content".to_owned(),
        ),
    ]);
    let script = r#"
        let context = state::capture_context();
        if context["stack_name"] != "acowtancy-uat" { throw("stack"); }
        if context["key"] != "new-content" { throw("key"); }
        if !str::ends_with(state::capture_context_path(), "/.effigy/state/capture-context/acowtancy-uat/new-content.json") { throw("context path"); }
        if state::capture_source() != ".effigy/state/captures/new-content.json" { throw("source"); }
        if state::capture_destination_ref() != "oci://ghcr.io/acowtancy/state:new-content" { throw("ref"); }
    "#;

    execute_rhai_script_with_runtime_context(&context, None, script, &[], &callbacks())
        .expect("execute");
}

#[test]
fn execute_rhai_script_exposes_state_capture_set_in_capture_hook_context() {
    let root = temp_root("state-capture-set-context");
    let context_path = root.join(".effigy/state/capture-context/acowtancy-uat/new-content.json");
    fs::create_dir_all(context_path.parent().expect("context dir")).expect("context dir");
    fs::write(
        &context_path,
        r#"{
  "schema": "effigy.state-stack.capture-context.v1",
  "schema_version": 1,
  "stack_name": "acowtancy-uat",
  "capture_role": "uat-capture",
  "source_environment": "uat",
  "key": "new-content",
  "source": ".effigy/state/captures/new-content.json",
  "destination_ref": "oci://ghcr.io/acowtancy/state:new-content"
}
"#,
    )
    .expect("context");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "state:capture-new-content".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let _env = ScopedTestEnv::set_many(&[
        (
            "EFFIGY_STATE_CAPTURE_CONTEXT",
            ".effigy/state/capture-context/acowtancy-uat/new-content.json".to_owned(),
        ),
        (
            "EFFIGY_STATE_CAPTURE_SOURCE",
            ".effigy/state/captures/new-content.json".to_owned(),
        ),
        (
            "EFFIGY_STATE_CAPTURE_DESTINATION_REF",
            "oci://ghcr.io/acowtancy/state:new-content".to_owned(),
        ),
    ]);
    let script = r#"
        let context = state::capture_context();
        if context["stack_name"] != "acowtancy-uat" { throw("stack"); }
        if context["key"] != "new-content" { throw("key"); }

        let capture_set = state::capture_set(#{
            stack: context["stack_name"],
            profiles: ["new-content", "media"],
            key: context["key"],
            yes: true,
            push: true,
        });
        if capture_set["feature"] != "state.capture_set" { throw("feature"); }
        if capture_set["options"]["stack"] != "acowtancy-uat" { throw("capture set stack"); }
        if capture_set["options"]["profiles"][0] != "new-content" { throw("capture set profile"); }
        if capture_set["options"]["key"] != "new-content" { throw("capture set key"); }
        if capture_set["options"]["push"] != true { throw("capture set push"); }
    "#;

    execute_rhai_script_with_runtime_context(&context, None, script, &[], &callbacks())
        .expect("execute");
}

#[test]
fn execute_rhai_script_exposes_state_apply_context_helpers() {
    let root = temp_root("state-apply-context");
    let context_path = root.join(".effigy/state/apply-context/acowtancy-uat/legacy-media.json");
    fs::create_dir_all(context_path.parent().expect("context dir")).expect("context dir");
    fs::write(
        &context_path,
        r#"{
  "schema": "effigy.state-stack.apply-context.v1",
  "schema_version": 1,
  "stack_name": "acowtancy-uat",
  "layer": {
    "key": "legacy-media",
    "artifact_report": {
      "metadata": {
        "primary_files": ["/tmp/media.oci"]
      }
    }
  }
}
"#,
    )
    .expect("context");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "state:hook:legacy-media".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_STATE_APPLY_CONTEXT",
        ".effigy/state/apply-context/acowtancy-uat/legacy-media.json".to_owned(),
    )]);
    let script = r#"
        let context = state::apply_context();
        if context["stack_name"] != "acowtancy-uat" { throw("stack"); }
        if context["layer"]["key"] != "legacy-media" { throw("layer"); }
        if context["layer"]["artifact_report"]["metadata"]["primary_files"][0] != "/tmp/media.oci" { throw("artifact"); }
        if !str::ends_with(state::apply_context_path(), "/.effigy/state/apply-context/acowtancy-uat/legacy-media.json") { throw("context path"); }
    "#;

    execute_rhai_script_with_runtime_context(&context, None, script, &[], &callbacks())
        .expect("execute");
}

#[test]
fn execute_rhai_script_exposes_deploy_provider_context_and_report_helpers() {
    let root = temp_root("deploy-provider-context");
    let context_path = root.join(".effigy/runtime/deploy/provider/context.json");
    let report_path = root.join(".effigy/runtime/deploy/provider/report.json");
    fs::create_dir_all(context_path.parent().expect("context dir")).expect("context dir");
    fs::write(
        &context_path,
        r#"{
  "schema": "effigy.deploy-provider.context.v1",
  "phase": "preflight",
  "env": "uat",
  "provider": "render"
}
"#,
    )
    .expect("context");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "deploy-provider:render:preflight".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let _env = ScopedTestEnv::set_many(&[
        (
            "EFFIGY_DEPLOY_PROVIDER_CONTEXT",
            context_path.display().to_string(),
        ),
        (
            "EFFIGY_DEPLOY_PROVIDER_REPORT",
            report_path.display().to_string(),
        ),
    ]);
    let script = r#"
        let context = deploy::provider_context();
        if context["provider"] != "render" { throw("provider"); }
        if deploy::provider_context_path() == "" { throw("context path"); }
        if deploy::provider_report_path() == "" { throw("report path"); }
        deploy::provider_report(#{
            schema: "effigy.deploy-provider.report.v1",
            phase: "preflight",
            provider: "render",
            status: "planned",
            checks: [#{ name: "auth", status: "planned" }],
            warnings: [],
            blockers: [],
            files: [],
        });
    "#;

    execute_rhai_script_with_runtime_context(&context, None, script, &[], &callbacks())
        .expect("execute");
    let report = fs::read_to_string(&report_path).expect("report");
    assert!(report.contains(r#""provider": "render""#), "{report}");
    assert!(report.contains(r#""name": "auth""#), "{report}");
}

#[test]
fn execute_rhai_script_exposes_exec_run_helper() {
    let root = temp_root("exec-run");
    fs::write(root.join("input.sql"), "select 1;").expect("input");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let host = exec::run(["sh", "-lc", "printf host-ok"], #{ run_in: "host" });
            if !host["success"] || host["stdout"] != "host-ok" { throw("host exec"); }
            if host["route"]["run_in"] != "host" { throw("host route"); }

            let container = exec::run(
                ["mysql", "app"],
                #{
                    run_in: "container",
                    container: "web",
                    service: "db",
                    cwd: "db",
                    stdin_file: "../input.sql",
                    env: #{ MYSQL_PWD: "secret" },
                },
            );
            if !container["success"] { throw("container exec"); }
            if container["stdout"] != "exec:web:db:mysql,app" { throw("container stdout"); }
            if container["route"]["run_in"] != "container" { throw("container route"); }
            if container["route"]["container"] != "web" { throw("container name"); }
            if container["route"]["service"] != "db" { throw("container service"); }
        "#;

    let captured = Arc::new(Mutex::new(None::<Value>));
    let captured_exec = Arc::clone(&captured);
    let mut host_callbacks = callbacks();
    host_callbacks.container_exec_with_options = Arc::new(move |_, _, _, _, options| {
        *captured_exec.lock().expect("capture lock") = Some(options);
        Ok(HostCommandOutput {
            status: 0,
            success: true,
            stdout: "exec:web:db:mysql,app".to_owned(),
            stderr: String::new(),
        })
    });

    fs::create_dir_all(context.cwd.join("db")).expect("db cwd");
    execute_rhai_script(&context, script, &[], &host_callbacks).expect("execute");
    let captured = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("container exec options");
    assert_eq!(
        captured["cwd"],
        serde_json::json!(context
            .cwd
            .join("db")
            .canonicalize()
            .expect("canonical cwd")
            .display()
            .to_string())
    );
    assert_eq!(
        captured["stdin_file"],
        serde_json::json!(context
            .cwd
            .join("input.sql")
            .canonicalize()
            .expect("canonical stdin")
            .display()
            .to_string())
    );
    assert_eq!(captured["env"]["MYSQL_PWD"], serde_json::json!("secret"));
}

#[test]
fn execute_rhai_script_maps_host_exec_paths_into_local_workspace_during_handoff() {
    let host_root = temp_root("exec-handoff-host");
    let host_nested = host_root.join("bundle/scripts");
    fs::create_dir_all(&host_nested).expect("host nested");

    let local_root = temp_root("exec-handoff-local");
    let local_nested = local_root.join("bundle/scripts");
    fs::create_dir_all(&local_nested).expect("local nested");
    fs::write(local_nested.join("marker.txt"), "marker-ok").expect("marker");
    fs::write(local_nested.join("stdin.txt"), "stdin-ok").expect("stdin");

    let runtime_context = effigy_context::EffigyRuntimeContext::builder()
        .cwd_override(Some(host_nested.clone()))
        .repo_override(Some(host_root.clone()))
        .captured_env(effigy_context::CapturedEnv {
            container_handoff: Some("1".into()),
            ..effigy_context::CapturedEnv::default()
        })
        .capture()
        .expect("runtime context");

    let context = ScriptContext {
        cwd: local_root.clone(),
        repo_root: local_root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    let script = r#"
            let host = exec::run(
                ["sh", "-lc", "printf '%s|' \"$PWD\"; cat marker.txt; printf '|'; cat"],
                #{ run_in: "host", stdin_file: "stdin.txt" },
            );
            if !host["success"] { throw("host exec"); }
            if !host["stdout"].contains("marker-ok|stdin-ok") { throw(host["stdout"]); }
            if host["route"]["run_in"] != "host" { throw("host route"); }
        "#;

    execute_rhai_script_with_runtime_context(
        &context,
        Some(&runtime_context),
        script,
        &[],
        &callbacks(),
    )
    .expect("execute");
}

#[test]
fn execute_rhai_script_proves_mysql_seed_uses_container_exec_with_stdin_file() {
    let root = temp_root("mysql-seed");
    let seed_path = root.join("bundle/database/seeds/contactpatch.sql");
    fs::create_dir_all(seed_path.parent().expect("seed parent")).expect("seed parent dir");
    fs::write(&seed_path, "insert into contacts values (1);\n").expect("seed sql");

    let context = ScriptContext {
        cwd: root.join("bundle"),
        repo_root: root.clone(),
        task_name: "db:seed".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    fs::create_dir_all(&context.cwd).expect("bundle cwd");

    let captured = Arc::new(Mutex::new(
        None::<(PathBuf, String, Option<String>, Vec<String>, Value)>,
    ));
    let captured_exec = Arc::clone(&captured);
    let mut host_callbacks = callbacks();
    host_callbacks.container_exec_with_options =
        Arc::new(move |repo_root, name, service, command, options| {
            *captured_exec.lock().expect("capture lock") = Some((
                repo_root.to_path_buf(),
                name.to_owned(),
                service.map(str::to_owned),
                command.to_vec(),
                options,
            ));
            Ok(HostCommandOutput {
                status: 0,
                success: true,
                stdout: "mysql-seed-ok".to_owned(),
                stderr: String::new(),
            })
        });

    let script = r#"
            let result = exec::run(
                ["mysql", "--database", "app"],
                #{
                    run_in: "container",
                    container: "web",
                    service: "db",
                    stdin_file: "database/seeds/contactpatch.sql",
                },
            );
            if !result["success"] { throw("mysql seed failed"); }
            if result["stdout"] != "mysql-seed-ok" { throw("mysql seed stdout"); }
            if result["route"]["run_in"] != "container" { throw("mysql seed route"); }
            if result["route"]["container"] != "web" { throw("mysql seed container"); }
            if result["route"]["service"] != "db" { throw("mysql seed service"); }
        "#;

    execute_rhai_script(&context, script, &[], &host_callbacks).expect("execute");

    let captured = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("container exec call");
    assert_eq!(captured.0, root);
    assert_eq!(captured.1, "web");
    assert_eq!(captured.2, Some("db".to_owned()));
    assert_eq!(
        captured.3,
        vec![
            "mysql".to_owned(),
            "--database".to_owned(),
            "app".to_owned()
        ]
    );
    assert_eq!(
        captured.4["stdin_file"],
        serde_json::json!(seed_path
            .canonicalize()
            .expect("canonical seed")
            .display()
            .to_string())
    );
    assert_eq!(
        captured.4["cwd"],
        serde_json::json!(context
            .cwd
            .canonicalize()
            .expect("canonical cwd")
            .display()
            .to_string())
    );
}
