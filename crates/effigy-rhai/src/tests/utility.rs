use super::*;

#[test]
fn execute_rhai_script_exposes_fs_sha256_and_file_size_helpers() {
    let root = temp_root("fs-sha256-and-file-size");
    fs::write(root.join("payload.txt"), "hello world\n").expect("payload");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
        if fs::file_size("payload.txt") != 12 { throw("size"); }
        let digest = fs::sha256("payload.txt");
        if digest != "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447" {
            throw("sha256");
        }
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
            if path::parent("nested/copied.txt") != "nested" { throw("path parent"); }
            if path::parent("source.txt") != "" { throw("path parent empty"); }

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
fn execute_rhai_script_can_create_temp_files_and_list_recursive_files() {
    let root = temp_root("fs-recursive");
    fs::create_dir_all(root.join("crates/a/src")).expect("crate a");
    fs::create_dir_all(root.join("crates/b/src")).expect("crate b");
    fs::write(root.join("crates/a/src/lib.rs"), "fn a() {}\n").expect("a");
    fs::write(root.join("crates/b/src/lib.rs"), "fn b() {}\n").expect("b");
    fs::write(root.join("crates/b/src/readme.md"), "notes\n").expect("notes");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let temp = fs::make_temp_file("effigy-rhai-test");
            if !fs::is_file(temp) { throw("temp file"); }
            let all = fs::list_recursive("crates");
            if all.len() != 3 { throw("all"); }
            let rust = fs::list_recursive("crates", #{ extension: "rs" });
            if rust.len() != 2 { throw("rust"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_return_env_file_maps_and_parse_ints() {
    let root = temp_root("env-map-parse-int");
    fs::write(
        root.join("app.env"),
        "DATABASE_URL=postgres://local\nPORT=5432\n",
    )
    .expect("env");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let env_map = fs::env_file_map("app.env");
            if env_map["DATABASE_URL"] != "postgres://local" { throw("database url"); }
            if str::parse_int(env_map["PORT"]) != 5432 { throw("port"); }
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
fn execute_rhai_script_can_match_replace_and_escape_regex() {
    let root = temp_root("regex-surface");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            if !regex::is_match("^src/runner/.+\\.rs$", "src/runner/demo.rs") { throw("match"); }
            if regex::is_match("^src/runner/.+\\.rs$", "docs/demo.md") { throw("negative"); }
            let replaced = regex::replace("[0-9]+", "version-123", "456");
            if replaced != "version-456" { throw("replace"); }
            let escaped = regex::escape("a+b");
            if escaped != "a\\+b" { throw("escape"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn regex_surface_catalog_matches_live_host_argument_order() {
    use crate::surface::{
        rendered_signature, rhai_surface_functions, REGEX_PATTERN_FIRST_SIGNATURES,
    };

    assert_eq!(
        REGEX_PATTERN_FIRST_SIGNATURES
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        vec!["is_match", "replace", "captures"],
        "regex pattern-first signature table must stay name-aligned with catalog indexes"
    );

    let functions = rhai_surface_functions();
    for (name, expected_signature) in REGEX_PATTERN_FIRST_SIGNATURES {
        let function = functions
            .iter()
            .find(|entry| entry.module == "regex" && entry.name == *name)
            .unwrap_or_else(|| panic!("missing regex::{name} in surface catalog"));
        assert_eq!(
            rendered_signature(function),
            *expected_signature,
            "catalog signature for regex::{name} drifted from the live pattern-first host order"
        );
        assert_eq!(
            function.signature, *expected_signature,
            "raw catalog signature for regex::{name} drifted from REGEX_PATTERN_FIRST_SIGNATURES"
        );
    }

    let root = temp_root("regex-catalog-order");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    // Pattern-first is the registered order. Value-first looks like a silent
    // no-op that returns the pattern string — do not accept both.
    let script = r#"
            let pattern_first = regex::replace("[0-9]+", "version-123", "456");
            if pattern_first != "version-456" { throw("pattern-first host order"); }

            let value_first = regex::replace("version-123", "[0-9]+", "456");
            if value_first != "[0-9]+" { throw("value-first must not silently rewrite"); }
            if value_first == "version-456" { throw("must not accept both argument orders"); }

            if !regex::is_match("^v[0-9]+$", "v12") { throw("is_match pattern-first"); }
            if regex::is_match("v12", "^v[0-9]+$") { throw("is_match value-first must not match"); }

            let captures = regex::captures("(?<n>[0-9]+)", "id-9");
            if !captures["matched"] || captures["named"]["n"] != "9" { throw("captures pattern-first"); }
            let swapped = regex::captures("id-9", "(?<n>[0-9]+)");
            if swapped["matched"] { throw("captures value-first must not match"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_capture_regex_groups_and_write_structured_files() {
    let root = temp_root("regex-captures-and-structured-files");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let dsn = "mysql://root:secret@db:3306/acowtancy";
            let captures = regex::captures(
                "^(?<scheme>[a-z0-9]+)://(?<user>[^:]+):(?<pass>[^@]+)@(?<host>[^:/]+):(?<port>[0-9]+)/(?<database>.+)$",
                dsn
            );
            if !captures["matched"] { throw("matched"); }
            if captures["groups"][1] != "mysql" { throw("group"); }
            if captures["named"]["host"] != "db" { throw("host"); }
            if captures["named"]["database"] != "acowtancy" { throw("database"); }

            let payload = #{
                dsn: dsn,
                host: captures["named"]["host"],
                port: str::parse_int(captures["named"]["port"])
            };
            json::write_file("tmp/payload.json", payload);
            let roundtrip_json = json::read_file("tmp/payload.json");
            if roundtrip_json["host"] != "db" { throw("json host"); }
            if json::stringify_compact(roundtrip_json) != "{\"dsn\":\"mysql://root:secret@db:3306/acowtancy\",\"host\":\"db\",\"port\":3306}" {
                throw("compact json");
            }

            let manifest = #{
                bundle: #{ host: "acowtancy.legacy.test" },
                tasks: #{ sync_task: #{ task: "defer migrate/media https://www.acowtancy.com" } }
            };
            toml::write_file("tmp/manifest.toml", manifest);
            let roundtrip_toml = toml::read_file("tmp/manifest.toml");
            if roundtrip_toml["bundle"]["host"] != "acowtancy.legacy.test" { throw("toml host"); }
            if roundtrip_toml["tasks"]["sync_task"]["task"] != "defer migrate/media https://www.acowtancy.com" {
                throw("toml task");
            }

            let blueprint = #{
                services: [
                    #{
                        name: "api",
                        envVars: [#{ key: "DATABASE_URL", "sync": false }]
                    }
                ]
            };
            yaml::write_file("tmp/blueprint.yaml", blueprint);
            let roundtrip_yaml = yaml::read_file("tmp/blueprint.yaml");
            if roundtrip_yaml["services"][0]["name"] != "api" { throw("yaml service"); }
            if roundtrip_yaml["services"][0]["envVars"][0]["sync"] != false { throw("yaml sync"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
    let json_payload = fs::read_to_string(root.join("tmp/payload.json")).expect("json payload");
    assert!(json_payload.contains("\"host\": \"db\""));
    let toml_payload = fs::read_to_string(root.join("tmp/manifest.toml")).expect("toml payload");
    assert!(toml_payload.contains("[bundle]"));
    assert!(toml_payload.contains("task = \"defer migrate/media https://www.acowtancy.com\""));
    let yaml_payload = fs::read_to_string(root.join("tmp/blueprint.yaml")).expect("yaml payload");
    assert!(yaml_payload.contains("services:"));
    assert!(yaml_payload.contains("envVars:"));
}

#[test]
fn execute_rhai_script_can_parse_urls_and_mysql_dsns() {
    let root = temp_root("url-and-dsn-surface");
    let context = script_context(&root);
    let script = r#"
            let parsed = url::parse("https://user:pass@example.test:8443/a/b?mode=fast&debug=1#frag");
            if parsed["scheme"] != "https" { throw("scheme"); }
            if parsed["username"] != "user" { throw("username"); }
            if parsed["password"] != "pass" { throw("password"); }
            if parsed["host"] != "example.test" { throw("host"); }
            if parsed["port"] != 8443 { throw("port"); }
            if parsed["path"] != "/a/b" { throw("path"); }
            if parsed["path_segments"][0] != "a" || parsed["path_segments"][1] != "b" { throw("segments"); }
            if parsed["query"]["mode"] != "fast" || parsed["query"]["debug"] != "1" { throw("query"); }
            if parsed["fragment"] != "frag" { throw("fragment"); }
            if url::query_get("https://user:pass@example.test:8443/a/b?mode=fast&debug=1#frag", "mode") != "fast" {
                throw("query_get");
            }
            if url::query_get("https://user:pass@example.test:8443/a/b?mode=fast&debug=1#frag", "missing") != "" {
                throw("query_get missing");
            }

            let dsn = url::parse_mysql_dsn("mysql://root:secret@db:3306/acowtancy?charset=utf8mb4");
            if dsn["scheme"] != "mysql" { throw("dsn scheme"); }
            if dsn["username"] != "root" { throw("dsn username"); }
            if dsn["password"] != "secret" { throw("dsn password"); }
            if dsn["host"] != "db" { throw("dsn host"); }
            if dsn["port"] != 3306 { throw("dsn port"); }
            if dsn["database"] != "acowtancy" { throw("dsn database"); }
            if dsn["query"]["charset"] != "utf8mb4" { throw("dsn query"); }

            let pg = url::parse_pg_dsn("postgres://postgres:secret@db:5432/acowtancy?sslmode=disable");
            if pg["scheme"] != "postgres" { throw("pg scheme"); }
            if pg["username"] != "postgres" { throw("pg username"); }
            if pg["password"] != "secret" { throw("pg password"); }
            if pg["host"] != "db" { throw("pg host"); }
            if pg["port"] != 5432 { throw("pg port"); }
            if pg["database"] != "acowtancy" { throw("pg database"); }
            if pg["query"]["sslmode"] != "disable" { throw("pg query"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_can_capture_http_status_and_body_to_file() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buffer = [0; 1024];
        let _ = stream.read(&mut buffer).expect("read request");
        stream
            .write_all(
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: 11\r\n\r\nforced boom",
            )
            .expect("write response");
    });
    let root = temp_root("http-capture");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = format!(
        r#"
            let response = http::capture("POST", "http://{addr}/smoke", "tmp/response.txt", #{{}});
            if response["status"] != 500 {{ throw("status"); }}
            if response["success"] != false {{ throw("success"); }}
            if response["body"] != "forced boom" {{ throw("body"); }}
            if fs::read_file("tmp/response.txt") != "forced boom" {{ throw("file"); }}
        "#
    );

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
    server.join().expect("server");
}
