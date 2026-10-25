// Build this checkout's optimized release binary and activate it as the
// checkout-local stable `effigy` (`.local-install/bin/effigy`).
//
// The build goes through Oxide (`oxide build --release --bin effigy`), which
// keeps the Rust intermediates in its shared slot pool and uplifts the final
// binary to `<workspace>/target/release/effigy`. Only that exact artifact is
// installed: a missing release output fails instead of falling back to another
// profile or to plain Cargo. `EFFIGY_OXIDE_ESCAPE=cargo effigy install:local`
// is the documented plain-Cargo escape and produces the same path.
//
// Activation is staged and ordered so a failure cannot destroy the active
// binary or its version receipt:
//
//   1. clear the stale release artifact and build the release binary;
//   2. refuse to proceed if the active path is not a regular file;
//   3. copy the active binary/stamp into the owner-only rollback channel;
//   4. copy the release binary to `effigy.new`, verify the bytes and mark it
//      executable;
//   5. rename the staged binary over the active path;
//   6. publish the new version receipt by rename, never by truncating it.
let source = path::join(repo_root, "target/release/effigy");
let install_dir = path::join(repo_root, ".local-install/bin");
let target = path::join(install_dir, "effigy");
let staged_target = path::join(install_dir, "effigy.new");
let active_version_file = path::join(install_dir, "effigy.active-version");
let previous_target = path::join(install_dir, "effigy.previous");
let previous_version_file = path::join(install_dir, "effigy.previous.version");

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

fn run_check_stream(label, program, process_args) {
    let result = process::stream(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    throw "[error] " + label + " failed with status " + status;
}

fn run_check_stream(label, program, process_args, options) {
    let result = process::stream(program, process_args, options);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    throw "[error] " + label + " failed with status " + status;
}

fn capture_process(program, process_args) {
    let result = process::run(program, process_args);
    if !result["success"] {
        return ();
    }

    trim_line_endings(result["stdout"].to_string())
}

fn trim_line_endings(text) {
    str::replace(str::replace(text, "\r", ""), "\n", "")
}

fn workspace_package_version() {
    let cargo_toml = path::join(repo_root, "Cargo.toml");
    let in_workspace_package = false;
    for line in fs::read_lines(cargo_toml) {
        if line == "[workspace.package]" {
            in_workspace_package = true;
            continue;
        }
        if str::starts_with(line, "[") {
            in_workspace_package = false;
        }
        if in_workspace_package && str::starts_with(line, "version = \"") {
            return str::replace(
                str::replace(line, "version = \"", ""),
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

    let dirty = capture_process("git", ["status", "--short", "--untracked-files=normal"]);
    if dirty != () && str::trim(dirty) != "" {
        return "v" + base + "+local." + hash + ".dirty";
    }

    "v" + base + "+local." + hash
}

fn require_regular_file(path, label) {
    if fs::is_symlink(path) {
        throw "[error] refusing symlink " + label + ": " + path;
    }
    if !fs::exists(path) || !fs::is_file(path) {
        throw "[error] missing " + label + ": " + path;
    }
}

fn require_unsupported_artifact_free(path) {
    if fs::is_symlink(path) {
        throw "[error] refusing symlinked build artifact: " + path;
    }
    if fs::exists(path) && !fs::is_file(path) {
        throw "[error] refusing unsupported build artifact path: " + path;
    }
}

// A stale artifact from an earlier build must never stand in for the build this
// activation claims to install, so remove it first and require the build to
// produce its own output at the exact uplifted release path.
fn clear_stale_release_artifact(path) {
    require_unsupported_artifact_free(path);
    if fs::is_file(path) {
        fs::remove(path);
    }
}

fn require_replaceable_active_binary(path) {
    if !fs::exists(path) && !fs::is_symlink(path) {
        return;
    }
    if fs::is_symlink(path) || !fs::is_file(path) {
        throw "[error] refusing to replace non-regular active effigy binary: " + path
            + "\n[error] move it aside yourself if you want install:local to manage it";
    }
}

fn verify_staged_copy(source, staged, label) {
    let source_digest = fs::sha256(source);
    let staged_digest = fs::sha256(staged);
    if source_digest != staged_digest {
        try {
            fs::remove(staged);
        } catch (error) {
            log_warn("[warn] could not remove the refused staged copy: " + staged);
        }
        throw "[error] " + label + " is not a byte-identical copy of the release binary"
            + "\nsource sha256: " + source_digest
            + "\nstaged sha256: " + staged_digest;
    }
}

fn preserve_previous_local_install(target, previous_target, active_version_file, previous_version_file) {
    if fs::exists(target) && fs::is_file(target) {
        let staged_previous = previous_target + ".new";
        run_check("preserve previous local effigy binary", "cp", [target, staged_previous]);
        verify_staged_copy(target, staged_previous, "preserved previous local effigy binary");
        run_check(
            "restrict previous local effigy binary to owner-only",
            "chmod",
            ["0700", staged_previous],
        );
        run_check(
            "activate preserved previous local effigy binary",
            "mv",
            [staged_previous, previous_target],
        );
        if fs::exists(active_version_file) && fs::is_file(active_version_file) {
            let staged_previous_version = previous_version_file + ".new";
            run_check(
                "preserve previous local effigy version",
                "cp",
                [active_version_file, staged_previous_version],
            );
            run_check(
                "restrict previous local effigy version to owner-only",
                "chmod",
                ["0600", staged_previous_version],
            );
            run_check(
                "activate preserved previous local effigy version",
                "mv",
                [staged_previous_version, previous_version_file],
            );
        }
    }
}

fn activate_local_install(source, staged_target, target, active_version_file, version) {
    require_regular_file(source, "built effigy release binary");
    run_check("stage local effigy binary", "cp", [source, staged_target]);
    verify_staged_copy(source, staged_target, "staged local effigy binary");
    run_check(
        "mark staged local effigy binary executable",
        "chmod",
        ["0755", staged_target],
    );
    run_check("activate local effigy binary", "mv", [staged_target, target]);
    let staged_version_file = active_version_file + ".new";
    fs::write_file(staged_version_file, version + "\n");
    run_check(
        "activate local effigy version",
        "mv",
        [staged_version_file, active_version_file],
    );
}

clear_stale_release_artifact(source);

run_check_stream(
    "build --release --bin effigy through Oxide",
    "sh",
    ["scripts/oxide-selector.sh", "build", "--release", "--bin", "effigy"],
);

require_unsupported_artifact_free(source);

if !fs::exists(source) || !fs::is_file(source) {
    throw "[error] missing built effigy release binary: " + source
        + "\n[error] expected the release binary Oxide uplifts to target/release/effigy"
        + "\n[error] run `EFFIGY_OXIDE_ESCAPE=cargo effigy install:local` for the plain-Cargo escape";
}

fs::create_dir(install_dir);
require_replaceable_active_binary(target);
preserve_previous_local_install(target, previous_target, active_version_file, previous_version_file);
activate_local_install(source, staged_target, target, active_version_file, local_active_version());

log("[ok] built local effigy release binary");
log("[ok] installed " + target);
log("[ok] wrote local active version " + active_version_file);
