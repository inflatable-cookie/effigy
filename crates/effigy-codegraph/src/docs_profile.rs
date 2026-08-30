use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_docs_policy_graph_config, ManifestDocsPolicyGraphCardinality,
    ManifestDocsPolicyGraphConfig, TASK_MANIFEST_FILE,
};
use globset::GlobBuilder;
use serde::Serialize;

use crate::error::CodeGraphError;
use crate::support::sha256_hex;

pub(crate) const DOCS_PROFILE_FINGERPRINT_KEY: &str = "docs_profile_fingerprint";
const BASELINE_FINGERPRINT_SEED: &str = "docs-profile:baseline";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocsProfileState {
    Baseline,
    Configured(CompiledDocsProfile),
}

impl DocsProfileState {
    pub fn fingerprint(&self) -> String {
        match self {
            Self::Baseline => sha256_hex(BASELINE_FINGERPRINT_SEED.as_bytes()),
            Self::Configured(profile) => profile.fingerprint(),
        }
    }

    pub fn compiled(&self) -> Option<&CompiledDocsProfile> {
        match self {
            Self::Baseline => None,
            Self::Configured(profile) => Some(profile),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsProfile {
    pub roots: Vec<CompiledDocsRoot>,
    pub fields: BTreeMap<String, CompiledDocsField>,
    pub currentness: Option<CompiledDocsCurrentness>,
    pub kinds: BTreeMap<String, CompiledDocsKind>,
    pub relations: BTreeMap<String, CompiledDocsRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsRoot {
    pub relative: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsField {
    pub token: String,
    pub labels: Vec<String>,
    pub label_keys: Vec<String>,
    pub single_valued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsCurrentness {
    pub field: String,
    pub current: Vec<String>,
    pub historical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsKind {
    pub token: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub authority: i64,
    pub default_currentness: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledDocsRelation {
    pub token: String,
    pub labels: Vec<String>,
    pub label_keys: Vec<String>,
    pub headings: Vec<String>,
    pub heading_keys: Vec<String>,
}

impl CompiledDocsProfile {
    pub fn fingerprint(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("compiled docs profile is serializable");
        sha256_hex(&encoded)
    }

    pub fn contains_path(&self, relative_path: &str) -> bool {
        self.roots.iter().any(|root| root.contains(relative_path))
    }

    pub fn kind_for(&self, relative_path: &str) -> Option<&CompiledDocsKind> {
        if !self.contains_path(relative_path) {
            return None;
        }
        self.kinds.values().find(|kind| kind.matches(relative_path))
    }

    pub fn fields_for_label(&self, label: &str) -> Vec<&CompiledDocsField> {
        let key = normalize_compare(label);
        self.fields
            .values()
            .filter(|field| field.label_keys.iter().any(|candidate| candidate == &key))
            .collect()
    }

    pub fn relations_for_label(&self, label: &str) -> Vec<&CompiledDocsRelation> {
        let key = normalize_compare(label);
        self.relations
            .values()
            .filter(|relation| {
                relation
                    .label_keys
                    .iter()
                    .any(|candidate| candidate == &key)
            })
            .collect()
    }

    pub fn relations_for_heading(&self, heading: &str) -> Vec<&CompiledDocsRelation> {
        let key = normalize_compare(heading);
        self.relations
            .values()
            .filter(|relation| {
                relation
                    .heading_keys
                    .iter()
                    .any(|candidate| candidate == &key)
            })
            .collect()
    }
}

impl CompiledDocsRoot {
    fn contains(&self, relative_path: &str) -> bool {
        if self.relative.is_empty() {
            return true;
        }
        if self.is_dir {
            relative_path == self.relative
                || relative_path.starts_with(&(self.relative.clone() + "/"))
        } else {
            relative_path == self.relative
        }
    }
}

impl CompiledDocsKind {
    fn matches(&self, relative_path: &str) -> bool {
        let included = self
            .include
            .iter()
            .any(|pattern| glob_matches(pattern, relative_path));
        if !included {
            return false;
        }
        !self
            .exclude
            .iter()
            .any(|pattern| glob_matches(pattern, relative_path))
    }
}

pub fn load_docs_profile_state(repo_root: &Path) -> Result<DocsProfileState, CodeGraphError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let graph = load_docs_policy_graph_config(&manifest_path).map_err(|error| {
        CodeGraphError::validation(format!(
            "invalid documentation graph profile in {}: {error}",
            manifest_path.display()
        ))
    })?;
    match graph {
        None => Ok(DocsProfileState::Baseline),
        Some(graph) => Ok(DocsProfileState::Configured(compile_docs_profile(
            repo_root, &graph,
        )?)),
    }
}

pub fn compile_docs_profile(
    repo_root: &Path,
    graph: &ManifestDocsPolicyGraphConfig,
) -> Result<CompiledDocsProfile, CodeGraphError> {
    let repo_canonical = canonicalize_existing(repo_root, "repository root")?;
    let mut roots = Vec::new();
    for root in &graph.roots {
        roots.push(compile_root(repo_root, &repo_canonical, root)?);
    }

    let mut fields = BTreeMap::new();
    for (token, field) in &graph.fields {
        fields.insert(
            token.clone(),
            CompiledDocsField {
                token: token.clone(),
                labels: trim_values(&field.labels),
                label_keys: compare_keys(&field.labels),
                single_valued: matches!(field.cardinality, ManifestDocsPolicyGraphCardinality::One),
            },
        );
    }

    let currentness = graph
        .currentness
        .as_ref()
        .map(|currentness| CompiledDocsCurrentness {
            field: currentness.field.trim().to_owned(),
            current: trim_values(&currentness.current),
            historical: trim_values(&currentness.historical),
        });

    let mut kinds = BTreeMap::new();
    for (token, kind) in &graph.kinds {
        kinds.insert(
            token.clone(),
            CompiledDocsKind {
                token: token.clone(),
                include: normalize_globs(&kind.include, token, "include")?,
                exclude: normalize_globs(&kind.exclude, token, "exclude")?,
                authority: kind.authority,
                default_currentness: kind.default_currentness.as_str().to_owned(),
            },
        );
    }

    let mut relations = BTreeMap::new();
    for (token, relation) in &graph.relations {
        relations.insert(
            token.clone(),
            CompiledDocsRelation {
                token: token.clone(),
                labels: trim_values(&relation.labels),
                label_keys: compare_keys(&relation.labels),
                headings: trim_values(&relation.headings),
                heading_keys: compare_keys(&relation.headings),
            },
        );
    }

    let profile = CompiledDocsProfile {
        roots,
        fields,
        currentness,
        kinds,
        relations,
    };
    reject_kind_overlaps(repo_root, &profile)?;
    Ok(profile)
}

fn compile_root(
    repo_root: &Path,
    repo_canonical: &Path,
    root: &str,
) -> Result<CompiledDocsRoot, CodeGraphError> {
    let relative = normalize_docs_path(root);
    let joined = if relative.is_empty() {
        repo_root.to_path_buf()
    } else {
        repo_root.join(&relative)
    };
    if !joined.exists() {
        return Err(CodeGraphError::validation(format!(
            "docs_policy.graph root `{relative}` does not exist"
        )));
    }
    let canonical =
        canonicalize_existing(&joined, &format!("docs_policy.graph root `{relative}`"))?;
    if !canonical.starts_with(repo_canonical) {
        return Err(CodeGraphError::validation(format!(
            "docs_policy.graph root `{relative}` escapes the selected repository"
        )));
    }
    Ok(CompiledDocsRoot {
        relative,
        is_dir: canonical.is_dir(),
    })
}

fn reject_kind_overlaps(
    repo_root: &Path,
    profile: &CompiledDocsProfile,
) -> Result<(), CodeGraphError> {
    if profile.kinds.is_empty() {
        return Ok(());
    }
    for entry in crate::walk::scan_repo_files(repo_root)? {
        if entry.language_id != "markdown" || !profile.contains_path(&entry.relative_path) {
            continue;
        }
        let matches: Vec<&str> = profile
            .kinds
            .values()
            .filter(|kind| kind.matches(&entry.relative_path))
            .map(|kind| kind.token.as_str())
            .collect();
        if matches.len() > 1 {
            return Err(CodeGraphError::validation(format!(
                "docs_policy.graph kinds overlap on `{}`: {}",
                entry.relative_path,
                matches.join(", ")
            )));
        }
    }
    Ok(())
}

fn canonicalize_existing(path: &Path, label: &str) -> Result<PathBuf, CodeGraphError> {
    path.canonicalize().map_err(|error| {
        CodeGraphError::validation(format!("{label} is not a usable path: {error}"))
    })
}

fn trim_values(values: &[String]) -> Vec<String> {
    values.iter().map(|value| value.trim().to_owned()).collect()
}

fn normalize_docs_path(value: &str) -> String {
    value
        .trim()
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn normalize_globs(
    patterns: &[String],
    kind: &str,
    field: &str,
) -> Result<Vec<String>, CodeGraphError> {
    let mut normalized = Vec::new();
    for (index, pattern) in patterns.iter().enumerate() {
        let candidate = normalize_docs_path(pattern);
        if candidate.is_empty() {
            return Err(CodeGraphError::validation(format!(
                "docs_policy.graph.kinds.{kind}.{field}[{index}] must be a repository-relative glob"
            )));
        }
        compile_glob(&candidate).map_err(|error| {
            CodeGraphError::validation(format!(
                "docs_policy.graph.kinds.{kind}.{field}[{index}] is not a valid glob `{pattern}`: {error}"
            ))
        })?;
        normalized.push(candidate);
    }
    Ok(normalized)
}

fn compile_glob(pattern: &str) -> Result<globset::GlobMatcher, globset::Error> {
    Ok(GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()?
        .compile_matcher())
}

fn compare_keys(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalize_compare(value))
        .collect()
}

fn normalize_compare(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn glob_matches(pattern: &str, path: &str) -> bool {
    let normalized = normalize_docs_path(pattern);
    compile_glob(&normalized)
        .map(|matcher| matcher.is_match(path))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{glob_matches, normalize_docs_path};

    #[test]
    fn glob_matches_single_segment_and_recursive_patterns() {
        assert!(glob_matches(
            "handbook/playbooks/*.md",
            "handbook/playbooks/setup.md"
        ));
        assert!(!glob_matches(
            "handbook/playbooks/*.md",
            "handbook/playbooks/nested/setup.md"
        ));
        assert!(glob_matches("handbook/**/*.md", "handbook/a/b.md"));
        assert!(glob_matches("README.md", "README.md"));
        assert!(!glob_matches("handbook/*.md", "notes/intro.md"));
        assert!(glob_matches("./handbook/*.md", "handbook/setup.md"));
        assert!(glob_matches("setup*guide.md", "setup-guide.md"));
        assert!(!glob_matches("setup*guide.md", "setup-guide-extra.md"));
        assert!(glob_matches("setup?guide.md", "setup-guide.md"));
        assert!(!glob_matches("setup?guide.md", "setup--guide.md"));
    }

    #[test]
    fn normalize_docs_path_drops_current_dir_segments() {
        assert_eq!(normalize_docs_path("."), "");
        assert_eq!(normalize_docs_path("./handbook"), "handbook");
        assert_eq!(normalize_docs_path("./handbook/*.md"), "handbook/*.md");
    }
}
