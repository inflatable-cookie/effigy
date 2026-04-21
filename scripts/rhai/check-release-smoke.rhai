let bin_path = if args.is_empty() {
    path_join(repo_root, "target/release/effigy")
} else {
    args[0].to_string()
};

if !path_exists(bin_path) || !is_file(bin_path) {
    throw "[error] effigy binary is not executable: " + bin_path;
}

fn run_check(label, program, process_args) {
    let result = run_process(program, process_args);
    if !result["success"] {
        let status = result["status"].to_string();
        let stdout = result["stdout"].to_string();
        let stderr = result["stderr"].to_string();
        throw "[error] " + label + " failed with status " + status
            + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
    }
    log("[ok] " + label);
}

let tmp_dir = make_temp_dir("effigy-release-smoke");
let manifest_path = path_join(tmp_dir, "effigy.toml");
let cargo_toml_path = path_join(tmp_dir, "Cargo.toml");

write_file(
    manifest_path,
    "[catalog]\nalias = \"catalog_a\"\n\n[tasks]\nnoop = \"echo noop\"\n",
);
write_file(
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
run_check("completion bash", bin_path, ["completion", "bash"]);
run_check(
    "completion candidates",
    bin_path,
    ["completion", "candidates", "--repo", tmp_dir],
);

remove_path(tmp_dir);
log("[ok] release smoke checks passed");
