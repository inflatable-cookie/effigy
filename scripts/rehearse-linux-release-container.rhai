let container_name = "linux-release";
let image_name = "effigy-linux-release-builder:latest";
let host_artifacts_dir = path::join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let active_version = str::trim(effigy::active_version());
let copied_binary = path::join(host_artifacts_dir, "effigy-" + release_triple);
let proof_log = path::join(host_artifacts_dir, "rehearsal.txt");

fn down_container(name) {
    let output = container::down(name);
    if output.trim() != "" {
        log(output);
    }
}

fs::create_dir(host_artifacts_dir);
process::run(
    "colima",
    ["nerdctl", "--profile", "effigy-release", "--", "image", "rm", image_name],
);
let up = container::up(container_name, true);
if up.trim() != "" {
    log(up);
}

let build_script = "set -eu\n";
build_script += "mkdir -p /workspace/.effigy/linux-release/artifacts\n";
build_script += "export CARGO_TARGET_DIR=/workspace/.effigy/linux-release/target\n";
build_script += "export EFFIGY_INTERNAL_SUPPRESS_HEADER=1\n";
build_script += "export EFFIGY_ACTIVE_VERSION=" + str::shell_quote(active_version) + "\n";
build_script += "cd /workspace\n";
build_script += "rustc --version > /workspace/.effigy/linux-release/artifacts/toolchain.txt\n";
build_script += "cargo build --release --bin effigy\n";
build_script += "printf '%s\\n' \"$EFFIGY_ACTIVE_VERSION\" > \"${CARGO_TARGET_DIR}/release/effigy.active-version\"\n";
build_script += "printf '\\n\\n'\n";
build_script += "\"${CARGO_TARGET_DIR}/release/effigy\" help >/dev/null\n";
build_script += "smoke_output=$(\"${CARGO_TARGET_DIR}/release/effigy\" smoke:release \"${CARGO_TARGET_DIR}/release/effigy\")\n";
build_script += "printf '%s\\n\\n' \"$smoke_output\"\n";
build_script += "glibc_output=$(\"${CARGO_TARGET_DIR}/release/effigy\" release check-binary \"${CARGO_TARGET_DIR}/release/effigy\" --glibc-floor 2.35)\n";
build_script += "printf '%s\\n' \"$glibc_output\"\n";
build_script += "cp \"${CARGO_TARGET_DIR}/release/effigy\" \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n";
build_script += "cp \"${CARGO_TARGET_DIR}/release/effigy.active-version\" \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + ".active-version\"\n";
build_script += "{\n";
build_script += "  echo \"binary=${CARGO_TARGET_DIR}/release/effigy\"\n";
build_script += "  echo \"copied_binary=/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n";
build_script += "  echo \"release_triple=" + release_triple + "\"\n";
build_script += "  echo \"completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n";
build_script += "} > /workspace/.effigy/linux-release/artifacts/rehearsal.txt\n";

let shell_error = "";
try {
    // This stays as a container-owned shell script on purpose. The rehearsal is
    // a stateful in-container build flow with exports, command substitution,
    // redirection, and artifact writes inside `/workspace`; flattening it into
    // one-off Rhai host helpers would not simplify the behavior.
    let output = container::shell(container_name, build_script);
    if output.trim() != "" {
        log(output);
    }
} catch (error) {
    shell_error = error.to_string();
}
if shell_error != "" {
    down_container(container_name);
    throw "[error] linux release rehearsal failed\n" + shell_error;
}

down_container(container_name);
if !fs::exists(copied_binary) || !fs::is_file(copied_binary) {
    throw "[error] expected copied Linux rehearsal binary at " + copied_binary;
}
if !fs::exists(proof_log) || !fs::is_file(proof_log) {
    throw "[error] expected Linux rehearsal proof log at " + proof_log;
}
log("[ok] linux release rehearsal passed");
log("[ok] inspected artifact");
