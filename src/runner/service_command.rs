use std::path::{Path, PathBuf};

use effigy_catalog::pack::{resolve_catalog_layers, CatalogLayers, PackSelection};
use effigy_catalog::CatalogResolver;
use effigy_cli::{ServiceArgs, ServiceSubcommand};
use serde_json::json;

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

#[path = "service_command/pack.rs"]
mod pack;

pub(in crate::runner) use pack::{effigy_version, pack_health_finding};

pub(super) fn run_service(args: ServiceArgs) -> Result<String, RunnerError> {
    // Pack state is machine-global: `service pack` works outside a repo, so it
    // dispatches before root resolution.
    if let ServiceSubcommand::Pack(subcommand) = args.subcommand {
        return pack::run_service_pack(subcommand, args.output_json);
    }

    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let layers = catalog_layers(&repo_root);
    match args.subcommand {
        ServiceSubcommand::List => {
            run_service_list(&layers.resolver, &layers.selection, args.output_json)
        }
        ServiceSubcommand::Extract { service, dir } => run_service_extract(
            &repo_root,
            &layers.resolver,
            &service,
            dir.as_deref(),
            args.output_json,
        ),
        ServiceSubcommand::Pack(_) => unreachable!("handled above"),
    }
}

fn run_service_list(
    resolver: &CatalogResolver,
    selection: &PackSelection,
    output_json: bool,
) -> Result<String, RunnerError> {
    let fragments = resolver.list();
    if output_json {
        return Ok(json!({
            "schema": "effigy.service.list.v1",
            "schema_version": 1,
            "ok": true,
            "fragments": fragments.iter().map(|fragment| json!({
                "name": fragment.name,
                "source": fragment.source.to_string(),
            })).collect::<Vec<_>>(),
            "selection": pack::selection_payload(selection),
        })
        .to_string());
    }

    if fragments.is_empty() {
        return Ok("[info] no service fragments available".to_owned());
    }

    let mut lines = Vec::new();
    // A silent fallback would look identical to a healthy baseline machine, so
    // the warning leads the listing rather than trailing it.
    if let Some(warning) = selection.fallback_warning() {
        lines.push(warning);
    }
    lines.push(format!("[service] {} fragments", fragments.len()));
    lines.extend(
        fragments
            .into_iter()
            .map(|fragment| format!("{} [{}]", fragment.name, fragment.source)),
    );
    Ok(lines.join("\n"))
}

fn run_service_extract(
    repo_root: &Path,
    resolver: &CatalogResolver,
    service: &str,
    dir: Option<&Path>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let target_dir = resolve_extract_dir(repo_root, dir);
    let extracted_dir = resolver
        .extract(service, &target_dir)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let display = path_relative_to_repo(repo_root, &extracted_dir);

    if output_json {
        return Ok(json!({
            "schema": "effigy.service.extract.v1",
            "schema_version": 1,
            "ok": true,
            "service": service,
            "path": display,
        })
        .to_string());
    }

    Ok(format!(
        "[ok] extracted service fragment `{service}` to {display}"
    ))
}

fn catalog_layers(repo_root: &Path) -> CatalogLayers {
    resolve_catalog_layers(Some(repo_root), effigy_version())
}

fn resolve_extract_dir(repo_root: &Path, dir: Option<&Path>) -> PathBuf {
    match dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => repo_root.join(path),
        None => repo_root.join("infra/dev/catalog"),
    }
}

fn path_relative_to_repo(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
#[path = "service_command/tests.rs"]
mod tests;
