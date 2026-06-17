use crate::runner::run_command;
use crate::runner::tests::prelude::{fs, temp_workspace, write_manifest};
use effigy_cli::{CatalogArgs, CatalogCacheSubcommand, CatalogSubcommand, Command};
use effigy_routing::catalog_discovery_cache_file;

#[test]
fn catalog_cache_clear_removes_repo_discovery_cache() {
    let root = temp_workspace("catalog-cache-clear");
    write_manifest(&root.join("effigy.toml"), "[catalog]\nalias = \"root\"\n");
    let cache_file = catalog_discovery_cache_file(&root);
    fs::create_dir_all(cache_file.parent().expect("cache parent")).expect("mkdir cache parent");
    fs::write(&cache_file, "{}").expect("write cache");

    let output = run_command(Command::Catalog(CatalogArgs {
        subcommand: CatalogSubcommand::Cache {
            subcommand: CatalogCacheSubcommand::Clear,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("clear cache");

    assert!(output.contains("[ok] cleared catalog discovery cache"));
    assert!(!cache_file.exists(), "catalog cache file should be removed");
}
