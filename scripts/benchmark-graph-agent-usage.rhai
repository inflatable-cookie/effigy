let report_dir = path::join(repo_root, ".effigy/perf/graph-agent-benchmark");
fs::create_dir(report_dir);

let fixture_root = path::join(repo_root, "tests/fixtures/graph-agent-benchmark");

let bin_path = if !args.is_empty() {
    args[0].to_string()
} else if fs::exists(path::join(repo_root, ".local-install/bin/effigy")) {
    path::join(repo_root, ".local-install/bin/effigy")
} else if fs::exists(path::join(repo_root, "target/bootstrap-local/debug/effigy")) {
    path::join(repo_root, "target/bootstrap-local/debug/effigy")
} else if fs::exists(path::join(repo_root, "target/debug/effigy")) {
    path::join(repo_root, "target/debug/effigy")
} else {
    "effigy"
};

let targets = [
    #{
        label: "fixture-split-owner",
        optional: false,
        repo: path::join(fixture_root, "split-owner"),
        cases: [
            #{
                id: "split-owner-edit",
                preferred_tool: "graph",
                query: "where does shell exit cleanup prompt run",
                expected_edit_target: "src/closeout.rs",
                expected_test_file: "tests/closeout_test.rs",
                expected_test_task: "test",
                rg_pattern: "maybe_confirm_shell_exit_cleanup",
            },
            #{
                id: "exact-token-test",
                preferred_tool: "rg",
                query: "closeout_prompt_runs",
                expected_rg_path: "tests/closeout_test.rs",
                rg_pattern: "closeout_prompt_runs",
            },
        ],
    },
    #{
        label: "fixture-behavior-redirect",
        optional: false,
        repo: path::join(fixture_root, "behavior-redirect"),
        cases: [
            #{
                id: "redirect-owner",
                preferred_tool: "graph",
                query: "where are redirect responses handled",
                expected_primary: "src/http_redirect.rs",
                rg_pattern: "build_redirect_response",
            },
        ],
    },
    #{
        label: "fixture-validation-migration",
        optional: false,
        repo: path::join(fixture_root, "validation-migration"),
        cases: [
            #{
                id: "migration-validation-owner",
                preferred_tool: "graph",
                query: "where are config migrations validated before apply",
                expected_primary: "src/config_migration.rs",
                rg_pattern: "validate_manifest_migration_plan",
            },
        ],
    },
    #{
        label: "effigy-live",
        optional: false,
        repo: repo_root,
        cases: [
            #{
                id: "shell-exit-prompt",
                preferred_tool: "graph",
                query: "where does effigy prompt to shut containers down on shell exit",
                expected_edit_target: "src/runner/container_command/closeout.rs",
                rg_pattern: "maybe_confirm_container_shell_exit_cleanup",
            },
        ],
    },
    #{
        label: "underlay-reference-live",
        optional: true,
        repo: "/Users/tom/Dev/projects/underlay-reference",
        cases: [
            #{
                id: "admin-validation-commands",
                preferred_tool: "graph",
                query: "where are admin validation commands handled",
                expected_primary: "acme-client/src/commands/admin/validation-commands.ts",
                rg_pattern: "validation-commands",
            },
        ],
    },
    #{
        label: "decodelabs-brains-live",
        optional: true,
        repo: "/Users/tom/Dev/legacy/sites/brains",
        cases: [
            #{
                id: "codebase-hook-updates",
                preferred_tool: "graph",
                query: "where are codebase hook updates handled",
                expected_primary: "legacy/directory/front/hooks/_nodes/HttpCodebase.php",
                rg_pattern: "_onUpdateTicket",
            },
        ],
    },
];

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if result["success"] {
        return result;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn shell_quote(value) {
    str::shell_quote(value.to_string())
}

fn trim_text(value) {
    str::trim(value.to_string())
}

fn repo_available(target) {
    let repo = target["repo"].to_string();
    fs::exists(repo) && fs::exists(path::join(repo, "effigy.toml"))
}

fn maybe_extract_real_seconds(stderr_text) {
    for line in str::split_lines(stderr_text) {
        let trimmed = trim_text(line);
        if str::starts_with(trimmed, "real ") {
            return trim_text(str::replace(trimmed, "real ", ""));
        }
    }
    ()
}

fn contains_field(items, field, expected) {
    if items == () {
        return false;
    }
    for item in items {
        if item[field].to_string() == expected.to_string() {
            return true;
        }
    }
    false
}

fn first_field(items, field) {
    if items == () {
        return ();
    }
    if items.is_empty() {
        return ();
    }
    items[0][field].to_string()
}

fn item_count(items) {
    if items == () {
        return 0;
    }
    items.len()
}

fn render_timed_shell(command, stdout_path, stderr_path) {
    "set -eu\n"
        + "/usr/bin/time -p " + command
        + " > " + shell_quote(stdout_path)
        + " 2> " + shell_quote(stderr_path)
        + "\n"
}

fn run_graph_index(bin_path, repo_path) {
    log("[info] indexing " + repo_path);
    run_check(
        "graph index " + repo_path,
        bin_path,
        ["graph", "index", "--repo", repo_path, "--json"],
    );
}

fn run_graph_explore(bin_path, repo_path, query) {
    let temp_dir = fs::make_temp_dir("effigy-graph-benchmark");
    let stdout_path = path::join(temp_dir, "stdout.json");
    let stderr_path = path::join(temp_dir, "stderr.txt");
    let command = shell_quote(bin_path)
        + " graph explore " + shell_quote(query)
        + " --repo " + shell_quote(repo_path)
        + " --json";
    run_check(
        "graph explore " + repo_path + " :: " + query,
        "/bin/sh",
        ["-lc", render_timed_shell(command, stdout_path, stderr_path)],
    );
    let envelope = json::parse(fs::read_file(stdout_path));
    #{
        payload: envelope["result"]["payload"],
        elapsed_seconds: maybe_extract_real_seconds(fs::read_file(stderr_path)),
        stderr: fs::read_file(stderr_path),
    }
}

fn run_scan_json(bin_path, repo_path, scan_args) {
    let temp_dir = fs::make_temp_dir("effigy-scan-benchmark");
    let stdout_path = path::join(temp_dir, "stdout.json");
    let stderr_path = path::join(temp_dir, "stderr.txt");
    let command = shell_quote(bin_path) + " --json scan";
    for arg in scan_args {
        command += " " + shell_quote(arg.to_string());
    }
    command += " --repo " + shell_quote(repo_path);
    run_check(
        "scan " + repo_path,
        "/bin/sh",
        ["-lc", render_timed_shell(command, stdout_path, stderr_path)],
    );
    let envelope = json::parse(fs::read_file(stdout_path));
    #{
        payload: envelope["result"],
        elapsed_seconds: maybe_extract_real_seconds(fs::read_file(stderr_path)),
        stderr: fs::read_file(stderr_path),
    }
}

fn run_rg(repo_path, pattern) {
    let temp_dir = fs::make_temp_dir("effigy-rg-benchmark");
    let stdout_path = path::join(temp_dir, "stdout.txt");
    let stderr_path = path::join(temp_dir, "stderr.txt");
    let command = "rg -n --fixed-strings "
        + shell_quote(pattern)
        + " "
        + shell_quote(repo_path);
    let result = process::run(
        "/bin/sh",
        ["-lc", render_timed_shell(command, stdout_path, stderr_path)],
    );
    #{
        success: result["success"],
        stdout: if fs::exists(stdout_path) { fs::read_file(stdout_path) } else { "" },
        stderr: if fs::exists(stderr_path) { fs::read_file(stderr_path) } else { "" },
        elapsed_seconds: if fs::exists(stderr_path) {
            maybe_extract_real_seconds(fs::read_file(stderr_path))
        } else {
            ()
        },
    }
}

fn rg_first_hit_path(stdout_text, repo_path) {
    for line in str::split_lines(stdout_text) {
        let trimmed = trim_text(line);
        if trimmed == "" {
            continue;
        }
        let first_colon = trimmed.index_of(":");
        if first_colon >= 0 {
            let absolute = trimmed.sub_string(0, first_colon).to_string();
            if str::starts_with(absolute, repo_path.to_string() + "/") {
                return str::replace(absolute, repo_path.to_string() + "/", "");
            }
        }
    }
    ()
}

fn graph_packet_sufficient(benchmark_case, payload) {
    let primary = payload["primary"];
    let edit_targets = payload["edit_targets"];
    let likely_test_files = payload["likely_test_files"];
    let likely_test_tasks = payload["likely_test_tasks"];

    let primary_ok = if "expected_primary" in benchmark_case {
        contains_field(primary, "path", benchmark_case["expected_primary"])
    } else {
        true
    };
    let edit_ok = if "expected_edit_target" in benchmark_case {
        contains_field(edit_targets, "path", benchmark_case["expected_edit_target"])
    } else {
        true
    };
    let test_file_ok = if "expected_test_file" in benchmark_case {
        contains_field(likely_test_files, "path", benchmark_case["expected_test_file"])
    } else {
        true
    };
    let test_task_ok = if "expected_test_task" in benchmark_case {
        contains_field(likely_test_tasks, "name", benchmark_case["expected_test_task"])
    } else {
        true
    };

    primary_ok && edit_ok && test_file_ok && test_task_ok
}

fn graph_first_hit_correct(benchmark_case, payload) {
    if "expected_edit_target" in benchmark_case {
        let edit_targets = payload["edit_targets"];
        return edit_targets != ()
            && !edit_targets.is_empty()
            && edit_targets[0]["path"].to_string() == benchmark_case["expected_edit_target"].to_string();
    }
    if "expected_primary" in benchmark_case {
        let primary = payload["primary"];
        return primary != ()
            && !primary.is_empty()
            && primary[0]["path"].to_string() == benchmark_case["expected_primary"].to_string();
    }
    false
}

fn expected_rg_path(benchmark_case) {
    if "expected_rg_path" in benchmark_case {
        return benchmark_case["expected_rg_path"].to_string();
    }
    if "expected_primary" in benchmark_case {
        return benchmark_case["expected_primary"].to_string();
    }
    if "expected_edit_target" in benchmark_case {
        return benchmark_case["expected_edit_target"].to_string();
    }
    ()
}

fn evaluate_case(target, benchmark_case, bin_path) {
    let repo_path = target["repo"].to_string();
    let graph = run_graph_explore(bin_path, repo_path, benchmark_case["query"].to_string());
    let payload = graph["payload"];
    let rg = run_rg(repo_path, benchmark_case["rg_pattern"].to_string());

    let packet_sufficient = graph_packet_sufficient(benchmark_case, payload);
    let graph_hit = graph_first_hit_correct(benchmark_case, payload);
    let rg_hit_path = rg_first_hit_path(rg["stdout"], repo_path);
    let expected_rg = expected_rg_path(benchmark_case);
    let rg_hit = if rg_hit_path == () || expected_rg == () {
        false
    } else {
        rg_hit_path.to_string() == expected_rg.to_string()
    };

    let preferred = benchmark_case["preferred_tool"].to_string();
    let fallback_search_count = if preferred == "rg" {
        1
    } else if packet_sufficient {
        0
    } else {
        1
    };
    let resolved_via = if preferred == "rg" {
        "rg-preferred"
    } else if packet_sufficient {
        "graph"
    } else if rg_hit {
        "graph+rg"
    } else {
        "unresolved"
    };

    #{
        id: benchmark_case["id"].to_string(),
        preferred_tool: preferred,
        query: benchmark_case["query"].to_string(),
        graph_command_count: 1,
        fallback_search_count: fallback_search_count,
        graph_elapsed_seconds: graph["elapsed_seconds"],
        rg_elapsed_seconds: rg["elapsed_seconds"],
        graph_first_hit_correct: graph_hit,
        graph_packet_sufficient: packet_sufficient,
        rg_first_hit_correct: rg_hit,
        resolved_via: resolved_via,
        top_primary: first_field(payload["primary"], "path"),
        top_edit_target: first_field(payload["edit_targets"], "path"),
        likely_test_file_count: item_count(payload["likely_test_files"]),
        likely_test_task_count: item_count(payload["likely_test_tasks"]),
        rg_first_hit_path: rg_hit_path,
    }
}

fn render_case_line(case_result) {
    let line = "- " + case_result["id"].to_string();
    line += " :: preferred=" + case_result["preferred_tool"].to_string();
    line += ", resolved_via=" + case_result["resolved_via"].to_string();
    line += ", graph_hit=" + case_result["graph_first_hit_correct"].to_string();
    line += ", packet_sufficient=" + case_result["graph_packet_sufficient"].to_string();
    line += ", rg_hit=" + case_result["rg_first_hit_correct"].to_string();
    line
}

fn evaluate_scan_case(target, benchmark_case, bin_path) {
    let repo_path = target["repo"].to_string();
    let scan = run_scan_json(bin_path, repo_path, benchmark_case["args"]);
    let payload = scan["payload"];
    let findings = payload["findings"];
    let likely_test_files = payload["likely_test_files"];
    let likely_test_tasks = payload["likely_test_tasks"];
    let graph = payload["graph"];

    let likely_file_ok = if "expected_test_file" in benchmark_case {
        contains_field(likely_test_files, "path", benchmark_case["expected_test_file"])
    } else {
        true
    };
    let likely_task_ok = if "expected_test_task" in benchmark_case {
        contains_field(likely_test_tasks, "name", benchmark_case["expected_test_task"])
    } else {
        true
    };
    let graph_state = if graph != () { graph["state"].to_string() } else { "" };
    let graph_applied = if graph != () { graph["applied"] == true } else { false };

    #{
        id: benchmark_case["id"].to_string(),
        scan: benchmark_case["scan"].to_string(),
        elapsed_seconds: scan["elapsed_seconds"],
        graph_state: graph_state,
        graph_applied: graph_applied,
        finding_count: item_count(findings),
        likely_test_file_count: item_count(likely_test_files),
        likely_test_task_count: item_count(likely_test_tasks),
        checks_ok: likely_file_ok && likely_task_ok,
    }
}

fn render_scan_case_line(case_result) {
    "- " + case_result["id"].to_string()
        + " :: scan=" + case_result["scan"].to_string()
        + ", checks_ok=" + case_result["checks_ok"].to_string()
        + ", graph_state=" + case_result["graph_state"].to_string()
        + ", graph_applied=" + case_result["graph_applied"].to_string()
}

let index_lines = [
    "# Graph agent benchmark",
    "",
    "binary: " + bin_path,
    "generated_at: " + time::now_utc(),
    "",
];

let summary_targets = [];
let scan_targets = [
    #{
        label: "fixture-split-owner-scan",
        optional: false,
        repo: path::join(fixture_root, "split-owner"),
        cases: [
            #{
                id: "validation-gap-likely-test",
                scan: "validation-gaps",
                args: ["validation-gaps", "--path", "src/closeout.rs", "--graph-context"],
                expected_test_file: "tests/closeout_test.rs",
                expected_test_task: "test",
            },
        ],
    },
];

for target in targets {
    if !repo_available(target) {
        let line = "- " + target["label"].to_string() + ": skipped (repo unavailable)";
        index_lines.push(line);
        summary_targets.push(#{
            label: target["label"].to_string(),
            repo: target["repo"].to_string(),
            optional: target["optional"],
            skipped: true,
            skip_reason: "repo unavailable",
            cases: [],
        });
        continue;
    }

    run_graph_index(bin_path, target["repo"].to_string());

    let case_results = [];
    for benchmark_case in target["cases"] {
        log("[info] benchmark " + target["label"].to_string() + " :: " + benchmark_case["id"].to_string());
        case_results.push(evaluate_case(target, benchmark_case, bin_path));
    }

    let report_path = path::join(report_dir, target["label"].to_string() + ".md");
    let lines = [
        "# " + target["label"].to_string(),
        "",
        "repo: " + target["repo"].to_string(),
        "",
        "## cases",
    ];
    for case_result in case_results {
        lines.push(render_case_line(case_result));
    }
    fs::write_lines(report_path, lines);

    index_lines.push("- " + target["label"].to_string() + ": " + report_path);
    summary_targets.push(#{
        label: target["label"].to_string(),
        repo: target["repo"].to_string(),
        optional: target["optional"],
        skipped: false,
        cases: case_results,
        report_path: report_path,
    });
}

for target in scan_targets {
    if !repo_available(target) {
        let line = "- " + target["label"].to_string() + ": skipped (repo unavailable)";
        index_lines.push(line);
        summary_targets.push(#{
            label: target["label"].to_string(),
            repo: target["repo"].to_string(),
            optional: target["optional"],
            skipped: true,
            skip_reason: "repo unavailable",
            scan_cases: [],
        });
        continue;
    }

    run_graph_index(bin_path, target["repo"].to_string());

    let case_results = [];
    for benchmark_case in target["cases"] {
        log("[info] scan benchmark " + target["label"].to_string() + " :: " + benchmark_case["id"].to_string());
        case_results.push(evaluate_scan_case(target, benchmark_case, bin_path));
    }

    let report_path = path::join(report_dir, target["label"].to_string() + ".md");
    let lines = [
        "# " + target["label"].to_string(),
        "",
        "repo: " + target["repo"].to_string(),
        "",
        "## scan cases",
    ];
    for case_result in case_results {
        lines.push(render_scan_case_line(case_result));
    }
    fs::write_lines(report_path, lines);

    index_lines.push("- " + target["label"].to_string() + ": " + report_path);
    summary_targets.push(#{
        label: target["label"].to_string(),
        repo: target["repo"].to_string(),
        optional: target["optional"],
        skipped: false,
        scan_cases: case_results,
        report_path: report_path,
    });
}

let index_path = path::join(report_dir, "README.md");
fs::write_lines(index_path, index_lines);
let json_path = path::join(report_dir, "summary.json");
fs::write_file(
    json_path,
    json::stringify(#{
        binary: bin_path,
        generated_at: time::now_utc(),
        targets: summary_targets,
    }),
);
log("[ok] wrote graph benchmark index " + index_path);
log("[ok] wrote graph benchmark summary " + json_path);
