let source = path_join(repo_root, "target/debug/effigy");
let install_dir = path_join(repo_root, ".local-install/bin");
let target = path_join(install_dir, "effigy");

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

run_check("cargo build --bin effigy", "cargo", ["build", "--bin", "effigy"]);

if !path_exists(source) || !is_file(source) {
    throw "[error] missing built effigy binary: " + source;
}

create_dir(install_dir);
run_check("install local effigy binary", "cp", [source, target]);
run_check("mark local effigy binary executable", "chmod", ["0755", target]);

log("[ok] built local effigy binary");
log("[ok] installed " + target);
