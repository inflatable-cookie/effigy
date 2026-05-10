use super::*;

#[test]
fn cli_json_mode_task_wraps_task_run_payload() {
    let parsed = run_json_task_success("cli-json-task-success", "build", "printf build-ok");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_json_mode_parse_error_wraps_error_payload() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "cli");
    assert_eq!(parsed["command"]["name"], "parse");
    assert_eq!(parsed["error"]["kind"], "CliParseError");
}

#[test]
fn cli_json_mode_runner_error_wraps_runner_failure() {
    let root = temp_workspace("cli-json-runner-error-envelope");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("missing-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "missing-task");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("missing-task")));
}

#[test]
fn cli_json_mode_lock_conflict_wraps_runner_failure() {
    let root = temp_workspace("cli-json-lock-conflict");
    fs::write(root.join("effigy.toml"), "[tasks.dev]\nrun = \"sleep 2\"\n")
        .expect("write manifest");

    let root_for_thread = root.clone();
    let join = std::thread::spawn(move || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("dev")
            .arg("--repo")
            .arg(&root_for_thread)
            .env("NO_COLOR", "1")
            .output()
            .expect("run holding command")
    });

    let workspace_lock = root.join(".effigy/locks/task-dev.lock");
    wait_for_path_exists(
        &workspace_lock,
        Duration::from_secs(5),
        "task lock for task=dev",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("dev")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run conflicting command");

    let _ = join.join();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "dev");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("lock conflict")));
}

#[test]
fn cli_json_mode_watch_lock_conflict_has_unlock_remediation_hint() {
    let root = temp_workspace("cli-json-watch-lock-conflict");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.build]\nrun = \"sleep 2\"\n",
    )
    .expect("write manifest");

    let root_for_thread = root.clone();
    let join = std::thread::spawn(move || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("watch")
            .arg("--owner")
            .arg("effigy")
            .arg("--once")
            .arg("build")
            .arg("--repo")
            .arg(&root_for_thread)
            .env("NO_COLOR", "1")
            .output()
            .expect("run holding watch command")
    });

    let watch_lock = root.join(".effigy/locks/task-watch-build.lock");
    wait_for_path_exists(
        &watch_lock,
        Duration::from_secs(5),
        "watch lock for owner=effigy target=build",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("watch")
        .arg("--owner")
        .arg("effigy")
        .arg("--once")
        .arg("build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run conflicting watch command");

    let _ = join.join();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "watch");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("task:watch:build")));
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|msg| msg.contains("effigy tasks unlock task:watch:build")));
}

#[test]
fn cli_json_mode_watch_once_suppresses_target_stdout_for_machine_readable_output() {
    let root = temp_workspace("cli-json-watch-once-clean-envelope");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.test]\nrun = \"printf noisy-watch-output\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("watch")
        .arg("--owner")
        .arg("effigy")
        .arg("--once")
        .arg("test")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run json watch once");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "watch");
    assert_eq!(parsed["result"]["schema"], "effigy.watch.v1");
    assert!(
        !stdout.contains("noisy-watch-output"),
        "target stdout leaked into command envelope output"
    );
}

#[test]
fn cli_json_mode_unlock_watch_lock_reports_unlock_payload() {
    let root = temp_workspace("cli-json-unlock-watch-lock");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/task-watch-build.lock"), "{}").expect("write watch lock");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("tasks")
        .arg("unlock")
        .arg("task:watch:build")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run tasks unlock");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "tasks");
    assert_eq!(parsed["result"]["schema"], "effigy.unlock.v1");
    assert_eq!(parsed["result"]["all"], false);
    assert!(parsed["result"]["removed"]
        .as_array()
        .is_some_and(|entries| entries.iter().any(|entry| entry == "task:watch:build")));
}

#[test]
fn cli_json_mode_missing_task_wraps_runner_failure() {
    let (_root, output, parsed) = run_json_cli_command_with_manifest(
        "cli-json-missing-task",
        "[tasks.build]\nrun = \"printf build\"\n",
        &["does-not-exist"],
    );

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "does-not-exist");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
}

#[test]
fn cli_json_mode_deploy_model_wraps_deploy_payload() {
    let root = temp_workspace("cli-json-deploy-model");
    fs::write(
        root.join("effigy.toml"),
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "acme-dev"
workspace_subdir = "acme"
databases = ["acme", "acme_test"]

[bundle.dirs]
front = "acme-front"
admin = "acme-admin"
api = "acme-api"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("acme-front")).expect("mkdir front");
    fs::create_dir_all(root.join("acme-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("acme-api")).expect("mkdir api");
    fs::write(
        root.join("acme-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("acme-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("acme-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p acme-api\"\n[tasks.jobs]\nrun = \"cargo run -p acme-jobs {args}\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("model")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy model");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "deploy.model.v1");
    assert_eq!(parsed["result"]["app"]["bundle"], "underlay");
    assert_eq!(parsed["result"]["backing_services"][0]["name"], "postgres");
}

#[test]
fn cli_json_mode_deploy_export_render_wraps_export_payload() {
    let root = temp_workspace("cli-json-deploy-export-render");
    fs::write(
        root.join("effigy.toml"),
        "[bundle]\nbase = \"underlay\"\nhost = \"acme.test\"\nproject_name = \"acme-dev\"\nworkspace_subdir = \"acme\"\ndatabases = [\"acme\"]\n",
    )
    .expect("write root manifest");
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
    fs::write(
        root.join("app-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("export")
        .arg("render")
        .arg("--path")
        .arg(root.join("infra/render"))
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy export render");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "effigy.deploy.export.v1");
    assert_eq!(parsed["result"]["provider"], "render");
    assert_eq!(parsed["result"]["plan"], true);
}

#[test]
fn cli_json_mode_deploy_export_railway_wraps_export_payload() {
    let root = temp_workspace("cli-json-deploy-export-railway");
    fs::write(
        root.join("effigy.toml"),
        "[bundle]\nbase = \"underlay\"\nhost = \"acme.test\"\nproject_name = \"acme-dev\"\nworkspace_subdir = \"acme\"\ndatabases = [\"acme\"]\n",
    )
    .expect("write root manifest");
    fs::create_dir_all(root.join("app-front")).expect("mkdir front");
    fs::create_dir_all(root.join("app-admin")).expect("mkdir admin");
    fs::create_dir_all(root.join("app-api")).expect("mkdir api");
    fs::write(
        root.join("app-front/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: \"200.html\" }) } };\n",
    )
    .expect("write front svelte config");
    fs::write(
        root.join("app-admin/svelte.config.js"),
        "export default { kit: { adapter: adapter({ fallback: 'index.html' }) } };\n",
    )
    .expect("write admin svelte config");
    fs::write(
        root.join("app-front/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write front manifest");
    fs::write(
        root.join("app-admin/effigy.toml"),
        "[tasks.build]\nrun = \"bun x vite build\"\n",
    )
    .expect("write admin manifest");
    fs::write(
        root.join("app-api/effigy.toml"),
        "[tasks.build]\nrun = \"cargo build --release\"\n[tasks.api]\nrun = \"cargo run -p app-api\"\n",
    )
    .expect("write api manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("deploy")
        .arg("export")
        .arg("railway")
        .arg("--path")
        .arg(root.join("infra/railway"))
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run deploy export railway");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "deploy");
    assert_eq!(parsed["command"]["name"], "deploy");
    assert_eq!(parsed["result"]["schema"], "effigy.deploy.export.v1");
    assert_eq!(parsed["result"]["provider"], "railway");
    assert_eq!(parsed["result"]["plan"], true);
}
