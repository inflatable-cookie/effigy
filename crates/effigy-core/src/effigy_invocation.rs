use crate::shell::shell_quote;

pub fn resolve_effigy_invocation_prefix(
    test_harness_manifest_path: &str,
) -> Result<String, std::io::Error> {
    if let Some(explicit) = crate::executable_override::current() {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    if runs_from_a_test_harness() {
        let manifest_path = shell_quote(test_harness_manifest_path);
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }

    let executable = std::env::current_exe()?;
    Ok(shell_quote(&executable.display().to_string()))
}

/// True when this process runs from a test harness, so its Effigy invocation
/// names this checkout's build (`cargo run --bin effigy`) instead of an
/// installed binary. Fixture code may not drive shared machine state through
/// it.
pub fn runs_from_a_test_harness() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|executable| {
            executable
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name == "deps")
        })
        .unwrap_or(false)
}
