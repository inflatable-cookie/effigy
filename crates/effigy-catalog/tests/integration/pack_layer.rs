//! Integration proof that an installed catalog pack behaves as a real catalog
//! layer: below both overrides, above the compiled baseline, and usable by
//! deterministic compose assembly without changing assembly rules.

use std::collections::HashMap;

use effigy_catalog::assembly::{AssemblyResult, ComposeAssembler, ServiceDeclaration};
use effigy_catalog::fragment::{CatalogResolver, FragmentSource, InstalledPackLayer};

use super::{bundled_resolver, validate_compose_structure};

/// Build a pack-shaped fragment directory holding one `redis` fragment.
fn pack_layer(root: &std::path::Path, image: &str) -> InstalledPackLayer {
    let redis = root.join("redis");
    std::fs::create_dir_all(&redis).unwrap();
    std::fs::write(
        redis.join("service.toml"),
        "[service]\nname = \"redis\"\ndescription = \"pack redis\"\n\n\
         [params.version]\ntype = \"string\"\ndefault = \"7\"\n",
    )
    .unwrap();
    std::fs::write(
        redis.join("compose.fragment.yml"),
        format!("services:\n  {{{{ service_name }}}}:\n    image: {image}:{{{{ version }}}}\n"),
    )
    .unwrap();
    InstalledPackLayer {
        root: root.to_path_buf(),
        pack_id: "effigy-default-catalog".to_owned(),
        pack_version: "1.0.0".to_owned(),
    }
}

#[test]
fn installed_pack_outranks_the_compiled_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let layer = pack_layer(&dir.path().join("pack"), "pack-redis");
    let resolver = CatalogResolver::new(None, None).with_installed_pack(Some(layer));

    let fragment = resolver.resolve("redis").unwrap();
    assert!(
        matches!(fragment.source, FragmentSource::InstalledPack { .. }),
        "{:?}",
        fragment.source
    );
    assert!(fragment.compose_template.contains("pack-redis"));

    // Fragments the pack does not carry still come from the baseline.
    let mariadb = resolver.resolve("mariadb").unwrap();
    assert_eq!(mariadb.source, FragmentSource::Bundled);
}

#[test]
fn project_and_user_overrides_still_outrank_an_installed_pack() {
    let dir = tempfile::tempdir().unwrap();
    let layer = pack_layer(&dir.path().join("pack"), "pack-redis");

    for (label, project, user) in [
        ("user-redis", None, Some(dir.path().join("global"))),
        (
            "local-redis",
            Some(dir.path().join("local")),
            Some(dir.path().join("global")),
        ),
    ] {
        let override_root = project.clone().unwrap_or_else(|| user.clone().unwrap());
        let redis = override_root.join("redis");
        std::fs::create_dir_all(&redis).unwrap();
        std::fs::write(
            redis.join("service.toml"),
            "[service]\nname = \"redis\"\ndescription = \"override redis\"\n\n\
             [params.version]\ntype = \"string\"\ndefault = \"7\"\n",
        )
        .unwrap();
        std::fs::write(
            redis.join("compose.fragment.yml"),
            format!("services:\n  {{{{ service_name }}}}:\n    image: {label}\n"),
        )
        .unwrap();

        let resolver = CatalogResolver::new(project.clone(), user.clone())
            .with_installed_pack(Some(layer.clone()));
        let fragment = resolver.resolve("redis").unwrap();
        assert!(
            fragment.compose_template.contains(label),
            "expected `{label}` to win over the installed pack, got {}",
            fragment.compose_template
        );
    }
}

#[test]
fn list_reports_the_pack_layer_for_pack_owned_fragments() {
    let dir = tempfile::tempdir().unwrap();
    let layer = pack_layer(&dir.path().join("pack"), "pack-redis");
    let resolver = CatalogResolver::new(None, None).with_installed_pack(Some(layer));

    let listed = resolver.list();
    let redis = listed.iter().find(|f| f.name == "redis").unwrap();
    assert!(
        matches!(redis.source, FragmentSource::InstalledPack { .. }),
        "{:?}",
        redis.source
    );
    assert_eq!(
        listed.iter().find(|f| f.name == "php-fpm").unwrap().source,
        FragmentSource::Bundled
    );
    assert_eq!(
        redis.source.to_string(),
        "installed-pack (effigy-default-catalog 1.0.0)"
    );
}

#[test]
fn compose_assembly_through_a_pack_layer_matches_baseline_assembly_shape() {
    let dir = tempfile::tempdir().unwrap();
    let layer = pack_layer(&dir.path().join("pack"), "pack-redis");

    let declarations = vec![ServiceDeclaration {
        name: "cache".to_owned(),
        catalog: "redis".to_owned(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];
    let assemble = |resolver: CatalogResolver| -> AssemblyResult {
        ComposeAssembler::new(resolver)
            .assemble(
                &declarations,
                "demo",
                dir.path().to_str().unwrap(),
                ".effigy-catalog",
                1000,
                1000,
            )
            .unwrap()
    };

    let baseline = assemble(bundled_resolver());
    let through_pack = assemble(CatalogResolver::new(None, None).with_installed_pack(Some(layer)));

    let baseline_doc = validate_compose_structure(&baseline.compose_yaml);
    let pack_doc = validate_compose_structure(&through_pack.compose_yaml);

    // Same assembly rules, same service naming and labels — only the image
    // comes from the pack instead of the baseline fragment.
    assert_eq!(
        baseline_doc["services"]["cache"]["labels"],
        pack_doc["services"]["cache"]["labels"]
    );
    assert!(through_pack.compose_yaml.contains("pack-redis:7"));
    assert!(!baseline.compose_yaml.contains("pack-redis"));
}
