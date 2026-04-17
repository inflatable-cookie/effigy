//! Small shell utilities shared across crates.
//!
//! `shell_quote` is POSIX-single-quote escaping for building shell
//! command strings. `with_local_node_bin_path` prepends a local
//! `node_modules/.bin` directory (if present) onto the PATH of a child
//! process so npm/pnpm/yarn-style bin invocations resolve without an
//! explicit `npx`.
//!
//! Moved out of `src/runner/util/shell.rs` in batch 241 so the upcoming
//! `effigy-managed` extraction can depend on it from a shared crate
//! rather than reaching back into the runner binary.

use std::path::Path;
use std::process::Command as ProcessCommand;

pub fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

pub fn shell_quote(raw: &str) -> String {
    if raw.is_empty() {
        return "''".to_owned();
    }
    let escaped = raw.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}
