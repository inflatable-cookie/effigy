use super::{command_head, required_tools_for_command};

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
