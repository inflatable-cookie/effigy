use std::path::{Path, PathBuf};

use crate::shell::shell_quote;

/// Resolution order for the self-executable, most explicit first:
///
/// 1. thread-local override (`executable_override`) or `EFFIGY_EXECUTABLE`;
/// 2. the `deps`-parent profile binary (`<profile>/effigy`);
/// 3. the checkout `target/<profile>/effigy` uplift fallback;
/// 4. the raw `current_exe`.
///
/// Steps 2-4 are pure in [`resolve_argv_from`] so tests can pin the order
/// without spawning a process or touching the environment.
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
    let executable = std::env::current_exe()?;
    let source = resolve_source(explicit, &executable);
    render_source(&source)
}

/// Pick the winning source without touching the filesystem.
fn resolve_source(explicit: Option<String>, current_exe: &Path) -> InvocationSource {
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
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(profile.file_name().unwrap_or_default())
            .join("effigy"),
    )
}

/// Render one resolved source, failing with an actionable message when the
/// chosen binary is missing instead of silently carrying a dead path.
fn render_source(source: &InvocationSource) -> Result<Vec<String>, std::io::Error> {
    match source {
        InvocationSource::Explicit(value) => Ok(vec![value.clone()]),
        InvocationSource::ProfileBinary(binary) => Ok(vec![canonical_display(binary, "profile binary")?]),
        InvocationSource::TargetFallback(binary) => Ok(vec![canonical_display(
            binary,
            "checkout target fallback; set EFFIGY_EXECUTABLE to the effigy binary",
        )?]),
        InvocationSource::CurrentExe(executable) => Ok(vec![executable.display().to_string()]),
    }
}

fn canonical_display(path: &Path, context: &str) -> Result<String, std::io::Error> {
    path.canonicalize().map(|canonical| canonical.display().to_string()).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "effigy executable `{}` ({context}) is unavailable: {error}",
                path.display()
            ),
        )
    })
}

/// Pure, testable resolution order: explicit override wins, then the
/// `deps`-parent profile binary, then the checkout target fallback, then
/// the raw executable. No filesystem mutation; missing binaries fail via
/// [`render_source`] with an actionable message.
#[cfg(test)]
fn resolve_argv_from(
    explicit: Option<String>,
    current_exe: &Path,
) -> Result<(InvocationSource, Option<Vec<String>>), std::io::Error> {
    let source = resolve_source(explicit, current_exe);
    // Tests assert the source without requiring the binary to exist;
    // rendering is covered separately for the explicit/current-exe arms.
    let rendered = match &source {
        InvocationSource::Explicit(_) | InvocationSource::CurrentExe(_) => {
            Some(render_source(&source)?)
        }
        InvocationSource::ProfileBinary(_) | InvocationSource::TargetFallback(_) => None,
    };
    Ok((source, rendered))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_wins_and_trims() {
        let exe = Path::new("/usr/bin/effigy");
        let (source, rendered) =
            resolve_argv_from(Some("  /opt/effigy  ".to_owned()), exe).expect("explicit");
        assert_eq!(source, InvocationSource::Explicit("/opt/effigy".to_owned()));
        assert_eq!(rendered, Some(vec!["/opt/effigy".to_owned()]));
    }

    #[test]
    fn blank_explicit_falls_through_to_current_exe() {
        let exe = Path::new("/usr/bin/effigy");
        let (source, rendered) =
            resolve_argv_from(Some("   ".to_owned()), exe).expect("fallback");
        assert_eq!(
            source,
            InvocationSource::CurrentExe(exe.to_path_buf())
        );
        assert_eq!(rendered, Some(vec![exe.display().to_string()]));
    }

    #[test]
    fn non_deps_executable_resolves_to_itself() {
        let exe = Path::new("/usr/local/bin/effigy");
        let (source, rendered) = resolve_argv_from(None, exe).expect("current-exe");
        assert_eq!(
            source,
            InvocationSource::CurrentExe(exe.to_path_buf())
        );
        assert_eq!(rendered, Some(vec![exe.display().to_string()]));
    }

    #[test]
    fn deps_layout_selects_profile_or_target_fallback() {
        // A `deps`-parent executable resolves to the profile binary when it
        // exists, else to the checkout target fallback; neither arm may
        // silently carry a dead path, so rendering the fallback for a
        // missing binary must fail with an actionable message.
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path().join("debug");
        let deps = profile.join("deps");
        std::fs::create_dir_all(&deps).expect("mkdir deps");
        let exe = deps.join("effigy-abc123");
        std::fs::write(&exe, "x").expect("write exe");

        // No profile binary yet: target fallback source, and rendering it
        // fails closed with the EFFIGY_EXECUTABLE remedy.
        let (source, rendered) = resolve_argv_from(None, &exe).expect("source");
        assert!(matches!(source, InvocationSource::TargetFallback(_)));
        assert!(rendered.is_none());
        if let InvocationSource::TargetFallback(path) = source {
            let error = canonical_display(&path, "test").expect_err("missing fallback fails");
            assert!(
                error.to_string().contains(&path.display().to_string()),
                "{error}"
            );
        }

        // With a profile binary present, the profile arm wins.
        let binary = profile.join("effigy");
        std::fs::write(&binary, "x").expect("write binary");
        let (source, _) = resolve_argv_from(None, &exe).expect("source");
        assert_eq!(source, InvocationSource::ProfileBinary(binary));
    }
}
