use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use effigy_manifest::{LoadedCatalog, ManifestTask};
use effigy_routing::select_catalog_and_task;
use effigy_tasks::parse_task_reference_invocation;

use crate::task_graph;

pub(super) fn heavy_health_paths(catalogs: &[LoadedCatalog]) -> Vec<String> {
    let mut issues = BTreeSet::new();
    for catalog in catalogs {
        let Some(health) = catalog.manifest.tasks.get("health") else {
            continue;
        };
        let mut visited = HashSet::new();
        inspect_task(
            catalogs,
            catalog,
            "health",
            health,
            vec![format!("{}/health", catalog.alias)],
            &mut visited,
            &mut issues,
        );
    }
    issues.into_iter().collect()
}

fn inspect_task(
    catalogs: &[LoadedCatalog],
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
    path: Vec<String>,
    visited: &mut HashSet<(PathBuf, String)>,
    issues: &mut BTreeSet<String>,
) {
    let key = (catalog.manifest_path.clone(), task_name.to_owned());
    if !visited.insert(key) {
        return;
    }

    task_graph::for_each_task_command(task, &mut |command| {
        if is_full_test_command(command) {
            issues.insert(format!(
                "{} -> full test command `{}`",
                path.join(" -> "),
                command.trim()
            ));
        }
    });

    task_graph::for_each_task_reference(task, |reference| {
        let Ok((selector, _args)) = parse_task_reference_invocation(reference) else {
            return;
        };
        let label = selector
            .prefix
            .as_ref()
            .map(|prefix| format!("{prefix}/{}", selector.task_name))
            .unwrap_or_else(|| selector.task_name.clone());
        let mut next_path = path.clone();
        next_path.push(label);

        if matches!(selector.task_name.as_str(), "qa" | "test") {
            issues.insert(next_path.join(" -> "));
            return;
        }

        let Ok(selection) = select_catalog_and_task(&selector, catalogs, &catalog.catalog_root)
        else {
            return;
        };
        inspect_task(
            catalogs,
            selection.catalog,
            &selector.task_name,
            selection.task,
            next_path,
            visited,
            issues,
        );
    });
}

fn is_full_test_command(command: &str) -> bool {
    let tokens = command
        .to_ascii_lowercase()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/')))
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    tokens.windows(2).any(|pair| {
        matches!(
            pair,
            [first, second]
                if matches!(
                    (first.as_str(), second.as_str()),
                    ("cargo", "test")
                        | ("bun", "test")
                        | ("effigy", "qa")
                        | ("effigy", "test")
                        | ("npm", "test")
                        | ("pnpm", "test")
                        | ("yarn", "test")
                        | ("go", "test")
                        | ("dotnet", "test")
                        | ("mvn", "test")
                        | ("gradle", "test")
                        | ("make", "test")
                )
        )
    }) || tokens.windows(3).any(|triple| {
        matches!(
            triple,
            [first, second, third]
                if matches!(
                    (first.as_str(), second.as_str(), third.as_str()),
                    ("cargo", "nextest", "run")
                        | ("npm", "run", "test")
                        | ("python", "m", "pytest")
                )
        )
    }) || tokens
        .iter()
        .any(|token| matches!(token.as_str(), "pytest" | "vitest"))
}

#[cfg(test)]
#[path = "posture/tests.rs"]
mod tests;
