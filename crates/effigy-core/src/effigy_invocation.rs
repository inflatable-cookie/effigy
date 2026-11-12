use std::path::{Path, PathBuf};

use crate::shell::shell_quote;

/// Resolution order for the self-executable, most explicit first:
///
/// 1. thread-local override (`executable_override`) or `EFFIGY_EXECUTABLE`;
/// 2. the `deps`-parent profile binary (`<profile>/effigy`);
/// 3. the checkout `target/<profile>/effigy` uplift fallback;
/// 4. the raw `current_exe`.
///
/// The explicit arm never depends on the fallible `current_exe()` lookup:
/// [`resolve_argv_with`] returns an override before querying the current
/// executable, and its tests prove it against an injected failing lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InvocationSource {
    Explicit(String),
    ProfileBinary(PathBuf),
    TargetFallback(PathBuf),
    CurrentExe(PathBuf),
}

pub fn resolve_effigy_invocation_prefix() -> Result<String, std::io::Error> {
    Ok(resolve_effigy_invocation_argv()?
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" "))
}

/// The executable and fixed arguments, before any shell presentation.
pub fn resolve_effigy_invocation_argv() -> Result<Vec<String>, std::io::Error> {
    let explicit = crate::executable_override::current()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("EFFIGY_EXECUTABLE").ok());
    resolve_argv_with(explicit, std::env::current_exe)
}

/// Production resolution path with the executable lookup injected. The
/// explicit override wins without invoking the fallible lookup at all, so
/// an explicit setting keeps working even when `current_exe()` fails.
fn resolve_argv_with(
    explicit: Option<String>,
    current_exe: impl FnOnce() -> Result<PathBuf, std::io::Error>,
) -> Result<Vec<String>, std::io::Error> {
    if let Some(explicit) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(vec![explicit.trim().to_owned()]);
    }
    let executable = current_exe()?;
    render_source(&resolve_source(None, &executable))
}

/// Checkout `target/` directory the uplift fallback resolves against.
fn checkout_target_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target")
}

/// Pick the winning source without touching the filesystem.
fn resolve_source(explicit: Option<String>, current_exe: &Path) -> InvocationSource {
    resolve_source_with_target_dir(explicit, current_exe, &checkout_target_dir())
}

/// Order oracle with the fallback base injected, so tests pin the layout
/// against an isolated directory instead of the ambient checkout `target/`.
fn resolve_source_with_target_dir(
    explicit: Option<String>,
    current_exe: &Path,
    target_dir: &Path,
) -> InvocationSource {
    if let Some(explicit) = explicit.filter(|value| !value.trim().is_empty()) {
        return InvocationSource::Explicit(explicit.trim().to_owned());
    }
    let Some(profile) = current_exe
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "deps"))
        .and_then(Path::parent)
    else {
        return InvocationSource::CurrentExe(current_exe.to_path_buf());
    };
    let binary = profile.join("effigy");
    if binary.is_file() {
        return InvocationSource::ProfileBinary(binary);
    }
    // Oxide uplifts the CLI to this checkout's profile output.
    InvocationSource::TargetFallback(
        target_dir
            .join(profile.file_name().unwrap_or_default())
            .join("effigy"),
    )
}

/// Render one resolved source, failing with an actionable message when the
/// chosen binary is missing instead of silently carrying a dead path.
fn render_source(source: &InvocationSource) -> Result<Vec<String>, std::io::Error> {
    match source {
        InvocationSource::Explicit(value) => Ok(vec![value.clone()]),
        InvocationSource::ProfileBinary(binary) => {
            Ok(vec![canonical_display(binary, "profile binary")?])
        }
        InvocationSource::TargetFallback(binary) => Ok(vec![canonical_display(
            binary,
            "checkout target fallback; set EFFIGY_EXECUTABLE to the effigy binary",
        )?]),
        InvocationSource::CurrentExe(executable) => Ok(vec![executable.display().to_string()]),
    }
}

fn canonical_display(path: &Path, context: &str) -> Result<String, std::io::Error> {
    path.canonicalize()
        .map(|canonical| canonical.display().to_string())
        .map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!(
                    "effigy executable `{}` ({context}) is unavailable: {error}",
                    path.display()
                ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_wins_and_trims() {
        let exe = Path::new("/usr/bin/effigy");
        let source = resolve_source_with_target_dir(
            Some("  /opt/effigy  ".to_owned()),
            exe,
            Path::new("/tmp/target"),
        );
        assert_eq!(source, InvocationSource::Explicit("/opt/effigy".to_owned()));
        assert_eq!(
            render_source(&source).expect("explicit renders"),
            vec!["/opt/effigy".to_owned()]
        );
    }

    #[test]
    fn production_path_returns_explicit_without_querying_current_exe() {
        // The production path (`resolve_argv_with`) must honor an explicit
        // override even when the `current_exe()` lookup itself fails: the
        // injected lookup is never called.
        let rendered = resolve_argv_with(Some("/opt/effigy".to_owned()), || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no current executable",
            ))
        })
        .expect("explicit wins without current_exe");
        assert_eq!(rendered, vec!["/opt/effigy".to_owned()]);
        // Without an override the lookup failure propagates.
        resolve_argv_with(None, || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no current executable",
            ))
        })
        .expect_err("missing lookup without override fails");
    }

    #[test]
    fn blank_explicit_falls_through_to_current_exe() {
        let exe = Path::new("/usr/bin/effigy");
        let source =
            resolve_source_with_target_dir(Some("   ".to_owned()), exe, Path::new("/tmp/target"));
        assert_eq!(source, InvocationSource::CurrentExe(exe.to_path_buf()));
    }

    #[test]
    fn non_deps_executable_resolves_to_itself() {
        let exe = Path::new("/usr/local/bin/effigy");
        let source = resolve_source_with_target_dir(None, exe, Path::new("/tmp/target"));
        assert_eq!(source, InvocationSource::CurrentExe(exe.to_path_buf()));
    }

    #[test]
    fn deps_layout_selects_profile_or_isolated_target_fallback() {
        // Pinned against an isolated fallback base, never the ambient
        // checkout `target/` or a shared Oxide output: a missing fallback
        // binary fails closed with the EFFIGY_EXECUTABLE remedy, a present
        // one renders, and a present profile binary wins outright.
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path().join("debug");
        let deps = profile.join("deps");
        std::fs::create_dir_all(&deps).expect("mkdir deps");
        let exe = deps.join("effigy-abc123");
        std::fs::write(&exe, "x").expect("write exe");
        let target_dir = dir.path().join("target");

        let source = resolve_source_with_target_dir(None, &exe, &target_dir);
        let InvocationSource::TargetFallback(path) = source else {
            panic!("expected isolated target fallback, got {source:?}");
        };
        assert_eq!(path, target_dir.join("debug").join("effigy"));
        let error = render_source(&InvocationSource::TargetFallback(path.clone()))
            .expect_err("missing isolated fallback fails");
        assert!(
            error.to_string().contains(&path.display().to_string()),
            "{error}"
        );
        assert!(error.to_string().contains("EFFIGY_EXECUTABLE"), "{error}");

        // A fallback binary that exists renders through canonicalization.
        std::fs::create_dir_all(path.parent().expect("profile dir")).expect("mkdir");
        std::fs::write(&path, "x").expect("write fallback binary");
        let rendered = render_source(&InvocationSource::TargetFallback(path))
            .expect("present fallback renders");
        assert_eq!(rendered.len(), 1);

        // With a profile binary present, the profile arm wins.
        let binary = profile.join("effigy");
        std::fs::write(&binary, "x").expect("write binary");
        let source = resolve_source_with_target_dir(None, &exe, &target_dir);
        assert_eq!(source, InvocationSource::ProfileBinary(binary));
    }
}
