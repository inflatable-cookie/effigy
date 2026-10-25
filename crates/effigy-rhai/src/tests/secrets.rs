use super::*;
use crate::rhai_secrets::{
    active_test_rhai_secret_passphrase, install_test_rhai_secret_passphrase,
    read_rhai_secret_passphrase, rhai_script_consumes_secrets,
};

/// The two passphrase keys the Rhai reader consults before the local unlock
/// state. Both are process-global environment slots, shared with every other
/// fixture in a shared libtest process.
const TEST_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_TEST_SECRETS_PASSPHRASE";
const INTERNAL_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_INTERNAL_SECRET_PASSPHRASE";

#[test]
fn rhai_script_consumes_secrets_detects_the_secrets_module() {
    assert!(rhai_script_consumes_secrets(
        r#"let token = secrets::get("api_token");"#
    ));
    assert!(rhai_script_consumes_secrets(
        r#"if secrets::has("api_token") { 1 }"#
    ));
    assert!(rhai_script_consumes_secrets(
        r#"log(`token=${secrets::get("api_token")}`);"#
    ));
    assert!(!rhai_script_consumes_secrets(r#"log("container status");"#));
    assert!(!rhai_script_consumes_secrets(
        r#"// secrets::get("api_token")
log("ok");"#
    ));
    assert!(!rhai_script_consumes_secrets(
        r#"/* secrets::has("api_token") */ log("ok");"#
    ));
    assert!(!rhai_script_consumes_secrets(
        r#"log("do not call secrets::get here");"#
    ));
    assert!(!rhai_script_consumes_secrets(r#"mysecrets::status();"#));
    assert!(!rhai_script_consumes_secrets(r#"_secrets::status();"#));
}

#[test]
fn execute_rhai_script_exposes_declared_rhai_secrets() {
    let root = temp_root("rhai-secret-present");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let marker = root.join("secret.out");
    let script = format!(
        r#"
            if !secrets::has("api_token") {{ throw("missing"); }}
            let token = secrets::get("api_token");
            fs::write_file("{}", token);
        "#,
        marker.display()
    );

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "tok_secret");
}

#[test]
fn execute_rhai_script_exposes_dedicated_secrets_module() {
    let root = temp_root("rhai-secrets-module-present");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let marker = root.join("secret.out");
    let script = format!(
        r#"
            if !secrets::has("api_token") {{ throw("missing"); }}
            let token = secrets::get("api_token");
            fs::write_file("{}", token);
        "#,
        marker.display()
    );

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "tok_secret");
}

#[test]
fn execute_rhai_script_can_store_declared_rhai_secret() {
    let root = temp_root("rhai-secret-set");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["tasks", "rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    execute_rhai_script(
        &script_context(&root),
        r#"secrets::set("api_token", "generated_secret");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    let envelope =
        effigy_secrets::storage::read_envelope(&root.join(".effigy/secrets/local.vault"))
            .expect("read vault");
    let payload = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt vault");
    assert_eq!(
        payload
            .records
            .get("api_token")
            .expect("stored secret")
            .value
            .expose(),
        "generated_secret"
    );
}

/// The hosting binary's adopted session access is what a script resolves
/// with: it reaches the reader as memory, never as an environment value
/// (contract 032, 052 K12; effigy#338).
#[test]
fn execute_rhai_script_resolves_from_the_host_session_access() {
    let root = temp_root("rhai-secret-set-session-access");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::unset_many(&[TEST_SECRET_PASSPHRASE_ENV]);
    let _access = SessionAccessScope::install("vault-passphrase");

    execute_rhai_script(
        &script_context(&root),
        r#"secrets::set("api_token", "generated_secret");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    let envelope =
        effigy_secrets::storage::read_envelope(&root.join(".effigy/secrets/local.vault"))
            .expect("read vault");
    let payload = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt vault");
    assert_eq!(
        payload
            .records
            .get("api_token")
            .expect("stored secret")
            .value
            .expose(),
        "generated_secret"
    );
}

/// A passphrase left in the old internal environment variable is not a
/// source: no session access is read from an environment (effigy#338).
#[test]
fn execute_rhai_script_ignores_the_internal_passphrase_environment() {
    let root = temp_root("rhai-secret-set-internal-passphrase");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    let error = execute_rhai_script(
        &script_context(&root),
        r#"secrets::set("api_token", "generated_secret");"#,
        &[],
        &callbacks(),
    )
    .expect_err("an environment value is not session access");
    assert!(
        error.to_string().contains("interactive TTY")
            || error.to_string().contains("unlocked vault passphrase"),
        "{error}"
    );
}

/// This file's own session access provider: a process-wide value a test can
/// install and clear, standing in for the hosting binary's adopted grant.
fn test_session_access() -> (Option<effigy_secrets::SecretValue>, bool) {
    (
        SESSION_ACCESS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .map(effigy_secrets::SecretValue::new),
        false,
    )
}

static SESSION_ACCESS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

struct SessionAccessScope;

impl SessionAccessScope {
    fn install(passphrase: &str) -> Self {
        crate::install_session_access_provider(test_session_access);
        *SESSION_ACCESS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(passphrase.to_owned());
        Self
    }
}

impl Drop for SessionAccessScope {
    fn drop(&mut self) {
        *SESSION_ACCESS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

#[test]
fn execute_rhai_script_can_store_declared_rhai_secrets_in_batch() {
    let root = temp_root("rhai-secret-set-many");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["tasks", "rhai"]

[secrets.keys.oauth_key]
required = false
targets = ["rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    execute_rhai_script(
        &script_context(&root),
        r#"
            secrets::set_many(#{
                api_token: "generated_secret",
                oauth_key: "oauth_secret",
            });
            if secrets::get("api_token") != "generated_secret" { throw("api token"); }
            if secrets::get("oauth_key") != "oauth_secret" { throw("oauth key"); }
        "#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    let envelope =
        effigy_secrets::storage::read_envelope(&root.join(".effigy/secrets/local.vault"))
            .expect("read vault");
    let payload = envelope
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt vault");
    assert_eq!(
        payload
            .records
            .get("api_token")
            .expect("stored api token")
            .value
            .expose(),
        "generated_secret"
    );
    assert_eq!(
        payload
            .records
            .get("oauth_key")
            .expect("stored oauth key")
            .value
            .expose(),
        "oauth_secret"
    );
}

#[test]
fn execute_rhai_script_skips_unrelated_required_secrets_when_script_is_secret_free() {
    let root = temp_root("rhai-secret-free-unrelated-required");
    let marker = root.join("ran.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    let script = format!(r#"fs::write_file("{}", "ran");"#, marker.display());

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "ran");
}

#[test]
fn execute_rhai_script_ignores_comment_mentions_of_secrets_module() {
    let root = temp_root("rhai-secret-comment-mention");
    let marker = root.join("ran.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    let script = format!(
        r#"
            // later: secrets::get("api_token")
            /* secrets::has("api_token") */
            fs::write_file("{}", "ran");
        "#,
        marker.display()
    );

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "ran");
}

#[test]
fn execute_rhai_script_ignores_longer_identifiers_that_contain_secrets() {
    let root = temp_root("rhai-secret-mysecrets-module");
    let marker = root.join("ran.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    fs::write(root.join("mysecrets.rhai"), "fn status() { () }\n").expect("write mysecrets module");
    let script = format!(
        r#"
            import "mysecrets.rhai" as mysecrets;
            mysecrets::status();
            fs::write_file("{}", "ran");
        "#,
        marker.display()
    );

    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "ran");
}

#[test]
fn execute_rhai_script_blocks_missing_required_rhai_secret_before_side_effects() {
    let root = temp_root("rhai-secret-missing-required");
    let marker = root.join("should-not-run.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let script = format!(
        r#"
            fs::write_file("{}", "ran");
            secrets::get("api_token");
        "#,
        marker.display()
    );

    let error = execute_rhai_script(&script_context(&root), &script, &[], &callbacks())
        .expect_err("script should fail");

    assert!(error
        .to_string()
        .contains("required Rhai secret(s) missing from the vault"));
    assert!(
        !marker.exists(),
        "script should not run after preflight blocker"
    );
}

#[test]
fn execute_rhai_script_rejects_undeclared_and_wrong_target_secret_reads() {
    let root = temp_root("rhai-secret-wrong-target");
    write_rhai_secret_manifest(&root, r#"targets = ["tasks"]"#);

    let wrong_target = execute_rhai_script(
        &script_context(&root),
        r#"secrets::get("api_token");"#,
        &[],
        &callbacks(),
    )
    .expect_err("wrong target should fail");
    assert!(wrong_target
        .to_string()
        .contains("not declared for the `rhai` target"));

    let undeclared = execute_rhai_script(
        &script_context(&root),
        r#"secrets::has("missing");"#,
        &[],
        &callbacks(),
    )
    .expect_err("undeclared should fail");
    assert!(undeclared
        .to_string()
        .contains("is not declared under `[secrets.keys]`"));
}

#[test]
fn execute_rhai_script_redacts_secret_values_from_errors() {
    let root = temp_root("rhai-secret-error-redaction");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    let error = execute_rhai_script(
        &script_context(&root),
        r#"throw(secrets::get("api_token"));"#,
        &[],
        &callbacks(),
    )
    .expect_err("script should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("[REDACTED]"), "got: {rendered}");
    assert!(
        !rendered.contains("tok_secret"),
        "secret leaked: {rendered}"
    );
}

#[test]
fn execute_rhai_script_can_use_deploy_target_secret_when_allowed() {
    let root = temp_root("rhai-secret-deploy-target");
    write_rhai_secret_manifest(&root, r#"targets = ["deploy"]"#);
    write_test_vault(&root, "vault-passphrase", &[("api_token", "deploy_secret")]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let marker = root.join("deploy-secret.out");
    let script = format!(
        r#"
            if !secrets::has("api_token") {{ throw("missing"); }}
            fs::write_file("{}", secrets::get("api_token"));
        "#,
        marker.display()
    );

    execute_rhai_script_with_runtime_context_and_secret_targets(
        &script_context(&root),
        None,
        &script,
        &[],
        &callbacks(),
        &[RhaiSecretTarget::Deploy],
    )
    .expect("execute");

    assert_eq!(fs::read_to_string(marker).expect("marker"), "deploy_secret");
}

/// A fixture whose passphrase lives in the process environment does not own
/// it (papercut 8de0a24a).
///
/// `ScopedTestEnv` serialises *writers*, but the reader is production code and
/// is not covered by that guard: any writer outside this crate's guard — the
/// pre-#215 fixtures, or the unsynchronised `setenv` a shared macOS process
/// performs — can take the value away between the fixture's write and its
/// read. The shared-process job then reported `Rhai secrets require an
/// unlocked vault passphrase and secret input requires an interactive TTY`
/// even though the fixture had just set the key.
///
/// A neighbour scope's restore is the whole interleaving, so this control is
/// deterministic: the value is one process-global slot, not the fixture's.
#[test]
fn rhai_secret_passphrase_in_the_process_environment_belongs_to_the_whole_process() {
    let root = temp_root("rhai-secret-env-boundary");
    let vault_path = root.join(".effigy/secrets/local.vault");
    {
        let _fixture_scope = ScopedTestEnv::set_many(&[(
            TEST_SECRET_PASSPHRASE_ENV,
            "fixture-passphrase".to_owned(),
        )]);
        assert_eq!(
            read_rhai_secret_passphrase(&vault_path, true)
                .expect("read")
                .map(|value| value.expose().to_owned())
                .as_deref(),
            Some("fixture-passphrase")
        );
    }
    {
        // The neighbour's scope owns the same slot and restores the absence
        // this process started with when it ends.
        let _neighbour_scope = ScopedTestEnv::unset_many(&[TEST_SECRET_PASSPHRASE_ENV]);
        assert!(
            read_rhai_secret_passphrase(&vault_path, true)
                .expect("read")
                .is_none(),
            "the fixture's passphrase is process state, not the fixture's own"
        );
    }
}

/// The linked worktree fixture: one version-controlled manifest in both
/// checkouts, and a vault only the primary checkout has.
fn linked_worktree_secret_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let primary = root.join("primary");
    let worktree = root.join("worktrees/feature");
    let worktree_git_dir = primary.join(".git/worktrees/feature");
    fs::create_dir_all(&worktree_git_dir).expect("worktree git dir");
    fs::create_dir_all(&worktree).expect("worktree root");
    fs::write(worktree_git_dir.join("commondir"), "../..\n").expect("commondir");
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", worktree_git_dir.display()),
    )
    .expect("gitdir pointer");

    let manifest = r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["tasks", "rhai"]

[secrets.keys.oauth_key]
required = false
targets = ["tasks", "rhai"]
"#;
    // The manifest is version controlled, so both checkouts have it. The vault
    // is not, so only the primary checkout has one.
    fs::write(primary.join("effigy.toml"), manifest).expect("primary manifest");
    fs::write(worktree.join("effigy.toml"), manifest).expect("worktree manifest");
    write_test_vault(
        &primary,
        "vault-passphrase",
        &[("api_token", "primary_token"), ("oauth_key", "primary_key")],
    );
    (primary, worktree)
}

fn assert_shared_vault_mutation(primary: &Path, worktree: &Path) {
    assert!(
        !worktree.join(".effigy/secrets/local.vault").exists(),
        "the worktree must not fork its own vault"
    );
    let payload =
        effigy_secrets::storage::read_envelope(&primary.join(".effigy/secrets/local.vault"))
            .expect("parse vault")
            .decrypt_with_passphrase("vault-passphrase")
            .expect("decrypt vault");
    assert_eq!(
        payload
            .records
            .get("api_token")
            .expect("written secret")
            .value
            .expose(),
        "worktree_token"
    );
    assert_eq!(
        payload
            .records
            .get("oauth_key")
            .expect("primary-only secret survives")
            .value
            .expose(),
        "primary_key"
    );
}

/// A linked worktree must mutate the primary checkout's vault, not fork a
/// partial local one. Resolving reads and writes differently would let
/// `secrets::set` create a worktree vault holding only the written record,
/// which then shadows every primary-only record on the next read.
#[test]
fn rhai_secret_set_from_a_linked_worktree_mutates_the_shared_vault() {
    let root = temp_root("rhai-secret-set-worktree");
    let (primary, worktree) = linked_worktree_secret_fixture(&root);
    let passphrase_scope = install_test_rhai_secret_passphrase("vault-passphrase");

    execute_rhai_script(
        &script_context(&worktree),
        r#"secrets::set("api_token", "worktree_token");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    assert_shared_vault_mutation(&primary, &worktree);
    drop(passphrase_scope);
    assert!(
        active_test_rhai_secret_passphrase().is_none(),
        "the injected passphrase is cleaned up after success"
    );
}

/// The shared-process proof for papercut 8de0a24a: the linked-worktree
/// mutation mutates the shared vault while every passphrase slot in the
/// process environment is absent and other threads keep observing it, because
/// the passphrase reaches the reader through this thread's memory instead of
/// the global environment (contract 052 K12).
#[test]
fn rhai_secret_set_from_a_linked_worktree_is_isolated_from_shared_process_environment_activity() {
    let root = temp_root("rhai-secret-set-worktree-shared-process");
    let (primary, worktree) = linked_worktree_secret_fixture(&root);
    // Neighbouring fixtures own both passphrase slots for the whole mutation.
    // This is the state the shared-process job reported: the old fixture had
    // lost its passphrase and the reader refused the vault.
    let _conflicting_scopes =
        ScopedTestEnv::unset_many(&[TEST_SECRET_PASSPHRASE_ENV, INTERNAL_SECRET_PASSPHRASE_ENV]);
    let (observing_tx, observing_rx) = std::sync::mpsc::channel::<()>();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let observer = std::thread::spawn(move || {
        let mut reads = 0usize;
        let mut saw_passphrase = false;
        observing_tx.send(()).expect("observing");
        loop {
            reads += 1;
            saw_passphrase |= std::env::var_os(TEST_SECRET_PASSPHRASE_ENV).is_some();
            saw_passphrase |= std::env::var_os(INTERNAL_SECRET_PASSPHRASE_ENV).is_some();
            match stop_rx.recv_timeout(std::time::Duration::from_millis(1)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                _ => break,
            }
        }
        (reads, saw_passphrase)
    });
    observing_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("observer started");

    let passphrase_scope = install_test_rhai_secret_passphrase("vault-passphrase");
    execute_rhai_script(
        &script_context(&worktree),
        r#"secrets::set("api_token", "worktree_token");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");
    stop_tx.send(()).expect("stop observer");
    let (reads, saw_passphrase) = observer.join().expect("observer");

    assert!(
        reads >= 2,
        "the observer read the shared environment while the mutation ran: {reads}"
    );
    assert!(
        !saw_passphrase,
        "the mutation must not publish its passphrase to the shared environment"
    );
    assert!(
        std::env::var_os(TEST_SECRET_PASSPHRASE_ENV).is_none()
            && std::env::var_os(INTERNAL_SECRET_PASSPHRASE_ENV).is_none(),
        "the fixture leaves the neighbour's passphrase slots untouched"
    );
    assert_shared_vault_mutation(&primary, &worktree);
    assert!(
        active_test_rhai_secret_passphrase().is_some(),
        "the passphrase stays in the mutating thread"
    );
    drop(passphrase_scope);
    assert!(
        active_test_rhai_secret_passphrase().is_none(),
        "the injected passphrase is cleaned up after success"
    );
}

/// A failed mutation must still clear the injected passphrase, keep it off the
/// shared environment and out of the error it returns.
#[test]
fn rhai_secret_passphrase_seam_clears_after_a_failed_mutation_without_leaking() {
    let root = temp_root("rhai-secret-seam-error");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[("api_token", "primary_token")]);
    let _conflicting_scopes =
        ScopedTestEnv::unset_many(&[TEST_SECRET_PASSPHRASE_ENV, INTERNAL_SECRET_PASSPHRASE_ENV]);

    {
        let passphrase_scope = install_test_rhai_secret_passphrase("wrong-passphrase");
        let sibling_sees_seam =
            std::thread::spawn(|| active_test_rhai_secret_passphrase().is_some())
                .join()
                .expect("sibling thread");
        assert!(!sibling_sees_seam, "the seam is thread-local");
        assert!(active_test_rhai_secret_passphrase().is_some());

        let error = execute_rhai_script(
            &script_context(&root),
            r#"secrets::set("api_token", "should_not_persist");"#,
            &[],
            &callbacks(),
        )
        .expect_err("the wrong passphrase must not open the vault");
        let rendered = error.to_string();
        assert!(
            rendered.contains("failed to decrypt vault payload"),
            "got: {rendered}"
        );
        assert!(
            !rendered.contains("wrong-passphrase"),
            "the passphrase leaked: {rendered}"
        );
        assert!(
            !rendered.contains("should_not_persist"),
            "the secret leaked: {rendered}"
        );

        drop(passphrase_scope);
        assert!(
            active_test_rhai_secret_passphrase().is_none(),
            "the injected passphrase is cleaned up after an error"
        );
    }

    let payload = effigy_secrets::storage::read_envelope(&root.join(".effigy/secrets/local.vault"))
        .expect("parse vault")
        .decrypt_with_passphrase("vault-passphrase")
        .expect("decrypt vault");
    assert_eq!(
        payload
            .records
            .get("api_token")
            .expect("retained secret")
            .value
            .expose(),
        "primary_token",
        "a failed mutation must leave the vault unchanged"
    );
}

/// The ordinary locked-vault refusal stays in place: with no injected
/// passphrase, no passphrase environment and no local unlock state, the vault
/// exposes nothing. A script-only read takes the optional path, which is the
/// one that never prompts; the required-secret refusal is covered with an
/// unlocked vault by the missing-required control above.
#[test]
fn rhai_secrets_leave_a_locked_vault_unexposed() {
    let root = temp_root("rhai-secret-locked-vault");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["rhai"]
"#,
    )
    .expect("write manifest");
    write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
    let _locked_scopes =
        ScopedTestEnv::unset_many(&[TEST_SECRET_PASSPHRASE_ENV, INTERNAL_SECRET_PASSPHRASE_ENV]);

    let marker = root.join("locked.out");
    let script = format!(
        r#"if secrets::has("api_token") {{ fs::write_file("{}", "unlocked"); }}"#,
        marker.display()
    );
    execute_rhai_script(&script_context(&root), &script, &[], &callbacks()).expect("execute");

    assert!(
        !marker.exists(),
        "a locked vault must not expose its records"
    );
    assert!(
        read_rhai_secret_passphrase(&root.join(".effigy/secrets/local.vault"), true)
            .expect("read")
            .is_none(),
        "no passphrase is available to a locked vault"
    );
    assert!(
        active_test_rhai_secret_passphrase().is_none(),
        "the seam is not a globally permissive passphrase"
    );
}
