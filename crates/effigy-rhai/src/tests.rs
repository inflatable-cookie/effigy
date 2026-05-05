use super::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    render_host_log_message, resolve_script_path, EffigyCommandError, HostCallbacks,
    HostCommandOutput, ScriptContext, EFFIGY_RHAI_ARGS_JSON,
};
use crate::surface::{FEATURE_NAMES, MODULE_NAMES};
use std::collections::BTreeSet;
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
        container_down: Arc::new(|_, name, all| Ok(format!("down:{name}:{all}"))),
        container_shell: Arc::new(|_, name, service, command| {
            Ok(format!("shell:{name}:{}:{command}", service.unwrap_or("")))
        }),
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
        container_exec_with_options: Arc::new(|_, name, service, command, _| {
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
fn rhai_surface_module_names_are_unique() {
    let unique = MODULE_NAMES.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        MODULE_NAMES.len(),
        "duplicate Rhai module names in surface registry"
    );
}

#[test]
fn rhai_surface_feature_names_are_unique() {
    let unique = FEATURE_NAMES.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        FEATURE_NAMES.len(),
        "duplicate Rhai feature names in surface registry"
    );
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
            let task = task::run("demo:task", ["a", "b"]);
            if task != "task:demo:task:a,b" { throw("task"); }
            let effigy = effigy::run(["demo", "list"]);
            if !effigy["success"] || effigy["output"] != "demo list" { throw("effigy"); }
            let json = effigy::run_json(["demo", "list"]);
            if json["json"] != true { throw("json"); }
            if container::up("web", true) != "up:web:true" { throw("up"); }
            if container::down("web") != "down:web:false" { throw("down"); }
            if container::shell("web", "echo hi") != "shell:web::echo hi" { throw("shell"); }
            let exec = container::exec("web", "postgres", ["psql", "-c", "select 1"]);
            if !exec["success"] || exec["stdout"] != "exec:web:postgres:psql,-c,select 1" { throw("exec"); }
            let default_service = container::exec("web", ["pwd"]);
            if default_service["stdout"] != "exec:web::pwd" { throw("exec default"); }
            let tasks = task::list();
            if tasks["feature"] != "tasks.list" { throw("tasks list"); }
            let resolved = task::resolve("api/test");
            if resolved["options"]["selector"] != "api/test" { throw("task resolve"); }
            let status = container::status("stack");
            if status["feature"] != "container.status" { throw("container status"); }
            let status_all = container::status(#{ "all": true });
            if status_all["feature"] != "container.status" || status_all["options"]["all"] != true { throw("container status all"); }
            let logs = container::logs("stack", #{ service: "postgres" });
            if logs["options"]["service"] != "postgres" { throw("container logs"); }
            let data = container::data("list", "stack");
            if data["feature"] != "container.data" || data["options"]["operation"] != "list" { throw("container data"); }
            let stats = container::stats();
            if stats["feature"] != "container.stats" { throw("container stats"); }
            let docs = docs::check_links(#{ paths: ["docs/README.md"] });
            if docs["feature"] != "docs.check_links" { throw("docs"); }
            let bundle = bundle::inspect("underlay");
            if bundle["options"]["bundle"] != "underlay" { throw("bundle"); }
            let exported = bundle::emit("underlay", "tmp/bundle");
            if exported["options"]["path"] != "tmp/bundle" { throw("bundle emit"); }
            let deploy = deploy::emit(#{ provider: "render", path: "tmp/render", plan: true });
            if deploy["feature"] != "deploy.emit" || deploy["options"]["provider"] != "render" { throw("deploy emit"); }
            let gateway = gateway::status();
            if gateway["feature"] != "gateway.status" { throw("gateway"); }
            let scan = scan::god_files(#{ threshold: 900 });
            if scan["feature"] != "scan.god_files" { throw("scan"); }
            let cache = cache::inspect(#{ selector: "build" });
            if cache["options"]["selector"] != "build" { throw("cache"); }
            let unlock = unlock::scopes(#{ "all": true });
            if unlock["feature"] != "unlock.scopes" || unlock["options"]["all"] != true { throw("unlock scopes"); }
            let config = config::effective();
            if config["feature"] != "config.effective" { throw("config effective"); }
            let raw = config::raw();
            if raw["feature"] != "config.raw" { throw("config raw"); }
            let value = config::get("systems.dev.container");
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
            let streamed = process::stream("sh", ["-lc", "printf stream-ok"]);
            if !streamed["success"] { throw("stream"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_tee_process_output_and_capture() {
    let root = temp_root("tee-process");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let teed = process::tee("sh", ["-lc", "printf tee-out; printf tee-err >\u00262"]);
            if !teed["success"] { throw("tee"); }
            if teed["stdout"] != "tee-out" { throw("tee stdout"); }
            if teed["stderr"] != "tee-err" { throw("tee stderr"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_process_helpers_accept_cwd_and_env_options() {
    let root = temp_root("process-options");
    fs::create_dir_all(root.join("nested")).expect("nested dir");
    fs::write(root.join("input.txt"), "streamed-input").expect("input");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let buffered = process::run(
                "cat",
                [],
                #{ stdin_file: "input.txt" },
            );
            if buffered["stdout"] != "streamed-input" { throw("buffered stdin"); }

            let streamed = process::stream(
                "sh",
                ["-lc", "test \"$(cat)\" = streamed-input && pwd | grep '/nested$' >/dev/null && test \"$EFFIGY_RHAI_TEST\" = streamed"],
                #{ cwd: "nested", env: #{ EFFIGY_RHAI_TEST: "streamed" }, stdin_file: "../input.txt" },
            );
            if !streamed["success"] { throw("streamed options"); }

            let teed = process::tee(
                "sh",
                ["-lc", "printf '%s|%s|%s' \"$(cat)\" \"$PWD\" \"$EFFIGY_RHAI_TEST\""],
                #{ cwd: "nested", env: #{ EFFIGY_RHAI_TEST: "teed" }, stdin_file: "../input.txt" },
            );
            if !str::starts_with(teed["stdout"], "streamed-input|") { throw("teed stdin"); }
            if !str::ends_with(teed["stdout"], "/nested|teed") { throw("teed options"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_exposes_trim_string_helper() {
    let root = temp_root("trim-string");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let value = str::trim("  hello  ");
            if value != "hello" { throw("trim string"); }
            let empty = str::trim(());
            if empty != "" { throw("trim unit"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_exposes_low_level_string_and_file_helpers() {
    let root = temp_root("low-level-helpers");
    fs::write(root.join("source.txt"), "alpha\nbeta\n").expect("source");
    fs::write(root.join("replace.txt"), "host=example.test\nmode=dev\n").expect("replace");
    fs::write(
        root.join("app.env"),
        "# comment\nHOST=example.test\nexport MODE=dev\n",
    )
    .expect("env file");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            if !str::contains("alpha beta", "beta") { throw("contains"); }
            if !str::starts_with("alpha beta", "alpha") { throw("starts"); }
            if !str::ends_with("alpha beta", "beta") { throw("ends"); }
            if str::replace("alpha beta", "beta", "gamma") != "alpha gamma" { throw("replace"); }

            let inline = str::split_lines("one\ntwo\n");
            if inline.len() != 2 || inline[0] != "one" || inline[1] != "two" { throw("split"); }

            let copied = fs::copy("source.txt", "nested/copied.txt");
            if copied <= 0 { throw("copy size"); }
            if !fs::is_dir("nested") { throw("nested dir"); }

            let lines = fs::read_lines("nested/copied.txt");
            if lines.len() != 2 || lines[0] != "alpha" || lines[1] != "beta" { throw("read lines"); }

            fs::move_path("nested/copied.txt", "nested/moved.txt");
            if fs::exists("nested/copied.txt") { throw("copy still exists"); }
            if !fs::is_file("nested/moved.txt") { throw("move target"); }

            if !fs::copy_if_missing("source.txt", "nested/missing.txt") { throw("copy missing"); }
            if fs::copy_if_missing("source.txt", "nested/missing.txt") { throw("copy existing"); }

            if !fs::replace_in_file("replace.txt", "example.test", "local.test") { throw("replace in file"); }
            if fs::replace_in_file("replace.txt", "missing", "value") { throw("replace absent"); }

            if fs::env_file_get("app.env", "HOST") != "example.test" { throw("env get host"); }
            if fs::env_file_get("app.env", "MISSING") != "" { throw("env get missing"); }
            let before = fs::env_file_entries("app.env");
            if before["HOST"] != "example.test" || before["MODE"] != "dev" { throw("env entries before"); }
            if !fs::env_file_set("app.env", "HOST", "local.test") { throw("env set host"); }
            if !fs::env_file_set("app.env", "APP_NAME", "Cumberland Local") { throw("env append"); }
            if fs::env_file_set("app.env", "APP_NAME", "Cumberland Local") { throw("env unchanged"); }
            if fs::env_file_get("app.env", "HOST") != "local.test" { throw("env get updated"); }
            if fs::env_file_get("app.env", "APP_NAME") != "Cumberland Local" { throw("env get appended"); }
            if !fs::env_file_remove("app.env", "MODE") { throw("env remove"); }
            if fs::env_file_remove("app.env", "MODE") { throw("env remove missing"); }
            let after = fs::env_file_entries("app.env");
            if after.contains("MODE") { throw("env entries removed"); }
            if after["HOST"] != "local.test" || after["APP_NAME"] != "Cumberland Local" { throw("env entries after"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
    assert_eq!(
        fs::read_to_string(context.cwd.join("nested/moved.txt")).expect("read moved"),
        "alpha\nbeta\n"
    );
    assert_eq!(
        fs::read_to_string(context.cwd.join("nested/missing.txt")).expect("read missing copy"),
        "alpha\nbeta\n"
    );
    assert_eq!(
        fs::read_to_string(context.cwd.join("replace.txt")).expect("read replace"),
        "host=local.test\nmode=dev\n"
    );
    assert_eq!(
        fs::read_to_string(context.cwd.join("app.env")).expect("read env file"),
        "# comment\nHOST=local.test\nAPP_NAME=\"Cumberland Local\"\n"
    );
}

#[test]
fn execute_rhai_script_exposes_shell_quote_string_helper() {
    let root = temp_root("shell-quote-string");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let simple = str::shell_quote("secret");
            if simple != "'secret'" { throw("shell quote string"); }
            let with_quote = str::shell_quote("it's");
            if with_quote != "'it'\"'\"'s'" { throw("shell quote apostrophe"); }
            let empty = str::shell_quote(());
            if empty != "''" { throw("shell quote unit"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_exposes_generate_jwt_env_keys_helper() {
    let root = temp_root("generate-jwt-env-keys");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let jwt = random::jwt_env_keys();
            let private_key = str::trim(jwt["private_key"]);
            let public_key = str::trim(jwt["public_key"]);
            if private_key == "" { throw("missing private_key"); }
            if public_key == "" { throw("missing public_key"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_exposes_generate_random_base64_helper() {
    let root = temp_root("generate-random-base64");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let secret = random::base64(32);
            if str::trim(secret) == "" { throw("missing random secret"); }
            if random::base64(32) == secret { throw("random secret repeated"); }
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
        r#"process::run("effigy", ["tasks"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("recursive effigy process should fail");
    assert!(error.to_string().contains("typed host helper"));

    let error = execute_rhai_script(
        &context,
        r#"process::stream("effigy", ["tasks"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("recursive effigy stream process should fail");
    assert!(error.to_string().contains("typed host helper"));

    let error = execute_rhai_script(
        &context,
        r#"process::tee("effigy", ["tasks"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("recursive effigy tee process should fail");
    assert!(error.to_string().contains("typed host helper"));
}

#[test]
fn execute_rhai_script_allows_explicit_effigy_binary_paths() {
    let root = temp_root("explicit-effigy-binary-path");
    let binary = root.join("effigy");
    fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("write fake effigy binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&binary).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).expect("chmod");
    }
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    execute_rhai_script(
        &context,
        &format!(r#"process::run("{}", ["tasks"]);"#, binary.display()),
        &[],
        &callbacks(),
    )
    .expect("explicit binary path should be allowed");
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
            let response = http::request("POST", "http://{addr}/smoke", #{{ body: "ping" }});
            if response["status"] != 201 {{ throw("status"); }}
            if response["body"] != "created" {{ throw("body"); }}
            if response["success"] != true {{ throw("success"); }}
        "#
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server");
}

#[test]
fn execute_rhai_script_can_download_http_responses_to_a_file() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer).expect("read request");
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\n\r\nseed-env",
            )
            .expect("write response");
    });
    let root = temp_root("http-download");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
            let response = http::download("http://{addr}/file", "downloads/template.env");
            if response["status"] != 200 {{ throw("status"); }}
            if response["success"] != true {{ throw("success"); }}
            if response["size"] != 8 {{ throw("size"); }}
            if path::file_name(response["path"].to_string()) != "template.env" {{ throw("path"); }}
        "#
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    assert_eq!(
        fs::read_to_string(context.cwd.join("downloads/template.env")).expect("downloaded file"),
        "seed-env"
    );
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
            let entries = fs::list("fixture");
            if path::file_name(entries[0].to_string()) != "a.txt" { throw("first"); }
            if path::file_name(entries[1].to_string()) != "b.txt" { throw("second"); }
            if path::file_name(entries[2].to_string()) != "subdir" { throw("third"); }
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
            let matches = search::files("routes", "StatusCode::BAD_REQUEST", #{ glob: "*.rs", literal: true });
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

fn allowed_first_party_process_script(relative: &str, contents: &str) -> bool {
    match relative {
        "scripts/rhai/write-browser-proof-report.rhai" => {
            contents.contains("process::tee(\"cargo\", process_args)")
        }
        "scripts/rhai/check-release-smoke.rhai" => {
            contents.contains("process::run(program, process_args)")
        }
        "scripts/rhai/rehearse-linux-release-container.rhai" => {
            contents.contains("process::run(\n    \"colima\",")
        }
        "crates/effigy-catalog/starters/underlay/scripts/dev/ui-setup.rhai"
        | "crates/effigy-manifest/bundles/underlay/scripts/dev/ui-setup.rhai" => {
            contents.contains("process::stream(\"sh\", [\"-lc\", shell])")
        }
        "scripts/rhai/build-local-bin.rhai" => {
            contents.contains("process::stream(program, process_args, options)")
        }
        "scripts/rhai/install-local-bin-links.rhai" => contents.contains("process::run("),
        _ => false,
    }
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
        r"\b(?:time|path|fs|process|http|json|toml|str|random|search|config|task|container|scan|docs|deploy|system|demo|changelog|cache|gateway|bundle|service|catalog|doctor|contracts|unlock|test|effigy)\.[A-Za-z_][A-Za-z0-9_]*\s*\(",
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

fn collect_rhai_scripts(root: &Path) -> Vec<PathBuf> {
    let mut scripts = Vec::new();
    collect_rhai_scripts_into(root, &mut scripts);
    scripts.sort();
    scripts
}

fn strip_rhai_string_literals(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            escape = false;
            output.push(if in_double || in_backtick { ' ' } else { ch });
            continue;
        }

        if ch == '\\' {
            escape = true;
            output.push(if in_double || in_backtick { ' ' } else { ch });
            continue;
        }

        if ch == '"' && !in_backtick {
            in_double = !in_double;
            output.push(' ');
            continue;
        }

        if ch == '`' && !in_double {
            in_backtick = !in_backtick;
            output.push(' ');
            continue;
        }

        output.push(if in_double || in_backtick { ' ' } else { ch });
    }

    output
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
