use crate::shell::shell_quote;

pub fn resolve_effigy_invocation_prefix(
    test_harness_manifest_path: &str,
) -> Result<String, std::io::Error> {
    Ok(resolve_effigy_invocation_argv(test_harness_manifest_path)?
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" "))
}

/// The executable and fixed arguments, before any shell presentation.
pub fn resolve_effigy_invocation_argv(
    test_harness_manifest_path: &str,
) -> Result<Vec<String>, std::io::Error> {
    if let Some(explicit) = crate::executable_override::current() {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(vec![trimmed.to_owned()]);
        }
    }

    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(vec![trimmed.to_owned()]);
        }
    }

    let executable = std::env::current_exe()?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        return Ok([
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            test_harness_manifest_path,
            "--bin",
            "effigy",
            "--",
        ]
        .map(str::to_owned)
        .to_vec());
    }

    Ok(vec![executable.display().to_string()])
}
