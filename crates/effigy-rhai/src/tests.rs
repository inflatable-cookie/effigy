use super::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    render_host_log_message, resolve_script_path, EffigyCommandError, HostCallbacks,
    HostCommandOutput, ScriptContext, EFFIGY_RHAI_ARGS_JSON,
};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

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
        run_feature: Arc::new(|_, feature, options| {
            let payload = if feature == "config.get" {
                serde_json::json!({
                    "schema": "test.feature.v1",
                    "ok": true,
                    "feature": feature,
                    "options": options,
                    "value": "stack",
                })
            } else {
                serde_json::json!({
                    "schema": "test.feature.v1",
                    "ok": true,
                    "feature": feature,
                    "options": options,
                })
            };
            Ok(payload.to_string())
        }),
        container_up: Arc::new(|_, name, detach| Ok(format!("up:{name}:{detach}"))),
        container_down: Arc::new(|_, name| Ok(format!("down:{name}"))),
        container_shell: Arc::new(|_, name, command| Ok(format!("shell:{name}:{command}"))),
        container_exec: Arc::new(|_, name, service, command| {
            Ok(HostCommandOutput {
                status: 0,
                success: true,
                stdout: format!(
                    "exec:{name}:{}:{}",
                    service.unwrap_or(""),
                    command.join(",")
                ),
                stderr: String::new(),
            })
        }),
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
            let exec = container_exec("web", "postgres", ["psql", "-c", "select 1"]);
            if !exec["success"] || exec["stdout"] != "exec:web:postgres:psql,-c,select 1" { throw("exec"); }
            let default_service = container_exec("web", ["pwd"]);
            if default_service["stdout"] != "exec:web::pwd" { throw("exec default"); }
            let tasks = tasks_list();
            if tasks["feature"] != "tasks.list" { throw("tasks list"); }
            let resolved = task_resolve("api/test");
            if resolved["options"]["selector"] != "api/test" { throw("task resolve"); }
            let status = container_status("stack");
            if status["feature"] != "container.status" { throw("container status"); }
            let logs = container_logs("stack", #{ service: "postgres" });
            if logs["options"]["service"] != "postgres" { throw("container logs"); }
            let docs = docs_check_links(#{ paths: ["docs/README.md"] });
            if docs["feature"] != "docs.check_links" { throw("docs"); }
            let bundle = bundle_inspect("underlay");
            if bundle["options"]["bundle"] != "underlay" { throw("bundle"); }
            let exported = bundle_export("underlay", "tmp/bundle");
            if exported["options"]["path"] != "tmp/bundle" { throw("bundle export"); }
            let gateway = gateway_status();
            if gateway["feature"] != "gateway.status" { throw("gateway"); }
            let scan = scan_god_files(#{ threshold: 900 });
            if scan["feature"] != "scan.god_files" { throw("scan"); }
            let cache = cache_inspect(#{ selector: "build" });
            if cache["options"]["selector"] != "build" { throw("cache"); }
            let config = config_effective();
            if config["feature"] != "config.effective" { throw("config effective"); }
            let raw = config_raw();
            if raw["feature"] != "config.raw" { throw("config raw"); }
            let value = config_get("systems.dev.container");
            if value != "stack" { throw("config get"); }
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
fn execute_rhai_script_rejects_recursive_effigy_process_calls() {
    let root = temp_root("recursive-effigy-process");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    let error = execute_rhai_script(
        &context,
        r#"run_process("effigy", ["tasks"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("recursive effigy process should fail");
    assert!(error.to_string().contains("typed host helper"));

    let error = execute_rhai_script(
        &context,
        r#"run_process_stream("effigy", ["tasks"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("recursive effigy stream process should fail");
    assert!(error.to_string().contains("typed host helper"));
}

#[test]
fn execute_rhai_script_can_make_http_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer).expect("read request");
        stream
            .write_all(
                b"HTTP/1.1 201 Created\r\nContent-Type: text/plain\r\nContent-Length: 7\r\n\r\ncreated",
            )
            .expect("write response");
    });
    let root = temp_root("http-request");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
            let response = http_request("POST", "http://{addr}/smoke", #{{ body: "ping" }});
            if response["status"] != 201 {{ throw("status"); }}
            if response["body"] != "created" {{ throw("body"); }}
            if response["success"] != true {{ throw("success"); }}
        "#
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server");
}

#[test]
fn execute_rhai_script_can_list_directory_entries() {
    let root = temp_root("list-dir");
    let fixture_dir = root.join("fixture");
    fs::create_dir_all(&fixture_dir).expect("fixture dir");
    fs::write(fixture_dir.join("b.txt"), "b").expect("file b");
    fs::write(fixture_dir.join("a.txt"), "a").expect("file a");
    fs::create_dir_all(fixture_dir.join("subdir")).expect("subdir");

    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let entries = list_dir("fixture");
            if path_file_name(entries[0].to_string()) != "a.txt" { throw("first"); }
            if path_file_name(entries[1].to_string()) != "b.txt" { throw("second"); }
            if path_file_name(entries[2].to_string()) != "subdir" { throw("third"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_search_files_without_rg() {
    let root = temp_root("search-files");
    let routes = root.join("routes");
    fs::create_dir_all(&routes).expect("routes dir");
    fs::write(
        routes.join("a.rs"),
        "fn main() { StatusCode::BAD_REQUEST.into_response() }\n",
    )
    .expect("rs file");
    fs::write(
        routes.join("notes.md"),
        "StatusCode::BAD_REQUEST.into_response()\n",
    )
    .expect("md file");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let matches = search_files("routes", "StatusCode::BAD_REQUEST", #{ glob: "*.rs", literal: true });
            if matches["count"] != 1 { throw("count"); }
            if matches["matches"][0]["line"] != 1 { throw("line"); }
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
        run_feature: callbacks().run_feature,
        container_up: callbacks().container_up,
        container_down: callbacks().container_down,
        container_shell: callbacks().container_shell,
        container_exec: callbacks().container_exec,
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
        if contents.contains("run_process(\"effigy\"")
            || contents.contains("run_process(`effigy`")
            || contents.contains("run_effigy(")
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
        "first-party Rhai scripts must use typed host helpers instead of recursive Effigy calls: {}",
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
        if !contents.contains("run_process(") && !contents.contains("run_process_stream(") {
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
        "first-party Rhai run_process usage must be explicitly allowlisted or replaced with typed host helpers: {}",
        violations.join(", ")
    );
}

fn allowed_first_party_process_script(relative: &str, contents: &str) -> bool {
    match relative {
        "scripts/rhai/write-browser-proof-report.rhai" => {
            contents.contains("run_process(\"cargo\", process_args)")
        }
        "scripts/rhai/check-release-smoke.rhai" => {
            contents.contains("run_process(program, process_args)")
        }
        "scripts/rhai/rehearse-linux-release-container.rhai" => {
            contents.contains("run_process(\n    \"colima\",")
        }
        "crates/effigy-catalog/starters/underlay/scripts/dev/ui-setup.rhai" => {
            contents.contains("run_process_stream(\"sh\", [\"-lc\", shell])")
        }
        _ => false,
    }
}

fn collect_rhai_scripts(root: &Path) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    collect_rhai_scripts_into(root, &mut scripts);
    scripts.sort();
    scripts
}

fn collect_rhai_scripts_into(dir: &Path, scripts: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name == "target" || file_name == ".git" || file_name == ".effigy" {
            continue;
        }
        if path.is_dir() {
            collect_rhai_scripts_into(&path, scripts);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rhai")
        {
            scripts.push(path);
        }
    }
}
