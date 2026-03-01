use std::env;
use std::path::PathBuf;
use std::process::{exit, Command};

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = repo_root.join("scripts/check-quality-gates.sh");
    let args: Vec<String> = env::args().skip(1).collect();

    let status = Command::new("bash")
        .arg(script)
        .args(args)
        .current_dir(repo_root)
        .status()
        .expect("failed to launch scripts/check-quality-gates.sh");

    exit(status.code().unwrap_or(1));
}
