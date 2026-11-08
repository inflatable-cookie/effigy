// The target is part of the build, never merely the output's label. Missing
// target libraries or a cross linker must fail before a receipt can be written.
//
// Cargo intermediates and the three artifact roles stay in the build's own
// private tree on the builder's writable layer (effigy#743, effigy#749). The
// `/workspace` source bind is read-only: this script never writes through it.
// The host transfers the three roles after positive closure.
// The declared boundary. Rhai globals are not in a function's scope, so the
// script builder and the post-build checks each name the same literals.
fn artifact_build_tree() { "/var/tmp/effigy-artifact-build" }
fn artifact_caps() { #{"binary": 209715200, "receipt": 65536, "sidecar": 4096} }

fn workspace_artifact_build_script(release_triple, active_version) {
    let build_tree = artifact_build_tree();
    let caps = artifact_caps();
    if !["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
         "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"].contains(release_triple) {
        throw "unsupported Linux workspace artifact target: " + release_triple;
    }
    let target_dir = build_tree + "/target";
    let release_dir = target_dir + "/" + release_triple + "/release";
    let built_binary = release_dir + "/effigy";
    let built_sidecar = release_dir + "/effigy.active-version";
    let built_receipt = release_dir + "/effigy.receipt";
    let build_script = "set -eu\n";
    build_script += "mkdir -p " + str::shell_quote(target_dir) + "\n";
    build_script += "chmod 700 " + str::shell_quote(build_tree) + " " + str::shell_quote(target_dir) + "\n";
    build_script += "export CARGO_TARGET_DIR=" + str::shell_quote(target_dir) + "\n";
    build_script += "export EFFIGY_INTERNAL_SUPPRESS_HEADER=1\n";
    build_script += "export EFFIGY_ACTIVE_VERSION=" + str::shell_quote(active_version) + "\n";
    build_script += "cd /workspace\n";
    build_script += "cargo build --release --bin effigy --target " + str::shell_quote(release_triple) + "\n";
    build_script += "printf '%s\\n' \"$EFFIGY_ACTIVE_VERSION\" > " + str::shell_quote(built_sidecar) + "\n";
    build_script += "binary_bytes=$(wc -c < " + str::shell_quote(built_binary) + " | tr -d ' ')\n";
    build_script += "if [ \"$binary_bytes\" -gt " + caps["binary"].to_string() + " ]; then echo \"artifact-over-cap $binary_bytes\"; exit 61; fi\n";
    build_script += "sidecar_bytes=$(wc -c < " + str::shell_quote(built_sidecar) + " | tr -d ' ')\n";
    build_script += "if [ \"$sidecar_bytes\" -gt " + caps["sidecar"].to_string() + " ]; then echo \"sidecar-over-cap $sidecar_bytes\"; exit 61; fi\n";
    build_script += "artifact_sha256=$(sha256sum " + str::shell_quote(built_binary) + " | cut -d' ' -f1)\n";
    build_script += "{\n";
    build_script += "  echo \"binary=" + built_binary + "\"\n";
    build_script += "  echo \"release_triple=" + release_triple + "\"\n";
    build_script += "  echo \"sha256=$artifact_sha256\"\n";
    build_script += "  echo \"build_kind=workspace-artifact\"\n";
    build_script += "  echo \"completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n";
    build_script += "} > " + str::shell_quote(built_receipt) + "\n";
    build_script += "receipt_bytes=$(wc -c < " + str::shell_quote(built_receipt) + " | tr -d ' ')\n";
    build_script += "if [ \"$receipt_bytes\" -gt " + caps["receipt"].to_string() + " ]; then echo \"receipt-over-cap $receipt_bytes\"; exit 61; fi\n";
    return build_script;
}

let host_artifacts_dir = path::join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let active_version = str::trim(effigy::active_version());
let export_caps = artifact_caps();
let copied_binary = path::join(host_artifacts_dir, "effigy-" + release_triple);
let receipt_path = copied_binary + ".receipt";

fs::create_dir(host_artifacts_dir);

let build_script = workspace_artifact_build_script(release_triple, active_version);

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
// The declared per-file caps hold on what the build exported, so a caller never
// receives an artifact above the boundary the build claims.
if fs::file_size(copied_binary) > export_caps["binary"] {
    throw "[error] Linux workspace artifact exceeds its declared 200 MiB cap";
}
if fs::file_size(receipt_path) > export_caps["receipt"] {
    throw "[error] Linux workspace artifact receipt exceeds its declared 64 KiB cap";
}
if fs::file_size(version_path) > export_caps["sidecar"] {
    throw "[error] Linux workspace artifact sidecar exceeds its declared 4 KiB cap";
}
log("[ok] linux workspace artifact built");
