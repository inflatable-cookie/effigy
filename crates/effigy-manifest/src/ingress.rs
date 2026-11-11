//! Source values retain their provenance and notices through composition.
//! Includes and local overlays compose before bootstrap normalisation; bundle
//! defaults normalise separately before filling the consumer's missing values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::retired_config::RetiredConfigDeclaration;
use crate::{ManifestError, TaskManifest};

#[derive(Debug, Clone)]
pub(crate) struct ManifestInput {
    pub value: Value,
    pub source_map: BTreeMap<String, PathBuf>,
    pub retired_config: RetiredConfigDeclaration,
    pub extend_paths: Vec<String>,
}

impl ManifestInput {
    pub fn new(value: Value, source: Option<&Path>) -> Self {
        let mut source_map = BTreeMap::new();
        if let Some(source) = source {
            record_value_sources("", &value, source, &mut source_map);
        }
        Self {
            retired_config: crate::legacy_compat::scan(&value),
            value,
            source_map,
            extend_paths: Vec::new(),
        }
    }

    pub fn normalize(&mut self) {
        self.retired_config
            .merge(crate::retired_bootstrap::translate(&mut self.value));
    }

    pub fn deserialize(&self, path: &Path) -> Result<TaskManifest, ManifestError> {
        let manifest = self
            .clone()
            .into_manifest()
            .map_err(|error| ManifestError::Parse {
                path: path.to_path_buf(),
                error,
            })?;
        manifest.validate(path)?;
        Ok(manifest)
    }
}

/// Missing defaults never replace a fragment's provenance. Replacement callers
/// remove their old entries first, so the same traversal serves both policies.
pub(crate) fn record_value_sources(
    path: &str,
    value: &Value,
    source: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) {
    if !path.is_empty() {
        out.entry(path.to_owned())
            .or_insert_with(|| source.to_path_buf());
    }
    if let Some(table) = value.as_table() {
        for (key, child) in table {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            record_value_sources(&child_path, child, source, out);
        }
    }
}
