use std::path::PathBuf;

use effigy_cli::{CatalogArgs, CatalogCacheSubcommand, CatalogSubcommand};
use effigy_routing::{catalog_discovery_cache_file, clear_catalog_discovery_cache};
use serde_json::json;

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

pub(super) fn run_catalog(args: CatalogArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        CatalogSubcommand::Cache { subcommand } => match subcommand {
            CatalogCacheSubcommand::Clear => {
                run_catalog_cache_clear(args.repo_override, args.output_json)
            }
        },
    }
}

fn run_catalog_cache_clear(
    repo_override: Option<PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(repo_override)?;
    let repo_root = resolved.resolved_root;
    let cache_file = catalog_discovery_cache_file(&repo_root);
    let removed = clear_catalog_discovery_cache(&repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        return Ok(json!({
            "schema": "effigy.catalog.cache.clear.v1",
            "schema_version": 1,
            "ok": true,
            "repo_root": repo_root,
            "cache_file": cache_file,
            "removed": removed,
        })
        .to_string());
    }

    if removed {
        Ok(format!(
            "[ok] cleared catalog discovery cache ({})",
            cache_file.display()
        ))
    } else {
        Ok(format!(
            "[info] catalog discovery cache already clear ({})",
            cache_file.display()
        ))
    }
}
