use super::ManifestJsPackageManager;
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
mod tests {
    use super::*;

    #[test]
    fn command_head_parsing_is_stable() {
        assert_eq!(command_head(""), "");
        assert_eq!(command_head("cargo test"), "cargo");
        assert_eq!(command_head("   pnpm run lint"), "pnpm");
    }

    #[test]
    fn required_tool_mapping_is_stable() {
        assert_eq!(
            required_tools_for_command("cargo build"),
            &["cargo", "rustc"]
        );
        assert_eq!(required_tools_for_command("bun x vitest"), &["bun", "node"]);
        assert_eq!(
            required_tools_for_command("pnpm exec vitest"),
            &["pnpm", "node"]
        );
        assert_eq!(
            required_tools_for_command("npx vitest run"),
            &["npm", "node"]
        );
        assert_eq!(required_tools_for_command("node script.js"), &["node"]);
        assert!(required_tools_for_command("echo hello").is_empty());
    }

    #[test]
    fn package_manager_binary_and_vitest_mapping_is_stable() {
        assert_eq!(
            js_package_manager_binary(ManifestJsPackageManager::Bun),
            Some("bun")
        );
        assert_eq!(
            js_package_manager_binary(ManifestJsPackageManager::Pnpm),
            Some("pnpm")
        );
        assert_eq!(
            js_package_manager_binary(ManifestJsPackageManager::Npm),
            Some("npm")
        );
        assert_eq!(
            js_package_manager_binary(ManifestJsPackageManager::Direct),
            None
        );

        assert_eq!(
            vitest_command_for_js_package_manager(ManifestJsPackageManager::Bun),
            ("bun x vitest run", "bun")
        );
        assert_eq!(
            vitest_command_for_js_package_manager(ManifestJsPackageManager::Pnpm),
            ("pnpm exec vitest run", "pnpm")
        );
        assert_eq!(
            vitest_command_for_js_package_manager(ManifestJsPackageManager::Npm),
            ("npx vitest run", "npm")
        );
        assert_eq!(
            vitest_command_for_js_package_manager(ManifestJsPackageManager::Direct),
            ("vitest run", "direct")
        );
    }
}
