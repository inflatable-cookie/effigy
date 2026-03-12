use std::env;
use std::path::PathBuf;
use std::process::{exit, Command};

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = env::args().skip(1).collect();
    let task = match args.as_slice() {
        [] => "qa:gates".to_owned(),
        [flag] if flag == "--docs-only" => "qa:docs".to_owned(),
        [flag] if flag == "--json-only" => "qa:json".to_owned(),
        [first, second] if first == "--json-only" && second == "--ci" => "qa:json:ci".to_owned(),
        [first, second] if first == "--all" && second == "--ci" => "qa:ci".to_owned(),
        _ => {
            eprintln!("unsupported effigy-qa arguments: {}", args.join(" "));
            exit(2);
        }
    };

    let status = Command::new("cargo")
        .args(["run", "--bin", "effigy", "--", &task, "--repo"])
        .arg(&repo_root)
        .current_dir(repo_root)
        .status()
        .expect("failed to launch Effigy QA task");

    exit(status.code().unwrap_or(1));
}
