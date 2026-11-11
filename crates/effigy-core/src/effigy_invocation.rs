use crate::shell::shell_quote;

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
    if let Some(explicit) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(vec![explicit.trim().to_owned()]);
    }

    let executable = std::env::current_exe()?;
    if let Some(profile) = executable
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "deps"))
        .and_then(std::path::Path::parent)
    {
        let binary = profile.join("effigy");
        let binary = if binary.is_file() {
            binary
        } else {
            // Oxide uplifts the CLI to this checkout's profile output.
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .join(profile.file_name().unwrap_or_default())
                .join("effigy")
        };
        return Ok(vec![binary.canonicalize()?.display().to_string()]);
    }

    Ok(vec![executable.display().to_string()])
}
