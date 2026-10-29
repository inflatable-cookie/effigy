use super::*;

#[test]
fn run_effigy_json_surfaces_callback_errors_as_runtime_errors() {
    let root = temp_root("effigy-json-error");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let callbacks = HostCallbacks {
        run_task: callbacks().run_task,
        run_effigy: Arc::new(|_, _, _| {
            Err(EffigyCommandError {
                message: "boom".to_owned(),
                rendered_output: "rendered".to_owned(),
            })
        }),
        run_feature: callbacks().run_feature,
        container_up: callbacks().container_up,
        container_down: callbacks().container_down,
        container_shell: callbacks().container_shell,
        container_exec: callbacks().container_exec,
        container_exec_with_options: callbacks().container_exec_with_options,
        container_exec_stream: callbacks().container_exec_stream,
    };

    let error = execute_rhai_script(&context, "effigy::run_json([\"demo\"]);", &[], &callbacks)
        .expect_err("must fail");
    assert!(error.to_string().contains("boom"));
}

#[test]
fn host_log_message_colors_known_status_prefixes() {
    let rendered = render_host_log_message(
        "[ok] passed\n[check] running\n[gateway] route installed\n[bootstrap] starting dev\n[next] inspect\n",
        true,
    );

    assert!(
        rendered.contains("\u{1b}["),
        "expected ansi styles in: {rendered:?}"
    );
    assert!(rendered.contains("[ok]"));
    assert!(rendered.contains(" passed"));
    assert!(rendered.contains("[check]"));
    assert!(rendered.contains(" running"));
    assert!(rendered.contains("[gateway]"));
    assert!(rendered.contains(" route installed"));
    assert!(rendered.contains("[bootstrap]"));
    assert!(rendered.contains(" starting dev"));
    assert!(rendered.contains("[next]"));
    assert!(rendered.contains(" inspect"));
}

#[test]
fn host_log_message_leaves_plain_text_unchanged_without_color() {
    let rendered = render_host_log_message("plain\n[warn] careful\n", false);
    assert_eq!(rendered, "plain\n[warn] careful\n");
}

#[test]
fn first_party_rhai_scripts_do_not_recursively_invoke_effigy() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if contents.contains("process::run(\"effigy\"")
            || contents.contains("process::run(`effigy`")
            || contents.contains("process::stream(\"effigy\"")
            || contents.contains("process::stream(`effigy`")
            || contents.contains("process::tee(\"effigy\"")
            || contents.contains("process::tee(`effigy`")
            || contents.contains("effigy::run(")
            || contents.contains("effigy::run_json(")
        {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use typed host helpers instead of recursive Effigy escape hatches: {}",
        violations.join(", ")
    );
}

#[test]
fn first_party_rhai_process_calls_are_allowlisted() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if !contents.contains("process::run(")
            && !contents.contains("process::stream(")
            && !contents.contains("process::tee(")
        {
            continue;
        }
        let relative = script
            .strip_prefix(&repo_root)
            .unwrap_or(&script)
            .display()
            .to_string();
        if !allowed_first_party_process_script(&relative, &contents) {
            violations.push(relative);
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai process helper usage must be explicitly allowlisted or replaced with typed host helpers: {}",
        violations.join(", ")
    );
}

#[test]
fn first_party_rhai_scripts_use_exec_run_for_container_commands() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if contents.contains("container::exec(") {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use exec::run(..., #{{ run_in: \"container\", ... }}) for container commands: {}",
        violations.join(", ")
    );
}

#[test]
fn linux_workspace_artifact_build_streams_container_output() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let script = repo_root.join("scripts/build-linux-workspace-artifact.rhai");
    let contents = fs::read_to_string(script).expect("read script");

    assert!(
        contents
            .contains("container::workspace_artifact_build([\"sh\", \"-lc\", build_script])"),
        "workspace artifact build must stream in-container cargo output through the typed streaming helper"
    );
    for capturing in ["container::shell(", "container::exec("] {
        assert!(
            !contents.contains(capturing),
            "workspace artifact build must not capture in-container cargo output through `{capturing}`"
        );
    }
}

#[test]
fn first_party_rhai_scripts_do_not_use_legacy_module_dot_calls() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let scripts = collect_rhai_scripts(&repo_root);
    let mut violations = Vec::new();
    let module_call = regex::Regex::new(
        r"\b(?:time|path|fs|process|http|json|toml|str|regex|random|search|config|task|container|scan|docs|deploy|system|demo|changelog|cache|gateway|bundle|service|catalog|doctor|contracts|unlock|test|effigy)\.[A-Za-z_][A-Za-z0-9_]*\s*\(",
    )
    .expect("module regex");
    for script in scripts {
        let contents = fs::read_to_string(&script).expect("read script");
        if module_call.is_match(&strip_rhai_string_literals(&contents)) {
            violations.push(
                script
                    .strip_prefix(&repo_root)
                    .unwrap_or(&script)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "first-party Rhai scripts must use `module::func(...)` syntax, not `module.func(...)`: {}",
        violations.join(", ")
    );
}

#[test]
fn execute_rhai_script_rejects_legacy_module_dot_calls() {
    let root = temp_root("legacy-dot-calls");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };

    let error = execute_rhai_script(
        &context,
        r#"process.run("sh", ["-lc", "printf nope"]);"#,
        &[],
        &callbacks(),
    )
    .expect_err("legacy module dot syntax should fail");

    assert!(
        error.to_string().contains("Variable not found: process"),
        "got: {error}"
    );
}

#[test]
fn first_party_script_inventory_excludes_generated_fixture_directories() {
    for indexed in [false, true] {
        let root = temp_root("script-policy-inventory");
        let source = root.join("scripts/source.rhai");
        for relative in [
            "scripts/source.rhai",
            "nested/target/generated.rhai",
            "nested/.effigy/local.rhai",
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("script parent")).expect("script directory");
            fs::write(&path, "container::exec(\"fixture\", [\"true\"]);").expect("script contents");
        }
        if indexed {
            for args in [vec!["init", "--quiet"], vec!["add", "scripts/source.rhai"]] {
                let output = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&root)
                    .args(args)
                    .output()
                    .expect("private fixture Git command");
                assert!(output.status.success(), "{output:?}");
            }
            fs::write(root.join("scripts/untracked.rhai"), "fixture input")
                .expect("untracked script");
        }
        assert_eq!(
            collect_rhai_scripts(&root),
            vec![source],
            "indexed={indexed}"
        );
        fs::remove_dir_all(root).expect("remove private script inventory fixture");
    }
}

#[cfg(unix)]
#[test]
fn workspace_artifact_producer_selects_the_target_and_isolates_receipts() {
    use effigy_test_support::process::FixtureCommandExt;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let root = temp_root("workspace-artifact-targets");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    // These are owned fixture programs, never the machine's Cargo or a builder.
    for (name, body) in [
        (
            "cargo",
            r#"#!/bin/sh
set -eu
[ "$#" = 6 ] && [ "$1" = build ] && [ "$2" = --release ] && [ "$3" = --bin ] && [ "$4" = effigy ] && [ "$5" = --target ]
[ "${FIXTURE_BUILD_FAIL:-0}" = 0 ] || exit 17
mkdir -p "$CARGO_TARGET_DIR/$6/release"
printf '%s\n' "$6" > "$CARGO_TARGET_DIR/$6/release/effigy"
"#,
        ),
        (
            "sha256sum",
            r#"#!/usr/bin/env python3
import hashlib, pathlib, sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
"#,
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let script = format!(
        "{}\nfn fixture_script(target, output) {{ fs::write_file(output, workspace_artifact_build_script(target, \"v0.15.0-test\")); }}",
        include_str!("../../../../scripts/build-linux-workspace-artifact.rhai")
    );
    let context = script_context(&root);
    let mut retained = Vec::new();
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
    ] {
        let output = root.join("build.sh");
        crate::call_rhai_function(
            &context,
            &script,
            "fixture_script",
            (triple.to_owned(), output.display().to_string()),
            &callbacks(),
        )
        .unwrap();
        // Only this run's root substitutes the container's mounted checkout.
        let body = fs::read_to_string(&output)
            .unwrap()
            .replace("/workspace", &root.display().to_string());
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let result = Command::new("/bin/sh")
            .args(["-c", &body])
            .env("PATH", &path)
            .output_owned()
            .unwrap();
        assert!(
            result.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let artifact = root
            .join(".effigy/linux-release/artifacts")
            .join(format!("effigy-{triple}"));
        assert_eq!(
            fs::read_to_string(&artifact).unwrap(),
            format!("{triple}\n")
        );
        let receipt = artifact.with_extension("receipt");
        let bytes = fs::read(&receipt).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains(&format!("release_triple={triple}\n")));
        retained.push((artifact.clone(), fs::read(&artifact).unwrap()));
        retained.push((receipt, bytes));
        // A failed target build cannot attest output, or overwrite prior output.
        let failed = Command::new("/bin/sh")
            .args(["-c", &body])
            .env("PATH", &path)
            .env("FIXTURE_BUILD_FAIL", "1")
            .output_owned()
            .unwrap();
        assert!(!failed.status.success());
        for (path, bytes) in &retained {
            assert_eq!(&fs::read(path).unwrap(), bytes);
        }
    }
    let rejected = root.join("unsupported.sh");
    crate::call_rhai_function(
        &context,
        &script,
        "fixture_script",
        (
            "x86_64-apple-darwin".to_owned(),
            rejected.display().to_string(),
        ),
        &callbacks(),
    )
    .expect_err("an unknown target cannot receive an attestation");
    assert!(!rejected.exists());
    assert!(!root
        .join(".effigy/linux-release/artifacts/rehearsal.txt")
        .exists());
}
