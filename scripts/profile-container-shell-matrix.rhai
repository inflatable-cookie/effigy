let report_dir = path::join(repo_root, ".effigy/perf/container-shell-matrix");
fs::create_dir(report_dir);

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

let matrix = [
    #{
        label: "decodelabs-brains",
        repo: "/Users/tom/Dev/legacy/sites/brains",
        container: "web",
    },
    #{
        label: "underlay-reference-stack",
        repo: "/Users/tom/Dev/projects/underlay-reference",
        container: "stack",
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

fn write_wrapper(wrapper_dir, name, log_var, real_var) {
    let script = "#!/bin/sh\n";
    let body = script
        + "printf '" + name + "\\t%s\\n' \"$*\" >> \"$" + log_var + "\"\n"
        + "exec \"$" + real_var + "\" \"$@\"\n";
    let path = path::join(wrapper_dir, name);
    fs::write_file(path, body);
    run_check("chmod " + name + " wrapper", "chmod", ["0755", path]);
    path
}

fn render_shell_command(bin_path, target, stdout_path, stderr_path) {
    let container_name = target["container"].to_string();
    let repo_path = shell_quote(target["repo"]);
    let stdout_file = shell_quote(stdout_path);
    let stderr_file = shell_quote(stderr_path);
    "/usr/bin/time -p "
        + shell_quote(bin_path)
        + " container "
        + container_name
        + " shell --repo "
        + repo_path
        + " --command 'true' > "
        + stdout_file
        + " 2> "
        + stderr_file
        + "\n"
}

fn read_if_exists(file_path) {
    if fs::exists(file_path) {
        return fs::read_file(file_path);
    }
    ""
}

fn trim_text(value) {
    str::trim(value.to_string())
}

fn run_trace(bin_path, target) {
    let wrapper_dir = fs::make_temp_dir("effigy-shell-trace");
    let trace_log = path::join(wrapper_dir, "invocations.log");
    let stdout_path = path::join(wrapper_dir, "stdout.txt");
    let stderr_path = path::join(wrapper_dir, "stderr.txt");
    let real_colima = process::run("/bin/sh", ["-lc", "command -v colima"])["stdout"]
        .to_string();
    let real_docker = process::run("/bin/sh", ["-lc", "command -v docker"])["stdout"]
        .to_string();
    let quoted_real_colima = shell_quote(str::trim(real_colima));
    let quoted_real_docker = shell_quote(str::trim(real_docker));
    let quoted_trace_log = shell_quote(trace_log);
    let quoted_wrapper_dir = shell_quote(wrapper_dir);
    write_wrapper(wrapper_dir, "colima", "EFFIGY_TRACE_LOG", "EFFIGY_REAL_COLIMA");
    write_wrapper(wrapper_dir, "docker", "EFFIGY_TRACE_LOG", "EFFIGY_REAL_DOCKER");

    let command = "set -eu\n";
    command += "export EFFIGY_REAL_COLIMA=" + quoted_real_colima + "\n";
    command += "export EFFIGY_REAL_DOCKER=" + quoted_real_docker + "\n";
    command += "PATH=" + quoted_wrapper_dir + ":$PATH\n";
    command += "export EFFIGY_TRACE_LOG=" + quoted_trace_log + "\n";
    command += render_shell_command(bin_path, target, stdout_path, stderr_path);
    run_check("trace " + target["label"], "/bin/sh", ["-lc", command]);
    #{
        trace: fs::read_file(trace_log),
        stdout: read_if_exists(stdout_path),
        stderr: read_if_exists(stderr_path),
    }
}

fn run_timing(bin_path, target) {
    let timing_dir = fs::make_temp_dir("effigy-shell-timing");
    let results = [];
    for index in [1, 2] {
        let stdout_path = path::join(timing_dir, "stdout-" + index.to_string() + ".txt");
        let stderr_path = path::join(timing_dir, "stderr-" + index.to_string() + ".txt");
        let command = "set -eu\n";
        command += render_shell_command(bin_path, target, stdout_path, stderr_path);
        run_check(
            "timing run " + target["label"] + " #" + index.to_string(),
            "/bin/sh",
            ["-lc", command],
        );
        results.push(#{
            stdout: read_if_exists(stdout_path),
            stderr: read_if_exists(stderr_path),
        });
    }
    results
}

fn render_entry_report(target, trace_result, timing_results) {
    let lines = [
        "# " + target["label"].to_string(),
        "",
        "repo: " + target["repo"].to_string(),
        "container: " + target["container"].to_string(),
        "",
        "## traced invocation",
        "```text",
        trim_text(trace_result["trace"]),
        "```",
        "",
        "## trace stderr",
        "```text",
        trim_text(trace_result["stderr"]),
        "```",
        "",
        "## steady-state timings",
    ];
    for timing in timing_results {
        lines.push("```text");
        lines.push(trim_text(timing["stderr"]));
        lines.push("```");
    }
    lines.push("");
    lines
}

let index_lines = [
    "# Container shell command matrix",
    "",
    "binary: " + bin_path,
    "generated_at: " + time::now_utc(),
    "",
];

for target in matrix {
    log("[info] profiling " + target["label"]);
    let trace_result = run_trace(bin_path, target);
    let timing_results = run_timing(bin_path, target);
    let report_path = path::join(report_dir, target["label"].to_string() + ".md");
    fs::write_lines(report_path, render_entry_report(target, trace_result, timing_results));
    log("[ok] " + target["label"] + " -> " + report_path);
    for timing in timing_results {
        let summary = trim_text(timing["stderr"]);
        if summary != "" {
            log(summary);
        }
    }
    index_lines.push("- " + target["label"].to_string() + ": " + report_path);
}

let index_path = path::join(report_dir, "README.md");
fs::write_lines(index_path, index_lines);
log("[ok] wrote matrix index " + index_path);
