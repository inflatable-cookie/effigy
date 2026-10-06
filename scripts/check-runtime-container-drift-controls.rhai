// Private positive/negative proof for the runtime/container drift guard.
// Disposable fixture trees only. No live runtime, no production scan.

fn collect_matches(root, pattern, options) {
    if root == () || str::trim(root) == "" {
        return [];
    }

    let result = search::files(root, pattern, options);
    if result["count"] == 0 {
        return [];
    }

    result["matches"]
}

fn normalize_path(entry_path, scan_root) {
    let rendered = entry_path.to_string();
    let prefix = scan_root + "/";
    if str::starts_with(rendered, prefix) {
        return str::replace(rendered, prefix, "");
    }

    rendered
}

fn unexpected_matches(scan_root, roots, pattern, options, allowed_path_regex) {
    let unexpected = [];
    for root in roots {
        let matches = collect_matches(root, pattern, options);
        for entry in matches {
            if !regex::is_match(allowed_path_regex, normalize_path(entry["path"], scan_root)) {
                unexpected.push(
                    normalize_path(entry["path"], scan_root)
                        + ":"
                        + entry["line"].to_string()
                        + ":"
                        + entry["text"].to_string()
                );
            }
        }
    }
    unexpected
}

fn require_contains(source, needle, label) {
    if !str::contains(source, needle) {
        throw "drift-guard controls: production script lost " + label + ": " + needle;
    }
}

fn require_absent(source, needle, label) {
    if str::contains(source, needle) {
        throw "drift-guard controls: production script regained prohibited " + label + ": " + needle;
    }
}

fn path_hit(unexpected, path_prefix) {
    for line in unexpected {
        if str::starts_with(line, path_prefix) {
            return true;
        }
    }
    false
}

fn expect_allowed(label, unexpected, path_prefix) {
    if path_hit(unexpected, path_prefix) {
        throw "drift-guard controls: authorized boundary failed (" + label + " at " + path_prefix + ")";
    }
    log("[ok] authorized " + label + " at " + path_prefix);
}

fn expect_rejected(label, unexpected, path_prefix) {
    if !path_hit(unexpected, path_prefix) {
        throw "drift-guard controls: prohibited " + label + " was not rejected at " + path_prefix;
    }
    log("[ok] rejected " + label + " at " + path_prefix);
}

let guard_source = fs::read_file(path::join(repo_root, "scripts/check-runtime-container-drift.rhai"));

require_contains(guard_source, `^src/runner/exec_command/tests\.rs$`, "cwd allowlist");
require_contains(
    guard_source,
    `src/runner/system_command/workspace_permissions_gnu_accept\.rs`,
    "gnu-accept raw-CLI allowlist",
);
require_contains(guard_source, "022-runtime-architecture-sanity-audit.md", "guard diagnostic owner");
require_absent(guard_source, "052-drift-guards-and-architecture-proof-matrix-strict-lane.md", "obsolete diagnostic");
require_absent(guard_source, "qa_group_command/execute", "qa-group cwd whole-file waiver");
require_absent(
    guard_source,
    "src/runner/system_command/workspace_permissions.rs$",
    "permission-prep whole-file waiver",
);

let cwd_pattern = `std::env::current_dir\(`;
let backend_pattern = `resolve_compose_backend|ComposeBackend`;
let raw_pattern = `Command::new\("(docker|colima|nerdctl)"`;
let compose_pattern = `compose_args\(`;
let glob = #{ glob: "*.rs" };

let cwd_allow = `^src/runner/exec_command/tests\.rs$`;
let backend_allow = `^(src/runner/doctor_ports\.rs|src/runner/exec_command/transport\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_command/lifecycle\.rs)$`;
let raw_allow = `^(src/runner/doctor_ports\.rs|src/runner/bootstrap_command/mod\.rs|src/runner/system_command/workspace_permissions_gnu_accept\.rs)$`;
let compose_allow = `^(crates/effigy-runtime/src/container_manager\.rs|src/runner/managed_shell\.rs|src/runner/demo_command/execute/task/selection\.rs|src/runner/exec_command/transport\.rs|src/runner/exec_command/transport/colima\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_runtime_prep/mod\.rs|src/runner/container_command/support\.rs|src/runner/deferral/run\.rs|src/runner/execute/pipeline/managed\.rs|src/runner/system_command/workspace_provisioning\.rs)$`;

require_contains(guard_source, cwd_allow, "cwd allowlist regex");
require_contains(guard_source, backend_allow, "backend allowlist regex");
require_contains(guard_source, raw_allow, "raw-CLI allowlist regex");
require_contains(guard_source, compose_allow, "compose_args allowlist regex");

let fixture = fs::make_temp_dir("effigy-runtime-container-drift-controls");
let runner = path::join(fixture, "src/runner");
let runtime = path::join(fixture, "crates/effigy-runtime/src");

fs::write_file(
    path::join(runner, "exec_command/tests.rs"),
    "fn fixture() { let _ = std::env::current_dir(); }\n",
);
fs::write_file(
    path::join(runner, "qa_group_command/execute.rs"),
    "fn production() { let _ = std::env::current_dir(); }\n",
);
fs::write_file(
    path::join(runner, "doctor_ports.rs"),
    "fn adapter() {\n    let _ = ComposeBackend::Docker;\n    let _ = std::process::Command::new(\"colima\");\n    let _ = compose_args(policy, [\"ps\"]);\n}\n",
);
fs::write_file(
    path::join(runner, "exec_command/transport.rs"),
    "fn adapter() { let _ = resolve_compose_backend(); let _ = compose_args(policy, [\"exec\"]); }\n",
);
fs::write_file(
    path::join(runner, "container_runtime_prep/prep.rs"),
    "fn adapter() { let _ = resolve_compose_backend(); let _ = compose_args(policy, [\"up\"]); }\n",
);
fs::write_file(
    path::join(runner, "container_command/lifecycle.rs"),
    "fn adapter() { let _ = ComposeBackend::ColimaNerdctl; }\n",
);
fs::write_file(
    path::join(runner, "bootstrap_command/mod.rs"),
    "fn adapter() { let _ = std::process::Command::new(\"docker\"); }\n",
);
fs::write_file(
    path::join(runner, "system_command/workspace_permissions_gnu_accept.rs"),
    "fn fixture() { let _ = std::process::Command::new(\"colima\"); }\n",
);
fs::write_file(
    path::join(runner, "system_command/workspace_permissions.rs"),
    "fn production_exec() { let _ = compose_args(policy, [\"exec\", \"-T\"]);\n    let _ = std::process::Command::new(\"colima\");\n}\n#[cfg(test)]\nfn fixture() { let _ = std::process::Command::new(\"colima\"); }\n",
);
fs::write_file(
    path::join(runner, "execute/pipeline/standard.rs"),
    "fn production() {\n    match ComposeBackend::Docker { _ => \"docker-compose\" }\n    let _ = compose_args(policy, [\"down\"]);\n}\n",
);
fs::write_file(
    path::join(runtime, "container_manager.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"ps\"]); }\n",
);
fs::write_file(
    path::join(runner, "managed_shell.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"up\"]); }\n",
);
fs::write_file(
    path::join(runner, "demo_command/execute/task/selection.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"exec\"]); }\n",
);
fs::write_file(
    path::join(runner, "exec_command/transport/colima.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"ps\"]); }\n",
);
fs::write_file(
    path::join(runner, "container_runtime_prep/mod.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"restart\"]); }\n",
);
fs::write_file(
    path::join(runner, "container_command/support.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"up\"]); }\n",
);
fs::write_file(
    path::join(runner, "deferral/run.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"exec\"]); }\n",
);
fs::write_file(
    path::join(runner, "execute/pipeline/managed.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"down\"]); }\n",
);
fs::write_file(
    path::join(runner, "system_command/workspace_provisioning.rs"),
    "fn adapter() { let _ = compose_args(policy, [\"exec\"]); }\n",
);

let runner_root = path::join(fixture, "src/runner");
let runtime_root = path::join(fixture, "crates/effigy-runtime/src");
let cwd_roots = [runner_root];
let shared_roots = [runner_root, runtime_root];

let cwd_hits = unexpected_matches(fixture, cwd_roots, cwd_pattern, glob, cwd_allow);
expect_allowed("cwd", cwd_hits, "src/runner/exec_command/tests.rs");
expect_rejected("current_dir", cwd_hits, "src/runner/qa_group_command/execute.rs");

let backend_hits = unexpected_matches(fixture, shared_roots, backend_pattern, glob, backend_allow);
expect_allowed("backend", backend_hits, "src/runner/doctor_ports.rs");
expect_allowed("backend", backend_hits, "src/runner/exec_command/transport.rs");
expect_rejected("backend", backend_hits, "src/runner/execute/pipeline/standard.rs");

let raw_hits = unexpected_matches(fixture, shared_roots, raw_pattern, glob, raw_allow);
expect_allowed("raw runtime", raw_hits, "src/runner/doctor_ports.rs");
expect_allowed("raw runtime", raw_hits, "src/runner/bootstrap_command/mod.rs");
expect_allowed("raw runtime", raw_hits, "src/runner/system_command/workspace_permissions_gnu_accept.rs");
expect_rejected("raw runtime", raw_hits, "src/runner/system_command/workspace_permissions.rs");

let compose_hits = unexpected_matches(fixture, shared_roots, compose_pattern, glob, compose_allow);
expect_allowed("compose_args", compose_hits, "crates/effigy-runtime/src/container_manager.rs");
expect_allowed("compose_args", compose_hits, "src/runner/system_command/workspace_provisioning.rs");
expect_rejected("compose_args", compose_hits, "src/runner/execute/pipeline/standard.rs");
expect_rejected("compose_args", compose_hits, "src/runner/doctor_ports.rs");
expect_rejected("compose_args", compose_hits, "src/runner/system_command/workspace_permissions.rs");

fs::remove(fixture);
log("[ok] runtime/container drift-guard positive and negative controls");
