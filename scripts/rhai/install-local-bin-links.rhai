let link_dir = if env("EFFIGY_LOCAL_BIN_DIR") == "" {
    path_join(env("HOME"), ".local/bin")
} else {
    env("EFFIGY_LOCAL_BIN_DIR")
};

let stable_target = path_join(repo_root, ".local-install/bin/effigy");
let dev_target = path_join(repo_root, "scripts/effigy-dev");
let stable_link = path_join(link_dir, "effigy");
let dev_link = path_join(link_dir, "effigy-dev");

fn ensure_file(path, label) {
    if !is_file(path) {
        throw `[error] missing ${label}: ${path}`;
    }
}

fn run_check(label, program, process_args) {
    let result = run_process(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn replace_symlink(target, link_path) {
    if path_exists(link_path) || is_symlink(link_path) {
        if !is_symlink(link_path) {
            throw `[error] refusing to replace non-symlink path: ${link_path}`;
        }
        remove_path(link_path);
    }
    create_symlink(target, link_path);
}

fn replace_wrapper(target, wrapper_path) {
    if path_exists(wrapper_path) || is_symlink(wrapper_path) {
        if !is_symlink(wrapper_path) && !is_file(wrapper_path) {
            throw `[error] refusing to replace unsupported path: ${wrapper_path}`;
        }
        remove_path(wrapper_path);
    }
    let script = "#!/usr/bin/env bash\nset -euo pipefail\nexec \"" + target + "\" \"$@\"\n";
    write_file(wrapper_path, script);
    run_check("mark wrapper executable", "chmod", ["0755", wrapper_path]);
}

ensure_file(stable_target, "stable effigy binary");
ensure_file(dev_target, "effigy-dev wrapper");

create_dir(link_dir);
replace_wrapper(stable_target, stable_link);
replace_symlink(dev_target, dev_link);

log(`[ok] wrote stable channel wrapper: ${stable_link} -> ${stable_target}`);
log(`[ok] linked dev channel: ${dev_link} -> ${dev_target}`);

let path_value = ":" + env("PATH") + ":";
let rendered_link_dir = ":" + link_dir + ":";
if !path_value.contains(rendered_link_dir) {
    log_warn(`[warn] ${link_dir} is not currently on PATH`);
    log_warn(`       add: export PATH="${link_dir}:$PATH"`);
}

log("[note] if your shell still resolves `effigy` as an alias, remove that alias from your shell rc and restart the shell");
