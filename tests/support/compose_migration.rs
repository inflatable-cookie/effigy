//! Integration fixtures predate the v0.16 `compose_file` retirement.
//!
//! A fixture that still writes the retired key resolves no policy: ordinary
//! policy resolution refuses it and names `effigy container migrate`. The
//! fixtures here therefore apply the same migration the command performs, once,
//! after writing the checkout. The migration is the repository state a
//! migrated consumer has, so a fixture's assertions see the ordinary catalog
//! declaration.

/// Apply `effigy container migrate`'s writes to every environment in
/// `repo_root` that still declares the retired `compose_file` key.
pub fn migrate_declared(repo_root: impl AsRef<std::path::Path>) {
    let repo_root = repo_root.as_ref();
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let Ok(manifest) = effigy_manifest::load_task_manifest(&manifest_path) else {
        return;
    };
    let Some(containers) = manifest.containers.as_ref() else {
        return;
    };
    let declared = containers
        .environments
        .iter()
        .filter(|(_, config)| config.compose_file.is_some())
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in declared {
        let plan = effigy_containers::policy::compose_file::plan_migration(repo_root, Some(&name))
            .unwrap_or_else(|error| panic!("{}: {error}", repo_root.display()));
        for fragment in &plan.migration.fragments {
            std::fs::create_dir_all(&fragment.directory).expect("fragment directory");
            std::fs::write(
                fragment.directory.join("service.toml"),
                &fragment.service_toml,
            )
            .expect("service.toml");
            std::fs::write(
                fragment.directory.join("compose.fragment.yml"),
                &fragment.compose_fragment,
            )
            .expect("compose fragment");
        }
        std::fs::write(&plan.manifest, &plan.manifest_after).expect("migrated manifest");
    }
}
