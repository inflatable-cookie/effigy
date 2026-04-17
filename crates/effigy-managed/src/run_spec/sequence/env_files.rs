use std::path::{Path, PathBuf};

use crate::ManagedError;
use effigy_manifest::ManifestEnvFileDirective;

use super::pathing::normalize_path;

pub fn normalize_env_file_directive(
    env_file: Option<&ManifestEnvFileDirective>,
    field_label: &str,
) -> Result<Option<Vec<String>>, ManagedError> {
    let Some(env_file) = env_file else {
        return Ok(None);
    };

    let entries = match env_file {
        ManifestEnvFileDirective::Single(value) => {
            vec![normalize_env_file_entry(value, field_label, None)?]
        }
        ManifestEnvFileDirective::Many(values) => {
            if values.is_empty() {
                return Err(ManagedError::task_invocation(format!(
                    "{field_label} is invalid: array cannot be empty"
                )));
            }
            let mut normalized = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                normalized.push(normalize_env_file_entry(value, field_label, Some(index))?);
            }
            normalized
        }
    };

    Ok(Some(entries))
}

pub fn resolve_env_file_paths(catalog_root: &Path, env_files: Option<&[String]>) -> Vec<PathBuf> {
    let defaults = vec![".env".to_owned()];
    let env_files = env_files.unwrap_or(defaults.as_slice());
    env_files
        .iter()
        .map(|env_file| {
            let resolved = if Path::new(env_file).is_absolute() {
                PathBuf::from(env_file)
            } else {
                catalog_root.join(env_file)
            };
            normalize_path(&resolved)
        })
        .collect::<Vec<PathBuf>>()
}

fn normalize_env_file_entry(
    value: &str,
    field_label: &str,
    index: Option<usize>,
) -> Result<String, ManagedError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        let suffix = index.map(|idx| format!("[{idx}]")).unwrap_or_default();
        return Err(ManagedError::task_invocation(format!(
            "{field_label}{suffix} is invalid: value cannot be empty"
        )));
    }
    Ok(normalized.to_owned())
}
