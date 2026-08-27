use super::*;

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
identity = "passphrase"
unlock = "passphrase"

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

    let raw = fs::read_to_string(root.join(".effigy/secrets/local.vault")).expect("read vault");
    let envelope = VaultEnvelope::from_json(&raw).expect("parse vault");
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

#[test]
fn execute_rhai_script_accepts_internal_secret_passphrase_env() {
    let root = temp_root("rhai-secret-set-internal-passphrase");
    fs::write(
        root.join("effigy.toml"),
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

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

    execute_rhai_script(
        &script_context(&root),
        r#"secrets::set("api_token", "generated_secret");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    let raw = fs::read_to_string(root.join(".effigy/secrets/local.vault")).expect("read vault");
    let envelope = VaultEnvelope::from_json(&raw).expect("parse vault");
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
identity = "passphrase"
unlock = "passphrase"

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

    let raw = fs::read_to_string(root.join(".effigy/secrets/local.vault")).expect("read vault");
    let envelope = VaultEnvelope::from_json(&raw).expect("parse vault");
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
fn execute_rhai_script_blocks_missing_required_rhai_secret_before_side_effects() {
    let root = temp_root("rhai-secret-missing-required");
    let marker = root.join("should-not-run.out");
    write_rhai_secret_manifest(&root, r#"targets = ["rhai"]"#);
    write_test_vault(&root, "vault-passphrase", &[]);
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);
    let script = format!(r#"fs::write_file("{}", "ran");"#, marker.display());

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

/// A linked worktree must mutate the primary checkout's vault, not fork a
/// partial local one. Resolving reads and writes differently would let
/// `secrets::set` create a worktree vault holding only the written record,
/// which then shadows every primary-only record on the next read.
#[test]
fn rhai_secret_set_from_a_linked_worktree_mutates_the_shared_vault() {
    let root = temp_root("rhai-secret-set-worktree");
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
identity = "passphrase"
unlock = "passphrase"

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
    let _env = ScopedTestEnv::set_many(&[(
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
        "vault-passphrase".to_owned(),
    )]);

    execute_rhai_script(
        &script_context(&worktree),
        r#"secrets::set("api_token", "worktree_token");"#,
        &[],
        &callbacks(),
    )
    .expect("execute");

    assert!(
        !worktree.join(".effigy/secrets/local.vault").exists(),
        "the worktree must not fork its own vault"
    );
    let raw = fs::read_to_string(primary.join(".effigy/secrets/local.vault")).expect("read vault");
    let payload = VaultEnvelope::from_json(&raw)
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
