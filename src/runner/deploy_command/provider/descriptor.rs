use std::path::Path;

use serde::Deserialize;

use crate::runner::error::RunnerError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner::deploy_command) struct DeployProviderDescriptor {
    pub provider: DeployProviderMetadata,
    #[serde(default)]
    pub capabilities: DeployProviderCapabilities,
    #[serde(default)]
    pub policy: DeployProviderPolicy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner::deploy_command) struct DeployProviderMetadata {
    pub schema: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner::deploy_command) struct DeployProviderCapabilities {
    #[serde(default)]
    pub checklist: Option<String>,
    #[serde(default)]
    pub export: Option<String>,
    #[serde(default)]
    pub validate: Option<String>,
    #[serde(default)]
    pub preflight: Option<String>,
    #[serde(default)]
    pub apply: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner::deploy_command) struct DeployProviderPolicy {
    #[serde(default)]
    pub creates_projects: bool,
    #[serde(default)]
    pub creates_services: bool,
    #[serde(default)]
    pub creates_resources: bool,
    #[serde(default)]
    pub creates_variables: bool,
    #[serde(default)]
    pub creates_domains: bool,
    #[serde(default)]
    pub prints_secret_values: bool,
}

pub(super) fn read_provider_descriptor(root: &Path) -> Result<DeployProviderDescriptor, RunnerError> {
    let path = root.join("provider.toml");
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read deploy provider descriptor {}: {error}",
            path.display()
        ))
    })?;
    toml::from_str(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse deploy provider descriptor {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn validate_capability_paths(
    root: &Path,
    provider_name: &str,
    capabilities: &DeployProviderCapabilities,
) -> Result<(), RunnerError> {
    for (phase, path) in [
        ("checklist", &capabilities.checklist),
        ("export", &capabilities.export),
        ("validate", &capabilities.validate),
        ("preflight", &capabilities.preflight),
        ("apply", &capabilities.apply),
        ("status", &capabilities.status),
    ] {
        let Some(path) = path else {
            continue;
        };
        validate_provider_relative_path(provider_name, phase, path)?;
        let resolved = root.join(path);
        if !resolved.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` capability `{phase}` points to missing script {}",
                resolved.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_provider_name(provider_name: &str) -> Result<(), RunnerError> {
    let valid = !provider_name.is_empty()
        && provider_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` has an invalid id; use ASCII letters, numbers, `.`, `_`, or `-`"
    )))
}

pub(super) fn validate_provider_relative_path(
    provider_name: &str,
    phase: &str,
    path: &str,
) -> Result<(), RunnerError> {
    let candidate = Path::new(path);
    let valid = !candidate.is_absolute()
        && candidate
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if valid {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "deploy provider `{provider_name}` capability `{phase}` must be a package-relative file path"
    )))
}


#[cfg(test)]
mod tests {
    use super::{
        read_provider_descriptor, validate_capability_paths, validate_provider_name,
        validate_provider_relative_path, DeployProviderCapabilities,
    };

    #[test]
    fn a_provider_id_must_be_ascii_and_path_safe() {
        validate_provider_name("acme-cdn_2").expect("a plain id is accepted");
        let error = validate_provider_name("../acme").expect_err("a path segment is not an id");
        assert!(
            error.to_string().contains("has an invalid id"),
            "got: {error}"
        );
    }

    #[test]
    fn a_capability_path_stays_inside_the_package() {
        validate_provider_relative_path("acme", "export", "scripts/export.rhai")
            .expect("a package-relative path is accepted");
        let error = validate_provider_relative_path("acme", "export", "../escape.rhai")
            .expect_err("a parent segment would leave the package");
        assert!(
            error
                .to_string()
                .contains("capability `export` must be a package-relative file path"),
            "got: {error}"
        );
    }

    #[test]
    fn a_capability_naming_a_missing_script_is_refused_with_its_path() {
        let root = tempfile::tempdir().expect("temp dir");
        let capabilities = DeployProviderCapabilities {
            export: Some("scripts/export.rhai".to_owned()),
            ..DeployProviderCapabilities::default()
        };
        let error = validate_capability_paths(root.path(), "acme", &capabilities)
            .expect_err("the capability points at no file");
        let message = error.to_string();
        assert!(
            message.contains("capability `export` points to missing script"),
            "got: {message}"
        );
    }

    #[test]
    fn a_descriptor_is_read_as_typed_metadata_and_refuses_unknown_fields() {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("provider.toml"),
            "[provider]\nschema = \"effigy.deploy-provider.v1\"\nname = \"acme\"\ndisplay_name = \"Acme\"\nversion = \"1.2.3\"\n",
        )
        .expect("write descriptor");
        let descriptor = read_provider_descriptor(root.path()).expect("read descriptor");
        assert_eq!(descriptor.provider.name, "acme");
        assert_eq!(descriptor.provider.version, "1.2.3");
        assert!(!descriptor.policy.creates_projects);

        std::fs::write(
            root.path().join("provider.toml"),
            "[provider]\nschema = \"effigy.deploy-provider.v1\"\nname = \"acme\"\ndisplay_name = \"Acme\"\nversion = \"1.2.3\"\nunexpected = true\n",
        )
        .expect("write descriptor");
        let error = read_provider_descriptor(root.path())
            .expect_err("an unknown descriptor field is refused, not ignored");
        assert!(
            error.to_string().contains("failed to parse deploy provider descriptor"),
            "got: {error}"
        );
    }
}
