use effigy_manifest::{
    ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig, TaskManifest,
};

#[test]
fn secrets_config_accepts_effigy_vault_backend_and_keys() {
    let secrets = toml::from_str::<ManifestSecretsConfig>(
        r#"
backend = "effigy-vault"

[vault]
path = ".effigy/secrets/local.vault"

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
        secrets
            .vault
            .as_ref()
            .and_then(|vault| vault.path.as_deref()),
        Some(".effigy/secrets/local.vault")
    );
    assert!(
        secrets
            .keys
            .get("database_url")
            .expect("database key")
            .required
    );
}

#[test]
fn retirement_vault_policy_keys_reject_every_value() {
    for key in ["identity", "unlock", "external"] {
        for value in ["\"passphrase\"", "false", "{}"] {
            let source = format!("[secrets.vault]\npath = \"local.vault\"\n{key} = {value}\n");
            let error = toml::from_str::<TaskManifest>(&source)
                .expect_err("retired key must fail")
                .to_string();
            assert!(error.contains("retired"), "{error}");
            assert!(error.contains("line 3"), "{error}");
            assert!(
                error.contains("delete this line; it only restated the passphrase default"),
                "{error}"
            );
        }
    }
}

#[test]
fn retirement_vault_policy_keys_name_the_declaring_file_and_line() {
    let root = tempfile::tempdir().expect("temporary manifest root");
    let manifest = root.path().join("effigy.toml");
    let fragment = root.path().join("secrets.toml");
    for key in ["identity", "unlock", "external"] {
        let source = format!("# vault settings\n[secrets.vault]\n{key} = \"passphrase\"\n");
        std::fs::write(&manifest, &source).expect("write root");
        let error = effigy_manifest::load_task_manifest(&manifest)
            .expect_err("retired root")
            .to_string();
        assert!(error.contains(&manifest.display().to_string()), "{error}");
        assert!(error.contains("line 3"), "{error}");
        assert!(
            error.contains("delete this line; it only restated the passphrase default"),
            "{error}"
        );

        std::fs::write(&fragment, source).expect("write fragment");
        std::fs::write(&manifest, "[manifest]\ninclude = [\"secrets.toml\"]\n")
            .expect("write root include");
        let error = effigy_manifest::load_task_manifest(&manifest)
            .expect_err("retired include")
            .to_string();
        assert!(error.contains(&fragment.display().to_string()), "{error}");
        assert!(error.contains("line 3"), "{error}");
        assert!(error.contains("retired"), "{error}");
    }
}

#[test]
fn retirement_vault_policy_keys_reject_dotted_and_inline_declarations() {
    let root = tempfile::tempdir().expect("temporary manifest root");
    let manifest = root.path().join("effigy.toml");
    for source in [
        "secrets.vault.identity = \"ssh-agent\"\n",
        "[secrets]\nvault = { unlock = \"passphrase\" }\n",
        "[secrets.\"vault\"]\n\"external\" = false\n",
    ] {
        std::fs::write(&manifest, source).expect("write manifest");
        let error = effigy_manifest::load_task_manifest(&manifest)
            .expect_err("retired spelling")
            .to_string();
        assert!(error.contains("retired"), "{error}");
        assert!(error.contains("line "), "{error}");
    }
}

#[test]
fn retirement_vault_policy_keys_name_bundle_exports() {
    let root = tempfile::tempdir().expect("temporary bundle root");
    let manifest = root.path().join("effigy.toml");
    let bundle = root.path().join("bundle");
    std::fs::create_dir(&bundle).expect("create bundle directory");
    std::fs::write(
        &manifest,
        "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
    )
    .expect("write root");
    std::fs::write(
        bundle.join("bundle.toml"),
        "[bundle]\nname = \"vault-retirement\"\ndefaults = \"export.toml\"\n",
    )
    .expect("write descriptor");
    let export = bundle.join("export.toml");
    for key in ["identity", "unlock", "external"] {
        std::fs::write(
            &export,
            format!("[secrets.vault]\n{key} = \"passphrase\"\n"),
        )
        .expect("write export");
        let error = effigy_manifest::load_task_manifest(&manifest)
            .expect_err("retired bundle key")
            .to_string();
        assert!(error.contains(&export.display().to_string()), "{error}");
        assert!(error.contains("line 2"), "{error}");
        assert!(error.contains("retired"), "{error}");
    }
}
