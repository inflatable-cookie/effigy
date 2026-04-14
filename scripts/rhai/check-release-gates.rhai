let tag = "";
let repo_url = "";

let i = 0;
while i < args.len {
    let arg = args[i].to_string();
    if arg == "--tag" {
        if i + 1 >= args.len {
            throw("missing value for --tag");
        }
        tag = args[i + 1].to_string();
        i += 2;
        continue;
    }
    if arg == "--repo-url" {
        if i + 1 >= args.len {
            throw("missing value for --repo-url");
        }
        repo_url = args[i + 1].to_string();
        i += 2;
        continue;
    }
    if arg == "-h" || arg == "--help" {
        let help = run_process("cargo", ["run", "--bin", "effigy", "--", "release", "--help"]);
        if help["stdout"] != "" { log(help["stdout"]); }
        if help["stderr"] != "" { log_warn(help["stderr"]); }
        if !help["success"] { throw("release --help failed"); }
        return;
    }
    log_warn(`[error] unknown option: ${arg} (supported: --tag, --repo-url)`);
    throw("unknown argument");
}

fn run_step(label, command_args) {
    log(`[check] ${label}`);
    let result = run_process("cargo", command_args);
    if result["stdout"] != "" { log(result["stdout"]); }
    if result["stderr"] != "" { log_warn(result["stderr"]); }
    if !result["success"] { throw(`step failed: ${label}`); }
    log(`[ok] ${label}`);
}

run_step("release gates", [
    "run", "--bin", "effigy", "--",
    "release", "gates", "--repo", repo_root
]);

if tag != "" {
    let verify_args = [
        "run", "--bin", "effigy", "--",
        "release", "verify-install", "--repo", repo_root, "--tag", tag
    ];
    if repo_url != "" {
        verify_args.push("--repo-url");
        verify_args.push(repo_url);
    }
    run_step("release install validation from tag", verify_args);
} else {
    log("[info] skipping tag install validation (no --tag provided)");
}

log("[ok] release gates passed");
