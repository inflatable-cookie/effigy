//! User-global per-developer Effigy configuration.
//!
//! Lives at `~/.effigy/config.toml`. Distinct from the user-global
//! catalog-fragment override directory (`~/.effigy/catalog/`), which is loaded
//! by `effigy-containers` separately. This file holds bundle-keyed
//! per-developer settings that should never be checked into a project's
//! `effigy.toml` because they reference paths only present on the local
//! machine (e.g. parent directories of library checkouts).
//!
//! Schema, with room to grow:
//!
//! ```toml
//! [bundle.decodelabs]
//! library_mounts = [
//!   "~/Dev/legacy/libraries/decodelabs",
//! ]
//! ```
//!
//! Each entry under `library_mounts` is a host path. When a project's
//! manifest declares `[bundle].base = "<name>"` matching one of the
//! `[bundle.<name>]` blocks, those library paths get bind-mounted onto the
//! workspace container under `/workspace-libraries/<basename>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ManifestError;

/// Filename of the user-global config file inside the Effigy home dir.
pub const USER_CONFIG_FILE: &str = "config.toml";

/// Top-level user-global config schema.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    #[serde(default)]
    pub bundle: BTreeMap<String, UserBundleConfig>,
}

/// Per-bundle user-global override block.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserBundleConfig {
    /// Parent directories whose contents should be bind-mounted into the
    /// workspace container under `/workspace-libraries/<basename>`.
    #[serde(default)]
    pub library_mounts: Vec<String>,
}

/// A library mount entry resolved from user-global config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMount {
    /// Original raw entry as written in `config.toml` (preserved for error
    /// messages and diagnostics).
    pub raw: String,
    /// Host-side path with `~` expanded. Not canonicalized — callers may
    /// canonicalize at mount-resolution time.
    pub host_path: PathBuf,
}

impl UserConfig {
    /// Returns the configured library mounts for the named bundle, with
    /// `~` and relative paths expanded against the user's home directory.
    /// Returns an empty vec if the bundle has no entry.
    pub fn library_mounts_for(&self, bundle_name: &str) -> Vec<LibraryMount> {
        self.bundle
            .get(bundle_name)
            .map(|cfg| {
                cfg.library_mounts
                    .iter()
                    .map(|raw| LibraryMount {
                        raw: raw.clone(),
                        host_path: expand_user_path(raw),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Locate the Effigy user-global home directory. Mirrors the convention used
/// by `effigy-containers::policy_support::effigy_home_dir`.
fn effigy_home_dir() -> Option<PathBuf> {
    if let Some(path) = test_user_config_home_override() {
        return Some(path);
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".effigy"))
}

/// Load the user-global config from `~/.effigy/config.toml`.
///
/// Returns an empty `UserConfig` when the file does not exist; that's the
/// expected state for users who haven't opted into any per-developer
/// overrides. Returns a `ManifestError` only when the file is present but
/// unreadable or malformed.
pub fn load_user_config() -> Result<UserConfig, ManifestError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(UserConfig::default());
    };
    let path = home.join(USER_CONFIG_FILE);
    load_user_config_from(&path)
}

/// Load the user-global config from an explicit path. Used by tests; also
/// reusable if downstream tooling ever wants to read a non-default file.
pub fn load_user_config_from(path: &Path) -> Result<UserConfig, ManifestError> {
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let content = std::fs::read_to_string(path).map_err(|error| ManifestError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    toml::from_str::<UserConfig>(&content).map_err(|error| ManifestError::Parse {
        path: path.to_path_buf(),
        error,
    })
}

/// Expand `~` and `~/...` against the user's home directory. Other path
/// shapes (absolute, relative without `~`) are returned as-is so the caller
/// can decide how to anchor relatives.
fn expand_user_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// Test-only override of the user-global Effigy home directory. Always
// compiled in (not `#[cfg(test)]`-gated) so downstream crate test modules
// can drive it. The thread-local defaults to `None`, so production paths
// are unaffected.
thread_local! {
    static TEST_USER_CONFIG_HOME: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn test_user_config_home_override() -> Option<PathBuf> {
    TEST_USER_CONFIG_HOME.with(|slot| slot.borrow().clone())
}

/// Run `f` with the user-global Effigy home overridden to `path`. Restores
/// the previous override on drop, including across panics. Intended for
/// test setup — production callers have no reason to use this.
pub fn with_test_user_config_home<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<PathBuf>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_USER_CONFIG_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
    let previous = TEST_USER_CONFIG_HOME.with(|slot| slot.borrow_mut().replace(path.to_path_buf()));
    let _guard = ResetGuard(previous);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_empty_config() {
        let tmp = tempdir().expect("tempdir");
        let cfg = load_user_config_from(&tmp.path().join("config.toml")).expect("load");
        assert!(cfg.bundle.is_empty());
    }

    #[test]
    fn parses_bundle_keyed_library_mounts() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[bundle.decodelabs]
library_mounts = [
  "~/Dev/legacy/libraries/decodelabs",
  "/abs/path/libs",
]

[bundle.underlay]
library_mounts = ["~/Dev/u-libs"]
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");

        let decodelabs = cfg.library_mounts_for("decodelabs");
        assert_eq!(decodelabs.len(), 2);
        assert_eq!(decodelabs[1].host_path, PathBuf::from("/abs/path/libs"));
        // First entry should have `~` expanded.
        let first = &decodelabs[0].host_path;
        assert!(
            !first.starts_with("~"),
            "expected `~` to expand, got {first:?}"
        );
        assert!(first.ends_with("Dev/legacy/libraries/decodelabs"));

        let underlay = cfg.library_mounts_for("underlay");
        assert_eq!(underlay.len(), 1);

        let unknown = cfg.library_mounts_for("does-not-exist");
        assert!(unknown.is_empty());
    }

    #[test]
    fn malformed_file_returns_parse_error() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not valid = toml = at all =").expect("write");
        let err = load_user_config_from(&path).expect_err("should fail");
        assert!(matches!(err, ManifestError::Parse { .. }));
    }

    #[test]
    fn unknown_field_under_bundle_block_is_rejected() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[bundle.decodelabs]
library_mounts = []
unexpected_field = "nope"
"#,
        )
        .expect("write");
        let err = load_user_config_from(&path).expect_err("should fail");
        assert!(matches!(err, ManifestError::Parse { .. }));
    }

    #[test]
    fn home_override_redirects_default_load_path() {
        let tmp = tempdir().expect("tempdir");
        let home = tmp.path();
        std::fs::write(
            home.join("config.toml"),
            r#"
[bundle.decodelabs]
library_mounts = ["~/from-override"]
"#,
        )
        .expect("write");
        with_test_user_config_home(home, || {
            let cfg = load_user_config().expect("load via default path");
            let mounts = cfg.library_mounts_for("decodelabs");
            assert_eq!(mounts.len(), 1);
            assert_eq!(mounts[0].raw, "~/from-override");
        });
    }
}
