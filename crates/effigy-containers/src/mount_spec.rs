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
//! - With `external = true`, the source may live anywhere on disk. Absolute
//!   paths are accepted directly; relative paths resolve from the repo root and
//!   may escape it. The source must still exist (we canonicalise to fail fast
//!   on misconfigured machines).
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
        let resolved = if expanded_path.is_absolute() {
            expanded_path.to_path_buf()
        } else {
            repo_root.join(expanded_path)
        };
        canonicalize_anywhere(container_name, &resolved, raw_host)?
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
    resolved: &Path,
    raw_for_error: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    resolved.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` external mount source `{raw_for_error}` (resolved to `{}`) is invalid: {error}",
            resolved.display()
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
    use std::path::{Path, PathBuf};

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

    /// The legacy `"host:container[:options]"` form: a repo-relative source
    /// resolves against the checkout, options are kept, and an absolute source
    /// is refused with the `external = true` remedy.
    #[test]
    fn legacy_form_cases() {
        let cases: [(&str, &str, Result<(&str, &str), &str>); 3] = [
            (
                "a repo-relative source resolves",
                "./assets:/srv/assets",
                Ok(("assets:", ":/srv/assets")),
            ),
            (
                "options are kept",
                "./assets:/srv/assets:ro",
                Ok(("assets:", ":/srv/assets:ro")),
            ),
            (
                "an absolute source is refused",
                "/etc/foo:/etc/foo",
                Err("must use a repo-relative source path"),
            ),
        ];
        for (name, spec, expected) in cases {
            let repo = temp_repo();
            std::fs::create_dir_all(repo.path().join("assets")).unwrap();
            match expected {
                Ok((source_tail, target)) => {
                    let resolved =
                        resolve_legacy_spec(repo.path(), "web", spec).expect(name);
                    let expected_root = repo.path().canonicalize().unwrap();
                    assert!(
                        resolved
                            .starts_with(&format!("{}/{source_tail}", expected_root.display())),
                        "{name}: got `{resolved}`"
                    );
                    assert!(resolved.ends_with(target), "{name}: got `{resolved}`");
                }
                Err(token) => {
                    let err = resolve_legacy_spec(repo.path(), "web", spec).expect_err(name);
                    let message = format!("{err}");
                    assert!(message.contains(token), "{name}: {message}");
                    assert!(message.contains("external = true"), "{name}: {message}");
                }
            }
        }
    }

    /// What one structured-mount row expects from `resolve_structured`.
    enum Expect {
        /// A resolved bind string that starts with the canonical source and
        /// ends with this target and options.
        Resolved { source: String, tail: &'static str },
        /// A refusal naming this token.
        Refused(&'static str),
    }

    struct Case {
        host: &'static str,
        external: bool,
        options: Vec<&'static str>,
        env: Option<(&'static str, String)>,
        expect: Expect,
        /// A host file the row's environment names, kept alive for the row.
        keep: Option<tempfile::NamedTempFile>,
    }

    /// A checkout with a repo-relative source, for the rows that resolve one.
    fn assets_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("assets")).expect("mkdir assets");
        repo
    }

    /// A checkout beside a sibling directory, for the escape row.
    fn escaping_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("mkdir src");
        std::fs::create_dir_all(root.join("sibling/assets")).expect("mkdir sibling assets");
        repo
    }

    /// The structured `[containers.<name>.host.mounts]` form: `external = true`
    /// admits an absolute or escaping source, options render comma-separated,
    /// and an unexpanded or absent source is refused by name.
    #[test]
    fn structured_form_cases() {
        let cases: [(&str, fn(&Path) -> PathBuf, fn(&Path, &Path) -> Case); 6] = [
            (
                "an absolute source without external",
                assets_repo,
                |_root, _repo| Case {
                    host: "${EFFIGY_TEST_MOUNT_ABS}",
                    external: false,
                    options: Vec::new(),
                    env: Some(("EFFIGY_TEST_MOUNT_ABS", "/etc/passwd".to_owned())),
                    expect: Expect::Refused("declare `external = true`"),
                    keep: None,
                },
            ),
            (
                "an external absolute source",
                assets_repo,
                |_root, _repo| {
                    let file = tempfile::NamedTempFile::new().unwrap();
                    let source = file.path().canonicalize().unwrap();
                    Case {
                        host: "${EFFIGY_TEST_MOUNT_EXT}",
                        external: true,
                        options: vec!["ro"],
                        env: Some((
                            "EFFIGY_TEST_MOUNT_EXT",
                            file.path().display().to_string(),
                        )),
                        expect: Expect::Resolved {
                            source: source.display().to_string(),
                            tail: ":/etc/x:ro",
                        },
                        keep: Some(file),
                    }
                },
            ),
            (
                "options render comma-separated",
                assets_repo,
                |_root, _repo| {
                    let file = tempfile::NamedTempFile::new().unwrap();
                    let source = file.path().canonicalize().unwrap();
                    Case {
                        host: "${EFFIGY_TEST_MOUNT_OPTS}",
                        external: true,
                        options: vec!["ro", "z"],
                        env: Some((
                            "EFFIGY_TEST_MOUNT_OPTS",
                            file.path().display().to_string(),
                        )),
                        expect: Expect::Resolved {
                            source: source.display().to_string(),
                            tail: ":/etc/x:ro,z",
                        },
                        keep: Some(file),
                    }
                },
            ),
            (
                "an absent environment variable",
                assets_repo,
                |_root, _repo| Case {
                    host: "${EFFIGY_TEST_MOUNT_ABSENT}",
                    external: true,
                    options: Vec::new(),
                    env: None,
                    expect: Expect::Refused("EFFIGY_TEST_MOUNT_ABSENT"),
                    keep: None,
                },
            ),
            (
                "an external source that does not exist",
                assets_repo,
                |_root, _repo| Case {
                    host: "/definitely/not/a/real/path/effigy/12345",
                    external: true,
                    options: Vec::new(),
                    env: None,
                    expect: Expect::Refused("external mount source"),
                    keep: None,
                },
            ),
            (
                "an external repo-relative escape",
                escaping_repo,
                |root, _repo| {
                    let source = root.join("sibling/assets").canonicalize().unwrap();
                    Case {
                        host: "../sibling/assets",
                        external: true,
                        options: Vec::new(),
                        env: None,
                        expect: Expect::Resolved {
                            source: source.display().to_string(),
                            tail: ":/etc/x",
                        },
                        keep: None,
                    }
                },
            ),
        ];
        for (name, setup, build) in cases {
            let root = temp_repo();
            let repo = setup(root.path());
            let case = build(root.path(), &repo);
            // The absence row needs the variable gone, not unset by accident.
            std::env::remove_var("EFFIGY_TEST_MOUNT_ABSENT");
            let _lock = crate::test_env_lock();
            if let Some((variable, value)) = &case.env {
                std::env::set_var(variable, value);
            }
            let table = make_table(
                case.host,
                "/etc/x",
                case.external,
                case.options.iter().map(|option| (*option).to_owned()).collect(),
            );
            let resolved = resolve_structured(&repo, "web", &table);
            for (variable, _) in case.env.iter() {
                std::env::remove_var(variable);
            }
            match (&case.expect, resolved) {
                (Expect::Resolved { source, tail }, Ok(resolved)) => {
                    assert!(
                        resolved.starts_with(&format!("{source}:")),
                        "{name}: got `{resolved}`, expected source `{source}`"
                    );
                    assert!(resolved.ends_with(tail), "{name}: got `{resolved}`");
                }
                (Expect::Resolved { .. }, Err(error)) => {
                    panic!("{name}: expected a resolved mount, got `{error}`")
                }
                (Expect::Refused(token), Err(error)) => {
                    let message = format!("{error}");
                    assert!(message.contains(token), "{name}: {message}");
                }
                (Expect::Refused(_), Ok(resolved)) => {
                    panic!("{name}: expected a refusal, got `{resolved}`")
                }
            }
            drop(case.keep);
        }
    }

    /// `expand_host_path` expands `~` and `${VAR}` and leaves anything else
    /// alone, refusing a malformed brace rather than guessing.
    #[test]
    fn expand_host_path_cases() {
        let _lock = crate::test_env_lock();
        let home = std::env::var("HOME").ok();
        if let Some(home) = &home {
            assert_eq!(
                expand_host_path("~/some/sub/path").unwrap(),
                format!("{home}/some/sub/path")
            );
            assert_eq!(expand_host_path("~").unwrap(), *home);
        }
        // A bare `$` needs its braces to be a variable.
        assert_eq!(
            expand_host_path("/home/$USER/x").unwrap(),
            "/home/$USER/x"
        );
        for (raw, empty) in [("${UNCLOSED", false), ("${}", true)] {
            let error = expand_host_path(raw).expect_err(raw);
            if empty {
                assert!(
                    matches!(error, MountInterpolationError::EmptyVarName(_)),
                    "{raw}: {error:?}"
                );
            } else {
                assert!(
                    matches!(error, MountInterpolationError::UnterminatedVar(_)),
                    "{raw}: {error:?}"
                );
            }
        }
    }
}
