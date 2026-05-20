let container_name = "linux-release";
let host_artifacts_dir = path::join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let active_version = str::trim(effigy::active_version());
let copied_binary = path::join(host_artifacts_dir, "effigy-" + release_triple);
let receipt_path = path::join(host_artifacts_dir, "rehearsal.txt");

fn down_container(name) {
    let output = container::down(name);
    if output.trim() != "" {
        log(output);
    }
}

fs::create_dir(host_artifacts_dir);
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
build_script += "cargo build --release --bin effigy\n";
build_script += "printf '%s\\n' \"$EFFIGY_ACTIVE_VERSION\" > \"${CARGO_TARGET_DIR}/release/effigy.active-version\"\n";
build_script += "cp \"${CARGO_TARGET_DIR}/release/effigy\" \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n";
build_script += "cp \"${CARGO_TARGET_DIR}/release/effigy.active-version\" \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + ".active-version\"\n";
build_script += "{\n";
build_script += "  echo \"binary=${CARGO_TARGET_DIR}/release/effigy\"\n";
build_script += "  echo \"copied_binary=/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n";
build_script += "  echo \"release_triple=" + release_triple + "\"\n";
build_script += "  echo \"build_kind=workspace-artifact\"\n";
build_script += "  echo \"completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n";
build_script += "} > /workspace/.effigy/linux-release/artifacts/rehearsal.txt\n";

let shell_error = "";
try {
    let output = container::shell(container_name, build_script);
    if output.trim() != "" {
        log(output);
    }
} catch (error) {
    shell_error = error.to_string();
}
if shell_error != "" {
    down_container(container_name);
    throw "[error] linux workspace artifact build failed\n" + shell_error;
}

down_container(container_name);
if !fs::exists(copied_binary) || !fs::is_file(copied_binary) {
    throw "[error] expected copied Linux workspace artifact at " + copied_binary;
}
if !fs::exists(receipt_path) || !fs::is_file(receipt_path) {
    throw "[error] expected Linux workspace artifact receipt at " + receipt_path;
}
log("[ok] linux workspace artifact built");
