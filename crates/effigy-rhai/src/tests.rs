use super::{
    execute_rhai_script, execute_rhai_script_with_runtime_context,
    execute_rhai_script_with_runtime_context_and_secret_targets, install_stop_requested_flag,
    load_script, load_script_args_from_env, render_host_log_message, resolve_script_path,
    EffigyCommandError, HostCallbacks, HostCommandOutput, RhaiSecretTarget, ScriptContext,
    EFFIGY_RHAI_ARGS_JSON, EFFIGY_RHAI_CATALOG_ROOT, EFFIGY_RHAI_INVOCATION_CWD,
};
use crate::surface::{FEATURE_NAMES, MODULE_NAMES};
use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

fn script_context(root: &Path) -> ScriptContext {
    ScriptContext {
        cwd: root.to_path_buf(),
        repo_root: root.to_path_buf(),
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    }
}

struct ScopedTestEnv {
    previous: Vec<(String, Option<String>)>,
}

impl ScopedTestEnv {
    fn set_many(values: &[(&str, String)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, value)| {
                let key = (*key).to_owned();
                let previous = std::env::var(&key).ok();
                std::env::set_var(&key, value);
                (key, previous)
            })
            .collect();
        Self { previous }
    }
}

impl Drop for ScopedTestEnv {
    fn drop(&mut self) {
        for (key, previous) in self.previous.drain(..) {
            if let Some(previous) = previous {
                std::env::set_var(key, previous);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[test]
fn execute_rhai_script_exposes_declared_rhai_secrets() {
    let root = temp_root("rhai-secret-present");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let marker = root.join("secret.out");
    let script = format!(
        r#"
            if !effigy::has_secret("api_token") {{ throw("missing"); }}
            let token = effigy::secret("api_token");
            fs::write_file("{}", token);
        "#,
        marker.display()
    );

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "tok_secret");
}

#[test]
fn execute_rhai_script_blocks_missing_required_rhai_secret_before_side_effects() {
    let root = temp_root("rhai-secret-missing-required");
    let marker = root.join("should-not-run.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let script = format!(r#"fs::write_file("{}", "ran");"#, marker.display());

    let error = execute_rhai_script(&script_context(&root), &script, &[], &callbacks())
        .expect_err("script should fail");

    assert!(error
        .to_string()
        .contains("required Rhai secret(s) missing from the vault"));
    assert!(
        !marker.exists(),
        "script should not run after preflight blocker"
    );
}

#[test]
fn execute_rhai_script_rejects_undeclared_and_wrong_target_secret_reads() {
    let root = temp_root("rhai-secret-wrong-target");
    write_rhai_secret_manifest(&root, r#"targets = ["tasks"]"#);

    let wrong_target = execute_rhai_script(
        &script_context(&root),
        r#"effigy::secret("api_token");"#,
        &[],
        &callbacks(),
    )
    .expect_err("wrong target should fail");
    assert!(wrong_target
        .to_string()
        .contains("not declared for the `rhai` target"));

    let undeclared = execute_rhai_script(
        &script_context(&root),
        r#"effigy::has_secret("missing");"#,
        &[],
        &callbacks(),
    )
    .expect_err("undeclared should fail");
    assert!(undeclared
        .to_string()
        .contains("is not declared under `[secrets.keys]`"));
}

#[test]
fn execute_rhai_script_redacts_secret_values_from_errors() {
    let root = temp_root("rhai-secret-error-redaction");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    let error = execute_rhai_script(
        &script_context(&root),
        r#"throw(effigy::secret("api_token"));"#,
        &[],
        &callbacks(),
    )
    .expect_err("script should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("[REDACTED]"), "got: {rendered}");
    assert!(
        !rendered.contains("tok_secret"),
        "secret leaked: {rendered}"
    );
}

#[test]
fn execute_rhai_script_can_use_deploy_target_secret_when_allowed() {
    let root = temp_root("rhai-secret-deploy-target");
    write_rhai_secret_manifest(&root, r#"targets = ["deploy"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "deploy_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let marker = root.join("deploy-secret.out");
    let script = format!(
        r#"
            if !effigy::has_secret("api_token") {{ throw("missing"); }}
            fs::write_file("{}", effigy::secret("api_token"));
        "#,
        marker.display()
    );

    execute_rhai_script_with_runtime_context_and_secret_targets(
        &script_context(&root),
        None,
        &script,
        &[],
        &callbacks(),
        &[RhaiSecretTarget::Deploy],
    )
    .expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "deploy_secret");
}

fn write_rhai_secret_manifest(root: &Path, target_line: &str) {
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.api_token]
required = true
{target_line}
"#
        ),
    )
    .expect("write manifest");
}

fn write_test_vault(root: &Path, passphrase: &str, records: &[(&str, &str)]) {
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let envelope = payload
        .encrypt_with_passphrase(passphrase)
        .expect("encrypt test vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        vault_path,
        envelope.to_json_pretty().expect("serialize test vault"),
    )
    .expect("write test vault");
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
fn execute_rhai_script_exposes_extended_rhai_feature_surface() {
    let root = temp_root("extended-surface");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let user_path = config::user_path();
            if user_path["feature"] != "config.user_path" { throw("config user path"); }

            let user_get = config::user_get("containers.backend");
            if user_get["feature"] != "config.user_get" || user_get["options"]["key"] != "containers.backend" { throw("config user get"); }

            let user_set = config::user_set("containers.backend", "containerd");
            if user_set["feature"] != "config.user_set" || user_set["options"]["value"] != "containerd" { throw("config user set"); }

            let user_unset = config::user_unset("containers.profile");
            if user_unset["feature"] != "config.user_unset" || user_unset["options"]["key"] != "containers.profile" { throw("config user unset"); }

            let state_plan = state::plan("uat");
            if state_plan["feature"] != "state.plan" || state_plan["options"]["stack"] != "uat" { throw("state plan"); }

            let state_apply = state::apply(#{ stack: "uat", yes: true });
            if state_apply["feature"] != "state.apply" || state_apply["options"]["stack"] != "uat" { throw("state apply"); }

            let state_capture = state::capture("uat", "baseline");
            if state_capture["feature"] != "state.capture" || state_capture["options"]["profile"] != "baseline" { throw("state capture"); }

            let state_history = state::history(#{ stack: "uat", limit: 5 });
            if state_history["feature"] != "state.history" || state_history["options"]["limit"] != 5 { throw("state history"); }

            let artifact_inspect = artifact::inspect("oci://ghcr.io/acme/app:seed");
            if artifact_inspect["feature"] != "artifact.inspect" { throw("artifact inspect"); }

            let artifact_stage = artifact::stage("oci://ghcr.io/acme/app:seed", #{ farmyard_handoff: true });
            if artifact_stage["feature"] != "artifact.stage" || artifact_stage["options"]["farmyard_handoff"] != true { throw("artifact stage"); }

            let artifact_capture = artifact::capture(
                "tmp/seed.sql",
                "oci://ghcr.io/acme/app:seed",
                #{ kind: "database", environment_label: "uat", push: true },
            );
            if artifact_capture["feature"] != "artifact.capture" || artifact_capture["options"]["push"] != true { throw("artifact capture"); }

            let cache_list = container::cache_list(#{ global: true, project: "cbs-dev" });
            if cache_list["feature"] != "container.cache_list" || cache_list["options"]["project"] != "cbs-dev" { throw("cache list"); }

            let cache_prune = container::cache_prune(#{ global: true, yes: true, kind: "rust-target" });
            if cache_prune["feature"] != "container.cache_prune" || cache_prune["options"]["kind"] != "rust-target" { throw("cache prune"); }

            let volume_list = container::volume_list(#{ global: true, orphans: true });
            if volume_list["feature"] != "container.volume_list" || volume_list["options"]["orphans"] != true { throw("volume list"); }

            let volume_prune = container::volume_prune(#{ dormant: true, yes: true });
            if volume_prune["feature"] != "container.volume_prune" || volume_prune["options"]["dormant"] != true { throw("volume prune"); }

            let dump = container::data_dump(#{
                name: "web",
                db_dumps: ["main=tmp/main.sql", "tmp/other.sql"],
                push: true,
            });
            if dump["feature"] != "container.data_dump" || dump["options"]["push"] != true { throw("data dump"); }

            let seed = container::data_seed(#{
                db_seeds: ["main=tmp/main.sql"],
                yes: true,
            });
            if seed["feature"] != "container.data_seed" || seed["options"]["db_seeds"][0] != "main=tmp/main.sql" { throw("data seed"); }

            let pull = container::data_pull_production("web");
            if pull["feature"] != "container.data_pull_production" || pull["options"]["name"] != "web" { throw("data pull production"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}

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
        runtime_context.repo_override().unwrap().display(),
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
    fs::create_dir_all(context_path.parent().unwrap()).expect("context dir");
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
            context_path.display().to_string(),
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
        if state::capture_source() != ".effigy/state/captures/new-content.json" { throw("source"); }
        if state::capture_destination_ref() != "oci://ghcr.io/acowtancy/state:new-content" { throw("ref"); }
    "#;

    execute_rhai_script_with_runtime_context(&context, None, script, &[], &callbacks())
        .expect("execute");
}

#[test]
fn execute_rhai_script_exposes_deploy_provider_context_and_report_helpers() {
    let root = temp_root("deploy-provider-context");
    let context_path = root.join(".effigy/runtime/deploy/provider/context.json");
    let report_path = root.join(".effigy/runtime/deploy/provider/report.json");
    fs::create_dir_all(context_path.parent().unwrap()).expect("context dir");
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
fn execute_rhai_script_proves_decodelabs_mysql_seed_uses_container_exec_with_stdin_file() {
    let root = temp_root("decodelabs-mysql-seed");
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
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
    let json_payload = fs::read_to_string(root.join("tmp/payload.json")).expect("json payload");
    assert!(json_payload.contains("\"host\": \"db\""));
    let toml_payload = fs::read_to_string(root.join("tmp/manifest.toml")).expect("toml payload");
    assert!(toml_payload.contains("[bundle]"));
    assert!(toml_payload.contains("task = \"defer migrate/media https://www.acowtancy.com\""));
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

fn allowed_first_party_process_script(relative: &str, contents: &str) -> bool {
    match relative {
        "scripts/write-browser-proof-report.rhai" => {
            contents.contains("process::tee(\"cargo\", process_args)")
        }
        "scripts/check-release-smoke.rhai" => {
            contents.contains("process::run(program, process_args)")
        }
        "scripts/rehearse-linux-release-container.rhai" => {
            contents.contains("process::run(\n    \"colima\",")
        }
        "crates/effigy-catalog/starters/underlay/scripts/dev/ui-setup.rhai"
        | "crates/effigy-manifest/bundles/underlay/scripts/dev/ui-setup.rhai" => {
            contents.contains("process::stream(\"sh\", [\"-lc\", shell])")
        }
        "scripts/build-local-bin.rhai" => {
            contents.contains("process::stream(program, process_args, options)")
        }
        "scripts/install-local-bin-links.rhai" => contents.contains("process::run("),
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
