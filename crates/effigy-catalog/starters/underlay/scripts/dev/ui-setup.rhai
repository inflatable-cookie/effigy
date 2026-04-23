// Underlay starter — frontend hydration helper for `dev` tab setup.
//
// Invoked by `tasks.dev` concurrent entries as a `setup = [{ rhai = ... }]`
// step. Runs before the app's dev server starts and ensures bun
// dependencies are installed on a fingerprint-controlled basis.
//
// This is starter-owned helper glue, not Effigy runtime. It supports the
// generic `app-*` layout and the Underlay reference `acme-*` layout. The
// Effigy runtime already supplies
// `run_process_stream`, `path_join`, `task_name`, and `repo_root` as
// script bindings.
//
// Contract expected by the managed `dev` task:
//   - `task_name` is the `name` field of the concurrent entry (e.g. "front").
//   - On failure, throw; the runtime will surface that as a setup error.

let runtime_root = path_join(repo_root, ".effigy/runtime");

let shell_header = "set -eu\n"
    + "\n"
    + "runtime_root='" + runtime_root + "'\n"
    + "lock_dir=\"$runtime_root/ui-setup.lock\"\n"
    + "mkdir -p \"$runtime_root\"\n"
    + "stamp_root=\"$runtime_root/ui-hydration\"\n"
    + "mkdir -p \"$stamp_root\"\n"
    + "setup_task_name='" + task_name + "'\n"
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
    + "    printf 'platform %s %s\\n' \"$(uname -s)\" \"$(uname -m)\"\n"
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

let shell_layout_helpers = "ensure_bun_install_if_present() {\n"
    + "  dir=\"$1\"\n"
    + "  label=\"$2\"\n"
    + "  if [ -f \"$dir/package.json\" ]; then\n"
    + "    ensure_bun_install \"$dir\" \"$label\"\n"
    + "  fi\n"
    + "}\n"
    + "\n"
    + "ensure_first_bun_install() {\n"
    + "  label=\"$1\"\n"
    + "  shift\n"
    + "  for dir in \"$@\"; do\n"
    + "    if [ -f \"$dir/package.json\" ]; then\n"
    + "      ensure_bun_install \"$dir\" \"$label\"\n"
    + "      return 0\n"
    + "    fi\n"
    + "  done\n"
    + "  printf '[warn] skipping %s; none of these package dirs exist: %s\\n' \"$label\" \"$*\"\n"
    + "}\n"
    + "\n";

let shell_targets = "ensure_bun_install_if_present \"../underlay\" \"underlay\"\n"
    + "ensure_bun_install_if_present \"../poodle\" \"poodle\"\n"
    + "ensure_bun_install_if_present \"app-client\" \"app-client\"\n"
    + "ensure_bun_install_if_present \"acme-client\" \"acme-client\"\n"
    + "\n"
    + "case \"$setup_task_name\" in\n"
    + "  front) ensure_first_bun_install \"front\" \"app-front\" \"acme-front\" \"front\" ;;\n"
    + "  admin) ensure_first_bun_install \"admin\" \"app-admin\" \"acme-admin\" \"admin\" ;;\n"
    + "  *) printf '[error] unsupported ui setup target %s\\n' \"$setup_task_name\"; exit 1 ;;\n"
    + "esac\n";

let shell = shell_header + shell_functions + shell_layout_helpers + shell_targets;

let result = run_process_stream("sh", ["-lc", shell]);
if !result["success"] {
    throw `ui setup failed for ${task_name}`;
}
