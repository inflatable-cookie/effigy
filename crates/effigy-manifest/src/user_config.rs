//! User-global per-developer Effigy configuration.
//!
//! Lives at `~/.effigy/config.toml`. Distinct from the retired user catalog-fragment
//! directory, which is ignored. This file holds bundle-keyed
//! per-developer settings that should never be checked into a project's
//! `effigy.toml` because they reference paths only present on the local
//! machine (e.g. parent directories of library checkouts).
//!
//! Schema, with room to grow:
//!
//! ```toml
//! [containers]
//! backend = "containerd"
//! profile = "effigy"
//! profile_disk_gib = 300
//!
//! [bundle.php_app]
//! library_mounts = [
//!   "~/Dev/legacy/libraries/php-app",
//! ]
//!
//! [scratch]
//! root = "/Volumes/Builds/effigy-scratch"
//! volume_uuid = "1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00"
//! require_separate_volume = true
//!
//! [programs.oxide]
//! fallback = "cargo"
//! allow = ["check", "build"]
//! force = false
//! ```
//!
//! `[programs.<name>]` is host-machine policy, so only this user-global file
//! may declare it. A project `effigy.toml` that names `programs` is refused as
//! an unknown manifest field, because one committed file must not silently
//! replace a program for every contributor.
//!
//! Each entry under `library_mounts` is a host path. When a project's
//! manifest declares `[bundle].base = "<name>"` matching one of the
//! `[bundle.<name>]` blocks, those library paths get bind-mounted onto the
//! workspace container under `/workspace-libraries/<basename>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ManifestError;

/// Filename of the user-global config file inside the Effigy home dir.
pub const USER_CONFIG_FILE: &str = "config.toml";

/// Top-level user-global config schema.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    #[serde(default)]
    pub containers: UserContainersConfig,
    #[serde(default)]
    pub bundle: BTreeMap<String, UserBundleConfig>,
    #[serde(default)]
    pub scratch: UserScratchConfig,
    /// Per-program fallbacks keyed by the program's name on `PATH`.
    ///
    /// The runner validates and activates these; this crate only parses the
    /// declared shape so a malformed block is a config error rather than a
    /// silently ignored setting.
    #[serde(default)]
    pub programs: BTreeMap<String, UserProgramConfig>,
}

/// One `[programs.<name>]` block: run `fallback` when `<name>` is unavailable
/// on the inherited `PATH`, or always with `force = true`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserProgramConfig {
    /// The program to run instead. Required by the runner, which refuses a
    /// block without one.
    #[serde(default)]
    pub fallback: Option<String>,
    /// Optional first-argument allow list. Omitted means unrestricted exact
    /// argv; present means only these first arguments run.
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    /// Activate the fallback even when the program is available on `PATH`.
    #[serde(default)]
    pub force: Option<bool>,
}

/// User-global per-run scratch directory preferences.
///
/// Unset means no change: Effigy leaves the operating system's temp directory
/// alone. A `root` moves the scratch of every host-placed run onto that
/// volume, and Effigy refuses a root it cannot prove is a separate, trusted
/// volume root (see the user guide). No path or UUID is hardcoded here: both
/// come from this file.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserScratchConfig {
    /// Directory that holds one subdirectory per run. Unset disables scratch.
    #[serde(default)]
    pub root: Option<String>,
    /// Volume UUID the root must live on, compared against the volume that
    /// actually holds it. Unset skips the pin.
    #[serde(default)]
    pub volume_uuid: Option<String>,
    /// Whether the root must be a separate volume from the boot volume.
    /// Unset (or `true`) requires it; `false` accepts any mounted volume
    /// that passes the structural checks.
    #[serde(default)]
    pub require_separate_volume: Option<bool>,
}

/// User-global container runtime preferences.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserContainersConfig {
    #[serde(default)]
    pub backend: Option<UserContainerBackendPreference>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub profile_disk_gib: Option<u64>,
}

/// User-global backend preference for runtime/container operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UserContainerBackendPreference {
    #[serde(alias = "colima-nerdctl")]
    Containerd,
    #[serde(alias = "docker-compose")]
    Docker,
}

/// Per-bundle user-global override block.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
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
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
            && self.bundle.is_empty()
            && self.scratch.is_empty()
            && self.programs.is_empty()
    }

    /// Returns the configured library mounts for the named bundle, with
    /// `~` and relative paths expanded against the user's home directory.
    /// Returns an empty vec if the bundle has no entry.
    pub fn library_mounts_for(&self, bundle_name: &str) -> Vec<LibraryMount> {
        self.bundle_config_for(bundle_name)
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

    fn bundle_config_for(&self, bundle_name: &str) -> Option<&UserBundleConfig> {
        self.bundle
            .get(bundle_name)
            .or_else(|| self.bundle.get(&bundle_name.replace('-', "_")))
            .or_else(|| self.bundle.get(&bundle_name.replace('_', "-")))
    }

    pub fn preferred_container_backend(&self) -> Option<UserContainerBackendPreference> {
        self.containers.backend
    }

    pub fn preferred_container_profile(&self) -> Option<&str> {
        self.containers.preferred_profile()
    }

    pub fn preferred_container_profile_disk_gib(&self) -> Option<u64> {
        self.containers.preferred_profile_disk_gib()
    }
}

impl UserScratchConfig {
    pub fn is_empty(&self) -> bool {
        self.root().is_none()
    }

    /// The configured scratch root with `~` expanded, or `None` when unset or
    /// blank (the default: no change).
    pub fn root(&self) -> Option<PathBuf> {
        self.root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(expand_user_path)
    }

    /// The UUID pin, trimmed, when configured.
    pub fn volume_uuid(&self) -> Option<&str> {
        self.volume_uuid
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// A root must be a separate volume unless this is explicitly `false`.
    pub fn require_separate_volume(&self) -> bool {
        self.require_separate_volume.unwrap_or(true)
    }
}

impl UserContainersConfig {
    pub fn is_empty(&self) -> bool {
        self.backend.is_none()
            && self.preferred_profile().is_none()
            && self.preferred_profile_disk_gib().is_none()
    }

    pub fn preferred_profile(&self) -> Option<&str> {
        self.profile
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn preferred_profile_disk_gib(&self) -> Option<u64> {
        self.profile_disk_gib.filter(|value| *value > 0)
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
    let Some(path) = user_config_path() else {
        return Ok(UserConfig::default());
    };
    load_user_config_from(&path)
}

/// Resolve the default user-global config path (`~/.effigy/config.toml`).
pub fn user_config_path() -> Option<PathBuf> {
    effigy_home_dir().map(|home| home.join(USER_CONFIG_FILE))
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

/// Write the user-global config to the default path, creating the Effigy home
/// directory when needed.
pub fn save_user_config(config: &UserConfig) -> Result<PathBuf, ManifestError> {
    let Some(path) = user_config_path() else {
        return Err(ManifestError::Render {
            path: PathBuf::from(USER_CONFIG_FILE),
            detail: "HOME is not set; cannot resolve user-global config path".to_owned(),
        });
    };
    save_user_config_to(&path, config)?;
    Ok(path)
}

/// Write the user-global config to an explicit path.
pub fn save_user_config_to(path: &Path, config: &UserConfig) -> Result<(), ManifestError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
            path: parent.to_path_buf(),
            error,
        })?;
    }
    let rendered = toml::to_string_pretty(config).map_err(|error| ManifestError::Render {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    std::fs::write(path, rendered).map_err(|error| ManifestError::Read {
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
[bundle.php_app]
library_mounts = [
  "~/Dev/legacy/libraries/php-app",
  "/abs/path/libs",
]

[bundle.platform]
library_mounts = ["~/Dev/u-libs"]
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");

        let php_app = cfg.library_mounts_for("php-app");
        assert_eq!(php_app.len(), 2);
        assert_eq!(php_app[1].host_path, PathBuf::from("/abs/path/libs"));
        // First entry should have `~` expanded.
        let first = &php_app[0].host_path;
        assert!(
            !first.starts_with("~"),
            "expected `~` to expand, got {first:?}"
        );
        assert!(first.ends_with("Dev/legacy/libraries/php-app"));

        let platform = cfg.library_mounts_for("platform");
        assert_eq!(platform.len(), 1);

        let unknown = cfg.library_mounts_for("does-not-exist");
        assert!(unknown.is_empty());
    }

    #[test]
    fn parses_user_global_scratch_preferences() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[scratch]
root = "  /Volumes/Builds/effigy-scratch  "
volume_uuid = " 1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00 "
require_separate_volume = false
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");
        assert_eq!(
            cfg.scratch.root(),
            Some(PathBuf::from("/Volumes/Builds/effigy-scratch"))
        );
        assert_eq!(
            cfg.scratch.volume_uuid(),
            Some("1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00")
        );
        assert!(!cfg.scratch.require_separate_volume());
        assert!(!cfg.is_empty());
    }

    #[test]
    fn unset_scratch_changes_nothing_and_requires_a_separate_volume_by_default() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[scratch]
root = "~/scratch"
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");
        assert!(cfg.scratch.volume_uuid().is_none());
        assert!(cfg.scratch.require_separate_volume());
        assert!(cfg.scratch.root().expect("root").ends_with("scratch"));

        let empty = UserConfig::default();
        assert!(empty.scratch.root().is_none());
        assert!(empty.is_empty());
        assert!(load_user_config_from(&tmp.path().join("missing.toml"))
            .expect("absent file")
            .scratch
            .root()
            .is_none());
    }

    #[test]
    fn parses_user_global_program_fallbacks() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[programs.oxide]
fallback = "cargo"
allow = ["check", "build"]
force = false

[programs.turbo]
fallback = "bun"
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");
        assert!(!cfg.is_empty());
        let oxide = cfg.programs.get("oxide").expect("oxide block");
        assert_eq!(oxide.fallback.as_deref(), Some("cargo"));
        assert_eq!(
            oxide.allow,
            Some(vec!["check".to_owned(), "build".to_owned()])
        );
        assert_eq!(oxide.force, Some(false));
        let turbo = cfg.programs.get("turbo").expect("turbo block");
        assert_eq!(turbo.fallback.as_deref(), Some("bun"));
        assert!(turbo.allow.is_none());
        assert!(turbo.force.is_none());
    }

    #[test]
    fn unknown_program_field_is_rejected() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[programs.oxide]
fallback = "cargo"
recurse = true
"#,
        )
        .expect("write");
        let err = load_user_config_from(&path).expect_err("should fail");
        assert!(matches!(err, ManifestError::Parse { .. }));
    }

    #[test]
    fn parses_user_global_container_preferences() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[containers]
backend = "containerd"
profile = "effigy"
profile_disk_gib = 180
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");
        assert_eq!(
            cfg.preferred_container_backend(),
            Some(UserContainerBackendPreference::Containerd)
        );
        assert_eq!(cfg.preferred_container_profile(), Some("effigy"));
        assert_eq!(cfg.preferred_container_profile_disk_gib(), Some(180));
    }

    #[test]
    fn parses_backend_aliases() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[containers]
backend = "colima-nerdctl"
"#,
        )
        .expect("write");
        let cfg = load_user_config_from(&path).expect("load");
        assert_eq!(
            cfg.preferred_container_backend(),
            Some(UserContainerBackendPreference::Containerd)
        );
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
[bundle.php_app]
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
[bundle.php_app]
library_mounts = ["~/from-override"]
"#,
        )
        .expect("write");
        with_test_user_config_home(home, || {
            let cfg = load_user_config().expect("load via default path");
            let mounts = cfg.library_mounts_for("php-app");
            assert_eq!(mounts.len(), 1);
            assert_eq!(mounts[0].raw, "~/from-override");
        });
    }
}
