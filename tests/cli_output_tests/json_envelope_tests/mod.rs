use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::support::{
    run_json_cli_command_with_manifest, run_json_task_success, temp_workspace, wait_for_path_exists,
};

fn write_numbered_lines(path: &Path, prefix: &str, line_count: usize) {
    let body = (0..line_count)
        .map(|idx| format!("{prefix}{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write numbered lines");
}

fn write_god_file_fixture(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_numbered_lines(&root.join("src/app.ts"), "const line_", 12);
}

fn write_duplicate_block_fixture(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let mut lines = vec![
        "pub fn shared_alpha() -> usize {".to_owned(),
        "    let seed = 1;".to_owned(),
    ];
    for idx in 0..18 {
        lines.push(format!("    let acc_{idx} = seed + {idx};"));
    }
    lines.push("    acc_17".to_owned());
    lines.push("}".to_owned());
    let block = lines.join("\n");
    fs::write(root.join("src/alpha.rs"), format!("{block}\n")).expect("write alpha");
    fs::write(root.join("src/beta.rs"), format!("{block}\n")).expect("write beta");
}

fn write_comment_ratio_fixture(root: &Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    let mut lines = (0..30)
        .map(|idx| format!("// commentary line {idx}"))
        .collect::<Vec<String>>();
    lines.extend((0..20).map(|idx| format!("const line_{idx} = {idx};")));
    fs::write(root.join("src/app.ts"), format!("{}\n", lines.join("\n"))).expect("write source");
}

fn write_src_fixture(root: &Path, rel: &str, contents: &str) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join(rel), contents).expect("write source");
}

fn run_scan_command(root: &Path, scan: &str, args: &[&str]) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command.arg("--json").arg("scan").arg(scan);
    for arg in args {
        command.arg(arg);
    }
    let output = command
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    serde_json::from_str(&stdout).expect("json parse")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn setup_workspace_app_path_bundle(root: &Path) {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/workspace-app-bundle");
    let bundle_dir = root.join("bundles/workspace-app");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
}

fn deploy_provider_source(provider: &str) -> String {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    format!(
        "[deploy.providers.{provider}]\nsource = {{ type = \"path\", dir = \"{}/external/providers/{provider}\" }}\n",
        repo_root.display()
    )
}

fn assert_scan_success(parsed: &Value, schema: &str, scan: &str) {
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(parsed["result"]["schema"], schema);
    assert_eq!(parsed["result"]["scan"], scan);
}

fn assert_scan_failure(parsed: &Value, schema: &str) {
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "scan");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(parsed["error"]["details"]["schema"], schema);
}

mod core;
mod runtime;
mod scan;
