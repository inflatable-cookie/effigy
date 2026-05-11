let bin_path = if args.is_empty() {
    path::join(repo_root, "target/release/effigy")
} else {
    args[0].to_string()
};

if !fs::exists(bin_path) || !fs::is_file(bin_path) {
    throw "[error] effigy binary is not executable: " + bin_path;
}

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if !result["success"] {
        let status = result["status"].to_string();
        let stdout = result["stdout"].to_string();
        let stderr = result["stderr"].to_string();
        throw "[error] " + label + " failed with status " + status
            + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
    }
    log("[ok] " + label);
}

let tmp_dir = fs::make_temp_dir("effigy-release-smoke");
let manifest_path = path::join(tmp_dir, "effigy.toml");
let cargo_toml_path = path::join(tmp_dir, "Cargo.toml");

fs::write_file(
    manifest_path,
    "[catalog]\nalias = \"catalog_a\"\n\n[tasks]\nnoop = \"echo noop\"\n",
);
fs::write_file(
    cargo_toml_path,
    "[package]\nname = \"release-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
);

run_check("help", bin_path, ["help"]);
run_check("tasks", bin_path, ["tasks", "--repo", tmp_dir]);
run_check(
    "prefixed builtin tasks",
    bin_path,
    ["catalog_a/tasks", "--repo", tmp_dir],
);
run_check("test plan", bin_path, ["test", "--plan", "--repo", tmp_dir]);
run_check(
    "prefixed builtin test plan",
    bin_path,
    ["catalog_a/test", "--plan", "--repo", tmp_dir],
);
run_check("completion bash", bin_path, ["config", "completion", "bash", "--export"]);
run_check(
    "completion candidates",
    bin_path,
    ["config", "completion", "candidates", "--repo", tmp_dir],
);

fs::remove(tmp_dir);
log("[ok] release smoke checks passed");
