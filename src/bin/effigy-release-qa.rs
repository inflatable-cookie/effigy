use std::path::PathBuf;
use std::process::{exit, Command};

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = repo_root.join("scripts/check-release-gates.sh");

    let status = Command::new("bash")
        .arg(script)
        .current_dir(repo_root)
        .status()
        .expect("failed to launch scripts/check-release-gates.sh");

    exit(status.code().unwrap_or(1));
}
