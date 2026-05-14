use super::*;

#[test]
fn run_effigy_json_surfaces_callback_errors_as_runtime_errors() {
    let root = temp_root("effigy-json-error");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let callbacks = HostCallbacks {
        run_task: callbacks().run_task,
        run_effigy: Arc::new(|_, _, _| {
            Err(EffigyCommandError {
                message: "boom".to_owned(),
                rendered_output: "rendered".to_owned(),
            })
        }),
        run_feature: callbacks().run_feature,
        container_up: callbacks().container_up,
        container_down: callbacks().container_down,
        container_shell: callbacks().container_shell,
        container_exec: callbacks().container_exec,
        container_exec_with_options: callbacks().container_exec_with_options,
    };

    let error = execute_rhai_script(&context, "effigy::run_json([\"demo\"]);", &[], &callbacks)
        .expect_err("must fail");
    assert!(error.to_string().contains("boom"));
}

#[test]
fn host_log_message_colors_known_status_prefixes() {
    let rendered = render_host_log_message(
        "[ok] passed\n[check] running\n[gateway] route installed\n[bootstrap] starting dev\n[next] inspect\n",
        true,
    );

    assert!(
        rendered.contains("\u{1b}["),
        "expected ansi styles in: {rendered:?}"
    );
    assert!(rendered.contains("[ok]"));
    assert!(rendered.contains(" passed"));
    assert!(rendered.contains("[check]"));
    assert!(rendered.contains(" running"));
    assert!(rendered.contains("[gateway]"));
    assert!(rendered.contains(" route installed"));
    assert!(rendered.contains("[bootstrap]"));
    assert!(rendered.contains(" starting dev"));
    assert!(rendered.contains("[next]"));
    assert!(rendered.contains(" inspect"));
}

#[test]
fn host_log_message_leaves_plain_text_unchanged_without_color() {
    let rendered = render_host_log_message("plain\n[warn] careful\n", false);
    assert_eq!(rendered, "plain\n[warn] careful\n");
}

#[test]
fn first_party_rhai_scripts_do_not_recursively_invoke_effigy() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if contents.contains("process::run(\"effigy\"")
            || contents.contains("process::run(`effigy`")
            || contents.contains("process::stream(\"effigy\"")
            || contents.contains("process::stream(`effigy`")
            || contents.contains("process::tee(\"effigy\"")
            || contents.contains("process::tee(`effigy`")
            || contents.contains("effigy::run(")
            || contents.contains("effigy::run_json(")
        {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use typed host helpers instead of recursive Effigy escape hatches: {}",
        violations.join(", ")
    );
}

#[test]
fn first_party_rhai_process_calls_are_allowlisted() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if !contents.contains("process::run(")
            && !contents.contains("process::stream(")
            && !contents.contains("process::tee(")
        {
            continue;
        }
        let relative = script
            .strip_prefix(&repo_root)
            .unwrap_or(&script)
            .display()
            .to_string();
        if !allowed_first_party_process_script(&relative, &contents) {
            violations.push(relative);
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai process helper usage must be explicitly allowlisted or replaced with typed host helpers: {}",
        violations.join(", ")
    );
}

#[test]
fn first_party_rhai_scripts_use_exec_run_for_container_commands() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if contents.contains("container::exec(") {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use exec::run(..., #{{ run_in: \"container\", ... }}) for container commands: {}",
        violations.join(", ")
    );
}

#[test]
fn first_party_rhai_scripts_do_not_use_legacy_module_dot_calls() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    let module_call = regex::Regex::new(
        r"\b(?:time|path|fs|process|http|json|toml|str|regex|random|search|config|task|container|scan|docs|deploy|system|demo|changelog|cache|gateway|bundle|service|catalog|doctor|contracts|unlock|test|effigy)\.[A-Za-z_][A-Za-z0-9_]*\s*\(",
    )
    .expect("module regex");
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if module_call.is_match(&strip_rhai_string_literals(&contents)) {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use `module::func(...)` syntax, not `module.func(...)`: {}",
        violations.join(", ")
    );
}

#[test]
fn execute_rhai_script_rejects_legacy_module_dot_calls() {
    let root = temp_root("legacy-dot-calls");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    let error = execute_rhai_script(
        &context,
        r#"process.run("sh", ["-lc", "printf nope"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("legacy module dot syntax should fail");

    assert!(
        error.to_string().contains("Variable not found: process"),
        "got: {error}"
    );
}
