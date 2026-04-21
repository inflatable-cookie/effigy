use super::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    render_host_log_message, resolve_script_path, EffigyCommandError, HostCallbacks, ScriptContext,
    EFFIGY_RHAI_ARGS_JSON,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-rhai-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn callbacks() -> HostCallbacks {
    HostCallbacks {
        run_task: Arc::new(|_, task, args| Ok(format!("task:{task}:{}", args.join(",")))),
        run_effigy: Arc::new(|_, args, force_json| {
            if force_json {
                Ok(format!(
                    "{{\"args\":{},\"json\":true}}",
                    serde_json::to_string(args).expect("json")
                ))
            } else {
                Ok(args.join(" "))
            }
        }),
        container_up: Arc::new(|_, name, detach| Ok(format!("up:{name}:{detach}"))),
        container_down: Arc::new(|_, name| Ok(format!("down:{name}"))),
        container_shell: Arc::new(|_, name, command| Ok(format!("shell:{name}:{command}"))),
    }
}

#[test]
fn load_script_reads_relative_path_from_cwd() {
    let root = temp_root("load-script");
    let script_path = root.join("scripts/test.rhai");
    fs::create_dir_all(script_path.parent().unwrap()).expect("scripts dir");
    fs::write(&script_path, "log(\"ok\");").expect("script");

    let loaded = load_script(Path::new("scripts/test.rhai"), &root).expect("load");
    assert!(loaded.contains("log"));
    assert_eq!(
        resolve_script_path(&root, Path::new("scripts/test.rhai")),
        script_path
    );
}

#[test]
fn load_script_args_from_env_decodes_json_array() {
    unsafe {
        std::env::set_var(EFFIGY_RHAI_ARGS_JSON, "[\"one\",\"two\"]");
    }
    let args = load_script_args_from_env().expect("args");
    assert_eq!(args, vec!["one".to_owned(), "two".to_owned()]);
    unsafe {
        std::env::remove_var(EFFIGY_RHAI_ARGS_JSON);
    }
}

#[test]
fn execute_rhai_script_exposes_task_effigy_and_container_helpers() {
    let root = temp_root("execute");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let task = run_task("demo:task", ["a", "b"]);
            if task != "task:demo:task:a,b" { throw("task"); }
            let effigy = run_effigy(["demo", "list"]);
            if !effigy["success"] || effigy["output"] != "demo list" { throw("effigy"); }
            let json = run_effigy_json(["demo", "list"]);
            if json["json"] != true { throw("json"); }
            if container_up("web", true) != "up:web:true" { throw("up"); }
            if container_down("web") != "down:web" { throw("down"); }
            if container_shell("web", "echo hi") != "shell:web:echo hi" { throw("shell"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_stream_process_output() {
    let root = temp_root("stream-process");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let streamed = run_process_stream("sh", ["-lc", "printf stream-ok"]);
            if !streamed["success"] { throw("stream"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

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
        container_up: callbacks().container_up,
        container_down: callbacks().container_down,
        container_shell: callbacks().container_shell,
    };

    let error = execute_rhai_script(&context, "run_effigy_json([\"demo\"]);", &[], &callbacks)
        .expect_err("must fail");
    assert!(error.to_string().contains("boom"));
}

#[test]
fn host_log_message_colors_known_status_prefixes() {
    let rendered = render_host_log_message("[ok] passed\n[check] running\n[next] inspect\n", true);

    assert!(
        rendered.contains("\u{1b}["),
        "expected ansi styles in: {rendered:?}"
    );
    assert!(rendered.contains("[ok]"));
    assert!(rendered.contains(" passed"));
    assert!(rendered.contains("[check]"));
    assert!(rendered.contains(" running"));
    assert!(rendered.contains("[next]"));
    assert!(rendered.contains(" inspect"));
}

#[test]
fn host_log_message_leaves_plain_text_unchanged_without_color() {
    let rendered = render_host_log_message("plain\n[warn] careful\n", false);
    assert_eq!(rendered, "plain\n[warn] careful\n");
}
