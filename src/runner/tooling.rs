use super::manifest::config_sections::ManifestJsPackageManager;
pub(super) fn command_head(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or_default()
}

pub(super) fn required_tools_for_command(command: &str) -> &'static [&'static str] {
    match command_head(command) {
        "cargo" => &["cargo", "rustc"],
        "bun" => &["bun", "node"],
        "pnpm" => &["pnpm", "node"],
        "npm" | "npx" => &["npm", "node"],
        "node" => &["node"],
        _ => &[],
    }
}

pub(super) fn js_package_manager_binary(manager: ManifestJsPackageManager) -> Option<&'static str> {
    match manager {
        ManifestJsPackageManager::Bun => Some("bun"),
        ManifestJsPackageManager::Pnpm => Some("pnpm"),
        ManifestJsPackageManager::Npm => Some("npm"),
        ManifestJsPackageManager::Direct => None,
    }
}

pub(super) fn vitest_command_for_js_package_manager(
    manager: ManifestJsPackageManager,
) -> (&'static str, &'static str) {
    match manager {
        ManifestJsPackageManager::Bun => ("bun x vitest run", "bun"),
        ManifestJsPackageManager::Pnpm => ("pnpm exec vitest run", "pnpm"),
        ManifestJsPackageManager::Npm => ("npx vitest run", "npm"),
        ManifestJsPackageManager::Direct => ("vitest run", "direct"),
    }
}

#[cfg(test)]
#[path = "tooling/tests.rs"]
mod tests;
