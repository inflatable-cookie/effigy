let host_artifacts_dir = path::join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let active_version = str::trim(effigy::active_version());
let copied_binary = path::join(host_artifacts_dir, "effigy-" + release_triple);
let receipt_path = path::join(host_artifacts_dir, "rehearsal.txt");

fs::create_dir(host_artifacts_dir);

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
build_script += "artifact_sha256=$(sha256sum \"/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\" | cut -d' ' -f1)\n";
build_script += "{\n";
build_script += "  echo \"binary=${CARGO_TARGET_DIR}/release/effigy\"\n";
build_script += "  echo \"copied_binary=/workspace/.effigy/linux-release/artifacts/effigy-" + release_triple + "\"\n";
build_script += "  echo \"release_triple=" + release_triple + "\"\n";
build_script += "  echo \"sha256=$artifact_sha256\"\n";
build_script += "  echo \"build_kind=workspace-artifact\"\n";
build_script += "  echo \"completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n";
build_script += "} > /workspace/.effigy/linux-release/artifacts/rehearsal.txt\n";

container::workspace_artifact_build(["sh", "-lc", build_script]);

if !fs::exists(copied_binary) || !fs::is_file(copied_binary) {
    throw "[error] expected copied Linux workspace artifact at " + copied_binary;
}
if !fs::exists(receipt_path) || !fs::is_file(receipt_path) {
    throw "[error] expected Linux workspace artifact receipt at " + receipt_path;
}
let version_path = copied_binary + ".active-version";
if !fs::is_file(version_path) || str::trim(fs::read_file(version_path)) != active_version {
    throw "[error] Linux workspace artifact active version does not match the host";
}
let receipt = fs::env_file_map(receipt_path);
if receipt["release_triple"] != release_triple || fs::file_size(copied_binary) == 0 {
    throw "[error] Linux workspace artifact build did not produce valid output for " + release_triple;
}
// The producer receipt names the digest its consumer authenticates; a receipt
// without one cannot bless the bytes (contract 052 §8.9).
if !receipt.contains("sha256") || receipt["sha256"] == "" {
    throw "[error] Linux workspace artifact receipt names no sha256 digest";
}
log("[ok] linux workspace artifact built");
