use std::path::{Path, PathBuf};

use effigy_catalog::CatalogResolver;
use effigy_cli::{ServiceArgs, ServiceSubcommand};
use serde_json::json;

use super::command_context::{current_working_dir, resolve_repo_root};
use super::error::RunnerError;

pub(super) fn run_service(args: ServiceArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let resolver = catalog_resolver(&repo_root);

    match args.subcommand {
        ServiceSubcommand::List => run_service_list(&resolver, args.output_json),
        ServiceSubcommand::Extract { service, dir } => run_service_extract(
            &repo_root,
            &resolver,
            &service,
            dir.as_deref(),
            args.output_json,
        ),
    }
}

fn run_service_list(resolver: &CatalogResolver, output_json: bool) -> Result<String, RunnerError> {
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
        })
        .to_string());
    }

    if fragments.is_empty() {
        return Ok("[info] no service fragments available".to_owned());
    }

    let mut lines = vec![format!("[service] {} fragments", fragments.len())];
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

fn catalog_resolver(repo_root: &Path) -> CatalogResolver {
    CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    )
}

fn resolve_extract_dir(repo_root: &Path, dir: Option<&Path>) -> PathBuf {
    match dir {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => repo_root.join(path),
        None => repo_root.join("infra/dev/catalog"),
    }
}

fn project_local_catalog_dir(repo_root: &Path) -> Option<PathBuf> {
    let path = repo_root.join("infra/dev/catalog");
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".effigy").join("catalog");
    path.is_dir().then_some(path)
}

fn path_relative_to_repo(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-catalog-command-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    #[test]
    fn catalog_list_reports_bundled_fragments() {
        let root = temp_repo("list");
        let rendered = run_service_list(&catalog_resolver(&root), false).expect("list");
        assert!(rendered.contains("[service]"));
        assert!(rendered.contains("php-fpm [bundled]"));
    }

    #[test]
    fn catalog_extract_defaults_to_project_override_dir() {
        let root = temp_repo("extract");
        let rendered = run_service_extract(&root, &catalog_resolver(&root), "nginx", None, false)
            .expect("extract");

        assert!(rendered.contains("infra/dev/catalog/nginx"));
        assert!(root.join("infra/dev/catalog/nginx/service.toml").exists());
        assert!(root
            .join("infra/dev/catalog/nginx/compose.fragment.yml")
            .exists());
    }
}
