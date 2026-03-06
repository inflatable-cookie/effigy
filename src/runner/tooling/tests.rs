use super::{
    command_head, js_package_manager_binary, required_tools_for_command,
    vitest_command_for_js_package_manager,
};
use crate::runner::manifest::config_sections::ManifestJsPackageManager;

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
