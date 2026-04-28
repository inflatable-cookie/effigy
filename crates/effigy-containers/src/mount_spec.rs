//! Resolution layer for `[containers.<name>.host].mounts`.
//!
//! Manifests accept two forms (see [`ManifestContainerHostMount`]):
//!
//! - Legacy `"host:container[:options]"` strings — repo-relative only.
//! - Structured tables — opt into out-of-repo sources via `external = true`,
//!   and support `${VAR}` (process env) and `~` expansion in `host`.
//!
//! Both forms are resolved here into a single canonical mount-spec string
//! (`<absolute-host-path>:<container>[:options]`) before being handed to the
//! compose / docker-run layer downstream.
//!
//! Policy:
//!
//! - Without `external = true`, the host source must canonicalise under the
//!   repo root (preserves the long-standing isolation guarantee).
//! - With `external = true`, the source may live anywhere on disk, but
//!   must still exist (we canonicalise to fail fast on misconfigured
//!   machines).
//!
//! `${VAR}` is the only interpolation form. `$VAR` (no braces) is left
//! intentionally unsupported to avoid shell-style ambiguity.

use std::path::{Path, PathBuf};

use effigy_manifest::{ManifestContainerHostMount, ManifestContainerHostMountTable};

use crate::ContainerPolicyError;

/// Render a manifest-level host mount declaration into a canonical
/// `host:container[:options]` mount-spec string.
pub(crate) fn resolve_host_mount(
    repo_root: &Path,
    container_name: &str,
    mount: &ManifestContainerHostMount,
) -> Result<String, ContainerPolicyError> {
    match mount {
        ManifestContainerHostMount::Spec(raw) => {
            resolve_legacy_spec(repo_root, container_name, raw)
        }
        ManifestContainerHostMount::Table(table) => {
            resolve_structured(repo_root, container_name, table)
        }
    }
}

fn resolve_legacy_spec(
    repo_root: &Path,
    container_name: &str,
    raw: &str,
) -> Result<String, ContainerPolicyError> {
    let trimmed = raw.trim();
    let mut parts = trimmed.splitn(3, ':');
    let source = parts.next().unwrap_or_default().trim();
    let target = parts.next().unwrap_or_default().trim();
    let options = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source.is_empty() || target.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` has invalid mount `{raw}`; expected `<source>:<target>[:options]`"
        )));
    }
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount `{raw}` must use a repo-relative source path; declare `external = true` in the structured form to source from outside the repo"
        )));
    }
    let canonical = canonicalize_under_repo(repo_root, container_name, source, raw)?;
    Ok(render(&canonical, target, options))
}

fn resolve_structured(
    repo_root: &Path,
    container_name: &str,
    table: &ManifestContainerHostMountTable,
) -> Result<String, ContainerPolicyError> {
    let target = table.container.trim();
    if target.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount has empty `container` path"
        )));
    }
    let raw_host = table.host.trim();
    if raw_host.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount has empty `host` path"
        )));
    }

    let expanded = expand_host_path(raw_host).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount `host` value `{raw_host}` could not be expanded: {error}"
        ))
    })?;

    let expanded_path = Path::new(&expanded);
    let canonical = if table.external {
        if !expanded_path.is_absolute() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` external mount `host` value `{raw_host}` (expanded to `{expanded}`) must resolve to an absolute path; use `~/...`, `${{VAR}}`, or a literal absolute path"
            )));
        }
        canonicalize_anywhere(container_name, &expanded, raw_host)?
    } else {
        if expanded_path.is_absolute() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `host` value `{raw_host}` resolved to an absolute path `{expanded}`; declare `external = true` to source from outside the repo"
            )));
        }
        canonicalize_under_repo(repo_root, container_name, &expanded, raw_host)?
    };

    let options = if table.options.is_empty() {
        None
    } else {
        Some(table.options.join(","))
    };
    Ok(render(&canonical, target, options.as_deref()))
}

fn render(canonical: &Path, target: &str, options: Option<&str>) -> String {
    match options {
        Some(options) if !options.is_empty() => {
            format!("{}:{target}:{options}", canonical.display())
        }
        _ => format!("{}:{target}", canonical.display()),
    }
}

fn canonicalize_under_repo(
    repo_root: &Path,
    container_name: &str,
    source: &str,
    raw_for_error: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.to_path_buf(),
            error,
        })?;
    let resolved = canonical_root.join(source);
    let canonical = resolved.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount source `{source}` is invalid: {error}"
        ))
    })?;
    if canonical.strip_prefix(&canonical_root).is_err() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` mount `{raw_for_error}` escapes the repo root"
        )));
    }
    Ok(canonical)
}

fn canonicalize_anywhere(
    container_name: &str,
    expanded: &str,
    raw_for_error: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let path = Path::new(expanded);
    path.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` external mount source `{raw_for_error}` (expanded to `{expanded}`) is invalid: {error}"
        ))
    })
}

/// Expand `${VAR}` references (against process env) and a leading `~`
/// (against `$HOME`) in a host path. `$VAR` without braces is left as-is
/// so dollar-bearing path segments don't get reinterpreted.
pub(crate) fn expand_host_path(raw: &str) -> Result<String, MountInterpolationError> {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if byte == b'$' && idx + 1 < bytes.len() && bytes[idx + 1] == b'{' {
            let close = raw[idx + 2..]
                .find('}')
                .ok_or_else(|| MountInterpolationError::UnterminatedVar(raw.to_owned()))?;
            let var_name = &raw[idx + 2..idx + 2 + close];
            if var_name.is_empty() {
                return Err(MountInterpolationError::EmptyVarName(raw.to_owned()));
            }
            let value = std::env::var(var_name)
                .map_err(|_| MountInterpolationError::MissingVar(var_name.to_owned()))?;
            out.push_str(&value);
            idx += 2 + close + 1;
        } else {
            out.push(byte as char);
            idx += 1;
        }
    }

    if let Some(rest) = out.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') {
            let home = std::env::var("HOME")
                .map_err(|_| MountInterpolationError::MissingVar("HOME".to_owned()))?;
            let mut expanded = String::with_capacity(home.len() + rest.len());
            expanded.push_str(&home);
            expanded.push_str(rest);
            return Ok(expanded);
        }
    }

    Ok(out)
}

#[derive(Debug)]
pub(crate) enum MountInterpolationError {
    UnterminatedVar(String),
    EmptyVarName(String),
    MissingVar(String),
}

impl std::fmt::Display for MountInterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnterminatedVar(raw) => {
                write!(f, "unterminated `${{...}}` reference in `{raw}`")
            }
            Self::EmptyVarName(raw) => write!(f, "empty `${{}}` reference in `{raw}`"),
            Self::MissingVar(name) => write!(f, "environment variable `{name}` is not set"),
        }
    }
}

impl std::error::Error for MountInterpolationError {}

/// Resolve every host mount on a container into a `Vec<String>` of
/// canonical mount-spec strings, preserving ordering. The returned
/// strings are what the compose / docker-run layer consumes.
pub(crate) fn resolve_host_mounts(
    repo_root: &Path,
    container_name: &str,
    mounts: &[ManifestContainerHostMount],
) -> Result<Vec<String>, ContainerPolicyError> {
    mounts
        .iter()
        .map(|mount| resolve_host_mount(repo_root, container_name, mount))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::ManifestContainerHostMountTable;
    use std::sync::Mutex;

    // Serialise tests that mutate process env so they don't trample
    // each other under `cargo test --jobs`.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn make_table(
        host: &str,
        container: &str,
        external: bool,
        options: Vec<String>,
    ) -> ManifestContainerHostMountTable {
        ManifestContainerHostMountTable {
            host: host.to_owned(),
            container: container.to_owned(),
            external,
            options,
        }
    }

    fn temp_repo() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn legacy_string_form_resolves_repo_relative_path() {
        let repo = temp_repo();
        std::fs::create_dir_all(repo.path().join("assets")).unwrap();
        let resolved = resolve_legacy_spec(repo.path(), "web", "./assets:/srv/assets").unwrap();
        let expected_root = repo.path().canonicalize().unwrap();
        assert!(
            resolved.starts_with(&format!("{}/assets:", expected_root.display())),
            "got `{resolved}`"
        );
        assert!(resolved.ends_with(":/srv/assets"));
    }

    #[test]
    fn legacy_string_form_preserves_options() {
        let repo = temp_repo();
        std::fs::create_dir_all(repo.path().join("assets")).unwrap();
        let resolved = resolve_legacy_spec(repo.path(), "web", "./assets:/srv/assets:ro").unwrap();
        assert!(resolved.ends_with(":/srv/assets:ro"));
    }

    #[test]
    fn legacy_string_form_rejects_absolute_path() {
        let repo = temp_repo();
        let err = resolve_legacy_spec(repo.path(), "web", "/etc/foo:/etc/foo").unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("must use a repo-relative source path"),
            "unexpected: {message}"
        );
        assert!(message.contains("external = true"), "unexpected: {message}");
    }

    #[test]
    fn structured_form_without_external_rejects_absolute_after_expansion() {
        let _guard = ENV_LOCK.lock().unwrap();
        let repo = temp_repo();
        std::env::set_var("EFFIGY_TEST_MOUNT_ABS", "/etc/passwd");
        let table = make_table("${EFFIGY_TEST_MOUNT_ABS}", "/etc/foo", false, Vec::new());
        let err = resolve_structured(repo.path(), "web", &table).unwrap_err();
        std::env::remove_var("EFFIGY_TEST_MOUNT_ABS");
        let message = format!("{err}");
        assert!(
            message.contains("declare `external = true`"),
            "unexpected: {message}"
        );
    }

    #[test]
    fn structured_form_with_external_resolves_absolute_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        let repo = temp_repo();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let outside_path = outside.path().to_path_buf();
        std::env::set_var("EFFIGY_TEST_MOUNT_EXT", outside_path.display().to_string());

        let table = make_table(
            "${EFFIGY_TEST_MOUNT_EXT}",
            "/home/dev/.ssh/config",
            true,
            vec!["ro".to_owned()],
        );
        let resolved = resolve_structured(repo.path(), "web", &table).unwrap();
        std::env::remove_var("EFFIGY_TEST_MOUNT_EXT");

        let canonical_outside = outside_path.canonicalize().unwrap();
        assert!(
            resolved.starts_with(&format!("{}:", canonical_outside.display())),
            "got `{resolved}`"
        );
        assert!(resolved.ends_with(":/home/dev/.ssh/config:ro"));
    }

    #[test]
    fn structured_form_options_render_comma_separated() {
        let _guard = ENV_LOCK.lock().unwrap();
        let repo = temp_repo();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::env::set_var(
            "EFFIGY_TEST_MOUNT_OPTS",
            outside.path().display().to_string(),
        );
        let table = make_table(
            "${EFFIGY_TEST_MOUNT_OPTS}",
            "/etc/x",
            true,
            vec!["ro".to_owned(), "z".to_owned()],
        );
        let resolved = resolve_structured(repo.path(), "web", &table).unwrap();
        std::env::remove_var("EFFIGY_TEST_MOUNT_OPTS");
        assert!(resolved.ends_with(":/etc/x:ro,z"), "got `{resolved}`");
    }

    #[test]
    fn structured_form_missing_env_var_errors_with_var_name() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("EFFIGY_TEST_MOUNT_ABSENT");
        let repo = temp_repo();
        let table = make_table("${EFFIGY_TEST_MOUNT_ABSENT}", "/etc/foo", true, Vec::new());
        let err = resolve_structured(repo.path(), "web", &table).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("EFFIGY_TEST_MOUNT_ABSENT"),
            "unexpected: {message}"
        );
    }

    #[test]
    fn structured_form_external_with_nonexistent_path_errors() {
        let repo = temp_repo();
        let table = make_table(
            "/definitely/not/a/real/path/effigy/12345",
            "/etc/foo",
            true,
            Vec::new(),
        );
        let err = resolve_structured(repo.path(), "web", &table).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("external mount source"),
            "unexpected: {message}"
        );
    }

    #[test]
    fn structured_form_external_requires_absolute_after_expansion() {
        let repo = temp_repo();
        // ./assets exists, but `external = true` doesn't anchor to
        // repo_root; a bare relative path can't be sourced from
        // "anywhere on disk" without ambiguity. Make the user be
        // explicit (use ~/ or ${VAR} or an absolute path).
        std::fs::create_dir_all(repo.path().join("assets")).unwrap();
        let table = make_table("./assets", "/srv/assets", true, Vec::new());
        let err = resolve_structured(repo.path(), "web", &table).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("must resolve to an absolute path"),
            "unexpected: {message}"
        );
    }

    #[test]
    fn expand_host_path_expands_tilde_against_home() {
        // Use the existing process HOME — mutating it races with other
        // tests that read it. We assert structural equivalence rather
        // than a specific path.
        let Ok(home) = std::env::var("HOME") else {
            // Bare-CI environments without HOME — skip rather than fail.
            return;
        };
        let expanded = expand_host_path("~/some/sub/path").unwrap();
        assert_eq!(expanded, format!("{home}/some/sub/path"));
        let bare = expand_host_path("~").unwrap();
        assert_eq!(bare, home);
    }

    #[test]
    fn expand_host_path_passes_dollar_without_braces_through() {
        let result = expand_host_path("/home/$USER/x").unwrap();
        assert_eq!(result, "/home/$USER/x");
    }

    #[test]
    fn expand_host_path_errors_on_unterminated_brace() {
        let err = expand_host_path("${UNCLOSED").unwrap_err();
        assert!(matches!(err, MountInterpolationError::UnterminatedVar(_)));
    }

    #[test]
    fn expand_host_path_errors_on_empty_brace() {
        let err = expand_host_path("${}").unwrap_err();
        assert!(matches!(err, MountInterpolationError::EmptyVarName(_)));
    }
}
