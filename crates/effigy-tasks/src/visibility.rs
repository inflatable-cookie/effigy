//! One discovery policy for inventory, help and shell completion.
use std::collections::BTreeSet;
use std::path::PathBuf;

use effigy_manifest::{
    LoadedCatalog, ManifestManagedConcurrentEntry, ManifestManagedRun, ManifestManagedRunStep,
    ManifestTask, ManifestTaskVisibility,
};

/// References are resolved using the ordinary selector resolver in the
/// declaring catalog's context. A miss or ambiguity hides nothing.
#[derive(Debug, Default)]
pub struct TaskVisibilityIndex {
    referenced: BTreeSet<(PathBuf, String)>,
}

impl TaskVisibilityIndex {
    pub fn new(catalogs: &[LoadedCatalog]) -> Self {
        let mut index = Self::default();
        for catalog in catalogs {
            for task in catalog.manifest.tasks.values() {
                for reference in task_references(task) {
                    let Ok((mut selector, _)) = crate::parse_task_reference_invocation(reference)
                    else {
                        continue;
                    };
                    if selector.prefix.is_none() {
                        selector.prefix = Some(catalog.alias.clone());
                    }
                    if let Ok(selection) = effigy_routing::select_catalog_and_task(
                        &selector,
                        catalogs,
                        &catalog.catalog_root,
                    ) {
                        index
                            .referenced
                            .insert((selection.catalog.catalog_root.clone(), selector.task_name));
                    }
                }
            }
        }
        index
    }

    /// Explicit visibility wins. Otherwise referenced tasks are internal;
    /// unnamespaced names and unreferenced tasks are public entry points.
    pub fn effective(
        &self,
        catalog: &LoadedCatalog,
        name: &str,
        task: &ManifestTask,
    ) -> (ManifestTaskVisibility, &'static str) {
        if let Some(visibility) = task.visibility {
            return (visibility, "declared");
        }
        if name.contains(':')
            && self
                .referenced
                .contains(&(catalog.catalog_root.clone(), name.to_owned()))
        {
            (ManifestTaskVisibility::Internal, "referenced-only")
        } else {
            (ManifestTaskVisibility::Public, "default")
        }
    }
}

fn task_references(task: &ManifestTask) -> Vec<&str> {
    let mut references = Vec::new();
    if let Some(ManifestManagedRun::Sequence(steps)) = &task.run {
        step_references(steps, &mut references);
    }
    concurrent_references(&task.concurrent, &mut references);
    for profile in task.profiles.values() {
        concurrent_references(&profile.concurrent, &mut references);
    }
    references
}

fn step_references<'a>(steps: &'a [ManifestManagedRunStep], references: &mut Vec<&'a str>) {
    for step in steps {
        match step {
            ManifestManagedRunStep::Command(command) => {
                if let Some(reference) = command.strip_prefix("task:") {
                    references.push(reference.trim());
                }
            }
            ManifestManagedRunStep::Step(step) => {
                if let Some(reference) = step.task.as_deref() {
                    references.push(reference);
                }
            }
        }
    }
}

fn concurrent_references<'a>(
    entries: &'a [ManifestManagedConcurrentEntry],
    references: &mut Vec<&'a str>,
) {
    for entry in entries {
        if let Some(reference) = &entry.task {
            references.push(reference);
        }
        step_references(&entry.setup, references);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{list_tasks, render_task_listing_json, render_task_listing_text, ListTasksRequest};
    use std::path::Path;

    fn catalog(alias: &str, body: &str, depth: usize) -> LoadedCatalog {
        let root = PathBuf::from(format!("/virtual/visibility/{alias}"));
        LoadedCatalog {
            alias: alias.to_owned(),
            manifest_path: root.join("effigy.toml"),
            catalog_root: root,
            manifest: toml::from_str(body).expect("valid task fixture"),
            bundle_root: None,
            defer_run: None,
            deferred_builtins: BTreeSet::new(),

            depth,
        }
    }

    #[test]
    fn visibility_resolves_sequences_profiles_arguments_and_cross_catalog_references() {
        let catalogs = vec![
            catalog(
                "root",
                r#"
[tasks]
entry = [{ task = "build" }, { task = "step:helper --flag 'two words'" }, { task = "child/compile:step" }]
"step:helper" = "echo step:helper"
build = "echo build"
standalone = "echo standalone"
kept = { run = "echo kept", visibility = "public", group = "Development" }
hidden = { run = "echo hidden", visibility = "internal" }
"profile-step:helper" = "echo profile"
"setup-step:helper" = "echo setup"
[tasks.dev]
mode = "tui"
concurrent = [{ task = "kept", setup = ["task:setup-step:helper"] }]
[tasks.dev.profiles.extra]
concurrent = [{ task = "profile-step:helper" }]
"#,
                0,
            ),
            catalog("child", "[tasks]\n\"compile:step\" = 'echo child'", 1),
        ];
        let visibility = TaskVisibilityIndex::new(&catalogs);
        for (name, expected, reason) in [
            ("entry", ManifestTaskVisibility::Public, "default"),
            ("build", ManifestTaskVisibility::Public, "default"),
            (
                "step:helper",
                ManifestTaskVisibility::Internal,
                "referenced-only",
            ),
            ("standalone", ManifestTaskVisibility::Public, "default"),
            ("kept", ManifestTaskVisibility::Public, "declared"),
            ("hidden", ManifestTaskVisibility::Internal, "declared"),
            (
                "profile-step:helper",
                ManifestTaskVisibility::Internal,
                "referenced-only",
            ),
            (
                "setup-step:helper",
                ManifestTaskVisibility::Internal,
                "referenced-only",
            ),
        ] {
            assert_eq!(
                visibility.effective(&catalogs[0], name, &catalogs[0].manifest.tasks[name]),
                (expected, reason),
                "{name}"
            );
        }
        assert_eq!(
            visibility.effective(
                &catalogs[1],
                "compile:step",
                &catalogs[1].manifest.tasks["compile:step"]
            ),
            (ManifestTaskVisibility::Internal, "referenced-only")
        );
    }

    #[test]
    fn visibility_text_groups_public_tasks_and_json_always_has_all_tasks() {
        let catalogs = vec![catalog(
            "root",
            r#"
[tasks]
entry = [{ task = "step:helper" }]
"step:helper" = "echo step:helper"
named = { run = "echo named", group = "Development" }
compact = { task = "step:helper", visibility = "public", group = "Development" }
"#,
            0,
        )];
        let precedence = Vec::new();
        let deferred_builtins = BTreeSet::new();
        let listing = |all| {
            list_tasks(
                ListTasksRequest {
                    all,
                    filter: None,
                    pretty_json: false,
                    resolved_root: Path::new("/virtual/visibility/root"),
                    precedence: &precedence,
                    resolve_probe: None,
                    deferred_builtins: &deferred_builtins,
                },
                &catalogs,
            )
            .expect("list tasks")
        };
        let public = listing(false);
        let all = listing(true);
        let text = render_task_listing_text(&public).expect("text");
        assert!(text.contains("Development"));
        assert!(text.contains("General"));
        assert!(!text.contains("echo step:helper"));
        assert!(render_task_listing_text(&all)
            .expect("all text")
            .contains("echo step:helper"));
        let json = render_task_listing_json(&public, false).expect("json");
        assert_eq!(
            json,
            render_task_listing_json(&all, false).expect("all json")
        );
        let payload: serde_json::Value = serde_json::from_str(&json).expect("json payload");
        let helper = payload["catalog_tasks"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["task"] == "step:helper")
            .expect("step:helper row");
        assert_eq!(helper["visibility"], "internal");
        assert_eq!(helper["visibility_reason"], "referenced-only");
        assert!(helper["group"].is_null());
    }
}
