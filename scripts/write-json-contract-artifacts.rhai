let log_path = path::join(repo_root, "json-contracts.log");
let selected_path = path::join(repo_root, "json-contracts-selected.json");

let result = process::tee(
    "cargo",
    ["run", "--bin", "effigy", "--", "contracts", "check-json", "--full", "--print-selected=json"],
);

let stdout = result["stdout"].to_string();
let stderr = result["stderr"].to_string();

if !result["success"] {
    throw "[error] contracts check-json failed"
        + "\nstdout:\n" + stdout
        + "\nstderr:\n" + stderr;
}

fs::write_file(log_path, stdout);

for line in str::split_lines(stdout) {
    if str::starts_with(line, "{\"selected\":") {
        fs::write_file(selected_path, line + "\n");
        log("[ok] wrote " + log_path);
        log("[ok] wrote " + selected_path);
        return;
    }
}

throw "[error] json selection artifact line not found in " + log_path;
