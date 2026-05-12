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

fn collect_matches_many(roots, pattern, options) {
    let combined = [];
    for root in roots {
        let matches = collect_matches(root, pattern, options);
        for entry in matches {
            combined.push(entry);
        }
    }
    combined
}

fn normalize_path(path) {
    let rendered = path.to_string();
    let prefix = repo_root + "/";
    if str::starts_with(rendered, prefix) {
        return str::replace(rendered, prefix, "");
    }

    rendered
}

fn render_match(entry) {
    normalize_path(entry["path"])
        + ":"
        + entry["line"].to_string()
        + ":"
        + entry["text"].to_string()
}

fn check_no_unlisted_matches(name, roots, pattern, options, allowed_path_regex) {
    let matches = collect_matches_many(roots, pattern, options);
    if matches.len() == 0 {
        log("[ok] " + name);
        return 0;
    }

    let unexpected = [];
    for entry in matches {
        if !regex::is_match(allowed_path_regex, normalize_path(entry["path"])) {
            unexpected.push(render_match(entry));
        }
    }

    if unexpected.len() == 0 {
        log("[ok] " + name + " (allowlisted debt only)");
        return 0;
    }

    log_warn("[fail] " + name);
    for line in unexpected {
        log_warn(line);
    }
    return 1;
}

let failure_count = 0;

failure_count += check_no_unlisted_matches(
    "runner current_dir calls",
    ["src/runner"],
    `std::env::current_dir\(`,
    #{ glob: "*.rs" },
    `^src/runner/exec_command/tests\.rs$`,
);

failure_count += check_no_unlisted_matches(
    "runner compose backend branching",
    ["src/runner", "crates/effigy-runtime/src"],
    `resolve_compose_backend|ComposeBackend`,
    #{ glob: "*.rs" },
    `^(src/runner/doctor_ports\.rs|src/runner/exec_command/transport\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_command/lifecycle\.rs)$`,
);

failure_count += check_no_unlisted_matches(
    "runner raw container CLIs",
    ["src/runner", "crates/effigy-runtime/src"],
    `Command::new\("(docker|colima|nerdctl)"`,
    #{ glob: "*.rs" },
    `^(src/runner/doctor_ports\.rs|src/runner/bootstrap_command/mod\.rs)$`,
);

failure_count += check_no_unlisted_matches(
    "compose_args callers outside manager adapters",
    ["src/runner", "crates/effigy-runtime/src"],
    `compose_args\(`,
    #{ glob: "*.rs" },
    `^(crates/effigy-runtime/src/container_manager\.rs|src/runner/managed_shell\.rs|src/runner/demo_command/execute/task/selection\.rs|src/runner/exec_command/transport\.rs|src/runner/exec_command/transport/colima\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_runtime_prep/mod\.rs|src/runner/container_command/support\.rs|src/runner/deferral/run\.rs|src/runner/execute/pipeline/standard\.rs|src/runner/execute/pipeline/managed\.rs|src/runner/system_command/workspace_provisioning\.rs)$`,
);

failure_count += check_no_unlisted_matches(
    "legacy docker capture helper callers",
    ["src/runner", "crates/effigy-runtime/src"],
    `run_docker_capture`,
    #{ glob: "*.rs" },
    `^$`,
);

failure_count += check_no_unlisted_matches(
    "legacy container exec capture callers",
    ["src/runner", "crates/effigy-rhai/src"],
    `run_container_exec_capture`,
    #{ glob: "*.rs" },
    `^(src/runner/db_seed\.rs|src/runner/container_command/data\.rs|src/runner/container_command/lifecycle\.rs|src/runner/container_command/mod\.rs)$`,
);

failure_count += check_no_unlisted_matches(
    "rhai direct container exec capture bypass",
    ["crates/effigy-rhai/src", "src/runner/script_command"],
    `run_container_exec_capture`,
    #{ glob: "*.rs" },
    `^$`,
);

if failure_count > 0 {
    throw "Runtime/container drift guard failed.\n"
        + "Either route the call through the current request/plan/manager surface, or add\n"
        + "a deliberate temporary allowance in scripts/check-runtime-container-drift.rhai and\n"
        + "document it in docs/specs/052-drift-guards-and-architecture-proof-matrix-strict-lane.md.";
}
