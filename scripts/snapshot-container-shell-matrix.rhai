let report_dir = path::join(repo_root, ".effigy/perf/container-shell-matrix");
let summary_path = path::join(report_dir, "summary.json");
let baseline_dir = path::join(report_dir, "baselines");

fn resolve_repo_path(path_value) {
    let rendered = path_value.to_string();
    if str::starts_with(rendered, "/") {
        return rendered;
    }
    path::join(repo_root, rendered)
}

if !fs::exists(summary_path) {
    throw "[error] missing matrix summary: " + summary_path
        + "\nrun `effigy perf:container-shell-matrix` first";
}

fn default_snapshot_path(baseline_dir) {
    let timestamp = time::now_utc();
    let compact = str::replace(str::replace(str::replace(timestamp, ":", ""), "-", ""), "T", "-");
    let name = "baseline-" + str::replace(compact, "Z", "") + ".json";
    path::join(baseline_dir, name)
}

let snapshot_path = if args.is_empty() {
    default_snapshot_path(baseline_dir)
} else {
    resolve_repo_path(args[0].to_string())
};

if !fs::exists(baseline_dir) {
    fs::create_dir(baseline_dir);
}

fs::write_file(snapshot_path, fs::read_file(summary_path));
log("[ok] wrote baseline snapshot " + snapshot_path);
log("[next] compare with `effigy perf:container-shell-compare " + snapshot_path + "`");
