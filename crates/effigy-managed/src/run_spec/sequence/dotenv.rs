use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::ManagedError;
use effigy_env::dotenv::parse_dotenv_entries;
use effigy_manifest::LoadedCatalog;

use super::env_files::resolve_env_file_paths;
use super::pathing::{
    find_catalog_by_normalized_root, normalize_path, resolve_catalog_reference_root,
    split_catalog_env_reference,
};

pub fn resolve_dotenv_env_entry(
    entry_ref: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    env_files: Option<&[String]>,
    dotenv_cache: &mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<Option<(String, String)>, ManagedError> {
    if let Some((catalog_path, env_key)) = split_catalog_env_reference(entry_ref) {
        let target_catalog_root = resolve_catalog_reference_root(catalog_path, repo_root);
        let Some(target_catalog) = find_catalog_by_normalized_root(catalogs, &target_catalog_root)
        else {
            return Ok(None);
        };
        let value = resolve_dotenv_entry_from_catalog(
            &target_catalog.catalog_root,
            env_key,
            env_files,
            dotenv_cache,
        )?;
        return Ok(value.map(|value| (env_key.to_owned(), value)));
    }

    let value = resolve_dotenv_entry_from_catalog(repo_root, entry_ref, env_files, dotenv_cache)?;
    Ok(value.map(|value| (entry_ref.to_owned(), value)))
}

fn resolve_dotenv_entry_from_catalog(
    catalog_root: &Path,
    key: &str,
    env_files: Option<&[String]>,
    dotenv_cache: &mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<Option<String>, ManagedError> {
    let normalized_root = normalize_path(catalog_root);
    let env_file_paths = resolve_env_file_paths(&normalized_root, env_files);
    for env_file_path in env_file_paths {
        let entries = load_dotenv_entries_for_path(&env_file_path, dotenv_cache)?;
        if let Some(value) = entries.get(key) {
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

fn load_dotenv_entries_for_path<'a>(
    env_file_path: &Path,
    dotenv_cache: &'a mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<&'a BTreeMap<String, String>, ManagedError> {
    let env_file_path = normalize_path(env_file_path);
    if !dotenv_cache.contains_key(&env_file_path) {
        let parsed = parse_dotenv_file(&env_file_path)?;
        dotenv_cache.insert(env_file_path.clone(), parsed);
    }
    Ok(dotenv_cache
        .get(&env_file_path)
        .expect("dotenv cache entry should exist"))
}

fn parse_dotenv_file(env_file: &Path) -> Result<BTreeMap<String, String>, ManagedError> {
    let src = match fs::read_to_string(env_file) {
        Ok(src) => src,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(ManagedError::task_invocation(format!(
                "failed to read env file `{}`: {error}",
                env_file.display()
            )));
        }
    };
    Ok(parse_dotenv_entries(&src))
}
