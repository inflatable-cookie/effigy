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

#[cfg(test)]
#[path = "tooling/tests.rs"]
mod tests;
