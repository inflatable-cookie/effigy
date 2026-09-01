use super::{
    execute_rhai_script, execute_rhai_script_with_runtime_context,
    execute_rhai_script_with_runtime_context_and_secret_targets, install_stop_requested_flag,
    load_script, load_script_args_from_env, render_host_log_message, resolve_script_path,
    EffigyCommandError, HostCallbacks, HostCommandOutput, RhaiSecretTarget, ScriptContext,
    EFFIGY_RHAI_ARGS_JSON, EFFIGY_RHAI_CATALOG_ROOT, EFFIGY_RHAI_INVOCATION_CWD,
};
use crate::surface::{rhai_feature_descriptors, RhaiFeatureDispatch, FEATURE_NAMES, MODULE_NAMES};
use effigy_secrets::{SecretValue, VaultEnvelope, VaultPlaintextPayload, VaultSecretRecord};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

mod engine_limits;
mod forge;
mod git;
mod host_surface;
mod prompt;
mod runtime;
mod script_policy;
mod secrets;
mod semver;
mod storage;
mod utility;

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
        container_exec_stream: Arc::new(|_, name, service, command| {
            Ok(HostCommandOutput {
                status: 0,
                success: true,
                stdout: format!(
                    "stream:{name}:{}:{}",
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
        "crates/effigy-manifest/tests/fixtures/workspace-app-bundle/scripts/dev/ui-setup.rhai"
        | "external/bundles/workspace-app/scripts/dev/ui-setup.rhai"
        | "external/bundles/underlay/scripts/dev/ui-setup.rhai" => {
            contents.contains("process::stream(\"sh\", [\"-lc\", shell])")
        }
        "external/providers/render/scripts/apply.rhai"
        | "external/providers/render/scripts/preflight.rhai"
        | "external/providers/render/scripts/status.rhai" => {
            contents.contains("process::run(\"curl\",")
                && contents.contains("process::run(\"curl\", [\"--version\"])")
        }
        "scripts/write-json-contract-artifacts.rhai" => contents.contains("process::tee("),
        "scripts/build-local-bin.rhai" => {
            contents.contains("process::stream(program, process_args, options)")
        }
        "scripts/install-local-bin-links.rhai" => contents.contains("process::run("),
        "scripts/benchmark-graph-agent-usage.rhai" => {
            contents.contains("process::run(program, process_args)")
                && contents.contains("process::run(")
        }
        "scripts/benchmark-docs-context.rhai" => {
            contents.contains("process::run(program, process_args)")
        }
        "scripts/profile-container-shell-matrix.rhai" => {
            contents.contains("process::run(program, process_args)")
                && contents.contains("process::run(\"/bin/sh\",")
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
fn rhai_surface_feature_descriptors_cover_feature_names() {
    let names = FEATURE_NAMES.iter().copied().collect::<BTreeSet<_>>();
    let descriptor_names = rhai_feature_descriptors()
        .iter()
        .map(|descriptor| descriptor.id)
        .collect::<BTreeSet<_>>();

    assert_eq!(descriptor_names, names);

    for descriptor in rhai_feature_descriptors() {
        assert!(!descriptor.module().is_empty());
        assert!(!descriptor.function().is_empty());
        assert!(!descriptor.option_style.is_empty());
        assert!(!descriptor.safety.is_empty());
        assert!(matches!(
            descriptor.dispatch,
            RhaiFeatureDispatch::Runner | RhaiFeatureDispatch::HostHandled
        ));
    }
}

#[test]
fn load_script_reads_relative_path_from_cwd() {
    let root = temp_root("load-script");
    let script_path = root.join("scripts/test.rhai");
    fs::create_dir_all(script_path.parent().expect("scripts dir")).expect("scripts dir");
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
