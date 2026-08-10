use effigy_manifest::{load_task_manifest, ManifestBundleBase, ManifestBundleConfig};

#[test]
fn bundle_config_accepts_path_block() {
    let bundle = toml::from_str::<ManifestBundleConfig>(
        r#"
base = { type = "path", dir = "bundles/acme" }
"#,
    )
    .expect("parse bundle config");

    assert!(matches!(
        bundle.base.as_ref(),
        Some(ManifestBundleBase::Path { dir }) if dir == "bundles/acme"
    ));
}

#[test]
fn bundle_config_accepts_git_and_oci_blocks() {
    let git = toml::from_str::<ManifestBundleConfig>(
        r#"
base = { type = "git", url = "git@github.com:acme/effigy-bundle.git", ref = "main" }
"#,
    )
    .expect("parse git bundle config");
    assert!(matches!(
        git.base.as_ref(),
        Some(ManifestBundleBase::Git { url, r#ref })
            if url == "git@github.com:acme/effigy-bundle.git" && r#ref.as_deref() == Some("main")
    ));

    let oci = toml::from_str::<ManifestBundleConfig>(
        r#"
base = { type = "oci", url = "ghcr.io/acme/effigy-bundle:v1.2.3" }
"#,
    )
    .expect("parse oci bundle config");
    assert!(matches!(
        oci.base.as_ref(),
        Some(ManifestBundleBase::Oci { url }) if url == "ghcr.io/acme/effigy-bundle:v1.2.3"
    ));
}

#[test]
fn bundle_config_rejects_legacy_string_base_with_migration_error() {
    let error = toml::from_str::<ManifestBundleConfig>(
        r#"
base = "workspace-app"
"#,
    )
    .expect_err("legacy string base should be rejected");

    assert!(
        error
            .to_string()
            .contains("string `[bundle].base` value `workspace-app` has been removed"),
        "{error}"
    );
}

#[test]
fn bundle_config_rejects_base_path_with_migration_error() {
    let error = toml::from_str::<ManifestBundleConfig>(
        r#"
base_path = "bundles/acme"
"#,
    )
    .expect_err("base_path should be rejected");

    assert!(
        error
            .to_string()
            .contains("`[bundle].base_path` has been removed"),
        "{error}"
    );
}

#[test]
fn bundle_config_rejects_legacy_name_with_migration_error() {
    let error = toml::from_str::<ManifestBundleConfig>(
        r#"
name = "workspace-app"
"#,
    )
    .expect_err("legacy bundle name should be rejected");

    assert!(
        error
            .to_string()
            .contains("legacy `[bundle].name` has been removed"),
        "{error}"
    );
}

#[test]
fn removed_ambient_catalog_config_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("effigy.toml");
    std::fs::write(
        &manifest_path,
        r#"
[catalog]
alias = "root"

[catalog.discovery]
enabled = false
"#,
    )
    .expect("write manifest");

    let error =
        load_task_manifest(&manifest_path).expect_err("removed discovery config should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("discovery"), "{rendered}");
    assert!(rendered.contains("unknown field"), "{rendered}");
}
