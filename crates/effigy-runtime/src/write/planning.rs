use std::ffi::OsString;
use std::path::Path;

use effigy_containers::ContainerAction;
use effigy_containers::{
    colima::shutdown_compose_commands, EffectiveComposeSource, EffectiveContainerPolicy,
};
use serde_yaml::{Mapping, Value};

use crate::container_manager::{compose_invocation_plan_from_args, runtime_invocation_plan};
use crate::signals::{run_compose_plan_capture, run_runtime_plan_capture};
use crate::EffigyRuntimeError;

pub(super) fn shutdown_container_with_manager_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    for (args, label) in shutdown_compose_commands(policy) {
        let plan = compose_invocation_plan_from_args(
            repo_root,
            policy,
            args,
            ContainerAction::Shutdown,
            label,
        )?;
        run_compose_plan_capture(policy, &plan)?;
    }
    Ok(())
}

pub(super) fn remove_generated_runtime_artifacts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let runtime_dir = repo_root.join(".effigy/runtime/compose");
    if !runtime_dir.exists() {
        return Ok(());
    }

    std::fs::remove_dir_all(&runtime_dir).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to remove generated runtime directory {}: {error}",
            runtime_dir.display()
        ))
    })
}

pub(super) fn remove_generated_service_images(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Ok(());
    }

    let Some(compose_file) = policy.compose_files.first() else {
        return Ok(());
    };
    if !compose_file.exists() {
        return Ok(());
    }

    let image_refs = load_generated_service_image_refs(compose_file, &policy.project_name)?;
    for image_ref in image_refs {
        remove_runtime_image_allow_missing(repo_root, policy, &image_ref)?;
    }
    Ok(())
}

fn load_generated_service_image_refs(
    compose_file: &Path,
    project_name: &str,
) -> Result<Vec<String>, EffigyRuntimeError> {
    let content = std::fs::read_to_string(compose_file).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to read generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    let parsed: Value = serde_yaml::from_str(&content).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to parse generated compose file {} while resolving built images: {error}",
            compose_file.display()
        ))
    })?;
    Ok(select_generated_service_image_refs(&parsed, project_name))
}

pub fn select_generated_service_image_refs(parsed: &Value, project_name: &str) -> Vec<String> {
    let Some(services) = parsed.get("services").and_then(Value::as_mapping) else {
        return Vec::new();
    };

    services
        .iter()
        .filter_map(|(service_name, service_def)| {
            let service_name = service_name.as_str()?;
            let service_def: &Mapping = service_def.as_mapping()?;
            service_def.get("build")?;
            if let Some(image) = service_def
                .get("image")
                .and_then(Value::as_str)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                return Some(image.to_owned());
            }
            Some(format!("{project_name}-{service_name}:latest"))
        })
        .collect()
}

fn remove_runtime_image_allow_missing(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    image_ref: &str,
) -> Result<(), EffigyRuntimeError> {
    let docker_args = vec![
        OsString::from("image"),
        OsString::from("rm"),
        OsString::from("-f"),
        OsString::from(image_ref),
    ];
    let plan = runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &docker_args,
        ContainerAction::Shutdown,
        &format!("remove generated image `{image_ref}`"),
    )?;

    let output = run_runtime_plan_capture(&plan)?;
    if output.status.success() || image_remove_failure_is_missing(&output) {
        return Ok(());
    }
    Err(EffigyRuntimeError::task_invocation(format!(
        "failed to remove generated image `{image_ref}` (code {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

fn image_remove_failure_is_missing(output: &std::process::Output) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("no such image")
        || combined.contains("not found")
        || combined.contains("no such object")
}

#[cfg(test)]
mod tests {
    use super::select_generated_service_image_refs;

    #[test]
    fn generated_service_image_refs_use_declared_image_when_present() {
        let parsed: serde_yaml::Value = serde_yaml::from_str(
            r#"
services:
  app:
    build: .
    image: ghcr.io/example/app:dev
  db:
    image: postgres:16
"#,
        )
        .expect("parse compose");

        let refs = select_generated_service_image_refs(&parsed, "demo");

        assert_eq!(refs, vec!["ghcr.io/example/app:dev"]);
    }

    #[test]
    fn generated_service_image_refs_fallback_to_project_service_tag() {
        let parsed: serde_yaml::Value = serde_yaml::from_str(
            r#"
services:
  app:
    build:
      context: .
  worker:
    build: ./worker
"#,
        )
        .expect("parse compose");

        let refs = select_generated_service_image_refs(&parsed, "demo");

        assert_eq!(refs, vec!["demo-app:latest", "demo-worker:latest"]);
    }
}
