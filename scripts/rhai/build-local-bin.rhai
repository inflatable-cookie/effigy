let source = path_join(repo_root, "target/bootstrap-local/debug/effigy");
let install_dir = path_join(repo_root, ".local-install/bin");
let target = path_join(install_dir, "effigy");
let active_version_file = path_join(install_dir, "effigy.active-version");

fn run_check(label, program, process_args) {
    let result = run_process(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn capture_process(program, process_args) {
    let result = run_process(program, process_args);
    if !result["success"] {
        return ();
    }

    trim_line_endings(result["stdout"].to_string())
}

fn trim_line_endings(text) {
    replace_string(replace_string(text, "\r", ""), "\n", "")
}

fn workspace_package_version() {
    let cargo_toml = path_join(repo_root, "Cargo.toml");
    let in_workspace_package = false;
    for line in read_lines(cargo_toml) {
        if line == "[workspace.package]" {
            in_workspace_package = true;
            continue;
        }
        if string_starts_with(line, "[") {
            in_workspace_package = false;
        }
        if in_workspace_package && string_starts_with(line, "version = \"") {
            return replace_string(
                replace_string(line, "version = \"", ""),
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

    let dirty_state = capture_process(
        "git",
        ["status", "--porcelain", "--untracked-files=normal"],
    );
    let dirty = dirty_state != () && dirty_state != "";
    if dirty {
        return "v" + base + "+local." + hash + ".dirty";
    }
    "v" + base + "+local." + hash
}

run_check(
    "cargo build --bin effigy",
    "sh",
    ["-lc", "CARGO_TARGET_DIR=target/bootstrap-local cargo build --bin effigy"],
);

if !path_exists(source) || !is_file(source) {
    throw "[error] missing built effigy binary: " + source;
}

create_dir(install_dir);
run_check("install local effigy binary", "cp", [source, target]);
run_check("mark local effigy binary executable", "chmod", ["0755", target]);
write_file(active_version_file, local_active_version() + "\n");

log("[ok] built local effigy binary");
log("[ok] installed " + target);
log("[ok] wrote local active version " + active_version_file);
