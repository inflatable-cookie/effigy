let container_name = "linux-release";
let host_artifacts_dir = path::join(repo_root, ".effigy/linux-release/artifacts");
let release_triple = if env("EFFIGY_LINUX_RELEASE_TRIPLE") == "" {
    "x86_64-unknown-linux-gnu"
} else {
    env("EFFIGY_LINUX_RELEASE_TRIPLE")
};
let active_version = str::trim(effigy::active_version());
let copied_binary = path::join(host_artifacts_dir, "effigy-" + release_triple);
let proof_log = path::join(host_artifacts_dir, "rehearsal.txt");
let build_tree = "/var/tmp/effigy-artifact-build";
let release_dir = build_tree + "/target/" + release_triple + "/release";

fs::create_dir(host_artifacts_dir);

// The maintained producer owns compile, capped transfer and publication. This
// rehearsal then smokes the container-local binary. `/workspace` is read-only;
// neither step writes through the source bind.
task::run("workspace:linux:artifact", []);

if !fs::exists(copied_binary) || !fs::is_file(copied_binary) {
    throw "[error] expected published Linux rehearsal binary at " + copied_binary;
}

let smoke_script = "set -eu\n";
smoke_script += "bin=" + str::shell_quote(release_dir + "/effigy") + "\n";
smoke_script += "export EFFIGY_INTERNAL_SUPPRESS_HEADER=1\n";
smoke_script += "\"$bin\" help >/dev/null\n";
smoke_script += "smoke_output=$(\"$bin\" smoke:release \"$bin\")\n";
smoke_script += "printf '%s\\n\\n' \"$smoke_output\"\n";
smoke_script += "glibc_output=$(\"$bin\" release check-binary \"$bin\" --glibc-floor 2.35)\n";
smoke_script += "printf '%s\\n' \"$glibc_output\"\n";

let shell_error = "";
try {
    let output = container::exec_stream(container_name, ["sh", "-lc", smoke_script]);
    if !output["success"] {
        throw "container smoke command exited with status " + output["status"].to_string();
    }
} catch (error) {
    shell_error = error.to_string();
}
if shell_error != "" {
    throw "[error] linux release rehearsal failed\n" + shell_error;
}

fs::write_file(proof_log, "binary=" + release_dir + "/effigy\n" + "published=" + copied_binary + "\n" + "release_triple=" + release_triple + "\n" + "active_version=" + active_version + "\n");
if !fs::exists(proof_log) || !fs::is_file(proof_log) {
    throw "[error] expected Linux rehearsal proof log at " + proof_log;
}
log("[ok] linux release rehearsal passed");
log("[ok] inspected artifact");
