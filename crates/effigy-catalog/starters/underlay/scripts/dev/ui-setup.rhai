// Underlay starter — frontend hydration helper for `dev` tab setup.
//
// Invoked by `tasks.dev` concurrent entries as a `setup = [{ rhai = ... }]`
// step. Runs before the app's dev server starts and ensures bun
// dependencies are installed on a fingerprint-controlled basis.
//
// This is starter-owned helper glue, not Effigy runtime — adapt it for
// your repo's frontend layout. The Effigy runtime already supplies
// `run_process_stream`, `path_join`, `task_name`, and `repo_root` as
// script bindings.
//
// Contract expected by the managed `dev` task:
//   - `task_name` is the `name` field of the concurrent entry (e.g. "front").
//   - On failure, throw; the runtime will surface that as a setup error.

let target = if task_name == "front" {
    "app-front"
} else if task_name == "admin" {
    "app-admin"
} else {
    throw `unsupported ui setup target '${task_name}'`;
};

let runtime_root = path_join(repo_root, ".effigy/runtime");

let shell_header = "set -eu\n"
    + "\n"
    + "runtime_root='" + runtime_root + "'\n"
    + "lock_dir=\"$runtime_root/ui-setup.lock\"\n"
    + "mkdir -p \"$runtime_root\"\n"
    + "stamp_root=\"$runtime_root/ui-hydration\"\n"
    + "mkdir -p \"$stamp_root\"\n"
    + "target_package='" + target + "'\n"
    + "\n"
    + "while ! mkdir \"$lock_dir\" 2>/dev/null; do\n"
    + "  sleep 0.2\n"
    + "done\n"
    + "\n"
    + "cleanup() {\n"
    + "  rmdir \"$lock_dir\" 2>/dev/null || true\n"
    + "}\n"
    + "trap cleanup EXIT INT TERM\n"
    + "\n";

let shell_functions = "package_fingerprint() {\n"
    + "  dir=\"$1\"\n"
    + "  (\n"
    + "    cd \"$dir\"\n"
    + "    if [ -f bun.lock ]; then\n"
    + "      cksum package.json bun.lock\n"
    + "    else\n"
    + "      cksum package.json\n"
    + "    fi\n"
    + "  ) | cksum | awk '{print $1\":\"$2}'\n"
    + "}\n"
    + "\n"
    + "ensure_bun_install() {\n"
    + "  dir=\"$1\"\n"
    + "  label=\"$2\"\n"
    + "\n"
    + "  if [ ! -f \"$dir/package.json\" ]; then\n"
    + "    printf '[warn] skipping %s; no package.json at %s\\n' \"$label\" \"$dir\"\n"
    + "    return 0\n"
    + "  fi\n"
    + "\n"
    + "  fingerprint=$(package_fingerprint \"$dir\")\n"
    + "  stamp_path=\"$stamp_root/$label.stamp\"\n"
    + "\n"
    + "  if [ -f \"$stamp_path\" ] \\\n"
    + "    && [ \"$(cat \"$stamp_path\")\" = \"$fingerprint\" ] \\\n"
    + "    && [ -d \"$dir/node_modules\" ] \\\n"
    + "    && [ -n \"$(ls -A \"$dir/node_modules\" 2>/dev/null)\" ]; then\n"
    + "    printf '[info] %s already hydrated\\n' \"$label\"\n"
    + "    return 0\n"
    + "  fi\n"
    + "\n"
    + "  printf '[info] bun install -> %s\\n' \"$label\"\n"
    + "  rm -f \"$stamp_path\"\n"
    + "  (\n"
    + "    cd \"$dir\"\n"
    + "    bun install\n"
    + "  )\n"
    + "  printf '%s\\n' \"$fingerprint\" > \"$stamp_path\"\n"
    + "}\n"
    + "\n";

// Edit this block to match your repo's frontend layout. The Underlay
// reference repo hydrates two sibling shared packages (`../underlay`,
// `../poodle`), a shared in-repo package (`acme-client`), and the
// per-tab target app.
let shell_targets = "ensure_bun_install \"app-client\" \"app-client\"\n"
    + "ensure_bun_install \"$target_package\" \"$target_package\"\n";

let shell = shell_header + shell_functions + shell_targets;

let result = run_process_stream("sh", ["-lc", shell]);
if !result["success"] {
    throw `ui setup failed for ${target}`;
}
