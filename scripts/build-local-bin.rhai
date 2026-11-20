// Build this checkout's optimized release binary and stage it as the
// checkout-local stable `effigy` (`.local-install/bin/effigy`).
//
// The build goes through Oxide (`oxide build --release --bin effigy`), which
// keeps the Rust intermediates in its shared slot pool and uplifts the final
// binary to `<workspace>/target/release/effigy`. Only that exact artifact is
// installed: a missing release output fails instead of falling back to another
// profile or to plain Cargo. `EFFIGY_OXIDE_ESCAPE=cargo effigy install:local`
// is the documented plain-Cargo escape and produces the same path.
//
// On macOS the staged copy is then signed with Effigy's one stable local
// code-signing identity, so successive rebuilds keep the same designated
// requirement instead of looking like a new program to Gatekeeper. The identity
// policy lives in `scripts/local-codesign.sh`; the selection runs before the
// build so an identity problem fails in seconds, and a signing failure fails
// the install before anything staged replaces the active one.
//
// Install is three steps in `config/tasks.toml`, and this file is the first:
//
//   1. build-local-bin.rhai (here): select the identity, build, stage and sign
//      the release binary as `effigy.new`, keep the active binary and its
//      receipt as the rollback channel, and stage the version and source
//      receipt as `.new` files. The active path is not touched.
//   2. install-local-bin-links.rhai: maintain the PATH launcher.
//   3. activate-local-bin.rhai: rename `effigy.new` over the active path, then
//      publish the staged receipts. Binary activation is its last spawn.
//
// Why the swap waits for the last step: the runner admits every step by
// launching a kernel helper from the effigy binary path on disk. Once the
// active path holds the new build, a step admitted by the old parent would
// launch the new helper and be refused on protocol mismatch (v0.14.1 to v0.15
// crosses helper protocol 6). So the active path changes only in the final
// step, and nothing is admitted after it.
//
// The source receipt is what Recipe E verifies a host binary against. It names
// the final activated bytes (after signing), the checkout's HEAD and tree, the
// active version written beside it, the build kind and the feature set. A dirty
// checkout is recorded as `dirty=true`, never hidden. Without git the install
// still succeeds, the head and tree are left empty and the dirty flag is true,
// so the receipt names no source and Recipe E refuses it.
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

// Sign the staged copy with the selected local identity. The helper is the
// whole policy (selection, provisioning, `EFFIGY_LOCAL_CODESIGN=off`, the
// non-macOS no-op), and an empty identity means this activation was asked to
// install without one. A refused signature removes the staged copy, as a
// refused staged copy does, and never leaves it to be activated later.
fn sign_staged_local_binary(staged_target, codesign_identity) {
    if codesign_identity == "" {
        return;
    }

    let result = process::run(
        "sh",
        ["scripts/local-codesign.sh", "sign", codesign_identity, staged_target],
    );
    if result["success"] {
        return;
    }

    try {
        fs::remove(staged_target);
    } catch (error) {
        log_warn("[warn] could not remove the refused staged copy: " + staged_target);
    }
    throw "[error] sign staged local effigy binary failed with status "
        + result["status"].to_string()
        + "\n[error] set EFFIGY_LOCAL_CODESIGN=off to skip local signing; guide 010 covers the identity"
        + "\nstdout:\n" + result["stdout"].to_string()
        + "\nstderr:\n" + result["stderr"].to_string();
}

// Resolve the identity before the build: selecting, provisioning or refusing it
// is cheap, and a release build is not. The helper prints the identity it
// selected, nothing when signing is skipped, and one explanatory line on stderr.
fn local_codesign_identity() {
    let result = process::run("sh", ["scripts/local-codesign.sh", "select"]);
    let note = str::trim(result["stderr"].to_string());
    if !result["success"] {
        throw "[error] local effigy signing identity selection failed with status "
            + result["status"].to_string()
            + "\n[error] set EFFIGY_LOCAL_CODESIGN=off to skip local signing; guide 010 covers the identity"
            + "\nstdout:\n" + result["stdout"].to_string()
            + "\nstderr:\n" + result["stderr"].to_string();
    }
    if note != "" {
        log("[note] " + note);
    }

    str::trim(result["stdout"].to_string())
}

// Stage the release binary beside the active path and make it ready to
// activate: byte-identical to the build, signed, and executable. Nothing here
// touches the active path.
fn stage_local_binary(source, staged_target, codesign_identity) {
    require_regular_file(source, "built effigy release binary");
    run_check("stage local effigy binary", "cp", [source, staged_target]);
    verify_staged_copy(source, staged_target, "staged local effigy binary");
    sign_staged_local_binary(staged_target, codesign_identity);
    run_check(
        "mark staged local effigy binary executable",
        "chmod",
        ["0755", staged_target],
    );
}

// Keep the rollback channel holding the binary that was active before this
// install. When the active binary already holds the bytes just staged, an
// earlier attempt activated this build and its rollback copy is the one to keep:
// copying the active binary over it would replace the real previous install
// with the new build.
fn preserve_previous_local_install(target, staged_target, previous_target, active_version_file, previous_version_file) {
    if !(fs::exists(target) && fs::is_file(target)) {
        return;
    }
    if fs::sha256(target) == fs::sha256(staged_target) {
        log("[ok] active effigy already holds this build; keeping the existing rollback binary");
        return;
    }

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

// The source receipt names the bytes the active path will hold, so it is
// computed from the staged copy: activation renames those same bytes into place.
fn local_source_receipt(binary, version) {
    let head = capture_process("git", ["rev-parse", "HEAD"]);
    let tree = capture_process("git", ["rev-parse", "HEAD^{tree}"]);
    let known = !(head == () || head == "" || tree == () || tree == "");
    let status = process::run("git", ["status", "--short", "--untracked-files=normal"]);
    let dirty = if !known || !status["success"] || str::trim(status["stdout"].to_string()) != "" {
        "true"
    } else {
        "false"
    };
    let completed = capture_process("date", ["-u", "+%Y-%m-%dT%H:%M:%SZ"]);
    "source_head=" + (if known { head } else { "" }) + "\n"
        + "source_tree=" + (if known { tree } else { "" }) + "\n"
        + "binary_sha256=" + fs::sha256(binary) + "\n"
        + "active_version=" + version + "\n"
        + "build_kind=install-local\n"
        + "features=\n"
        + "dirty=" + dirty + "\n"
        + "completed_at=" + (if completed == () { "" } else { completed }) + "\n"
}

// Stage the version receipt and the source receipt beside their active paths.
// Activation renames both into place after the binary.
fn stage_local_receipts(target, staged_target, active_version_file, version) {
    fs::write_file(active_version_file + ".new", version + "\n");
    fs::write_file(
        target + ".source-receipt.new",
        local_source_receipt(staged_target, version),
    );
}

let codesign_identity = local_codesign_identity();

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
stage_local_binary(source, staged_target, codesign_identity);
preserve_previous_local_install(target, staged_target, previous_target, active_version_file, previous_version_file);
stage_local_receipts(target, staged_target, active_version_file, local_active_version());

log("[ok] built local effigy release binary");
log("[ok] staged " + staged_target + "; activate-local-bin.rhai installs it as the last step");
