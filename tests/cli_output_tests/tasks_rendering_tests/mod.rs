use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::support::temp_workspace;

const BUILTIN_TASKS_SECTION: &str = "\
Built-in Tasks
──────────────
- help : Show general help (same as --help)
- config : Show supported project effigy.toml configuration keys and examples
- container : Operate manifest-defined Colima-backed container environments
- doctor : Built-in remedial health checks for environment, manifests, and task references
- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan
- tasks : List discovered catalogs and available tasks
- watch : Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs
- init : Initialize baseline effigy.toml scaffold with dry-run/force controls
- migrate : Migrate package scripts into [tasks] with preview/apply flow
- unlock : Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)
- cache : Inspect and invalidate phase-1 task cache metadata (`inspect`, `invalidate`)
- completion : Generate shell completion scripts (`bash`, `zsh`, `fish`)
- scan : Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-in-src`, `attention-markers`, and `stale-suppressions`

";

fn run_effigy(args: &[&str], repo: Option<&Path>, color: bool) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    for arg in args {
        command.arg(arg);
    }
    if let Some(repo) = repo {
        command.arg("--repo").arg(repo);
    }
    if color {
        command.env("EFFIGY_COLOR", "always").env_remove("NO_COLOR");
    } else {
        command.env("NO_COLOR", "1");
    }

    let output = command.output().expect("run effigy");
    assert!(output.status.success());
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn write_catalog_build_workspace(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    fs::write(root.join("effigy.toml"), "").expect("write root manifest");
    let catalog = root.join("cattle-grid");
    fs::create_dir_all(&catalog).expect("mkdir catalog");
    fs::write(
        catalog.join("effigy.toml"),
        "[catalog]\nalias = \"cattle-grid\"\n[tasks.build]\nrun = \"tsc -p tsconfig.json {args}\"\n",
    )
    .expect("write catalog manifest");
    root
}

fn write_managed_profiles_manifest(root: &Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "catalog_a/api" }]

[tasks.dev.profiles.front]
concurrent = [{ task = "catalog_c/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "catalog_b/dev" }]
"#,
    )
    .expect("write manifest");
}

fn extract_tail<'a>(stdout: &'a str, anchor: &str) -> &'a str {
    let start = stdout.find(anchor).expect("section anchor");
    &stdout[start + 1..]
}

mod color_and_json;
mod listings;
mod managed_profiles;
mod resolve;
