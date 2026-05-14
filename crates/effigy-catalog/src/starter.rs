//! Starter fragment loading and resolution.
//!
//! Starters are embedded scaffold shapes emitted by `effigy init`. Each
//! starter lives under `crates/effigy-catalog/starters/<name>/` with a
//! `starter.toml` descriptor declaring its identity, emitted files, and
//! optional post-emission guidance.
//!
//! For v1 starters are bundled-only. Filesystem/user overrides and
//! remote/versioned sources are intentionally out of scope (see roadmap
//! `g02.021`).

use std::collections::BTreeSet;

use serde::Deserialize;

/// Embedded starter assets — bundled into the binary at compile time.
#[derive(rust_embed::RustEmbed)]
#[folder = "starters/"]
struct BundledStarters;

/// Errors raised while loading or emitting starters.
#[derive(Debug, thiserror::Error)]
pub enum StarterError {
    /// No starter with that name is registered in the bundled catalog.
    #[error("starter not found: {name}")]
    NotFound { name: String },

    /// The starter directory exists but has no `starter.toml`.
    #[error("missing starter.toml in starter '{name}'")]
    MissingDescriptor { name: String },

    /// The `starter.toml` descriptor could not be parsed.
    #[error("invalid starter.toml in starter '{name}': {reason}")]
    InvalidDescriptor { name: String, reason: String },

    /// The descriptor references a source file that is not bundled.
    #[error("starter '{name}' declares file '{path}' that is not bundled")]
    MissingSource { name: String, path: String },

    /// A bundled file could not be decoded as UTF-8.
    #[error("starter '{name}' has invalid UTF-8 in file '{path}'")]
    InvalidEncoding { name: String, path: String },
}

/// Raw `starter.toml` contents.
#[derive(Debug, Clone, Deserialize)]
struct StarterManifest {
    starter: StarterIdentity,
    #[serde(default)]
    files: Vec<StarterFileEntry>,
    #[serde(default)]
    guidance: Option<StarterGuidance>,
}

#[derive(Debug, Clone, Deserialize)]
struct StarterIdentity {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StarterFileEntry {
    target: String,
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StarterGuidance {
    text: String,
}

/// A loaded starter ready for emission.
#[derive(Debug, Clone)]
pub struct Starter {
    /// Starter name (directory name, also carried in the descriptor).
    pub name: String,
    /// Human-readable description from the descriptor.
    pub description: String,
    /// Files to emit into the target repo.
    pub files: Vec<StarterFile>,
    /// Optional post-emission guidance text.
    pub guidance: Option<String>,
}

/// One file in a starter: source contents bundled at compile time,
/// destined for `target` relative to the target repo root.
#[derive(Debug, Clone)]
pub struct StarterFile {
    /// Path relative to the target repo root.
    pub target: String,
    /// Bundled contents to write.
    pub contents: String,
}

/// Summary info for a starter (for listing).
#[derive(Debug, Clone)]
pub struct StarterInfo {
    pub name: String,
    pub description: String,
}

/// Resolves starters from the bundled starter catalog.
///
/// For v1 this is a stateless wrapper; the type exists so later batches
/// can introduce filesystem overrides without changing callers.
#[derive(Debug, Default, Clone, Copy)]
pub struct StarterResolver;

impl StarterResolver {
    /// Construct a new resolver. Equivalent to `StarterResolver::default()`.
    pub fn new() -> Self {
        Self
    }

    /// Resolve a starter by name, loading its descriptor and bundled files.
    pub fn resolve(&self, name: &str) -> Result<Starter, StarterError> {
        let descriptor = Self::load_descriptor(name)?;

        if descriptor.starter.name != name {
            return Err(StarterError::InvalidDescriptor {
                name: name.to_string(),
                reason: format!(
                    "starter.toml name field '{}' does not match directory '{}'",
                    descriptor.starter.name, name
                ),
            });
        }

        let mut files = Vec::with_capacity(descriptor.files.len());
        for entry in &descriptor.files {
            let source_path = entry.source.clone().unwrap_or_else(|| entry.target.clone());
            let key = format!("{name}/{source_path}");
            let data = BundledStarters::get(&key).ok_or_else(|| StarterError::MissingSource {
                name: name.to_string(),
                path: source_path.clone(),
            })?;
            let contents = std::str::from_utf8(data.data.as_ref())
                .map_err(|_| StarterError::InvalidEncoding {
                    name: name.to_string(),
                    path: source_path.clone(),
                })?
                .to_string();
            files.push(StarterFile {
                target: entry.target.clone(),
                contents,
            });
        }

        Ok(Starter {
            name: descriptor.starter.name,
            description: descriptor.starter.description,
            files,
            guidance: descriptor.guidance.map(|g| g.text),
        })
    }

    /// List every registered starter in the bundled catalog, sorted by name.
    ///
    /// A directory is "registered" when it contains a parseable
    /// `starter.toml`. Directories without one are silently skipped so
    /// reference-only starter trees do not
    /// show up in listings.
    pub fn list(&self) -> Vec<StarterInfo> {
        let mut names = BTreeSet::new();
        for path in BundledStarters::iter() {
            if let Some((name, rest)) = path.split_once('/') {
                if rest == "starter.toml" {
                    names.insert(name.to_string());
                }
            }
        }
        names
            .into_iter()
            .filter_map(|name| {
                self.resolve(&name).ok().map(|starter| StarterInfo {
                    name: starter.name,
                    description: starter.description,
                })
            })
            .collect()
    }

    fn load_descriptor(name: &str) -> Result<StarterManifest, StarterError> {
        let descriptor_key = format!("{name}/starter.toml");
        let data = BundledStarters::get(&descriptor_key).ok_or_else(|| {
            // Distinguish "directory does not exist" from "directory exists
            // but lacks a descriptor" so misconfigured starters surface
            // clearly.
            let prefix = format!("{name}/");
            if BundledStarters::iter().any(|p| p.starts_with(&prefix)) {
                StarterError::MissingDescriptor {
                    name: name.to_string(),
                }
            } else {
                StarterError::NotFound {
                    name: name.to_string(),
                }
            }
        })?;
        let text = std::str::from_utf8(data.data.as_ref()).map_err(|_| {
            StarterError::InvalidDescriptor {
                name: name.to_string(),
                reason: "invalid UTF-8 in starter.toml".to_string(),
            }
        })?;
        toml::from_str(text).map_err(|error| StarterError::InvalidDescriptor {
            name: name.to_string(),
            reason: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_starter_loads_with_expected_files() {
        let resolver = StarterResolver::new();
        let starter = resolver
            .resolve("minimal")
            .expect("minimal starter should resolve");

        assert_eq!(starter.name, "minimal");
        assert!(!starter.description.is_empty());
        assert_eq!(starter.files.len(), 2);
        let targets: Vec<&str> = starter.files.iter().map(|f| f.target.as_str()).collect();
        assert!(targets.contains(&"effigy.toml"));
        assert!(targets.contains(&"README.md"));
        assert!(starter.guidance.is_none());
    }

    #[test]
    fn minimal_starter_contents_include_baseline_scaffold_markers() {
        let resolver = StarterResolver::new();
        let starter = resolver.resolve("minimal").unwrap();
        let body = &starter.files[0].contents;

        for expected in [
            "[tasks]",
            "ping = \"printf ok\"",
            "# [tasks.dev]",
            "# [tasks.validate]",
        ] {
            assert!(
                body.contains(expected),
                "expected scaffold to contain '{expected}', got:\n{body}"
            );
        }
    }

    #[test]
    fn list_reports_registered_starters_sorted_by_name() {
        let resolver = StarterResolver::new();
        let listing = resolver.list();

        let names: Vec<&str> = listing.iter().map(|info| info.name.as_str()).collect();
        assert!(
            names.contains(&"minimal"),
            "minimal should be listed; got {listing:?}"
        );
        assert!(
            names.contains(&"northstar"),
            "northstar should be listed; got {listing:?}"
        );
        // Listing is sorted so `--list` output is stable.
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "listing is expected to be sorted by name");
    }

    #[test]
    fn resolve_unknown_starter_returns_not_found() {
        let resolver = StarterResolver::new();
        match resolver.resolve("does-not-exist") {
            Err(StarterError::NotFound { name }) => assert_eq!(name, "does-not-exist"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn northstar_starter_resolves_with_all_declared_files_and_guidance() {
        let resolver = StarterResolver::new();
        let starter = resolver
            .resolve("northstar")
            .expect("northstar starter should resolve");

        assert_eq!(starter.name, "northstar");
        let targets: Vec<&str> = starter.files.iter().map(|f| f.target.as_str()).collect();
        for expected in [
            "effigy.toml",
            "README.md",
            "AGENTS.md",
            "CHANGELOG.md",
            "docs/README.md",
            "docs/vision/README.md",
            "docs/vision/001-product-vision.md",
            "docs/roadmaps/README.md",
            "docs/logs/README.md",
            "docs/policy/vision-next-task-verbs.txt",
        ] {
            assert!(
                targets.contains(&expected),
                "expected northstar to declare {expected}; got {targets:?}"
            );
        }
        let guidance = starter.guidance.expect("northstar ships guidance text");
        assert!(guidance.contains("<PROJECT_NAME>"));
        assert!(guidance.contains("qa:northstar"));
    }
}
