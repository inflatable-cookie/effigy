let default_repo_url = "https://github.com/inflatable-cookie/effigy.git";
let repo_url = default_repo_url;
let tag = env("GITHUB_REF_NAME");

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
        let help = run_process("cargo", ["run", "--bin", "effigy", "--", "release", "verify-install", "--help"]);
        if help["stdout"] != "" { log(help["stdout"]); }
        if help["stderr"] != "" { log_warn(help["stderr"]); }
        if !help["success"] { throw("release verify-install --help failed"); }
        return;
    }
    log_warn(`[error] unknown option: ${arg} (supported: --tag, --repo-url)`);
    throw("unknown argument");
}

if tag == "" {
    log_warn("[error] tag is required (pass --tag <tag> or set GITHUB_REF_NAME)");
    throw("missing tag");
}

let result = run_process("cargo", [
    "run", "--bin", "effigy", "--",
    "release", "verify-install",
    "--repo", repo_root,
    "--tag", tag,
    "--repo-url", repo_url,
]);
if result["stdout"] != "" { log(result["stdout"]); }
if result["stderr"] != "" { log_warn(result["stderr"]); }
if !result["success"] { throw("release verify-install failed"); }
