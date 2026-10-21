use std::path::{Path, PathBuf};

use effigy_catalog::{CatalogResolver, FragmentSource};
use effigy_cli::{ServiceArgs, ServiceSubcommand};
use serde_json::{json, Value};

use super::command_context::resolve_active_repo_root;
use super::error::RunnerError;

pub(super) fn run_service(args: ServiceArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let resolver = effigy_containers::catalog_resolver(Some(&repo_root))?;
    match args.subcommand {
        ServiceSubcommand::List => run_service_list(&repo_root, &resolver, args.output_json),
        ServiceSubcommand::Extract { service, dir } => run_service_extract(
            &repo_root,
            &resolver,
            &service,
            dir.as_deref(),
            args.output_json,
        ),
    }
}

fn source_json(repo_root: &Path, source: &FragmentSource) -> Value {
    json!({
        "source": source.handle(),
        "layer": source.layer(),
        "path": source.path().map(|path| path_relative_to_repo(repo_root, path)),
    })
}

fn source_label(repo_root: &Path, source: &FragmentSource) -> String {
    match source.path() {
        Some(path) => format!(
            "{}: {}",
            source.handle(),
            path_relative_to_repo(repo_root, path)
        ),
        None => source.handle().to_owned(),
    }
}

fn run_service_list(
    repo_root: &Path,
    resolver: &CatalogResolver,
    output_json: bool,
) -> Result<String, RunnerError> {
    let fragments = resolver.list();
    let sources = resolver.source_handles();
    if output_json {
        return Ok(json!({
            "schema": "effigy.service.list.v2",
            "schema_version": 2,
            "ok": true,
            "sources": sources,
            "fragments": fragments.iter().map(|fragment| {
                let mut entry = source_json(repo_root, &fragment.source);
                entry["name"] = json!(fragment.name);
                entry["qualified"] = json!(fragment.qualified_name());
                entry["shadows"] = fragment
                    .shadows
                    .iter()
                    .map(|source| source_json(repo_root, source))
                    .collect();
                entry
            }).collect::<Vec<_>>(),
        })
        .to_string());
    }

    if fragments.is_empty() {
        return Ok("[info] no service fragments available".to_owned());
    }

    let mut lines = vec![format!(
        "[service] {} fragments (sources: {})",
        fragments.len(),
        sources.join(", ")
    )];
    lines.extend(fragments.into_iter().map(|fragment| {
        let mut line = format!(
            "{} [{}]",
            fragment.name,
            source_label(repo_root, &fragment.source)
        );
        if !fragment.shadows.is_empty() {
            let shadows = fragment
                .shadows
                .iter()
                .map(FragmentSource::handle)
                .collect::<Vec<_>>()
                .join(", ");
            line.push_str(&format!(" shadows {shadows}"));
        }
        line
    }));
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
