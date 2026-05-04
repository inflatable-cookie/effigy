let source = path.join(repo_root, "target/bootstrap-local/debug/effigy");
let install_dir = path.join(repo_root, ".local-install/bin");
let target = path.join(install_dir, "effigy");
let staged_target = path.join(install_dir, "effigy.new");
let active_version_file = path.join(install_dir, "effigy.active-version");

fn run_check(label, program, process_args) {
    let result = process.run(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn run_check_stream(label, program, process_args) {
    let result = process.stream(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    throw "[error] " + label + " failed with status " + status;
}

fn run_check_stream(label, program, process_args, options) {
    let result = process.stream(program, process_args, options);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    throw "[error] " + label + " failed with status " + status;
}

fn capture_process(program, process_args) {
    let result = process.run(program, process_args);
    if !result["success"] {
        return ();
    }

    trim_line_endings(result["stdout"].to_string())
}

fn trim_line_endings(text) {
    str.replace(str.replace(text, "\r", ""), "\n", "")
}

fn workspace_package_version() {
    let cargo_toml = path.join(repo_root, "Cargo.toml");
    let in_workspace_package = false;
    for line in fs.read_lines(cargo_toml) {
        if line == "[workspace.package]" {
            in_workspace_package = true;
            continue;
        }
        if str.starts_with(line, "[") {
            in_workspace_package = false;
        }
        if in_workspace_package && str.starts_with(line, "version = \"") {
            return str.replace(
                str.replace(line, "version = \"", ""),
                "\"",
                "",
            );
        }
    }

    throw "[error] failed to resolve workspace.package.version from " + cargo_toml;
}

fn local_active_version() {
    let base = workspace_package_version();
    let hash = capture_process("git", ["rev-parse", "--short=7", "HEAD"]);
    if hash == () || hash == "" {
        return "v" + base + "+local.unknown";
    }

    "v" + base + "+local." + hash
}

run_check_stream(
    "cargo build --bin effigy",
    "cargo",
    ["build", "--bin", "effigy"],
    #{ env: #{ CARGO_TARGET_DIR: "target/bootstrap-local" } },
);

if !fs.exists(source) || !fs.is_file(source) {
    throw "[error] missing built effigy binary: " + source;
}

fs.create_dir(install_dir);
run_check("stage local effigy binary", "cp", [source, staged_target]);
run_check(
    "mark staged local effigy binary executable",
    "chmod",
    ["0755", staged_target],
);
run_check("activate local effigy binary", "mv", [staged_target, target]);
fs.write_file(active_version_file, local_active_version() + "\n");

log("[ok] built local effigy binary");
log("[ok] installed " + target);
log("[ok] wrote local active version " + active_version_file);
