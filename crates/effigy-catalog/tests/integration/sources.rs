//! Catalog source precedence, qualified names and fragment requirements.
//! Every fixture lives in one TempDir.

use std::path::{Path, PathBuf};

use effigy_catalog::fragment::{CatalogResolver, FragmentSource};
use effigy_catalog::CatalogError;
use semver::Version;

const LAYERS: [&str; 3] = ["project", "bundle", "declared"];

fn write_fragment(root: &Path, name: &str, description: &str, extra: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("service.toml"),
        format!("{extra}[service]\nname = \"{name}\"\ndescription = \"{description}\"\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("compose.fragment.yml"),
        "services:\n  \"{{ service_name }}\":\n    image: fixture\n",
    )
    .unwrap();
    dir
}

struct Sources {
    _owner: tempfile::TempDir,
    roots: [PathBuf; 3],
}

impl Sources {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let roots = LAYERS.map(|layer| {
            let root = owner.path().join(layer);
            std::fs::create_dir(&root).unwrap();
            root
        });
        Self {
            _owner: owner,
            roots,
        }
    }

    fn resolver(&self) -> CatalogResolver {
        CatalogResolver::new(Some(self.roots[0].clone()))
            .with_bundle_catalog(Some(self.roots[1].clone()))
            .with_declared_source("acme", self.roots[2].clone())
            .unwrap()
    }
}

fn expected_source(layer: &str, path: PathBuf) -> FragmentSource {
    match layer {
        "project" => FragmentSource::Project(path),
        "bundle" => FragmentSource::Bundle(path),
        _ => FragmentSource::Declared {
            name: "acme".into(),
            path,
        },
    }
}

/// Every combination of the three directory layers, for a name the compiled
/// baseline also provides (`redis`) and one it does not (`custom`).
#[test]
fn precedence_matrix_across_all_four_layers() {
    for mask in 0u8..8 {
        let sources = Sources::new();
        let present = (0..3)
            .filter(|bit| mask & (1 << bit) != 0)
            .collect::<Vec<_>>();
        let mut dirs = Vec::new();
        for &index in &present {
            for name in ["redis", "custom"] {
                let dir = write_fragment(&sources.roots[index], name, LAYERS[index], "");
                if name == "redis" {
                    dirs.push((index, dir));
                }
            }
        }
        let resolver = sources.resolver();
        let redis = resolver.resolve("redis").unwrap();
        let custom = resolver.resolve("custom");
        match present.first() {
            Some(&winner) => {
                assert_eq!(
                    redis.schema.service.description, LAYERS[winner],
                    "mask {mask}"
                );
                assert_eq!(
                    redis.source,
                    expected_source(LAYERS[winner], dirs[0].1.clone()),
                    "mask {mask}"
                );
                assert_eq!(
                    custom.unwrap().schema.service.description,
                    LAYERS[winner],
                    "mask {mask}"
                );
            }
            None => {
                assert_eq!(redis.source, FragmentSource::Baseline, "mask {mask}");
                assert!(
                    matches!(custom, Err(CatalogError::FragmentNotFound { .. })),
                    "mask {mask}"
                );
            }
        }
        let listed = resolver
            .list()
            .into_iter()
            .find(|info| info.name == "redis")
            .unwrap();
        let mut order = dirs
            .iter()
            .map(|(index, dir)| expected_source(LAYERS[*index], dir.clone()))
            .collect::<Vec<_>>();
        order.push(FragmentSource::Baseline);
        assert_eq!(listed.source, order[0], "mask {mask}");
        assert_eq!(listed.shadows, order[1..].to_vec(), "mask {mask}");
    }
}

#[test]
fn declared_sources_keep_their_declared_order() {
    let owner = tempfile::tempdir().unwrap();
    let first = owner.path().join("first");
    let second = owner.path().join("second");
    write_fragment(&first, "queue", "first", "");
    write_fragment(&second, "queue", "second", "");
    let resolver = CatalogResolver::new(None)
        .with_declared_source("zeta", first)
        .unwrap()
        .with_declared_source("alpha", second)
        .unwrap();
    assert_eq!(resolver.resolve("queue").unwrap().source.handle(), "zeta");
    assert_eq!(
        resolver.source_handles(),
        vec!["project", "zeta", "alpha", "baseline"]
    );
}

#[test]
fn qualified_names_select_exactly_one_source() {
    let sources = Sources::new();
    for (index, layer) in LAYERS.iter().enumerate() {
        write_fragment(&sources.roots[index], "redis", layer, "");
    }
    let resolver = sources.resolver();
    for (qualified, description) in [
        ("project/redis", Some("project")),
        ("bundle/redis", Some("bundle")),
        ("acme/redis", Some("declared")),
        ("baseline/redis", None),
    ] {
        let fragment = resolver.resolve(qualified).unwrap();
        assert_eq!(fragment.name, "redis", "{qualified}");
        match description {
            Some(description) => {
                assert_eq!(
                    fragment.schema.service.description, description,
                    "{qualified}"
                )
            }
            None => assert_eq!(fragment.source, FragmentSource::Baseline),
        }
    }
    assert!(matches!(
        resolver.resolve("acme/postgres"),
        Err(CatalogError::FragmentNotFound { .. })
    ));
    let error = resolver.resolve("other/redis").unwrap_err().to_string();
    assert!(
        error.contains("unknown catalog source `other`")
            && error.contains("project, bundle, acme, baseline"),
        "{error}"
    );
    for invalid in ["acme/", "/redis", "acme/a/b", "acme/..", ".."] {
        assert!(resolver.resolve(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn bundle_qualifier_is_unknown_without_a_bundle_catalog() {
    let resolver = CatalogResolver::new(None);
    assert!(matches!(
        resolver.resolve("bundle/redis"),
        Err(CatalogError::UnknownSource { .. })
    ));
    assert_eq!(resolver.source_handles(), vec!["project", "baseline"]);
}

#[test]
fn declared_source_handles_are_validated() {
    let owner = tempfile::tempdir().unwrap();
    for name in ["project", "bundle", "baseline", "", "a/b", "a:b", " acme"] {
        assert!(
            CatalogResolver::new(None)
                .with_declared_source(name, owner.path().into())
                .is_err(),
            "{name:?}"
        );
    }
    assert!(CatalogResolver::new(None)
        .with_declared_source("acme", owner.path().into())
        .unwrap()
        .with_declared_source("acme", owner.path().into())
        .is_err());
}

#[test]
fn an_older_effigy_names_the_release_a_fragment_requires() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("acme");
    write_fragment(
        &root,
        "queue",
        "future",
        "[requires]\neffigy = \">=0.16\"\nschema = 2\n\n",
    );
    let resolver = |version: &str| {
        CatalogResolver::new(None)
            .with_declared_source("acme", root.clone())
            .unwrap()
            .with_effigy_version(Version::parse(version).unwrap())
    };
    let error = resolver("0.15.0").resolve("queue").unwrap_err().to_string();
    assert!(error.contains("needs Effigy >=0.16"), "{error}");
    assert!(error.contains("this is Effigy 0.15.0"), "{error}");
    assert!(error.contains("from source `acme`"), "{error}");
    // Schema 2 is still beyond this build, so a newer release number alone
    // does not make the fragment renderable.
    let error = resolver("0.16.0").resolve("queue").unwrap_err().to_string();
    assert!(error.contains("fragment schema 2"), "{error}");
}

#[test]
fn requirements_are_checked_before_the_schema_is_parsed() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("acme");
    let dir = write_fragment(&root, "queue", "future", "");
    // A future schema can change shapes this release would reject outright.
    std::fs::write(
        dir.join("service.toml"),
        "[requires]\neffigy = \">=0.20\"\n\n[service]\nname = 7\n",
    )
    .unwrap();
    let error = CatalogResolver::new(None)
        .with_declared_source("acme", root)
        .unwrap()
        .resolve("queue")
        .unwrap_err()
        .to_string();
    assert!(error.contains("needs Effigy >=0.20"), "{error}");
}

#[test]
fn compiled_baseline_fragments_meet_their_own_requirements() {
    let resolver = CatalogResolver::new(None);
    for info in resolver.list() {
        resolver
            .resolve(&info.qualified_name())
            .unwrap_or_else(|error| panic!("{}: {error}", info.name));
    }
}

/// A retired catalog name is refused from every layer, not only the compiled
/// baseline: a project, bundle or declared source that still carries `minio/`
/// cannot reintroduce it, and `service list` never advertises it (effigy#424).
#[test]
fn a_source_local_retired_catalog_name_is_refused_not_resolved() {
    let sources = Sources::new();
    write_fragment(&sources.roots[0], "minio", "project-local retry", "");
    write_fragment(&sources.roots[1], "minio", "bundle-local retry", "");
    write_fragment(&sources.roots[2], "minio", "declared-local retry", "");
    let resolver = sources.resolver();

    for name in ["minio", "project/minio", "bundle/minio", "acme/minio"] {
        let error = resolver.resolve(name).expect_err("minio is retired");
        assert!(
            matches!(error, CatalogError::RetiredCatalogService { .. }),
            "{name} should refuse as retired, got {error:?}"
        );
        assert!(
            error.to_string().contains("catalog = \"silo\""),
            "{name}: {error}"
        );
    }
    assert!(
        resolver.list().iter().all(|info| info.name != "minio"),
        "a retired name must never be listed as resolvable: {:?}",
        resolver
            .list()
            .iter()
            .map(|info| info.name.as_str())
            .collect::<Vec<_>>()
    );

    // A non-retired name from the same layers still resolves normally.
    let silo = write_fragment(&sources.roots[0], "silo", "project-local silo", "");
    let resolved = resolver.resolve("project/silo").unwrap();
    assert_eq!(resolved.name, "silo");
    assert_eq!(
        resolved.source,
        FragmentSource::Project(silo),
        "only retired names are blocked; ordinary project fragments still win"
    );
}
