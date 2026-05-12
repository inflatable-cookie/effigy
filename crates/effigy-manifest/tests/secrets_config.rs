use effigy_manifest::{
    ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig,
    ManifestSecretsUnlockPolicy, ManifestSecretsVaultIdentity, TaskManifest,
};

#[test]
fn secrets_config_accepts_effigy_vault_backend_and_keys() {
    let secrets = toml::from_str::<ManifestSecretsConfig>(
        r#"
backend = "effigy-vault"

[vault]
path = ".effigy/secrets/local.vault"
identity = "ssh-agent"
unlock = "key-and-passphrase"

[keys.database_url]
required = true
targets = ["tasks", "containers"]
description = "Application database connection URL"

[keys.render_api_key]
targets = ["deploy", "rhai"]
"#,
    )
    .expect("parse secrets config");

    assert_eq!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault));
    let vault = secrets.vault.as_ref().expect("vault config");
    assert_eq!(vault.path.as_deref(), Some(".effigy/secrets/local.vault"));
    assert_eq!(vault.identity, Some(ManifestSecretsVaultIdentity::SshAgent));
    assert_eq!(
        vault.unlock,
        Some(ManifestSecretsUnlockPolicy::KeyAndPassphrase)
    );

    let database = secrets.keys.get("database_url").expect("database key");
    assert!(database.required);
    assert_eq!(
        database.targets,
        vec![
            ManifestSecretTarget::Tasks,
            ManifestSecretTarget::Containers
        ]
    );
    assert_eq!(
        database.description.as_deref(),
        Some("Application database connection URL")
    );

    let render = secrets.keys.get("render_api_key").expect("render key");
    assert!(!render.required);
    assert_eq!(
        render.targets,
        vec![ManifestSecretTarget::Deploy, ManifestSecretTarget::Rhai]
    );
}

#[test]
fn secrets_config_accepts_external_backend_placeholder() {
    let secrets = toml::from_str::<ManifestSecretsConfig>(
        r#"
backend = "external"

[external]
adapter = "varlock"

[keys.token]
required = true
targets = ["deploy"]
"#,
    )
    .expect("parse external secrets config");

    assert_eq!(secrets.backend, Some(ManifestSecretsBackend::External));
    assert_eq!(
        secrets
            .external
            .as_ref()
            .and_then(|config| config.adapter.as_deref()),
        Some("varlock")
    );
}

#[test]
fn secrets_config_uses_safe_declaration_defaults() {
    let secrets = toml::from_str::<ManifestSecretsConfig>(
        r#"
[keys.optional_token]
"#,
    )
    .expect("parse minimal secrets config");

    assert_eq!(secrets.backend, None);
    assert!(secrets.vault.is_none());
    let key = secrets.keys.get("optional_token").expect("optional token");
    assert!(!key.required);
    assert!(key.targets.is_empty());
    assert!(key.description.is_none());
}

#[test]
fn secrets_config_rejects_unknown_backend() {
    let error = toml::from_str::<ManifestSecretsConfig>(
        r#"
backend = "key-only"
"#,
    )
    .expect_err("unknown backend should fail");

    assert!(
        error.to_string().contains("unknown variant `key-only`"),
        "{error}"
    );
}

#[test]
fn secrets_config_rejects_unknown_target() {
    let error = toml::from_str::<ManifestSecretsConfig>(
        r#"
[keys.api_key]
targets = ["deploy", "browser"]
"#,
    )
    .expect_err("unknown target should fail");

    assert!(
        error.to_string().contains("unknown variant `browser`"),
        "{error}"
    );
}

#[test]
fn task_manifest_accepts_root_secrets_section() {
    let manifest = toml::from_str::<TaskManifest>(
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
unlock = "passphrase"

[secrets.keys.database_url]
required = true
targets = ["tasks"]

[tasks.dev]
run = "echo dev"
"#,
    )
    .expect("parse manifest");

    let secrets = manifest.secrets.expect("manifest secrets");
    assert_eq!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault));
    assert_eq!(
        secrets.vault.as_ref().and_then(|vault| vault.unlock),
        Some(ManifestSecretsUnlockPolicy::Passphrase)
    );
    assert!(
        secrets
            .keys
            .get("database_url")
            .expect("database key")
            .required
    );
}
