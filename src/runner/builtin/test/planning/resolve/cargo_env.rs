use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::runner::manifest::ManifestEnvEntry;
use crate::runner::model::catalog::LoadedCatalog;
use crate::runner::util::parse_dotenv_entries;

const BUILTIN_CARGO_ENV_FALLBACK_KEYS: [&str; 2] = ["CARGO_HOME", "CARGO_TARGET_DIR"];

pub(super) fn resolve_manifest_cargo_env(catalog: &LoadedCatalog) -> BTreeMap<String, String> {
    let mut from_profiles = BTreeMap::<String, String>::new();
    let mut from_values = BTreeMap::<String, String>::new();
    for (entry_name, entry) in &catalog.manifest.env {
        match entry {
            ManifestEnvEntry::Value(value) if entry_name.starts_with("CARGO_") => {
                from_values.insert(entry_name.clone(), value.clone());
            }
            ManifestEnvEntry::Profile(entries) => {
                for table in entries {
                    for (key, value) in table {
                        if key.starts_with("CARGO_") {
                            from_profiles.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    from_profiles.extend(from_values);
    apply_builtin_cargo_env_fallback(&catalog.catalog_root, &mut from_profiles);
    from_profiles
}

fn apply_builtin_cargo_env_fallback(catalog_root: &Path, cargo_env: &mut BTreeMap<String, String>) {
    for key in BUILTIN_CARGO_ENV_FALLBACK_KEYS {
        if cargo_env.contains_key(key) {
            continue;
        }
        if let Ok(value) = std::env::var(key) {
            cargo_env.insert(key.to_owned(), value);
        }
    }

    if BUILTIN_CARGO_ENV_FALLBACK_KEYS
        .iter()
        .all(|key| cargo_env.contains_key(*key))
    {
        return;
    }

    let dotenv = parse_dotenv_file_best_effort(&catalog_root.join(".env"));
    for key in BUILTIN_CARGO_ENV_FALLBACK_KEYS {
        if cargo_env.contains_key(key) {
            continue;
        }
        if let Some(value) = dotenv.get(key) {
            cargo_env.insert(key.to_owned(), value.to_owned());
        }
    }
}

fn parse_dotenv_file_best_effort(path: &Path) -> BTreeMap<String, String> {
    let Ok(src) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    parse_dotenv_entries(&src)
}
