// Activate the build that build-local-bin.rhai staged. This is the last step of
// `install:local`, and it is the only one that changes the active path.
//
// Order: the binary is renamed over `.local-install/bin/effigy` first, then the
// staged version and source receipt are renamed into place. A rename failure on
// the binary leaves the active binary and its receipts exactly as they were.
//
// No process is started after the active binary is replaced. The runner admits
// a step by launching a helper from the effigy path on disk, so the swap has to
// be the last thing this step does. The receipt renames are Rhai `fs` calls,
// which spawn nothing. This script uses only Rhai primitives that effigy
// v0.14.1 already provides, since the previous binary runs it on upgrade day.
let install_dir = path::join(repo_root, ".local-install/bin");
let target = path::join(install_dir, "effigy");
let staged_target = path::join(install_dir, "effigy.new");
let active_version_file = path::join(install_dir, "effigy.active-version");
let receipt_file = target + ".source-receipt";

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn require_regular_file(path, label) {
    if fs::is_symlink(path) {
        throw "[error] refusing symlink " + label + ": " + path;
    }
    if !fs::exists(path) || !fs::is_file(path) {
        throw "[error] missing " + label + ": " + path;
    }
}

require_regular_file(staged_target, "staged local effigy binary");
require_regular_file(active_version_file + ".new", "staged local effigy version");
require_regular_file(receipt_file + ".new", "staged local effigy source receipt");

run_check("activate local effigy binary", "mv", [staged_target, target]);
fs::move_path(active_version_file + ".new", active_version_file);
fs::move_path(receipt_file + ".new", receipt_file);

log("[ok] installed " + target);
log("[ok] wrote local active version " + active_version_file);
