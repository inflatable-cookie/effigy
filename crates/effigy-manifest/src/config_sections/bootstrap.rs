//! `[bootstrap]` — the one-command declaration for someone else's project
//! (Q-009, contract 020).
//!
//! `setup` is an ordered list of ordinary task selectors and `start` is one
//! ordinary task selector, so both dispatch through the same pipeline as
//! `effigy <selector>`. `submodules = true` initialises submodules recursively
//! at their recorded commits. Pinned extra sources live in `[sources.<name>]`
//! and replace the removed `[bootstrap].children` table.

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapConfig {
    /// Ordered task selectors run after the checkout, its sources and its
    /// submodules are ready.
    #[serde(default)]
    pub setup: Vec<String>,
    /// The one task selector started last unless `--no-start` is passed.
    #[serde(default)]
    pub start: Option<String>,
    /// Recursively initialise submodules at their recorded commits.
    #[serde(default)]
    pub submodules: bool,
}

/// One pinned extra source acquired beside the project checkout.
///
/// Exactly one of `rev` and `rev_file` selects the commit. An existing working
/// copy is never reset, fetched forward or cleaned: a different HEAD or local
/// changes are reported as drift and left untouched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestPinnedSourceConfig {
    pub git: String,
    #[serde(default)]
    pub rev: Option<String>,
    /// A file holding the pinned commit, relative to the manifest. The brief
    /// spells it `rev_file`; kebab-case is accepted as an alias.
    #[serde(default, rename = "rev_file", alias = "rev-file")]
    pub rev_file: Option<String>,
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::{ManifestBootstrapConfig, ManifestPinnedSourceConfig};
    fn parse_sources(text: &str) -> Vec<(String, ManifestPinnedSourceConfig)> {
        let manifest: crate::TaskManifest = toml::from_str(text).expect("parse manifest");
        manifest.sources.into_iter().collect()
    }

    #[test]
    fn bootstrap_accepts_selector_lists_and_one_start() {
        let bootstrap: ManifestBootstrapConfig = toml::from_str(
            r#"
setup = ["workspace:js:prepare", "db:seed:schema"]
start = "dev"
submodules = true
"#,
        )
        .expect("parse bootstrap");
        assert_eq!(
            bootstrap.setup,
            vec!["workspace:js:prepare".to_owned(), "db:seed:schema".to_owned()]
        );
        assert_eq!(bootstrap.start.as_deref(), Some("dev"));
        assert!(bootstrap.submodules);
    }

    #[test]
    fn bootstrap_defaults_to_nothing_configured() {
        let bootstrap: ManifestBootstrapConfig = toml::from_str("").expect("parse bootstrap");
        assert!(bootstrap.setup.is_empty());
        assert_eq!(bootstrap.start, None);
        assert!(!bootstrap.submodules);
    }

    #[test]
    fn pinned_source_reads_rev_or_rev_file() {
        let sources = parse_sources(
            r#"
[sources.book]
git = "git@github.com:contact-patch/book.git"
rev = "0123456789abcdef0123456789abcdef01234567"
path = "../book"

[sources.content]
git = "git@github.com:acme/content.git"
rev_file = "content.rev"
path = "../content"
"#,
        );
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].0, "book");
        assert_eq!(sources[0].1.rev.as_deref().map(str::len), Some(40));
        assert_eq!(sources[0].1.path, "../book");
        assert_eq!(sources[1].0, "content");
        assert_eq!(sources[1].1.rev_file.as_deref(), Some("content.rev"));
    }
}
