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

    let executable = std::env::current_exe()?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(test_harness_manifest_path);
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }

    Ok(shell_quote(&executable.display().to_string()))
}
