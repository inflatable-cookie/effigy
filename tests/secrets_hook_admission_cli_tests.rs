//! A manifest's declared `[secrets.vault].generate` hook is a durable
//! descendant of `secrets = "required"` task startup. The static tree
//! classifier must admit it before the task body, so `secrets init` and the
//! hook's task run inside the admitted tree instead of refusing a late durable
//! effect. An undeclared late durable child still refuses before its effects.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};

const PASS_PHRASE: &str = "vault-passphrase";

/// The hook task body the fixture declares. `init-then-set` covers a missing
/// vault; `set-only` covers a missing key in an existing vault.
const GENERATE_INIT_THEN_SET: &str =
    "[{ task = \"secrets init\" }, { task = \"secrets set api_token\" }]";
const GENERATE_SET_ONLY: &str = "{ task = \"secrets set api_token\" }";

fn write_hook_manifest(root: &Path, generate: &str) {
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
generate = {{ task = "secrets:generate-dev" }}

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf %s \"$API_TOKEN\" > captured.out'"
secrets = "required"

[tasks."secrets:generate-dev"]
run = {generate}
"#
        ),
    )
    .expect("write manifest");
}

fn write_capture_manifest_without_hook(root: &Path) {
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.capture]
run = "sh -lc 'printf %s \"$API_TOKEN\" > captured.out'"
secrets = "required"
"#,
    )
    .expect("write manifest");
}

/// A legacy envelope at the configured path is the vault read/write path the
/// kernel still accepts, so the fixture controls the stored records directly.
fn write_vault(root: &Path, records: &[(&str, &str)]) {
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let envelope = payload
        .encrypt_with_passphrase(PASS_PHRASE)
        .expect("encrypt test vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        vault_path,
        envelope.to_json_pretty().expect("serialize test vault"),
    )
    .expect("write test vault");
}

fn home(root: &Path) -> PathBuf {
    let home = root.join("fixture-home");
    fs::create_dir_all(&home).expect("mkdir fixture home");
    home
}

fn effigy(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(root)
        // Keep the fixture's machine state, vault gates and durable state home
        // inside its own directory: never the operator's.
        .env("HOME", home(root))
        .env("TMPDIR", home(root))
        .env("EFFIGY_TEST_SECRETS_PASSPHRASE", PASS_PHRASE)
        .env("EFFIGY_TEST_SECRETS_VALUE", "tok_generated")
        .env_remove("EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE")
        .stdin(Stdio::null())
        .output()
        .expect("run effigy")
}

fn text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn captured(root: &Path) -> String {
    fs::read_to_string(root.join("captured.out")).expect("task body wrote captured.out")
}

/// A durable admission creates the checkout journal; a foreground run never
/// opens the database (contract 052 §3.1).
fn ran_durable(root: &Path) -> bool {
    root.join(".effigy/state/kernel.db").is_file()
}

#[test]
fn required_task_generates_a_missing_vault_before_its_body() {
    let root = support::temp_workspace("effigy-hook-admission", "missing-vault");
    write_hook_manifest(&root, GENERATE_INIT_THEN_SET);

    let run = effigy(&root, &["capture"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(captured(&root), "tok_generated");
    assert!(
        root.join(".effigy/secrets/local.vault").exists(),
        "the configured hook created the vault"
    );
    assert!(
        ran_durable(&root),
        "the declared hook is a durable descendant: the tree was admitted with the checkout journal"
    );
}

#[test]
fn required_task_generates_a_missing_key_in_an_existing_vault() {
    let root = support::temp_workspace("effigy-hook-admission", "missing-key");
    write_hook_manifest(&root, GENERATE_SET_ONLY);
    write_vault(&root, &[("unrelated", "kept")]);

    let run = effigy(&root, &["capture"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(captured(&root), "tok_generated");
    assert!(
        ran_durable(&root),
        "the hook's effects stay in the admitted tree"
    );
}

#[test]
fn stored_required_key_skips_the_generation_hook() {
    let root = support::temp_workspace("effigy-hook-admission", "existing-key");
    write_hook_manifest(&root, r#""sh -lc 'printf ran > hook-ran.out; exit 3'""#);
    write_vault(&root, &[("api_token", "tok_stored")]);

    let run = effigy(&root, &["capture"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(captured(&root), "tok_stored");
    assert!(
        !root.join("hook-ran.out").exists(),
        "a stored required key must not run the generation hook"
    );
    // The class is static: the manifest's declared hook selects durability
    // before startup, whether or not this run needs generation.
    assert!(ran_durable(&root));
}

#[test]
fn generation_hook_failure_never_spawns_the_task_body() {
    let root = support::temp_workspace("effigy-hook-admission", "hook-error");
    write_hook_manifest(&root, r#""sh -lc 'printf ran > hook-ran.out; exit 9'""#);

    let first = effigy(&root, &["capture"]);
    assert!(!first.status.success(), "a failing hook must fail the run");
    let report = text(&first);
    assert!(
        report.contains("[secrets.vault].generate"),
        "unexpected error: {report}"
    );
    assert!(
        root.join("hook-ran.out").exists(),
        "the declared hook ran before failing"
    );
    assert!(
        !root.join("captured.out").exists(),
        "no task body may run when generation fails: {report}"
    );

    // The failed run settles its gates and journal; the same fixture can try
    // again immediately instead of waiting on a stale holder.
    let second = effigy(&root, &["capture"]);
    assert!(!second.status.success(), "the hook still fails");
    assert!(
        text(&second).contains("[secrets.vault].generate"),
        "second attempt did not reach the hook again: {}",
        text(&second)
    );
}

#[test]
fn plain_task_without_required_secrets_stays_foreground() {
    let root = support::temp_workspace("effigy-hook-admission", "plain-foreground");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.plain]\nrun = \"sh -lc 'printf ok > plain.out'\"\n",
    )
    .expect("write manifest");

    let run = effigy(&root, &["plain"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(
        fs::read_to_string(root.join("plain.out")).expect("plain body"),
        "ok"
    );
    assert!(
        !ran_durable(&root),
        "an ordinary task must not gain a journal from the hook admission rule"
    );
}

#[test]
fn required_secret_task_without_a_declared_hook_stays_foreground() {
    let root = support::temp_workspace("effigy-hook-admission", "no-hook-foreground");
    write_capture_manifest_without_hook(&root);
    write_vault(&root, &[("api_token", "tok_stored")]);

    let run = effigy(&root, &["capture"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(captured(&root), "tok_stored");
    assert!(
        !ran_durable(&root),
        "no declared hook means no durable descendant to admit"
    );
}

#[test]
fn an_undeclared_late_durable_child_runs_and_journals() {
    let root = support::temp_workspace("effigy-hook-admission", "late-durable-child");
    let binary = env!("CARGO_BIN_EXE_effigy");
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[tasks.outer]
run = "sh -lc '{binary} --run-id late-child inner'"

[tasks.inner]
run = "sh -lc 'printf inner > inner.out'"
"#
        ),
    )
    .expect("write manifest");

    // Q-011: a nested run is an ordinary invocation. The parent is a
    // foreground tree, and the child classifies its own durability instead of
    // being refused as a late durable effect.
    let run = effigy(&root, &["outer"]);
    assert!(run.status.success(), "{}", text(&run));
    assert_eq!(
        fs::read_to_string(root.join("inner.out")).expect("nested body"),
        "inner"
    );
    assert!(
        ran_durable(&root),
        "the nested durable run journalled in its own checkout"
    );
}
