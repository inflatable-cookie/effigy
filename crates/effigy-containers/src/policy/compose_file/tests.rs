use super::*;
use crate::policy::load::load_container_policy;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/compose-file-migration"
);

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Checkout {
    fixture: effigy_test_support::Fixture,
    root: PathBuf,
    home: PathBuf,
}

fn checkout(files: &[(&str, &str)]) -> Checkout {
    let fixture = effigy_test_support::Fixture::new("effigy-container-policy-").unwrap();
    let root = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    let root = root.canonicalize().unwrap();
    Checkout {
        fixture,
        root,
        home,
    }
}

fn consumer(name: &str) -> Checkout {
    let checkout = checkout(&[]);
    copy_tree(&Path::new(FIXTURES).join(name), &checkout.root);
    checkout
}

/// Generated Compose, Dockerfiles and configs. Declaration inputs differ by
/// construction: migration rewrites the manifest that produced them.
fn rendered(checkout: &Checkout, name: &str) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
    crate::with_test_effigy_home(&checkout.home, || {
        let policy = load_container_policy(&checkout.root, Some(name)).unwrap();
        let render = policy.render.as_ref().expect("render");
        let files = render
            .closure
            .files()
            .iter()
            .filter(|(name, _)| !name.starts_with("inputs/"))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect();
        let volumes = policy
            .managed_volumes
            .iter()
            .map(|volume| volume.name.clone())
            .collect();
        (files, volumes)
    })
}

fn apply(checkout: &Checkout, name: &str) -> MigrationPlan {
    let plan = crate::with_test_effigy_home(&checkout.home, || {
        plan_migration(&checkout.root, Some(name)).unwrap()
    });
    for fragment in &plan.migration.fragments {
        std::fs::create_dir_all(&fragment.directory).unwrap();
        std::fs::write(
            fragment.directory.join("service.toml"),
            &fragment.service_toml,
        )
        .unwrap();
        std::fs::write(
            fragment.directory.join("compose.fragment.yml"),
            &fragment.compose_fragment,
        )
        .unwrap();
    }
    std::fs::write(&plan.manifest, &plan.manifest_after).unwrap();
    plan
}

fn round_trip(checkout: &Checkout, name: &str) -> MigrationPlan {
    let before = rendered(checkout, name);
    let plan = apply(checkout, name);
    std::fs::remove_file(&plan.migration.compose_file).unwrap();
    let after = rendered(checkout, name);
    assert_eq!(before, after, "migration changed the render of `{name}`");
    plan
}

#[test]
fn linux_release_renders_identically_before_and_after_migration() {
    let checkout = consumer("linux-release");
    let plan = round_trip(&checkout, "linux-release");
    let [fragment] = plan.migration.fragments.as_slice() else {
        panic!("one builder fragment");
    };
    assert_eq!(fragment.name, "linux-release-builder");
    let compose = &fragment.compose_fragment;
    assert!(compose.contains("context: '{{ repo_root }}'"), "{compose}");
    assert!(compose.contains("dockerfile: infra/release/linux/Dockerfile"));
    assert!(
        compose.contains("'{{ repo_root }}:/workspace'"),
        "{compose}"
    );
    assert!(compose.contains("'{{ project_name }}-cargo-registry:/root/.cargo/registry'"));
    let manifest = std::fs::read_to_string(checkout.root.join("config/containers.toml")).unwrap();
    assert!(!manifest.contains("compose_file"));
    assert!(manifest.contains("[containers.\"linux-release\".services.builder]"));
    assert!(manifest.contains("catalog = \"linux-release-builder\""));
    // Comments and unrelated keys survive.
    assert!(manifest.contains("# Stable private builder protocol identity"));
    assert!(manifest.contains("project_name = \"effigy-artifact-builder-v1\""));
    // The repository's own migrated declaration is exactly what migrate writes.
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let committed = repo.join("infra/dev/catalog/linux-release-builder");
    assert_eq!(
        std::fs::read_to_string(committed.join("service.toml")).unwrap(),
        fragment.service_toml
    );
    assert_eq!(
        std::fs::read_to_string(committed.join("compose.fragment.yml")).unwrap(),
        fragment.compose_fragment
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("config/containers.toml")).unwrap(),
        manifest
    );
}

#[test]
fn soundcheck_linux_arm_renders_identically_before_and_after_migration() {
    let checkout = consumer("soundcheck");
    let (_, volumes) = rendered(&checkout, "linux-arm");
    assert_eq!(
        volumes,
        [
            "soundcheck-linux-arm-linux-arm-target-soundcheck",
            "soundcheck-linux-arm-linux-arm-target-library",
            "soundcheck-linux-arm-linux-arm-cargo-registry",
        ]
    );
    let plan = round_trip(&checkout, "linux-arm");
    let compose = &plan.migration.fragments[0].compose_fragment;
    assert!(
        compose.contains("context: '{{ repo_root }}/infra/linux-arm'"),
        "{compose}"
    );
    assert!(compose.contains("'{{ repo_root }}/vendor:/workspace/soundcheck/vendor:ro'"));
    assert!(
        compose.contains("'{{ repo_root }}/../soundcheck-library:/workspace/soundcheck-library'")
    );
    assert!(compose.contains("'{{ repo_root }}/infra/linux-arm/cargo-vendor.config.toml:/usr/local/cargo/config.toml:ro'"));
    let manifest = std::fs::read_to_string(checkout.root.join("effigy.toml")).unwrap();
    assert!(manifest.contains("[containers.\"linux-arm\".services.builder]"));
    assert!(manifest.contains("[systems.linux-arm]"));
}

#[test]
fn translated_render_is_one_shared_volume_per_project_volume() {
    let checkout = checkout(&[
        (
            "effigy.toml",
            "[containers]\ndefault = \"dev\"\n[containers.dev]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
        ),
        (
            "compose.yml",
            "services:\n  app:\n    image: example:app\n    volumes: [\"shared:/data\"]\n  worker:\n    image: example:worker\n    depends_on: [app]\n    volumes: [\"shared:/data\", \"./jobs:/jobs:ro\"]\nvolumes:\n  shared: {}\n",
        ),
    ]);
    let (files, volumes) = rendered(&checkout, "dev");
    assert_eq!(volumes, ["checkout-dev-shared"]);
    let compose = files
        .iter()
        .find(|(name, _)| name.ends_with("dev.generated.compose.yml"))
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).unwrap())
        .expect("generated compose");
    assert_eq!(
        compose.matches("checkout-dev-shared:/data").count(),
        2,
        "{compose}"
    );
    assert!(compose.contains("depends_on"));
    // Every translated service carries checkout ownership, as the retired
    // direct ownership override did.
    assert_eq!(
        compose.matches("com.effigy.checkout").count(),
        2,
        "{compose}"
    );
    round_trip(&checkout, "dev");
}

#[test]
fn translation_warns_and_refuses_inexpressible_compose() {
    let cases = [
        ("networks:\n  edge: {}\n", "top-level `networks`"),
        (
            "volumes:\n  data:\n    external: true\n",
            "volume `data` declares options",
        ),
        ("x-common: &c\n  image: x\n", "top-level `x-common`"),
    ];
    for (extra, expected) in cases {
        let compose = format!("services:\n  app:\n    image: example\n{extra}");
        assert_refused(&compose, expected);
    }
    let services = [
        (
            "    environment:\n      HOME: ${HOME}\n",
            "Compose interpolation",
        ),
        ("    command: echo {{ x }}\n", "template syntax"),
        (
            "    volumes: [\"missing:/data\"]\n",
            "undeclared volume `missing`",
        ),
        (
            "    volumes:\n      - type: volume\n        source: data\n        target: /data\n",
            "long-syntax `volume` mount",
        ),
        (
            "    extends: {file: base.yml, service: app}\n",
            "uses `extends`",
        ),
        ("    networks: [edge]\n", "uses `networks`"),
    ];
    for (body, expected) in services {
        let compose = format!("services:\n  app:\n    image: example\n{body}");
        assert_refused(&compose, expected);
    }
}

fn assert_refused(compose: &str, expected: &str) {
    let checkout = checkout(&[
        (
            "effigy.toml",
            "[containers.dev]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
        ),
        ("compose.yml", compose),
    ]);
    let config = effigy_manifest::load_task_manifest(&checkout.root.join("effigy.toml"))
        .unwrap()
        .containers
        .unwrap()
        .environments
        .remove("dev")
        .unwrap();
    let error = translate(&checkout.root, "dev", &config)
        .unwrap_err()
        .to_string();
    assert!(error.contains(expected), "{expected}: {error}");
    assert!(
        error.contains("cannot be read as a catalog fragment"),
        "{error}"
    );
}

#[test]
fn fragment_names_never_shadow_the_compiled_catalog() {
    assert_eq!(
        fragment_name("Linux Release", "builder"),
        "linux-release-builder"
    );
    let checkout = checkout(&[
        (
            "effigy.toml",
            "[containers.php]\ncompose_file = \"compose.yml\"\nprimary_service = \"fpm\"\n",
        ),
        ("compose.yml", "services:\n  fpm:\n    image: example\n"),
    ]);
    let config = effigy_manifest::load_task_manifest(&checkout.root.join("effigy.toml"))
        .unwrap()
        .containers
        .unwrap()
        .environments
        .remove("php")
        .unwrap();
    let migration = translate(&checkout.root, "php", &config).unwrap().unwrap();
    assert_eq!(migration.fragments[0].name, "php-fpm-migrated");
    assert_eq!(migration.services()["fpm"], "php-fpm-migrated");
}

#[test]
fn a_different_on_disk_fragment_refuses_instead_of_shadowing() {
    let checkout = consumer("soundcheck");
    let directory = checkout
        .root
        .join(PROJECT_LOCAL_CATALOG_DIR)
        .join("linux-arm-builder");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("service.toml"),
        "[service]\nname = \"other\"\n",
    )
    .unwrap();
    let error = crate::with_test_effigy_home(&checkout.home, || {
        load_container_policy(&checkout.root, Some("linux-arm")).unwrap_err()
    });
    assert!(
        error
            .to_string()
            .contains("already holds a different fragment"),
        "{error}"
    );
    drop(checkout.fixture);
}

#[test]
fn migration_plan_requires_a_compose_file_declaration() {
    let checkout = checkout(&[(
        "effigy.toml",
        "[containers.dev]\nprimary_service = \"app\"\n[containers.dev.services.app]\ncatalog = \"redis\"\n",
    )]);
    let error = crate::with_test_effigy_home(&checkout.home, || {
        plan_migration(&checkout.root, Some("dev")).unwrap_err()
    });
    assert!(error.to_string().contains("nothing to migrate"), "{error}");
}

#[test]
fn inline_container_tables_migrate_in_place() {
    let checkout = checkout(&[
        (
            "effigy.toml",
            "[containers]\n# keep me\ndev = { compose_file = \"compose.yml\", primary_service = \"app\" }\n",
        ),
        ("compose.yml", "services:\n  app:\n    image: example\n"),
    ]);
    let plan = round_trip(&checkout, "dev");
    assert!(plan.manifest_after.contains("# keep me"));
    assert!(
        plan.manifest_after
            .contains("services = { app = { catalog = \"dev-app\" } }"),
        "{}",
        plan.manifest_after
    );
}
