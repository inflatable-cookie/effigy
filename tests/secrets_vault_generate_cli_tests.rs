//! `[secrets.vault].generate` may create required Rhai secrets that are not in
//! the vault yet, while every other Rhai secret read keeps refusing missing
//! required keys.

mod support;

use std::fs;
use std::process::{Command, Output, Stdio};

fn write_fixture(root: &std::path::Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
generate = { rhai = "generate.rhai", run_in = "host" }

[secrets.keys.api_token]
required = true
targets = ["tasks", "rhai"]

[secrets.keys.other_token]
required = true
targets = ["tasks", "rhai"]

[tasks.read_other]
run = [{ rhai = "read_other.rhai" }]
run_in = "host"
"#,
    )
    .expect("write manifest");
    // The hook checks and fills only api_token; other_token stays missing.
    fs::write(
        root.join("generate.rhai"),
        r#"
            if !secrets::has("api_token") {
                secrets::set_many(#{ api_token: "generated_token" });
            }
        "#,
    )
    .expect("write generate script");
    fs::write(
        root.join("read_other.rhai"),
        r#"secrets::get("other_token");"#,
    )
    .expect("write read script");
}

fn effigy(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(root)
        .env("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase")
        .env_remove("EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE")
        .stdin(Stdio::null())
        .output()
        .expect("run effigy")
}

#[test]
fn secrets_init_generate_hook_creates_missing_required_rhai_secret() {
    let root = support::temp_workspace("effigy-secrets-generate", "required");
    write_fixture(&root);

    let init = effigy(&root, &["secrets", "init"]);
    let stderr = String::from_utf8_lossy(&init.stderr);
    assert!(init.status.success(), "secrets init failed: {stderr}");
    assert!(
        !stderr.contains("required Rhai secret(s) missing"),
        "{stderr}"
    );

    let list = effigy(&root, &["--json", "secrets", "list"]);
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let doctor = effigy(&root, &["secrets", "doctor"]);
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(
        !report.contains("`api_token` is missing"),
        "generated secret should be stored: {report}"
    );
    assert!(
        report.contains("`other_token` is missing"),
        "the hook did not generate other_token: {report}"
    );
}

#[test]
fn rhai_reads_outside_the_generate_hook_still_refuse_missing_required_secrets() {
    let root = support::temp_workspace("effigy-secrets-generate", "scoped");
    write_fixture(&root);
    let init = effigy(&root, &["secrets", "init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let read = effigy(&root, &["read_other"]);
    let stderr = String::from_utf8_lossy(&read.stderr);
    assert!(
        !read.status.success(),
        "read of a missing required secret must fail"
    );
    assert!(
        stderr.contains("required Rhai secret(s) missing from the vault"),
        "unexpected error: {stderr}"
    );
    assert!(stderr.contains("other_token"), "{stderr}");
    assert!(
        !stderr.contains("generated_token"),
        "secret value leaked: {stderr}"
    );
}
