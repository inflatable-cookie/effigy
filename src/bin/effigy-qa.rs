#[cfg(not(test))]
use std::env;
#[cfg(not(test))]
use std::path::PathBuf;
#[cfg(not(test))]
use std::process::{exit, Command};

#[cfg(not(test))]
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

    let status = match Command::new("cargo")
        .args(["run", "--bin", "effigy", "--", &task, "--repo"])
        .arg(&repo_root)
        .current_dir(repo_root)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to launch Effigy QA task: {}", e);
            exit(1);
        }
    };

    exit(status.code().unwrap_or(1));
}

#[cfg(test)]
fn main() {}
