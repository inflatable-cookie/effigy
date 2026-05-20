let report_dir = path::join(repo_root, ".effigy/perf/container-shell-matrix");
let default_summary_path = path::join(report_dir, "summary.json");

if args.is_empty() {
    throw "[error] usage: effigy perf:container-shell-compare <baseline-summary.json> [candidate-summary.json] [threshold-seconds]\n"
        + "example: effigy perf:container-shell-compare .effigy/perf/container-shell-matrix/baseline.json";
}

let baseline_path = args[0].to_string();
let candidate_path = if args.len() >= 2 {
    args[1].to_string()
} else {
    default_summary_path
};
let threshold_seconds = if args.len() >= 3 {
    args[2].to_string()
} else {
    "0.25"
};

fn load_summary(path_value) {
    if !fs::exists(path_value) {
        throw "[error] summary file not found: " + path_value;
    }

    let parsed = json::read_file(path_value);
    if !parsed.contains("targets") {
        throw "[error] summary file missing `targets`: " + path_value;
    }
    parsed
}

fn parse_seconds_hundredths(raw) {
    let rendered = raw.to_string();
    let captures = regex::captures(`^([0-9]+)(?:\.([0-9]+))?$`, rendered);
    if !captures["matched"] {
        throw "[error] invalid seconds value: " + rendered;
    }

    let whole = str::parse_int(captures["groups"][1].to_string());
    let fraction = if captures["groups"].len > 2 && captures["groups"][2] != () {
        captures["groups"][2].to_string()
    } else {
        ""
    };
    let hundredths = if fraction == "" {
        0
    } else if fraction.len >= 2 {
        let fraction_captures = regex::captures(`^([0-9][0-9]).*$`, fraction);
        let group = fraction_captures["groups"][1];
        let digits = group.to_string();
        str::parse_int(digits)
    } else {
        str::parse_int(fraction + "0")
    };
    whole * 100 + hundredths
}

fn render_seconds_hundredths(value) {
    let whole = value / 100;
    let fraction = value % 100;
    if fraction < 10 {
        return whole.to_string() + ".0" + fraction.to_string();
    }
    whole.to_string() + "." + fraction.to_string()
}

fn worst_steady_state_hundredths(target) {
    let values = target["steady_state_real_seconds"];
    if values.len == 0 {
        throw "[error] target `" + target["label"].to_string() + "` has no steady-state timings";
    }

    let worst = 0;
    for value in values {
        let parsed = parse_seconds_hundredths(value.to_string());
        if parsed > worst {
            worst = parsed;
        }
    }
    worst
}

fn index_targets(summary) {
    let indexed = #{};
    for target in summary["targets"] {
        indexed[target["label"].to_string()] = target;
    }
    indexed
}

let baseline = load_summary(baseline_path);
let candidate = load_summary(candidate_path);
let baseline_targets = index_targets(baseline);
let candidate_targets = index_targets(candidate);
let threshold_hundredths = parse_seconds_hundredths(threshold_seconds);
let failures = [];
let warnings = [];
let summaries = [];

for label in baseline_targets.keys() {
    if !candidate_targets.contains(label) {
        warnings.push("candidate missing target `" + label + "`");
        continue;
    }

    let before = baseline_targets[label];
    let after = candidate_targets[label];
    let before_ready = before["primary_service_exec_ready"];
    let after_ready = after["primary_service_exec_ready"];
    let before_worst = worst_steady_state_hundredths(before);
    let after_worst = worst_steady_state_hundredths(after);
    let delta = after_worst - before_worst;

    summaries.push(
        label
            + ": "
            + before["primary_service_exec_ready_summary"].to_string()
            + " -> "
            + after["primary_service_exec_ready_summary"].to_string()
            + ", worst steady-state "
            + render_seconds_hundredths(before_worst)
            + "s -> "
            + render_seconds_hundredths(after_worst)
            + "s"
    );

    if before_ready != after_ready {
        failures.push(
            "readiness changed for `" + label + "`: "
                + before["primary_service_exec_ready_summary"].to_string()
                + " -> "
                + after["primary_service_exec_ready_summary"].to_string()
        );
    }

    if delta > threshold_hundredths {
        failures.push(
            "steady-state regression for `" + label + "`: +"
                + render_seconds_hundredths(delta)
                + "s (threshold "
                + render_seconds_hundredths(threshold_hundredths)
                + "s)"
        );
    }
}

for label in candidate_targets.keys() {
    if !baseline_targets.contains(label) {
        warnings.push("candidate added target `" + label + "`");
    }
}

for line in summaries {
    log("[info] " + line);
}
for line in warnings {
    log_warn("[warn] " + line);
}

if !failures.is_empty() {
    let message = failures.join("\n");
    throw "[error] container shell matrix regression detected\n" + message;
}

log("[ok] no readiness flips or regressions above " + threshold_seconds + "s");
