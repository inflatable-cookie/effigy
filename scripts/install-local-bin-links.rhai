// Maintain the launcher on PATH for this checkout's stable local install.
//
// A fresh install writes the maintained wrapper. Anything already at the
// launcher path that install:local did not generate is left byte-for-byte
// alone and explained: a customized launcher or a custom symlink is the
// operator's, and this task never clobbers, executes or reinterprets it.
//
// Only a regular file that positively matches the maintained wrapper format
// and a recognised stable target (`<checkout>/.local-install/bin/effigy`) is
// refreshed, and then only by writing a staged file and renaming it over the
// old one.
let link_dir = if env("EFFIGY_LOCAL_BIN_DIR") == "" {
    path::join(env("HOME"), ".local/bin")
} else {
    env("EFFIGY_LOCAL_BIN_DIR")
};

let stable_target = path::join(repo_root, ".local-install/bin/effigy");
let stable_link = path::join(link_dir, "effigy");
let staged_link = path::join(link_dir, ".effigy-wrapper.new");

fn require_regular_file(path, label) {
    if !fs::exists(path) || !fs::is_file(path) {
        throw `[error] missing ${label}: ${path}`;
    }
    if fs::is_symlink(path) {
        throw `[error] refusing symlink ${label}: ${path}`;
    }
}

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if result["success"] {
        return;
    }

    let status = result["status"].to_string();
    let stdout = result["stdout"].to_string();
    let stderr = result["stderr"].to_string();
    throw "[error] " + label + " failed with status " + status
        + "\nstdout:\n" + stdout + "\nstderr:\n" + stderr;
}

fn wrapper_script(target) {
    "#!/usr/bin/env bash\nset -euo pipefail\nexec \"" + target + "\" \"$@\"\n"
}

// Read a regular file only when it is valid UTF-8 text. Unreadable or binary
// content is reported as unknown, never as a wrapper to overwrite.
fn read_text_or_none(path) {
    try {
        return fs::read_file(path);
    } catch (error) {
        return ();
    }
}

// Return the target when `contents` is exactly the maintained wrapper for a
// recognised stable target, otherwise (). Both the format and the target have
// to be positively known before anything is refreshed.
fn recognised_generated_target(contents) {
    let prefix = "#!/usr/bin/env bash\nset -euo pipefail\nexec \"";
    let suffix = "\" \"$@\"\n";
    if !str::starts_with(contents, prefix) || !str::ends_with(contents, suffix) {
        return ();
    }

    let candidate = str::replace(str::replace(contents, prefix, ""), suffix, "");
    if candidate == "" || candidate == contents {
        return ();
    }
    if !str::starts_with(candidate, "/") {
        return ();
    }
    if str::contains(candidate, "\"") || str::contains(candidate, "\n") {
        return ();
    }
    if !str::ends_with(candidate, "/.local-install/bin/effigy") {
        return ();
    }

    candidate
}

fn write_wrapper(target, wrapper_path, staged_path) {
    fs::write_file(staged_path, wrapper_script(target));
    run_check("mark wrapper executable", "chmod", ["0755", staged_path]);
    run_check("activate stable channel wrapper", "mv", [staged_path, wrapper_path]);
}

fn explain_preserved(predicate, link_path) {
    log_warn(`[warn] preserved existing ${predicate}: ${link_path}`);
    log_warn("       it is not the wrapper install:local generates, so it was left untouched");
    log_warn("       to adopt the maintained wrapper, move it aside and re-run `effigy install:local`");
}

require_regular_file(stable_target, "stable effigy binary");

fs::create_dir(link_dir);

if fs::is_symlink(stable_link) {
    explain_preserved("symlink launcher", stable_link);
} else if !fs::exists(stable_link) {
    write_wrapper(stable_target, stable_link, staged_link);
    log(`[ok] wrote stable channel wrapper: ${stable_link} -> ${stable_target}`);
} else if !fs::is_file(stable_link) {
    throw `[error] refusing to replace unsupported path: ${stable_link}`;
} else {
    let contents = read_text_or_none(stable_link);
    let recognised = if contents == () { () } else { recognised_generated_target(contents) };
    if recognised == () {
        explain_preserved("custom launcher", stable_link);
    } else if recognised == stable_target {
        log(`[ok] stable channel wrapper already current: ${stable_link} -> ${stable_target}`);
    } else {
        write_wrapper(stable_target, stable_link, staged_link);
        log(`[ok] refreshed generated stable channel wrapper: ${stable_link} -> ${stable_target}`);
        log(`[ok] replaced previous generated target: ${recognised}`);
    }
}

let path_value = ":" + env("PATH") + ":";
let rendered_link_dir = ":" + link_dir + ":";
if !path_value.contains(rendered_link_dir) {
    log_warn(`[warn] ${link_dir} is not currently on PATH`);
    log_warn(`       add: export PATH="${link_dir}:$PATH"`);
}

log("[note] if your shell still resolves `effigy` as an alias, remove that alias from your shell rc and restart the shell");
