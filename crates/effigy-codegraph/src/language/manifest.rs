use effigy_manifest::config_sections::ManifestReleaseGateConfig;
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestBootstrapRun, ManifestBundleBase,
    ManifestContainerServiceConfig, ManifestDistributionConfig, ManifestManagedRun,
    ManifestManagedRunStep, ManifestManagedRunStepTable, ManifestReleaseConfig, ManifestTask,
    ManifestWorkspaceContainerRef, TASK_MANIFEST_FILE,
};
use toml::{map::Map as TomlMap, Value};

use crate::error::CodeGraphError;
use crate::extractor::{capability_set, extractor_id, GraphSink, LanguageIndexer, SourceFile};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, ExtractorCapability,
    ExtractorRecord, FileRecord, SymbolRecord,
};
use crate::support::{full_span, id_fragment, normalize_rel_path, provenance_for_file};
use crate::{ExtractorId, GraphId};

pub struct ManifestIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl ManifestIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("manifest-structure").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for ManifestIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["toml".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".toml")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let parsed: Value = toml::from_str(&file.content).map_err(|error| {
            CodeGraphError::validation(format!(
                "failed to parse TOML {}: {error}",
                file.relative_path
            ))
        })?;
        let span = full_span(&file.content);
        let file_symbol = SymbolRecord {
            id: GraphId::new(format!("symbol:manifest:file:{}", file.relative_path))?,
            kind: "manifest".to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: span.clone(),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("manifest"),
            ),
        };
        let file_symbol_id = file_symbol.id.clone();
        sink.push_symbol(file_symbol);
        let maybe_table = parsed.as_table();
        if let Some(table) = maybe_table {
            for (key, value) in table {
                top_level_entry(
                    key,
                    value,
                    &file_symbol_id,
                    file,
                    file_record,
                    sink,
                    &self.extractor_id,
                    &self.version,
                )?;
            }
        }
        if maybe_table.is_some_and(|table| {
            should_extract_effigy_manifest_relations(&file.relative_path, table)
        }) {
            if let Err(error) = extract_effigy_manifest_relations(
                file,
                file_record,
                &file_symbol_id,
                sink,
                &self.extractor_id,
                &self.version,
            ) {
                sink.push_diagnostic(DiagnosticRecord {
                    id: GraphId::new(format!("diag:manifest-semantic:{}", file.relative_path))?,
                    severity: DiagnosticSeverity::Warning,
                    message: error.to_string(),
                    file_id: Some(file_record.id.clone()),
                    span: Some(full_span(&file.content)),
                    provenance: provenance_for_file(
                        &self.extractor_id,
                        &self.version,
                        file,
                        Confidence::Syntactic,
                        Some("manifest-semantic-fallback"),
                    ),
                });
            }
        }
        Ok(())
    }
}

fn top_level_entry(
    key: &str,
    value: &Value,
    owner_id: &GraphId,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    let entry_id = GraphId::new(format!("symbol:manifest:{}:{key}", file.relative_path))?;
    sink.push_symbol(SymbolRecord {
        id: entry_id.clone(),
        kind: "manifest-section".to_owned(),
        display_name: key.to_owned(),
        canonical_name: format!("{}::{key}", file.relative_path),
        file_id: file_record.id.clone(),
        span: full_span(&file.content),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("manifest-section"),
        ),
    });
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:contains:{owner_id}:{entry_id}"))?,
        kind: "contains".to_owned(),
        from_id: owner_id.clone(),
        to_id: Some(entry_id.clone()),
        unresolved_target: None,
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("containment"),
        ),
    });
    match key {
        "tasks" => {
            if let Some(table) = value.as_table() {
                for task_name in table.keys() {
                    let task_id = GraphId::new(format!(
                        "symbol:manifest:{}:task:{task_name}",
                        file.relative_path
                    ))?;
                    sink.push_symbol(SymbolRecord {
                        id: task_id.clone(),
                        kind: "task".to_owned(),
                        display_name: task_name.clone(),
                        canonical_name: format!("task::{task_name}"),
                        file_id: file_record.id.clone(),
                        span: full_span(&file.content),
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("task"),
                        ),
                    });
                    sink.push_edge(EdgeRecord {
                        id: GraphId::new(format!("edge:contains:{entry_id}:{task_id}"))?,
                        kind: "contains".to_owned(),
                        from_id: entry_id.clone(),
                        to_id: Some(task_id.clone()),
                        unresolved_target: None,
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("containment"),
                        ),
                    });
                }
            }
        }
        "containers" | "systems" | "distribution" | "release" | "bundle" | "deploy" | "state" => {
            if let Some(table) = value.as_table() {
                for child_name in table.keys() {
                    let child_id = GraphId::new(format!(
                        "symbol:manifest:{}:{key}:{child_name}",
                        file.relative_path
                    ))?;
                    sink.push_symbol(SymbolRecord {
                        id: child_id.clone(),
                        kind: format!("manifest-{key}-entry"),
                        display_name: child_name.clone(),
                        canonical_name: format!("{key}::{child_name}"),
                        file_id: file_record.id.clone(),
                        span: full_span(&file.content),
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some(key),
                        ),
                    });
                    sink.push_edge(EdgeRecord {
                        id: GraphId::new(format!("edge:contains:{entry_id}:{child_id}"))?,
                        kind: "contains".to_owned(),
                        from_id: entry_id.clone(),
                        to_id: Some(child_id),
                        unresolved_target: None,
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("containment"),
                        ),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn extract_effigy_manifest_relations(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    let loaded = load_task_manifest_with_inspection(file.path()).map_err(|error| {
        CodeGraphError::validation(format!(
            "failed to compose manifest {}: {error}",
            file.relative_path
        ))
    })?;
    for edge in &loaded.include_graph {
        let child_path = edge
            .child
            .strip_prefix(&file.repo_root)
            .map(normalize_rel_path)
            .unwrap_or_else(|_| edge.child.display().to_string());
        push_resolved_edge(
            sink,
            file_symbol_id,
            "includes-manifest",
            &crate::extractor::file_graph_id(&child_path)?,
            &format!("include:{child_path}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }

    index_tasks(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        &loaded.manifest.tasks,
    )?;
    index_systems(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.systems.as_ref(),
    )?;
    index_containers(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.containers.as_ref(),
    )?;
    index_bundle(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.bundle.as_ref(),
    )?;
    index_release(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.release.as_ref(),
    )?;
    index_distribution(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.distribution.as_ref(),
    )?;
    index_bootstrap(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.bootstrap.as_ref(),
    )?;
    index_demos(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        &loaded.manifest.demos,
    )?;
    if let Some(table) = loaded.effective_value.as_table() {
        index_docs_policy_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("docs_policy"),
        )?;
        index_test_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("test"),
        )?;
        index_secrets_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("secrets"),
        )?;
        index_deploy_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("deploy"),
        )?;
        index_state_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("state"),
        )?;
    }
    Ok(())
}

fn index_tasks(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    tasks: &std::collections::BTreeMap<String, ManifestTask>,
) -> Result<(), CodeGraphError> {
    let tasks_section_id = manifest_section_id(file, "tasks")?;
    for (task_name, task) in tasks {
        let task_id = manifest_named_symbol_id(file, "task", task_name)?;
        push_symbol(
            sink,
            task_id.clone(),
            "task",
            task_name,
            &format!("task::{task_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "task",
        );
        push_contains_edge(
            sink,
            &tasks_section_id,
            &task_id,
            &format!("task:{task_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &task_id,
            &format!("task-root:{task_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        if let Some(system) = task.system.as_deref() {
            push_unresolved_edge(
                sink,
                &task_id,
                "task-system",
                system,
                &format!("task-system:{task_name}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        if let Some(workspace) = task.workspace.as_deref() {
            push_unresolved_edge(
                sink,
                &task_id,
                "task-workspace",
                workspace,
                &format!("task-workspace:{task_name}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        push_unresolved_edge(
            sink,
            &task_id,
            "task-run-in",
            task.run_in().as_str(),
            &format!("task-run-in:{task_name}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
        if let Some(run) = task.run.as_ref() {
            index_run_binding(
                file,
                &task_id,
                sink,
                extractor_id,
                extractor_version,
                &format!("task:{task_name}"),
                run,
            )?;
        }
    }
    Ok(())
}

fn index_systems(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    systems: Option<&effigy_manifest::ManifestSystemsConfig>,
) -> Result<(), CodeGraphError> {
    let Some(systems) = systems else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "systems")?;
    for (system_name, system) in &systems.systems {
        let system_id = manifest_named_symbol_id(file, "system", system_name)?;
        push_symbol(
            sink,
            system_id.clone(),
            "system",
            system_name,
            &format!("system::{system_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "system",
        );
        push_contains_edge(
            sink,
            &section_id,
            &system_id,
            &format!("system:{system_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &system_id,
            &format!("system-root:{system_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        if let Some(default_workspace) = system.default_workspace.as_deref() {
            push_unresolved_edge(
                sink,
                &system_id,
                "system-default-workspace",
                default_workspace,
                &format!("system-default-workspace:{system_name}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        if let Some(container) = system.container.as_ref() {
            index_workspace_container_ref(
                file,
                &system_id,
                sink,
                extractor_id,
                extractor_version,
                &format!("system-container:{system_name}"),
                container,
            )?;
        }
        for (workspace_name, workspace) in &system.workspaces {
            let workspace_id = manifest_nested_symbol_id(
                file,
                &["system", system_name, "workspace", workspace_name],
            )?;
            push_symbol(
                sink,
                workspace_id.clone(),
                "workspace",
                workspace_name,
                &format!("system::{system_name}::workspace::{workspace_name}"),
                file,
                file_record,
                extractor_id,
                extractor_version,
                "workspace",
            );
            push_contains_edge(
                sink,
                &system_id,
                &workspace_id,
                &format!("workspace:{system_name}:{workspace_name}"),
                file,
                extractor_id,
                extractor_version,
            )?;
            if let Some(container) = workspace.container.as_ref() {
                index_workspace_container_ref(
                    file,
                    &workspace_id,
                    sink,
                    extractor_id,
                    extractor_version,
                    &format!("workspace-container:{system_name}:{workspace_name}"),
                    container,
                )?;
            }
        }
    }
    Ok(())
}

fn index_containers(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    containers: Option<&effigy_manifest::ManifestContainersConfig>,
) -> Result<(), CodeGraphError> {
    let Some(containers) = containers else {
        return Ok(());
    };
    let section_id = manifest_section_id(file, "containers")?;
    for (container_name, container) in &containers.environments {
        let container_id = manifest_named_symbol_id(file, "container", container_name)?;
        push_symbol(
            sink,
            container_id.clone(),
            "container",
            container_name,
            &format!("container::{container_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "container",
        );
        push_contains_edge(
            sink,
            &section_id,
            &container_id,
            &format!("container:{container_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &container_id,
            &format!("container-root:{container_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        if let Some(primary_service) = container.primary_service.as_deref() {
            push_unresolved_edge(
                sink,
                &container_id,
                "container-primary-service",
                primary_service,
                &format!("container-primary-service:{container_name}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        for (service_name, service) in &container.services {
            index_container_service(
                file,
                file_record,
                &container_id,
                sink,
                extractor_id,
                extractor_version,
                container_name,
                service_name,
                service,
            )?;
        }
    }
    Ok(())
}

fn index_container_service(
    file: &SourceFile,
    file_record: &FileRecord,
    container_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    container_name: &str,
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<(), CodeGraphError> {
    let service_id = manifest_nested_symbol_id(
        file,
        &["container", container_name, "service", service_name],
    )?;
    push_symbol(
        sink,
        service_id.clone(),
        "container-service",
        service_name,
        &format!("container::{container_name}::service::{service_name}"),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "service",
    );
    push_contains_edge(
        sink,
        container_id,
        &service_id,
        &format!("service:{container_name}:{service_name}"),
        file,
        extractor_id,
        extractor_version,
    )?;
    push_unresolved_edge(
        sink,
        &service_id,
        "service-catalog",
        &service.catalog,
        &format!("service-catalog:{container_name}:{service_name}"),
        file,
        extractor_id,
        extractor_version,
        Confidence::Exact,
    )?;
    if let Some(variant) = service.variant.as_deref() {
        push_unresolved_edge(
            sink,
            &service_id,
            "service-variant",
            variant,
            &format!("service-variant:{container_name}:{service_name}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(config) = service.config.as_deref() {
        push_unresolved_edge(
            sink,
            &service_id,
            "service-config",
            config,
            &format!("service-config:{container_name}:{service_name}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Syntactic,
        )?;
    }
    Ok(())
}

fn index_bundle(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    bundle: Option<&effigy_manifest::ManifestBundleConfig>,
) -> Result<(), CodeGraphError> {
    let Some(bundle) = bundle else {
        return Ok(());
    };
    let bundle_id = manifest_section_id(file, "bundle")?;
    push_symbol(
        sink,
        bundle_id.clone(),
        "bundle",
        "bundle",
        &format!("{}::bundle", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "bundle",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &bundle_id,
        "bundle-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(base) = bundle.base.as_ref() {
        match base {
            ManifestBundleBase::Path { dir } => push_unresolved_edge(
                sink,
                &bundle_id,
                "bundle-base-path",
                dir,
                "bundle-base-path",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?,
            ManifestBundleBase::Git { url, r#ref } => {
                push_unresolved_edge(
                    sink,
                    &bundle_id,
                    "bundle-base-git",
                    url,
                    "bundle-base-git",
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
                if let Some(reference) = r#ref.as_deref() {
                    push_unresolved_edge(
                        sink,
                        &bundle_id,
                        "bundle-base-ref",
                        reference,
                        "bundle-base-ref",
                        file,
                        extractor_id,
                        extractor_version,
                        Confidence::Exact,
                    )?;
                }
            }
            ManifestBundleBase::Oci { url } => push_unresolved_edge(
                sink,
                &bundle_id,
                "bundle-base-oci",
                url,
                "bundle-base-oci",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?,
        }
    }
    for input_name in bundle.inputs.keys() {
        let input_id = manifest_nested_symbol_id(file, &["bundle", "input", input_name])?;
        push_symbol(
            sink,
            input_id.clone(),
            "bundle-input",
            input_name,
            &format!("bundle::input::{input_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "bundle-input",
        );
        push_contains_edge(
            sink,
            &bundle_id,
            &input_id,
            &format!("bundle-input:{input_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
    }
    Ok(())
}

fn index_release(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    release: Option<&ManifestReleaseConfig>,
) -> Result<(), CodeGraphError> {
    let Some(release) = release else {
        return Ok(());
    };
    let release_id = manifest_section_id(file, "release")?;
    push_symbol(
        sink,
        release_id.clone(),
        "release",
        "release",
        &format!("{}::release", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "release",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &release_id,
        "release-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    for (field, value) in [
        ("version-file", release.version_file.as_deref()),
        ("version-path", release.version_path.as_deref()),
        ("changelog", release.changelog.as_deref()),
        ("tag-format", release.tag_format.as_deref()),
    ] {
        if let Some(value) = value {
            push_unresolved_edge(
                sink,
                &release_id,
                field,
                value,
                &format!("release-{field}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    for (gate_name, gate) in &release.gates {
        let gate_id = manifest_nested_symbol_id(file, &["release", "gate", gate_name])?;
        push_symbol(
            sink,
            gate_id.clone(),
            "release-gate",
            gate_name,
            &format!("release::gate::{gate_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "release-gate",
        );
        push_contains_edge(
            sink,
            &release_id,
            &gate_id,
            &format!("release-gate:{gate_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        let command = match gate {
            ManifestReleaseGateConfig::Command(command) => command.as_str(),
            ManifestReleaseGateConfig::Detailed(details) => details.command.as_str(),
        };
        push_unresolved_edge(
            sink,
            &gate_id,
            "release-gate-command",
            command,
            &format!("release-gate-command:{gate_name}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    Ok(())
}

fn index_distribution(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    distribution: Option<&ManifestDistributionConfig>,
) -> Result<(), CodeGraphError> {
    let Some(distribution) = distribution else {
        return Ok(());
    };
    let distribution_id = manifest_section_id(file, "distribution")?;
    push_symbol(
        sink,
        distribution_id.clone(),
        "distribution",
        "distribution",
        &format!("{}::distribution", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "distribution",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &distribution_id,
        "distribution-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(preflight) = distribution.preflight.as_ref() {
        if let Some(task) = preflight.docs_task.as_deref() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-docs-task",
                task,
                "distribution-docs-task",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        if let Some(task) = preflight.smoke_task.as_deref() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-smoke-task",
                task,
                "distribution-smoke-task",
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    if let Some(metadata) = distribution.metadata.as_ref() {
        for path in metadata.required_docs.iter().flatten() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-required-doc",
                path,
                &format!("distribution-required-doc:{}", id_fragment(path)),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        for path in metadata.required_files.iter().flatten() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-required-file",
                path,
                &format!("distribution-required-file:{}", id_fragment(path)),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    Ok(())
}

fn index_bootstrap(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    bootstrap: Option<&effigy_manifest::ManifestBootstrapConfig>,
) -> Result<(), CodeGraphError> {
    let Some(bootstrap) = bootstrap else {
        return Ok(());
    };
    let bootstrap_id = manifest_section_id(file, "bootstrap")?;
    push_symbol(
        sink,
        bootstrap_id.clone(),
        "bootstrap",
        "bootstrap",
        &format!("{}::bootstrap", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "bootstrap",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &bootstrap_id,
        "bootstrap-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    if let Some(run) = bootstrap.run.as_ref() {
        index_bootstrap_run(
            file,
            &bootstrap_id,
            sink,
            extractor_id,
            extractor_version,
            "bootstrap-run",
            run,
        )?;
    }
    if let Some(start) = bootstrap.start.as_ref() {
        for (index, selector) in start.selectors().into_iter().enumerate() {
            push_unresolved_edge(
                sink,
                &bootstrap_id,
                "bootstrap-start-task",
                selector,
                &format!("bootstrap-start:{index}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
    }
    for (index, child) in bootstrap.children.iter().enumerate() {
        let child_name = format!("child-{}", index + 1);
        let child_id = manifest_nested_symbol_id(file, &["bootstrap", "child", &child_name])?;
        push_symbol(
            sink,
            child_id.clone(),
            "bootstrap-child",
            &child.path,
            &format!("bootstrap::child::{}", child.path),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "bootstrap-child",
        );
        push_contains_edge(
            sink,
            &bootstrap_id,
            &child_id,
            &format!("bootstrap-child:{index}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        push_unresolved_edge(
            sink,
            &child_id,
            "bootstrap-child-repo",
            &child.repo,
            &format!("bootstrap-child-repo:{index}"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
        if let Some(run) = child.run.as_ref() {
            index_bootstrap_run(
                file,
                &child_id,
                sink,
                extractor_id,
                extractor_version,
                &format!("bootstrap-child-run:{index}"),
                run,
            )?;
        }
    }
    Ok(())
}

fn index_bootstrap_run(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    run: &ManifestBootstrapRun,
) -> Result<(), CodeGraphError> {
    let task = run.as_manifest_task();
    if let Some(run) = task.run.as_ref() {
        index_run_binding(
            file,
            owner_id,
            sink,
            extractor_id,
            extractor_version,
            label,
            run,
        )?;
    }
    Ok(())
}

fn index_demos(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    demos: &std::collections::BTreeMap<String, effigy_manifest::ManifestDemoConfig>,
) -> Result<(), CodeGraphError> {
    if demos.is_empty() {
        return Ok(());
    }
    let demos_section_id = manifest_named_symbol_id(file, "demos", "all")?;
    push_symbol(
        sink,
        demos_section_id.clone(),
        "demos",
        "demos",
        &format!("{}::demos", file.relative_path),
        file,
        file_record,
        extractor_id,
        extractor_version,
        "demos",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &demos_section_id,
        "demos-root",
        file,
        extractor_id,
        extractor_version,
    )?;
    for (demo_id_name, demo) in demos {
        let demo_id = manifest_named_symbol_id(file, "demo", demo_id_name)?;
        push_symbol(
            sink,
            demo_id.clone(),
            "demo",
            demo_id_name,
            &format!("demo::{demo_id_name}"),
            file,
            file_record,
            extractor_id,
            extractor_version,
            "demo",
        );
        push_contains_edge(
            sink,
            &demos_section_id,
            &demo_id,
            &format!("demo:{demo_id_name}"),
            file,
            extractor_id,
            extractor_version,
        )?;
        if let Some(task) = demo.task.as_deref() {
            push_unresolved_edge(
                sink,
                &demo_id,
                "demo-task",
                task,
                &format!("demo-task:{demo_id_name}"),
                file,
                extractor_id,
                extractor_version,
                Confidence::Exact,
            )?;
        }
        if let Some(run) = demo.run.as_ref() {
            index_run_binding(
                file,
                &demo_id,
                sink,
                extractor_id,
                extractor_version,
                &format!("demo:{demo_id_name}"),
                run,
            )?;
        }
    }
    Ok(())
}

fn index_docs_policy_raw(
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

fn index_test_raw(
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

fn index_secrets_raw(
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

fn index_deploy_raw(
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

fn index_state_raw(
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

fn index_run_like_raw(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    value: &Value,
) -> Result<(), CodeGraphError> {
    match value {
        Value::String(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        ),
        Value::Array(steps) => {
            for (index, step) in steps.iter().enumerate() {
                index_run_step_raw(
                    file,
                    owner_id,
                    sink,
                    extractor_id,
                    extractor_version,
                    &format!("{label}:{index}"),
                    step,
                )?;
            }
            Ok(())
        }
        Value::Table(_) => index_run_step_raw(
            file,
            owner_id,
            sink,
            extractor_id,
            extractor_version,
            label,
            value,
        ),
        _ => Ok(()),
    }
}

fn index_run_step_raw(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    value: &Value,
) -> Result<(), CodeGraphError> {
    match value {
        Value::String(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        ),
        Value::Table(table) => {
            if let Some(command) = table.get("run").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-command",
                    command,
                    &format!("{label}:run"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(task) = table.get("task").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-step-task",
                    task,
                    &format!("{label}:task"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(rhai) = table.get("rhai").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-step-rhai",
                    rhai,
                    &format!("{label}:rhai"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(run_in) = table.get("run_in").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-run-in",
                    run_in,
                    &format!("{label}:run-in"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn index_run_binding(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    run: &ManifestManagedRun,
) -> Result<(), CodeGraphError> {
    match run {
        ManifestManagedRun::Command(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            &format!("{label}:command"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        ),
        ManifestManagedRun::Sequence(steps) => {
            for (index, step) in steps.iter().enumerate() {
                index_run_step(
                    file,
                    owner_id,
                    sink,
                    extractor_id,
                    extractor_version,
                    &format!("{label}:{index}"),
                    step,
                )?;
            }
            Ok(())
        }
    }
}

fn index_run_step(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    step: &ManifestManagedRunStep,
) -> Result<(), CodeGraphError> {
    match step {
        ManifestManagedRunStep::Command(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        ),
        ManifestManagedRunStep::Step(step) => index_step_table(
            file,
            owner_id,
            sink,
            extractor_id,
            extractor_version,
            label,
            step,
        ),
    }
}

fn index_step_table(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    step: &ManifestManagedRunStepTable,
) -> Result<(), CodeGraphError> {
    if let Some(command) = step.run.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            &format!("{label}:run"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(task) = step.task.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-step-task",
            task,
            &format!("{label}:task"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    if let Some(rhai) = step.rhai.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-step-rhai",
            rhai,
            &format!("{label}:rhai"),
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        )?;
    }
    Ok(())
}

fn index_workspace_container_ref(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    container: &ManifestWorkspaceContainerRef,
) -> Result<(), CodeGraphError> {
    match container {
        ManifestWorkspaceContainerRef::Named(name) => push_unresolved_edge(
            sink,
            owner_id,
            "workspace-container-ref",
            name,
            label,
            file,
            extractor_id,
            extractor_version,
            Confidence::Exact,
        ),
        ManifestWorkspaceContainerRef::Inline(inline) => {
            if let Some(image) = inline.image.as_deref() {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "workspace-inline-image",
                    image,
                    &format!("{label}:image"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            if let Some(mount) = inline.mount.as_deref() {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "workspace-inline-mount",
                    mount,
                    &format!("{label}:mount"),
                    file,
                    extractor_id,
                    extractor_version,
                    Confidence::Exact,
                )?;
            }
            Ok(())
        }
    }
}

fn push_symbol(
    sink: &mut GraphSink,
    id: GraphId,
    kind: &str,
    display_name: &str,
    canonical_name: &str,
    file: &SourceFile,
    file_record: &FileRecord,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    detail: &str,
) {
    sink.push_symbol(SymbolRecord {
        id,
        kind: kind.to_owned(),
        display_name: display_name.to_owned(),
        canonical_name: canonical_name.to_owned(),
        file_id: file_record.id.clone(),
        span: full_span(&file.content),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some(detail),
        ),
    });
}

fn push_contains_edge(
    sink: &mut GraphSink,
    from_id: &GraphId,
    to_id: &GraphId,
    label: &str,
    file: &SourceFile,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    push_resolved_edge(
        sink,
        from_id,
        "contains",
        to_id,
        label,
        file,
        extractor_id,
        extractor_version,
        Confidence::Exact,
    )
}

fn push_resolved_edge(
    sink: &mut GraphSink,
    from_id: &GraphId,
    kind: &str,
    to_id: &GraphId,
    label: &str,
    file: &SourceFile,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:{kind}:{}:{}", from_id, id_fragment(label)))?,
        kind: kind.to_owned(),
        from_id: from_id.clone(),
        to_id: Some(to_id.clone()),
        unresolved_target: None,
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

fn push_unresolved_edge(
    sink: &mut GraphSink,
    from_id: &GraphId,
    kind: &str,
    unresolved_target: &str,
    label: &str,
    file: &SourceFile,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:{kind}:{}:{}", from_id, id_fragment(label)))?,
        kind: kind.to_owned(),
        from_id: from_id.clone(),
        to_id: None,
        unresolved_target: Some(unresolved_target.to_owned()),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

fn manifest_section_id(file: &SourceFile, section: &str) -> Result<GraphId, CodeGraphError> {
    GraphId::new(format!("symbol:manifest:{}:{section}", file.relative_path))
}

fn manifest_named_symbol_id(
    file: &SourceFile,
    kind: &str,
    name: &str,
) -> Result<GraphId, CodeGraphError> {
    GraphId::new(format!(
        "symbol:manifest:{}:{kind}:{}",
        file.relative_path,
        id_fragment(name)
    ))
}

fn manifest_nested_symbol_id(file: &SourceFile, parts: &[&str]) -> Result<GraphId, CodeGraphError> {
    let suffix = parts
        .iter()
        .map(|part| id_fragment(part))
        .collect::<Vec<_>>()
        .join(":");
    GraphId::new(format!("symbol:manifest:{}:{suffix}", file.relative_path))
}

fn should_extract_effigy_manifest_relations(
    relative_path: &str,
    table: &TomlMap<String, Value>,
) -> bool {
    !is_bundle_descriptor_path(relative_path)
        && (is_named_effigy_manifest(relative_path) || looks_like_effigy_manifest(table))
}

fn is_named_effigy_manifest(relative_path: &str) -> bool {
    relative_path == TASK_MANIFEST_FILE
        || relative_path.ends_with(&format!("/{TASK_MANIFEST_FILE}"))
        || relative_path.ends_with(".effigy.toml")
}

fn is_bundle_descriptor_path(relative_path: &str) -> bool {
    relative_path == "bundle.toml" || relative_path.ends_with("/bundle.toml")
}

fn looks_like_effigy_manifest(table: &TomlMap<String, Value>) -> bool {
    table.keys().any(|key| {
        matches!(
            key.as_str(),
            "manifest"
                | "catalog"
                | "bundle"
                | "defer"
                | "env"
                | "data"
                | "state"
                | "deploy"
                | "test"
                | "package_manager"
                | "scan"
                | "shell"
                | "env_schema"
                | "secrets"
                | "docs_policy"
                | "task_defaults"
                | "bootstrap"
                | "isolation"
                | "containers"
                | "systems"
                | "distribution"
                | "release"
                | "demos"
                | "tasks"
        )
    })
}
