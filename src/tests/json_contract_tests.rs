use super::{
    run_doctor, run_manifest_task_with_cwd, run_tasks, DoctorArgs, RunnerError, TasksArgs,
};
use crate::TaskInvocation;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn parse_json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("parse json")
}

fn assert_schema_v1(parsed: &serde_json::Value, schema: &str) {
    assert_eq!(parsed["schema"], schema);
    assert_eq!(parsed["schema_version"], 1);
}

fn run_invocation_json(root: PathBuf, name: &str, args: &[&str]) -> serde_json::Value {
    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: name.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        },
        root,
    )
    .expect("run invocation");
    parse_json(&out)
}

fn run_completion_candidates_json(root: PathBuf) -> serde_json::Value {
    run_invocation_json(root, "completion", &["candidates", "--json"])
}

fn assert_candidates_cache_policy(
    parsed: &serde_json::Value,
    hit: bool,
    state: &str,
    effective_ttl_ms: i64,
    ttl_source: &str,
) {
    assert_eq!(parsed["cache_hit"], hit);
    assert_eq!(parsed["cache_state"], state);
    assert_eq!(parsed["effective_cache_ttl_ms"], effective_ttl_ms);
    assert_eq!(parsed["cache_ttl_source"], ttl_source);
}

#[test]
fn tasks_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["managed_profiles"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn tasks_filtered_json_contract_has_versioned_shape_and_filter_fields() {
    let root = temp_workspace("tasks-filtered-json-contract");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["notes"].is_array());
}

#[test]
fn tasks_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-json-contract-resolve");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: Some("farmyard/api".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "api");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "workspace");
    assert_eq!(parsed["resolve"]["lock_scopes"][1], "task:api");
}

#[test]
fn tasks_filtered_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-filtered-json-contract-resolve");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf build\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("build".to_owned()),
            resolve_selector: Some("farmyard/build".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "build");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "build");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "workspace");
    assert_eq!(parsed["resolve"]["lock_scopes"][1], "task:build");
}

#[test]
fn doctor_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("doctor-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ok]\nrun = \"printf ok\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    })
    .expect("run doctor json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["summary"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["fixes"].is_array());
    assert!(parsed["root_resolution"].is_object());
}

#[test]
fn doctor_json_contract_with_health_stdout_remains_valid_json() {
    let root = temp_workspace("doctor-json-contract-health-stdout");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf healthy\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    })
    .expect("run doctor json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["findings"].is_array());
}

#[test]
fn doctor_explain_json_contract_has_selection_and_deferral_fields() {
    let root = temp_workspace("doctor-explain-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "farmyard/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("run doctor explain json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.explain.v1");
    assert_eq!(parsed["request"]["task"], "farmyard/build");
    assert!(parsed["request"]["args"].is_array());
    assert_eq!(parsed["selection"]["status"], "ok");
    assert!(parsed["selection"]["evidence"].is_array());
    assert!(parsed["candidates"].is_array());
    assert!(parsed["deferral"]["considered"].is_boolean());
    assert!(parsed["deferral"]["selected"].is_boolean());
    assert!(parsed["reasoning"]["selection"].is_string());
    assert!(parsed["reasoning"]["deferral"].is_string());
}

#[test]
fn doctor_explain_json_snapshot_prefix_is_stable() {
    let root = temp_workspace("doctor-explain-json-snapshot");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "farmyard/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("run doctor explain json");

    let parsed = parse_json(&out);
    let keys = parsed
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    assert_eq!(
        keys,
        vec![
            "ambiguity_candidates".to_owned(),
            "candidates".to_owned(),
            "deferral".to_owned(),
            "reasoning".to_owned(),
            "request".to_owned(),
            "root_resolution".to_owned(),
            "schema".to_owned(),
            "schema_version".to_owned(),
            "selection".to_owned(),
        ]
    );
    assert_schema_v1(&parsed, "effigy.doctor.explain.v1");
    assert_eq!(parsed["request"]["task"], "farmyard/build");
    assert_eq!(
        parsed["reasoning"]["selection"],
        "selected catalog by explicit task prefix"
    );
}

#[test]
fn builtin_test_plan_json_contract_has_versioned_shape_and_suite_source_fields() {
    let root = temp_workspace("test-plan-json-contract");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "--json".to_owned()],
        },
        root,
    )
    .expect("run test --plan --json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.test.plan.v1");
    assert!(parsed["targets"].is_array());
    let first = parsed["targets"]
        .as_array()
        .and_then(|targets| targets.first())
        .expect("target entry");
    assert!(first["suite_source"].is_string());
    assert!(first["available_suites"].is_array());
    assert!(first["fallback_chain"].is_array());
}

#[test]
fn builtin_test_results_json_contract_has_versioned_shape_and_hint_fields() {
    let root = temp_workspace("test-results-json-contract");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec![
                "--json".to_owned(),
                "vitest".to_owned(),
                "user-service".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero test failure");

    let rendered = match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.test.results.v1");
    assert!(parsed["targets"].is_array());
    assert!(parsed["failures"].is_array());
    assert!(parsed["hint"].is_object());
    assert_eq!(
        parsed["hint"]["kind"],
        serde_json::Value::String("selected-suite-filter-no-match".to_owned())
    );
}

#[test]
fn builtin_help_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(temp_workspace("help-json-contract"), "help", &["--json"]);
    assert_schema_v1(&parsed, "effigy.help.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["topic"], "general");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn builtin_config_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(
        temp_workspace("config-json-contract"),
        "config",
        &["--json"],
    );
    assert_schema_v1(&parsed, "effigy.config.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "reference");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("effigy.toml Reference")));
}

#[test]
fn builtin_completion_json_contract_has_versioned_shape() {
    let root = temp_workspace("completion-json-contract");
    write_manifest(&root.join("effigy.toml"), "");
    let parsed = run_invocation_json(root, "completion", &["bash", "--json"]);
    assert_schema_v1(&parsed, "effigy.completion.v1");
    assert_eq!(parsed["shell"], "bash");
    assert!(parsed["script"].is_string());
    assert!(parsed["commands"].is_array());
}

#[test]
fn builtin_completion_candidates_json_contract_has_versioned_shape() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let parsed = run_invocation_json(
        root,
        "completion",
        &["candidates", "--prefix", "farm", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.completion.candidates.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["prefix"], "farm");
    assert_candidates_cache_policy(&parsed, false, "miss_initial", 2000, "default");
    assert_eq!(parsed["manifest_count"], 2);
    assert!(parsed["cache_age_ms"].is_null());
    assert!(parsed["cache_ttl_ms"].is_null());
    let candidates = parsed["candidates"].as_array().expect("candidates array");
    assert!(candidates
        .iter()
        .any(|value| value.as_str() == Some("farmyard/api")));
}

#[test]
fn builtin_completion_candidates_json_contract_reports_cache_hit_on_unchanged_rerun() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-cache-hit");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );

    let first_parsed = run_completion_candidates_json(root.clone());
    assert_eq!(first_parsed["schema"], "effigy.completion.candidates.v1");
    assert_candidates_cache_policy(&first_parsed, false, "miss_initial", 2000, "default");
    assert!(first_parsed["cache_age_ms"].is_null());
    assert!(first_parsed["cache_ttl_ms"].is_null());
    let effective_ttl_ms = first_parsed["effective_cache_ttl_ms"]
        .as_u64()
        .expect("effective cache ttl must be numeric");
    assert!(effective_ttl_ms >= 100);
    assert!(effective_ttl_ms <= 30_000);
    let ttl_source = first_parsed["cache_ttl_source"]
        .as_str()
        .expect("cache_ttl_source must be string");
    assert!(matches!(ttl_source, "default" | "env"));

    let second_parsed = run_completion_candidates_json(root);
    assert_eq!(second_parsed["cache_hit"], true);
    assert_eq!(second_parsed["cache_state"], "hit");
    assert_eq!(second_parsed["manifest_count"], 1);
    let cache_age_ms = second_parsed["cache_age_ms"]
        .as_u64()
        .expect("cache_age_ms must be numeric on hit");
    assert!(cache_age_ms <= effective_ttl_ms);
    assert_eq!(
        second_parsed["cache_ttl_ms"].as_u64(),
        Some(effective_ttl_ms)
    );
    assert_eq!(
        second_parsed["effective_cache_ttl_ms"].as_u64(),
        Some(effective_ttl_ms)
    );
    assert_eq!(second_parsed["cache_ttl_source"].as_str(), Some(ttl_source));
}

#[test]
fn builtin_completion_candidates_json_contract_expires_cache_after_ttl() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-cache-ttl-expiry");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );

    let first_parsed = run_completion_candidates_json(root.clone());
    assert_candidates_cache_policy(&first_parsed, false, "miss_initial", 2000, "default");
    assert!(first_parsed["cache_age_ms"].is_null());
    assert!(first_parsed["cache_ttl_ms"].is_null());

    thread::sleep(Duration::from_millis(2200));

    let second_parsed = run_completion_candidates_json(root);
    assert_candidates_cache_policy(&second_parsed, false, "miss_ttl", 2000, "default");
    assert!(second_parsed["cache_age_ms"].is_null());
    assert!(second_parsed["cache_ttl_ms"].is_null());
}

#[test]
fn builtin_completion_candidates_json_contract_invalidates_cache_on_manifest_change_with_preserved_mtime(
) {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-mtime-invalidation");
    let manifest_path = root.join("effigy.toml");
    write_manifest(&manifest_path, "[tasks.build]\nrun = \"printf root\"\n");

    let first_parsed = run_completion_candidates_json(root.clone());
    assert_candidates_cache_policy(&first_parsed, false, "miss_initial", 2000, "default");
    assert!(first_parsed["cache_age_ms"].is_null());
    assert!(first_parsed["cache_ttl_ms"].is_null());

    let original_modified = fs::metadata(&manifest_path)
        .expect("manifest metadata")
        .modified()
        .expect("manifest modified");
    write_manifest(
        &manifest_path,
        "[tasks.build]\nrun = \"printf root\"\n[tasks.deploy]\nrun = \"printf deploy\"\n",
    );
    let manifest_file = fs::OpenOptions::new()
        .write(true)
        .open(&manifest_path)
        .expect("open manifest for timestamp reset");
    manifest_file
        .set_times(fs::FileTimes::new().set_modified(original_modified))
        .expect("restore manifest modified time");

    let second_parsed = run_completion_candidates_json(root);
    assert_candidates_cache_policy(
        &second_parsed,
        false,
        "miss_manifest_change",
        2000,
        "default",
    );
    assert!(second_parsed["cache_age_ms"].is_null());
    assert!(second_parsed["cache_ttl_ms"].is_null());
    assert!(second_parsed["candidates"]
        .as_array()
        .expect("candidates array")
        .iter()
        .any(|value| value.as_str() == Some("deploy")));
}

#[test]
fn builtin_completion_candidates_json_contract_reports_env_ttl_policy() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[(
        "EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS",
        Some("750".to_owned()),
    )]);
    let root = temp_workspace("completion-candidates-ttl-env-policy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );

    let first_parsed = run_completion_candidates_json(root.clone());
    assert_candidates_cache_policy(&first_parsed, false, "miss_initial", 750, "env");
    assert!(first_parsed["cache_age_ms"].is_null());
    assert!(first_parsed["cache_ttl_ms"].is_null());

    let second_parsed = run_completion_candidates_json(root);
    assert_eq!(second_parsed["cache_hit"], true);
    assert_eq!(second_parsed["cache_state"], "hit");
    assert_eq!(second_parsed["effective_cache_ttl_ms"], 750);
    assert_eq!(second_parsed["cache_ttl_source"], "env");
    assert_eq!(second_parsed["cache_ttl_ms"], 750);
}

#[test]
fn builtin_completion_candidates_json_contract_reports_invalid_env_ttl_policy() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[(
        "EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS",
        Some("not-a-number".to_owned()),
    )]);
    let root = temp_workspace("completion-candidates-ttl-env-invalid-policy");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );

    let parsed = run_completion_candidates_json(root);
    assert_candidates_cache_policy(&parsed, false, "miss_initial", 2000, "env_invalid");
    assert!(parsed["cache_age_ms"].is_null());
    assert!(parsed["cache_ttl_ms"].is_null());
}

#[test]
fn builtin_completion_candidates_text_includes_builtin_and_task_selectors() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-text-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["candidates".to_owned()],
        },
        root,
    )
    .expect("run completion candidates");

    assert!(out.lines().any(|line| line == "help"));
    assert!(out.lines().any(|line| line == "build"));
    assert!(out.lines().any(|line| line == "farmyard/api"));
}

#[test]
fn builtin_completion_bash_script_uses_dynamic_candidates_probe() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-bash-dynamic-candidates");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["bash".to_owned()],
        },
        root,
    )
    .expect("run completion bash");

    assert!(out.contains("effigy completion candidates --prefix \"$cur\""));
}

#[test]
fn builtin_completion_candidates_prefix_requires_value() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-prefix-missing");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec!["candidates".to_owned(), "--prefix".to_owned()],
        },
        root,
    )
    .expect_err("completion candidates --prefix should fail without value");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("completion candidates argument --prefix requires a value"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn builtin_completion_candidates_help_json_uses_help_schema() {
    let _guard = test_lock().lock().expect("lock");
    let _env = EnvGuard::set_many(&[("EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS", None)]);
    let root = temp_workspace("completion-candidates-help-json");
    write_manifest(&root.join("effigy.toml"), "");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "completion".to_owned(),
            args: vec![
                "candidates".to_owned(),
                "--help".to_owned(),
                "--json".to_owned(),
            ],
        },
        root,
    )
    .expect("run completion candidates --help --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.help.v1");
    assert_eq!(parsed["topic"], "completion-candidates");
}

#[test]
fn builtin_init_json_contract_has_versioned_shape() {
    let parsed = run_invocation_json(temp_workspace("init-json-contract"), "init", &["--json"]);
    assert_schema_v1(&parsed, "effigy.init.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["written"], true);
    assert_eq!(parsed["dry_run"], false);
    assert!(parsed["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("effigy.toml")));
    assert!(parsed["content"]
        .as_str()
        .is_some_and(|text| text.contains("[tasks]")));
}

#[test]
fn builtin_migrate_json_contract_has_versioned_shape() {
    let root = temp_workspace("migrate-json-contract");
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
    "build": "npm run compile",
    "test": "vitest run"
  }
}
"#,
    )
    .expect("write package scripts");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks]\nbuild = \"printf old\"\n",
    );

    let parsed = run_invocation_json(root, "migrate", &["--json"]);
    assert_schema_v1(&parsed, "effigy.migrate.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["apply"], false);
    assert_eq!(parsed["written"], false);
    assert!(parsed["added"].is_array());
    assert!(parsed["conflicts"].is_array());
}

#[test]
fn builtin_unlock_json_contract_has_versioned_shape() {
    let root = temp_workspace("unlock-json-contract");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/workspace.lock"), "{}").expect("write workspace lock");

    let repo_arg = root.display().to_string();
    let parsed = run_invocation_json(
        root,
        "unlock",
        &["--repo", &repo_arg, "--json", "workspace"],
    );
    assert_schema_v1(&parsed, "effigy.unlock.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["all"], false);
    assert!(parsed["removed"].is_array());
    assert!(parsed["missing"].is_array());
}

#[test]
fn builtin_watch_bounded_json_contract_has_versioned_shape() {
    let root = temp_workspace("watch-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let parsed = run_invocation_json(
        root,
        "watch",
        &["--owner", "effigy", "--once", "--json", "build"],
    );
    assert_schema_v1(&parsed, "effigy.watch.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["runs"], 1);
}

#[test]
fn task_run_json_contract_reclaims_stale_lock_and_remains_valid_payload() {
    let root = temp_workspace("task-run-json-stale-lock-reclaim");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(
        root.join(".effigy/locks/workspace.lock"),
        r#"{"scope":"workspace","pid":999999,"started_at_epoch_ms":0}"#,
    )
    .expect("write stale lock");

    let parsed = run_invocation_json(root, "build", &["--json"]);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["exit_code"], 0);
}

#[test]
fn catalog_task_run_json_contract_success_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-success");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );

    let parsed = run_invocation_json(root, "build", &["--json"]);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["exit_code"], 0);
    assert_eq!(parsed["stdout"], "build-ok");
}

#[test]
fn catalog_task_run_json_contract_failure_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "fail".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect_err("expected non-zero task failure");

    let rendered = match err {
        RunnerError::CommandJsonFailure { rendered } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["task"], "fail");
    assert_eq!(parsed["exit_code"], 9);
    assert_eq!(parsed["stdout"], "fail-out");
    assert_eq!(parsed["stderr"], "fail-err");
}

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-json-contract-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

fn with_cwd<F, T>(cwd: &PathBuf, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = test_lock().lock().expect("lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    original: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
