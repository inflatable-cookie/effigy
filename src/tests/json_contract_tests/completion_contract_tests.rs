use super::prelude::*;

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
