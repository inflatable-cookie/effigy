use toml::Value;

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::model::{Confidence, FileRecord};
use crate::{ExtractorId, GraphId};

use super::support::{
    index_run_like_raw, index_run_step_raw, manifest_nested_symbol_id, manifest_section_id,
    push_contains_edge, push_resolved_edge, push_symbol, push_unresolved_edge,
};

pub(super) fn index_docs_policy_raw(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    value: Option<&Value>,
) -> Result<(), CodeGraphError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "docs_policy")?;
    push_symbol(
        sink,
        section_id.clone(),
        "docs-policy",
        "docs_policy",
        &format!("{}::docs_policy", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "docs-policy",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &section_id,
        "docs-policy-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(indexes) = table.get("indexes").and_then(Value::as_table) {
        for (index_name, index_value) in indexes {
            let Some(index_table) = index_value.as_table() else {
                continue;
            };
            let index_id = manifest_nested_symbol_id(file, &["docs-policy", "index", index_name])?;
            push_symbol(
                sink,
                index_id.clone(),
                "docs-policy-index",
                index_name,
                &format!("docs_policy::index::{index_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "docs-policy-index",
            );
            push_contains_edge(
                sink,
                &section_id,
                &index_id,
                &format!("docs-policy-index:{index_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            if let Some(path) = index_table.get("file").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &index_id,
                    "docs-policy-file",
                    path,
                    &format!("docs-policy-file:{index_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(path) = index_table.get("dir").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &index_id,
                    "docs-policy-dir",
                    path,
                    &format!("docs-policy-dir:{index_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(section) = index_table.get("section").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &index_id,
                    "docs-policy-section",
                    section,
                    &format!("docs-policy-section:{index_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
        }
    }
    if let Some(next_actions) = table.get("next_actions").and_then(Value::as_table) {
        for (action_name, action_value) in next_actions {
            let Some(action_table) = action_value.as_table() else {
                continue;
            };
            let action_id =
                manifest_nested_symbol_id(file, &["docs-policy", "next-action", action_name])?;
            push_symbol(
                sink,
                action_id.clone(),
                "docs-policy-next-action",
                action_name,
                &format!("docs_policy::next_action::{action_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "docs-policy-next-action",
            );
            push_contains_edge(
                sink,
                &section_id,
                &action_id,
                &format!("docs-policy-next-action:{action_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            for (field, kind) in [
                ("index", "docs-policy-next-action-index"),
                ("heading", "docs-policy-next-action-heading"),
                ("allowlist_file", "docs-policy-next-action-allowlist"),
            ] {
                if let Some(value) = action_table.get(field).and_then(Value::as_str) {
                    push_unresolved_edge(
                        sink,
                        &action_id,
                        kind,
                        value,
                        &format!("{kind}:{action_name}"),
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn index_test_raw(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    value: Option<&Value>,
) -> Result<(), CodeGraphError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "test")?;
    push_symbol(
        sink,
        section_id.clone(),
        "test-config",
        "test",
        &format!("{}::test", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "test-config",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &section_id,
        "test-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(max_parallel) = table.get("max_parallel").and_then(Value::as_integer) {
        push_unresolved_edge(
            sink,
            &section_id,
            "test-max-parallel",
            &max_parallel.to_string(),
            "test-max-parallel",
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(mode) = table.get("cargo_env_match").and_then(Value::as_str) {
        push_unresolved_edge(
            sink,
            &section_id,
            "test-cargo-env-match",
            mode,
            "test-cargo-env-match",
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(runners) = table.get("runners").and_then(Value::as_table) {
        for (runner_name, runner_value) in runners {
            let runner_id = manifest_nested_symbol_id(file, &["test", "runner", runner_name])?;
            push_symbol(
                sink,
                runner_id.clone(),
                "test-runner",
                runner_name,
                &format!("test::runner::{runner_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "test-runner",
            );
            push_contains_edge(
                sink,
                &section_id,
                &runner_id,
                &format!("test-runner:{runner_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            match runner_value {
                Value::String(command) => push_unresolved_edge(
                    sink,
                    &runner_id,
                    "test-runner-command",
                    command,
                    &format!("test-runner-command:{runner_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?,
                Value::Table(runner_table) => {
                    if let Some(command) = runner_table.get("command").and_then(Value::as_str) {
                        push_unresolved_edge(
                            sink,
                            &runner_id,
                            "test-runner-command",
                            command,
                            &format!("test-runner-command:{runner_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(suites) = table.get("suites").and_then(Value::as_table) {
        for (suite_name, suite_value) in suites {
            let suite_id = manifest_nested_symbol_id(file, &["test", "suite", suite_name])?;
            push_symbol(
                sink,
                suite_id.clone(),
                "test-suite",
                suite_name,
                &format!("test::suite::{suite_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "test-suite",
            );
            push_contains_edge(
                sink,
                &section_id,
                &suite_id,
                &format!("test-suite:{suite_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            match suite_value {
                Value::String(command) => push_unresolved_edge(
                    sink,
                    &suite_id,
                    "test-suite-command",
                    command,
                    &format!("test-suite-command:{suite_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?,
                Value::Table(suite_table) => {
                    if let Some(run) = suite_table.get("run").and_then(Value::as_str) {
                        push_unresolved_edge(
                            sink,
                            &suite_id,
                            "test-suite-command",
                            run,
                            &format!("test-suite-command:{suite_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                    if let Some(env_file) = suite_table.get("env_file").and_then(Value::as_str) {
                        push_unresolved_edge(
                            sink,
                            &suite_id,
                            "test-suite-env-file",
                            env_file,
                            &format!("test-suite-env-file:{suite_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                    if let Some(policy) = suite_table.get("teardown_policy").and_then(Value::as_str)
                    {
                        push_unresolved_edge(
                            sink,
                            &suite_id,
                            "test-suite-teardown-policy",
                            policy,
                            &format!("test-suite-teardown-policy:{suite_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                    if let Some(setup) = suite_table.get("setup").and_then(Value::as_array) {
                        for (index, step) in setup.iter().enumerate() {
                            index_run_step_raw(
                                file,
                                &suite_id,
                                sink,
                                extractor_id,
                                extractor_version,
                                &format!("test-suite-setup:{suite_name}:{index}"),
                                step,
                            )?;
                        }
                    }
                    if let Some(teardown) = suite_table.get("teardown").and_then(Value::as_array) {
                        for (index, step) in teardown.iter().enumerate() {
                            index_run_step_raw(
                                file,
                                &suite_id,
                                sink,
                                extractor_id,
                                extractor_version,
                                &format!("test-suite-teardown:{suite_name}:{index}"),
                                step,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(super) fn index_secrets_raw(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    value: Option<&Value>,
) -> Result<(), CodeGraphError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "secrets")?;
    push_symbol(
        sink,
        section_id.clone(),
        "secrets",
        "secrets",
        &format!("{}::secrets", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "secrets",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &section_id,
        "secrets-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(backend) = table.get("backend").and_then(Value::as_str) {
        push_unresolved_edge(
            sink,
            &section_id,
            "secrets-backend",
            backend,
            "secrets-backend",
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(vault) = table.get("vault").and_then(Value::as_table) {
        if let Some(path) = vault.get("path").and_then(Value::as_str) {
            push_unresolved_edge(
                sink,
                &section_id,
                "secrets-vault-path",
                path,
                "secrets-vault-path",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        for (field, kind) in [
            ("identity", "secrets-vault-identity"),
            ("unlock", "secrets-vault-unlock"),
        ] {
            if let Some(value) = vault.get(field).and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &section_id,
                    kind,
                    value,
                    kind,
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
        }
        if let Some(generate) = vault.get("generate") {
            index_run_step_raw(
                file,
                &section_id,
                sink,
                extractor_id,
                extractor_version,
                "secrets-vault-generate",
                generate,
            )?;
        }
    }
    if let Some(external) = table.get("external").and_then(Value::as_table) {
        if let Some(adapter) = external.get("adapter").and_then(Value::as_str) {
            push_unresolved_edge(
                sink,
                &section_id,
                "secrets-external-adapter",
                adapter,
                "secrets-external-adapter",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    if let Some(keys) = table.get("keys").and_then(Value::as_table) {
        for (key_name, key_value) in keys {
            let Some(key_table) = key_value.as_table() else {
                continue;
            };
            let key_id = manifest_nested_symbol_id(file, &["secrets", "key", key_name])?;
            push_symbol(
                sink,
                key_id.clone(),
                "secret-key",
                key_name,
                &format!("secrets::key::{key_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "secret-key",
            );
            push_contains_edge(
                sink,
                &section_id,
                &key_id,
                &format!("secret-key:{key_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            if let Some(required) = key_table.get("required").and_then(Value::as_bool) {
                push_unresolved_edge(
                    sink,
                    &key_id,
                    "secret-key-required",
                    if required { "true" } else { "false" },
                    &format!("secret-key-required:{key_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(description) = key_table.get("description").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &key_id,
                    "secret-key-description",
                    description,
                    &format!("secret-key-description:{key_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Syntactic,
                )?;
            }
            if let Some(targets) = key_table.get("targets").and_then(Value::as_array) {
                for (index, target) in targets.iter().filter_map(Value::as_str).enumerate() {
                    push_unresolved_edge(
                        sink,
                        &key_id,
                        "secret-key-target",
                        target,
                        &format!("secret-key-target:{key_name}:{index}"),
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn index_deploy_raw(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    value: Option<&Value>,
) -> Result<(), CodeGraphError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "deploy")?;
    push_symbol(
        sink,
        section_id.clone(),
        "deploy",
        "deploy",
        &format!("{}::deploy", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "deploy",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &section_id,
        "deploy-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    let mut provider_ids = std::collections::BTreeMap::new();
    if let Some(providers) = table.get("providers").and_then(Value::as_table) {
        for (provider_name, provider_value) in providers {
            let Some(provider_table) = provider_value.as_table() else {
                continue;
            };
            let provider_id =
                manifest_nested_symbol_id(file, &["deploy", "provider", provider_name])?;
            provider_ids.insert(provider_name.clone(), provider_id.clone());
            push_symbol(
                sink,
                provider_id.clone(),
                "deploy-provider",
                provider_name,
                &format!("deploy::provider::{provider_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "deploy-provider",
            );
            push_contains_edge(
                sink,
                &section_id,
                &provider_id,
                &format!("deploy-provider:{provider_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            if let Some(source) = provider_table.get("source").and_then(Value::as_table) {
                if let Some(source_type) = source.get("type").and_then(Value::as_str) {
                    match source_type {
                        "path" => {
                            if let Some(dir) = source.get("dir").and_then(Value::as_str) {
                                push_unresolved_edge(
                                    sink,
                                    &provider_id,
                                    "deploy-provider-source-path",
                                    dir,
                                    &format!("deploy-provider-source-path:{provider_name}"),
                                    file,
                                    extractor_id,
                                    extractor_version,
                                    Confidence::Exact,
                                )?;
                            }
                        }
                        "git" => {
                            if let Some(url) = source.get("url").and_then(Value::as_str) {
                                push_unresolved_edge(
                                    sink,
                                    &provider_id,
                                    "deploy-provider-source-git",
                                    url,
                                    &format!("deploy-provider-source-git:{provider_name}"),
                                    file,
                                    extractor_id,
                                    extractor_version,
                                    Confidence::Exact,
                                )?;
                            }
                            if let Some(reference) = source.get("ref").and_then(Value::as_str) {
                                push_unresolved_edge(
                                    sink,
                                    &provider_id,
                                    "deploy-provider-source-ref",
                                    reference,
                                    &format!("deploy-provider-source-ref:{provider_name}"),
                                    file,
                                    extractor_id,
                                    extractor_version,
                                    Confidence::Exact,
                                )?;
                            }
                        }
                        "oci" => {
                            if let Some(url) = source.get("url").and_then(Value::as_str) {
                                push_unresolved_edge(
                                    sink,
                                    &provider_id,
                                    "deploy-provider-source-oci",
                                    url,
                                    &format!("deploy-provider-source-oci:{provider_name}"),
                                    file,
                                    extractor_id,
                                    extractor_version,
                                    Confidence::Exact,
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for (target_name, target_value) in table {
        if target_name == "providers" {
            continue;
        }
        let Some(target_table) = target_value.as_table() else {
            continue;
        };
        let target_id = manifest_nested_symbol_id(file, &["deploy", "target", target_name])?;
        push_symbol(
            sink,
            target_id.clone(),
            "deploy-target",
            target_name,
            &format!("deploy::target::{target_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "deploy-target",
        );
        push_contains_edge(
            sink,
            &section_id,
            &target_id,
            &format!("deploy-target:{target_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        for (field, kind) in [
            ("state", "deploy-target-state"),
            ("code_ref", "deploy-target-code-ref"),
            ("release_policy", "deploy-target-release-policy"),
            ("provider_project", "deploy-target-provider-project"),
            ("artifact_policy", "deploy-target-artifact-policy"),
        ] {
            if let Some(value) = target_table.get(field).and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &target_id,
                    kind,
                    value,
                    &format!("{kind}:{target_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
        }
        if let Some(provider) = target_table.get("provider").and_then(Value::as_table) {
            if let Some(adapter) = provider.get("adapter").and_then(Value::as_str) {
                if let Some(provider_id) = provider_ids.get(adapter) {
                    push_resolved_edge(
                        sink,
                        &target_id,
                        "deploy-target-provider",
                        provider_id,
                        &format!("deploy-target-provider:{target_name}:{adapter}"),
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                } else {
                    push_unresolved_edge(
                        sink,
                        &target_id,
                        "deploy-target-provider",
                        adapter,
                        &format!("deploy-target-provider:{target_name}:{adapter}"),
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                }
            }
            for (field, kind) in [
                ("project_id", "deploy-provider-project-id"),
                ("environment_id", "deploy-provider-environment-id"),
                ("preflight_scope", "deploy-provider-preflight-scope"),
            ] {
                if let Some(value) = provider.get(field).and_then(Value::as_str) {
                    push_unresolved_edge(
                        sink,
                        &target_id,
                        kind,
                        value,
                        &format!("{kind}:{target_name}"),
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                }
            }
            if let Some(skip_domains) = provider.get("skip_domains").and_then(Value::as_bool) {
                push_unresolved_edge(
                    sink,
                    &target_id,
                    "deploy-provider-skip-domains",
                    if skip_domains { "true" } else { "false" },
                    &format!("deploy-provider-skip-domains:{target_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(services) = provider.get("services").and_then(Value::as_table) {
                for (service_name, service_id_value) in services {
                    if let Some(service_id_value) = service_id_value.as_str() {
                        push_unresolved_edge(
                            sink,
                            &target_id,
                            "deploy-provider-service-id",
                            service_id_value,
                            &format!("deploy-provider-service-id:{target_name}:{service_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn index_state_raw(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    value: Option<&Value>,
) -> Result<(), CodeGraphError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "state")?;
    push_symbol(
        sink,
        section_id.clone(),
        "state",
        "state",
        &format!("{}::state", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "state",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &section_id,
        "state-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    for default_field in ["default", "default_stack"] {
        if let Some(default_stack) = table.get(default_field).and_then(Value::as_str) {
            push_unresolved_edge(
                sink,
                &section_id,
                "state-default-stack",
                default_stack,
                &format!("state-default-stack:{default_field}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    for (stack_name, stack_value) in table {
        if stack_name == "default" || stack_name == "default_stack" {
            continue;
        }
        let Some(stack_table) = stack_value.as_table() else {
            continue;
        };
        let stack_id = manifest_nested_symbol_id(file, &["state", "stack", stack_name])?;
        push_symbol(
            sink,
            stack_id.clone(),
            "state-stack",
            stack_name,
            &format!("state::stack::{stack_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "state-stack",
        );
        push_contains_edge(
            sink,
            &section_id,
            &stack_id,
            &format!("state-stack:{stack_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        for (field, kind) in [
            ("schema", "state-stack-schema"),
            ("name", "state-stack-name"),
            ("environment", "state-stack-environment"),
        ] {
            if let Some(value) = stack_table.get(field).and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    &stack_id,
                    kind,
                    value,
                    &format!("{kind}:{stack_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
        }
        if let Some(layers) = stack_table.get("layers").and_then(Value::as_array) {
            for (index, layer_value) in layers.iter().enumerate() {
                let Some(layer_table) = layer_value.as_table() else {
                    continue;
                };
                let layer_key = layer_table
                    .get("key")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("layer-{}", index + 1));
                let layer_id =
                    manifest_nested_symbol_id(file, &["state", stack_name, "layer", &layer_key])?;
                push_symbol(
                    sink,
                    layer_id.clone(),
                    "state-layer",
                    &layer_key,
                    &format!("state::stack::{stack_name}::layer::{layer_key}"),
                    file,
                    file_record,
                    extractor_id,
                    extractor_version,
                    "state-layer",
                );
                push_contains_edge(
                    sink,
                    &stack_id,
                    &layer_id,
                    &format!("state-layer:{stack_name}:{layer_key}"),
                    file,
                    extractor_id,
                    extractor_version,
                )?;
                for (field, kind) in [
                    ("role", "state-layer-role"),
                    ("source", "state-layer-source"),
                    ("apply_mode", "state-layer-apply-mode"),
                    ("environment_policy", "state-layer-environment-policy"),
                    ("artifact_kind", "state-layer-artifact-kind"),
                ] {
                    if let Some(value) = layer_table.get(field).and_then(Value::as_str) {
                        push_unresolved_edge(
                            sink,
                            &layer_id,
                            kind,
                            value,
                            &format!("{kind}:{stack_name}:{layer_key}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
                if let Some(depends_on) = layer_table.get("depends_on").and_then(Value::as_array) {
                    for (dep_index, dependency) in
                        depends_on.iter().filter_map(Value::as_str).enumerate()
                    {
                        push_unresolved_edge(
                            sink,
                            &layer_id,
                            "state-layer-depends-on",
                            dependency,
                            &format!("state-layer-depends-on:{stack_name}:{layer_key}:{dep_index}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
                if let Some(hook) = layer_table.get("hook") {
                    index_run_like_raw(
                        file,
                        &layer_id,
                        sink,
                        extractor_id,
                        extractor_version,
                        &format!("state-layer-hook:{stack_name}:{layer_key}"),
                        hook,
                    )?;
                }
            }
        }
        if let Some(captures) = stack_table.get("captures").and_then(Value::as_table) {
            for (capture_name, capture_value) in captures {
                let Some(capture_table) = capture_value.as_table() else {
                    continue;
                };
                let capture_id = manifest_nested_symbol_id(
                    file,
                    &["state", stack_name, "capture", capture_name],
                )?;
                push_symbol(
                    sink,
                    capture_id.clone(),
                    "state-capture",
                    capture_name,
                    &format!("state::stack::{stack_name}::capture::{capture_name}"),
                    file,
                    file_record,
                    extractor_id,
                    extractor_version,
                    "state-capture",
                );
                push_contains_edge(
                    sink,
                    &stack_id,
                    &capture_id,
                    &format!("state-capture:{stack_name}:{capture_name}"),
                    file,
                    extractor_id,
                    extractor_version,
                )?;
                for (field, kind) in [
                    ("role", "state-capture-role"),
                    ("source_env", "state-capture-source-env"),
                    ("source", "state-capture-source"),
                    ("ref", "state-capture-ref"),
                ] {
                    if let Some(value) = capture_table.get(field).and_then(Value::as_str) {
                        push_unresolved_edge(
                            sink,
                            &capture_id,
                            kind,
                            value,
                            &format!("{kind}:{stack_name}:{capture_name}"),
                            file,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
                if let Some(task) = capture_table.get("task") {
                    index_run_like_raw(
                        file,
                        &capture_id,
                        sink,
                        extractor_id,
                        extractor_version,
                        &format!("state-capture-task:{stack_name}:{capture_name}"),
                        task,
                    )?;
                }
            }
        }
    }
    Ok(())
}
