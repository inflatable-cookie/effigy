use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use crate::ManifestError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyConfig {
    #[serde(default)]
    pub indexes: BTreeMap<String, ManifestDocsPolicyIndexConfig>,
    #[serde(default, alias = "next_actions")]
    pub next_actions: BTreeMap<String, ManifestDocsPolicyNextActionConfig>,
    #[serde(default)]
    pub graph: Option<ManifestDocsPolicyGraphConfig>,
    #[serde(default)]
    pub sources: Option<ManifestDocsPolicySourcesConfig>,
}

impl ManifestDocsPolicyConfig {
    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        if let Some(graph) = &self.graph {
            graph.validate(manifest_path)?;
        }
        if let Some(sources) = &self.sources {
            sources.validate(manifest_path)?;
        }
        Ok(())
    }
}

/// Repository-owned cross-repository sharing declaration (`[docs_policy.sources]`).
///
/// This is the repository's own half of two-sided membership: a portfolio file
/// names where to look, and this table says the repository wants to be found.
/// Absent table means the repository is never searched from another checkout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicySourcesConfig {
    /// Opt in to cross-repository routing. Absent or `false` is `not-shared`.
    #[serde(default)]
    pub share: bool,
    /// Repository-relative files an agent should read first.
    #[serde(default, alias = "front_doors")]
    pub front_doors: Vec<String>,
    /// Repository-relative directories holding agent skills.
    #[serde(default, alias = "skill_roots")]
    pub skill_roots: Vec<String>,
}

impl ManifestDocsPolicySourcesConfig {
    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        for (index, front_door) in self.front_doors.iter().enumerate() {
            validate_repo_relative_path(
                manifest_path,
                &format!("docs_policy.sources.front_doors[{index}]"),
                front_door,
            )?;
        }
        for (index, skill_root) in self.skill_roots.iter().enumerate() {
            validate_repo_relative_path(
                manifest_path,
                &format!("docs_policy.sources.skill_roots[{index}]"),
                skill_root,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyIndexConfig {
    pub file: String,
    pub dir: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyNextActionConfig {
    pub index: String,
    pub heading: String,
    #[serde(alias = "allowlist_file")]
    pub allowlist_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyGraphConfig {
    pub roots: Vec<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, ManifestDocsPolicyGraphFieldConfig>,
    #[serde(default)]
    pub currentness: Option<ManifestDocsPolicyGraphCurrentnessConfig>,
    #[serde(default)]
    pub kinds: BTreeMap<String, ManifestDocsPolicyGraphKindConfig>,
    #[serde(default)]
    pub relations: BTreeMap<String, ManifestDocsPolicyGraphRelationConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDocsPolicyGraphCardinality {
    #[default]
    One,
    Many,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDocsPolicyGraphCurrentnessClass {
    Current,
    Historical,
    #[default]
    Unknown,
}

impl ManifestDocsPolicyGraphCurrentnessClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyGraphFieldConfig {
    pub labels: Vec<String>,
    #[serde(default)]
    pub cardinality: ManifestDocsPolicyGraphCardinality,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyGraphCurrentnessConfig {
    pub field: String,
    pub current: Vec<String>,
    pub historical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyGraphKindConfig {
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub authority: i64,
    #[serde(default, alias = "default_currentness")]
    pub default_currentness: ManifestDocsPolicyGraphCurrentnessClass,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyGraphRelationConfig {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub headings: Vec<String>,
}

impl ManifestDocsPolicyGraphConfig {
    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        if self.roots.is_empty() {
            return Err(graph_error(
                manifest_path,
                "`docs_policy.graph.roots` must contain at least one repository-relative path",
            ));
        }
        for (index, root) in self.roots.iter().enumerate() {
            validate_repo_relative_path(
                manifest_path,
                &format!("docs_policy.graph.roots[{index}]"),
                root,
            )?;
        }

        for (name, field) in &self.fields {
            validate_token(manifest_path, "docs_policy.graph.fields", name)?;
            validate_non_empty_label_list(
                manifest_path,
                &format!("docs_policy.graph.fields.{name}.labels"),
                &field.labels,
            )?;
        }

        if let Some(currentness) = &self.currentness {
            validate_currentness(manifest_path, currentness, &self.fields)?;
        }

        for (name, kind) in &self.kinds {
            validate_token(manifest_path, "docs_policy.graph.kinds", name)?;
            if kind.include.is_empty() {
                return Err(graph_error(
                    manifest_path,
                    format!(
                        "`docs_policy.graph.kinds.{name}.include` must contain at least one glob"
                    ),
                ));
            }
            for (index, pattern) in kind.include.iter().enumerate() {
                validate_repo_relative_path(
                    manifest_path,
                    &format!("docs_policy.graph.kinds.{name}.include[{index}]"),
                    pattern,
                )?;
            }
            for (index, pattern) in kind.exclude.iter().enumerate() {
                validate_repo_relative_path(
                    manifest_path,
                    &format!("docs_policy.graph.kinds.{name}.exclude[{index}]"),
                    pattern,
                )?;
            }
            if !(0..=100).contains(&kind.authority) {
                return Err(graph_error(
                    manifest_path,
                    format!(
                        "`docs_policy.graph.kinds.{name}.authority` must be an integer from 0 through 100"
                    ),
                ));
            }
        }

        for (name, relation) in &self.relations {
            validate_token(manifest_path, "docs_policy.graph.relations", name)?;
            if relation.labels.is_empty() && relation.headings.is_empty() {
                return Err(graph_error(
                    manifest_path,
                    format!(
                        "`docs_policy.graph.relations.{name}` must declare at least one label or heading selector"
                    ),
                ));
            }
            validate_non_empty_label_list(
                manifest_path,
                &format!("docs_policy.graph.relations.{name}.labels"),
                &relation.labels,
            )?;
            validate_non_empty_label_list(
                manifest_path,
                &format!("docs_policy.graph.relations.{name}.headings"),
                &relation.headings,
            )?;
        }

        Ok(())
    }
}

fn validate_currentness(
    manifest_path: &Path,
    currentness: &ManifestDocsPolicyGraphCurrentnessConfig,
    fields: &BTreeMap<String, ManifestDocsPolicyGraphFieldConfig>,
) -> Result<(), ManifestError> {
    let field_name = currentness.field.trim();
    if field_name.is_empty() {
        return Err(graph_error(
            manifest_path,
            "`docs_policy.graph.currentness.field` must be a non-empty field token",
        ));
    }
    if !fields.contains_key(field_name) {
        return Err(graph_error(
            manifest_path,
            format!(
                "`docs_policy.graph.currentness.field` references undeclared field `{field_name}`"
            ),
        ));
    }
    if currentness.current.is_empty() {
        return Err(graph_error(
            manifest_path,
            "`docs_policy.graph.currentness.current` must contain at least one value",
        ));
    }
    if currentness.historical.is_empty() {
        return Err(graph_error(
            manifest_path,
            "`docs_policy.graph.currentness.historical` must contain at least one value",
        ));
    }

    let mut current_set = BTreeSet::new();
    for (index, value) in currentness.current.iter().enumerate() {
        let normalized = normalize_compare_value(value).ok_or_else(|| {
            graph_error(
                manifest_path,
                format!(
                    "`docs_policy.graph.currentness.current[{index}]` must be a non-empty string"
                ),
            )
        })?;
        if !current_set.insert(normalized) {
            return Err(graph_error(
                manifest_path,
                format!(
                    "`docs_policy.graph.currentness.current` contains duplicate value `{value}`"
                ),
            ));
        }
    }

    let mut historical_set = BTreeSet::new();
    for (index, value) in currentness.historical.iter().enumerate() {
        let normalized = normalize_compare_value(value).ok_or_else(|| {
            graph_error(
                manifest_path,
                format!(
                    "`docs_policy.graph.currentness.historical[{index}]` must be a non-empty string"
                ),
            )
        })?;
        if current_set.contains(&normalized) {
            return Err(graph_error(
                manifest_path,
                format!(
                    "`docs_policy.graph.currentness` value `{value}` is in both `current` and `historical`"
                ),
            ));
        }
        if !historical_set.insert(normalized) {
            return Err(graph_error(
                manifest_path,
                format!(
                    "`docs_policy.graph.currentness.historical` contains duplicate value `{value}`"
                ),
            ));
        }
    }

    Ok(())
}

fn validate_token(manifest_path: &Path, map_path: &str, name: &str) -> Result<(), ManifestError> {
    if name.trim().is_empty() {
        return Err(graph_error(
            manifest_path,
            format!("`{map_path}` entries must use a non-empty token"),
        ));
    }
    if name.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err(graph_error(
            manifest_path,
            format!("`{map_path}.{name}` must not contain whitespace or control characters"),
        ));
    }
    Ok(())
}

fn validate_non_empty_label_list(
    manifest_path: &Path,
    field_path: &str,
    values: &[String],
) -> Result<(), ManifestError> {
    if field_path.ends_with(".labels") && values.is_empty() && field_path.contains(".fields.") {
        return Err(graph_error(
            manifest_path,
            format!("`{field_path}` must contain at least one non-empty label"),
        ));
    }
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            return Err(graph_error(
                manifest_path,
                format!("`{field_path}[{index}]` must be a non-empty string"),
            ));
        }
    }
    Ok(())
}

fn validate_repo_relative_path(
    manifest_path: &Path,
    field_path: &str,
    value: &str,
) -> Result<(), ManifestError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(graph_error(
            manifest_path,
            format!("`{field_path}` must be a non-empty repository-relative path"),
        ));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(graph_error(
            manifest_path,
            format!(
                "`{field_path}` must stay inside the selected repository; `{trimmed}` is absolute"
            ),
        ));
    }
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(graph_error(
                    manifest_path,
                    format!(
                        "`{field_path}` must stay inside the selected repository; `{trimmed}` escapes with `..`"
                    ),
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(graph_error(
                    manifest_path,
                    format!("`{field_path}` must stay inside the selected repository; `{trimmed}` is not repository-relative"),
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

fn normalize_compare_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn graph_error(manifest_path: &Path, detail: impl Into<String>) -> ManifestError {
    ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: detail.into(),
    }
}
