use effigy_manifest::config_sections::ManifestReleaseGateConfig;
use effigy_manifest::{
    ManifestBootstrapRun, ManifestBundleBase, ManifestContainerServiceConfig,
    ManifestDistributionConfig, ManifestReleaseConfig, ManifestWorkspaceContainerRef,
};

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::model::{Confidence, FileRecord};
use crate::{ExtractorId, GraphId};

use super::support::{
    index_run_binding, manifest_named_symbol_id, manifest_nested_symbol_id, manifest_section_id,
    push_contains_edge, push_resolved_edge, push_symbol, push_unresolved_edge, SemanticOrigin,
    SemanticSource,
};
use super::ManifestTasks;

pub(super) fn index_tasks(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    tasks: &ManifestTasks,
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "task",
        );
        push_contains_edge(
            sink,
            &tasks_section_id,
            &task_id,
            &format!("task:{task_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &task_id,
            &format!("task-root:{task_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        if let Some(system) = task.system.as_deref() {
            push_unresolved_edge(
                sink,
                &task_id,
                "task-system",
                system,
                &format!("task-system:{task_name}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                Confidence::Exact,
            )?;
        }
        push_unresolved_edge(
            sink,
            &task_id,
            "task-run-in",
            task.run_in().as_str(),
            &format!("task-run-in:{task_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
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

pub(super) fn index_systems(
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "system",
        );
        push_contains_edge(
            sink,
            &section_id,
            &system_id,
            &format!("system:{system_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &system_id,
            &format!("system-root:{system_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        if let Some(default_workspace) = system.default_workspace.as_deref() {
            push_unresolved_edge(
                sink,
                &system_id,
                "system-default-workspace",
                default_workspace,
                &format!("system-default-workspace:{system_name}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                "workspace",
            );
            push_contains_edge(
                sink,
                &system_id,
                &workspace_id,
                &format!("workspace:{system_name}:{workspace_name}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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

pub(super) fn index_containers(
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "container",
        );
        push_contains_edge(
            sink,
            &section_id,
            &container_id,
            &format!("container:{container_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        push_contains_edge(
            sink,
            file_symbol_id,
            &container_id,
            &format!("container-root:{container_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        if let Some(primary_service) = container.primary_service.as_deref() {
            push_unresolved_edge(
                sink,
                &container_id,
                "container-primary-service",
                primary_service,
                &format!("container-primary-service:{container_name}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                Confidence::Exact,
            )?;
        }
        for (service_name, service) in &container.services {
            index_container_service(
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                &container_id,
                sink,
                container_name,
                service_name,
                service,
            )?;
        }
    }
    Ok(())
}

fn index_container_service(
    source: SemanticSource<'_>,
    container_id: &GraphId,
    sink: &mut GraphSink,
    container_name: &str,
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<(), CodeGraphError> {
    let SemanticSource {
        file,
        file_record,
        extractor_id,
        extractor_version,
    } = source;
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "service",
    );
    push_contains_edge(
        sink,
        container_id,
        &service_id,
        &format!("service:{container_name}:{service_name}"),
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
    )?;
    push_unresolved_edge(
        sink,
        &service_id,
        "service-catalog",
        &service.catalog,
        &format!("service-catalog:{container_name}:{service_name}"),
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        Confidence::Exact,
    )?;
    if let Some(variant) = service.variant.as_deref() {
        push_unresolved_edge(
            sink,
            &service_id,
            "service-variant",
            variant,
            &format!("service-variant:{container_name}:{service_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            Confidence::Syntactic,
        )?;
    }
    Ok(())
}

pub(super) fn index_bundle(
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "bundle",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &bundle_id,
        "bundle-root",
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
    )?;
    if let Some(base) = bundle.base.as_ref() {
        match base {
            ManifestBundleBase::Path { dir } => push_unresolved_edge(
                sink,
                &bundle_id,
                "bundle-base-path",
                dir,
                "bundle-base-path",
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                Confidence::Exact,
            )?,
            ManifestBundleBase::Git { url, r#ref } => {
                push_unresolved_edge(
                    sink,
                    &bundle_id,
                    "bundle-base-git",
                    url,
                    "bundle-base-git",
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
                if let Some(reference) = r#ref.as_deref() {
                    push_unresolved_edge(
                        sink,
                        &bundle_id,
                        "bundle-base-ref",
                        reference,
                        "bundle-base-ref",
                        SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "bundle-input",
        );
        push_contains_edge(
            sink,
            &bundle_id,
            &input_id,
            &format!("bundle-input:{input_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
    }
    Ok(())
}

pub(super) fn index_release(
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "release",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &release_id,
        "release-root",
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "release-gate",
        );
        push_contains_edge(
            sink,
            &release_id,
            &gate_id,
            &format!("release-gate:{gate_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            Confidence::Exact,
        )?;
    }
    Ok(())
}

pub(super) fn index_distribution(
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "distribution",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &distribution_id,
        "distribution-root",
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
    )?;
    if let Some(preflight) = distribution.preflight.as_ref() {
        if let Some(task) = preflight.docs_task.as_deref() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-docs-task",
                task,
                "distribution-docs-task",
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
                &format!(
                    "distribution-required-doc:{}",
                    crate::support::id_fragment(path)
                ),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                Confidence::Exact,
            )?;
        }
        for path in metadata.required_files.iter().flatten() {
            push_unresolved_edge(
                sink,
                &distribution_id,
                "distribution-required-file",
                path,
                &format!(
                    "distribution-required-file:{}",
                    crate::support::id_fragment(path)
                ),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                Confidence::Exact,
            )?;
        }
    }
    Ok(())
}

pub(super) fn index_bootstrap(
    source: SemanticSource<'_>,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    tasks: &ManifestTasks,
    bootstrap: Option<&effigy_manifest::ManifestBootstrapConfig>,
) -> Result<(), CodeGraphError> {
    let SemanticSource {
        file,
        file_record,
        extractor_id,
        extractor_version,
    } = source;
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "bootstrap",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &bootstrap_id,
        "bootstrap-root",
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            let selector_id = manifest_nested_symbol_id(file, &["bootstrap", "start", selector])?;
            push_symbol(
                sink,
                selector_id.clone(),
                "task-selector",
                selector,
                &format!("selector::{selector}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
                "task-selector",
            );
            push_contains_edge(
                sink,
                &bootstrap_id,
                &selector_id,
                &format!("bootstrap-start-selector:{index}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
            )?;
            if tasks.contains_key(selector) {
                let task_id = manifest_named_symbol_id(file, "task", selector)?;
                push_resolved_edge(
                    sink,
                    &selector_id,
                    "entrypoint-task",
                    &task_id,
                    &format!("bootstrap-start:{index}"),
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            } else {
                push_unresolved_edge(
                    sink,
                    &selector_id,
                    "entrypoint-task",
                    selector,
                    &format!("bootstrap-start:{index}"),
                    SemanticSource::new(file, file_record, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
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
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "bootstrap-child",
        );
        push_contains_edge(
            sink,
            &bootstrap_id,
            &child_id,
            &format!("bootstrap-child:{index}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        push_unresolved_edge(
            sink,
            &child_id,
            "bootstrap-child-repo",
            &child.repo,
            &format!("bootstrap-child-repo:{index}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
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

pub(super) fn index_demos(
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
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
        "demos",
    );
    push_contains_edge(
        sink,
        file_symbol_id,
        &demos_section_id,
        "demos-root",
        SemanticSource::new(file, file_record, extractor_id, extractor_version),
    )?;
    for (demo_id_name, demo) in demos {
        let demo_id = manifest_named_symbol_id(file, "demo", demo_id_name)?;
        push_symbol(
            sink,
            demo_id.clone(),
            "demo",
            demo_id_name,
            &format!("demo::{demo_id_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
            "demo",
        );
        push_contains_edge(
            sink,
            &demos_section_id,
            &demo_id,
            &format!("demo:{demo_id_name}"),
            SemanticSource::new(file, file_record, extractor_id, extractor_version),
        )?;
        if let Some(task) = demo.task.as_deref() {
            push_unresolved_edge(
                sink,
                &demo_id,
                "demo-task",
                task,
                &format!("demo-task:{demo_id_name}"),
                SemanticSource::new(file, file_record, extractor_id, extractor_version),
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
            SemanticOrigin::new(file, extractor_id, extractor_version),
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
                    SemanticOrigin::new(file, extractor_id, extractor_version),
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
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
            Ok(())
        }
    }
}
