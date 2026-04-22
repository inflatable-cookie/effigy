let container_name = "linux-release";
let image_name = "effigy-linux-release-builder:latest";
let host_artifacts_dir = path_join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let copied_binary = path_join(host_artifacts_dir, "effigy-" + release_triple);
let proof_log = path_join(host_artifacts_dir, "rehearsal.txt");

fn run_effigy_checked(label, process_args) {
    let result = run_effigy(process_args);
    if !result["success"] {
        let status = result["status"].to_string();
        let stdout = result["output"].to_string();
        let stderr = result["error"].to_string();
        throw "[error] " + label + " failed with status " + status
            + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
    }
    let stdout = result["output"].to_string();
    if stdout.trim() != "" {
        log(stdout);
    }
    let stderr = result["rendered_output"].to_string();
    if stderr.trim() != "" {
        log_warn(stderr);
    }
    result
}

fn down_container(name) {
    let output = container_down(name);
    if output.trim() != "" {
        log(output);
    }
}

fn fail_with_cleanup(label, result, name) {
    down_container(name);
    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

create_dir(host_artifacts_dir);
run_process(
    "colima",
    ["nerdctl", "--profile", "effigy-release", "--", "image", "rm", image_name],
);
let up = container_up(container_name, true);
if up.trim() != "" {
    log(up);
}

let build_script =
    "set -eu\n"
    + "mkdir -p /workspace/.effigy/linux-release/artifacts\n"
    + "export CARGO_TARGET_DIR=/workspace/.effigy/linux-release/target\n"
    + "export EFFIGY_INTERNAL_SUPPRESS_HEADER=1\n"
    + "cd /workspace\n"
    + "rustc --version > /workspace/.effigy/linux-release/artifacts/toolchain.txt\n"
    + "cargo build --release --bin effigy\n"
    + "printf '\\n\\n'\n"
    + "\"${CARGO_TARGET_DIR}/release/effigy\" help >/dev/null\n"
    + "smoke_output=$(\"${CARGO_TARGET_DIR}/release/effigy\" smoke:release \"${CARGO_TARGET_DIR}/release/effigy\")\n"
    + "printf '%s\\n\\n' \"$smoke_output\"\n"
    + "glibc_output=$(\"${CARGO_TARGET_DIR}/release/effigy\" distribution check-glibc-floor --binary \"${CARGO_TARGET_DIR}/release/effigy\" --max-glibc 2.35)\n"
    + "printf '%s\\n' \"$glibc_output\"\n"
    + "cp \"${CARGO_TARGET_DIR}/release/effigy\" \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n"
    + "{\n"
    + "  echo \"binary=${CARGO_TARGET_DIR}/release/effigy\"\n"
    + "  echo \"copied_binary=/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n"
    + "  echo \"release_triple=" + release_triple + "\"\n"
    + "  echo \"completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n"
    + "} > /workspace/.effigy/linux-release/artifacts/rehearsal.txt\n";

let shell_result = run_effigy([
    "container",
    container_name,
    "shell",
    "--command",
    build_script,
]);
if !shell_result["success"] {
    fail_with_cleanup("linux release rehearsal", shell_result, container_name);
}

down_container(container_name);
if !path_exists(copied_binary) || !is_file(copied_binary) {
    throw "[error] expected copied Linux rehearsal binary at " + copied_binary;
}
if !path_exists(proof_log) || !is_file(proof_log) {
    throw "[error] expected Linux rehearsal proof log at " + proof_log;
}
log("[ok] linux release rehearsal passed");
log("[ok] inspected artifact");
