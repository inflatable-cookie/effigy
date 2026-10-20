//! `effigy container migrate`: write a retired `compose_file` environment out
//! as the project catalog fragments it is already read as.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_containers::policy::compose_file::{fragment_matches, plan_migration};
use effigy_containers::{load_container_policy, EffectiveContainerPolicy};

use super::RunnerError;
use crate::runner::command_context::resolve_active_command_context;

pub(super) fn run(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    let repo_root = context.resolved.resolved_root.as_path();
    let plan = plan_migration(repo_root, name)?;
    let container = plan.migration.container.clone();
    let before = render_outputs(&load_container_policy(repo_root, Some(&container))?);
    // Refuse before writing anything if a fragment directory is foreign.
    let mut pending = Vec::new();
    for fragment in &plan.migration.fragments {
        if !fragment.directory.exists() {
            pending.push(fragment);
        } else if !fragment_matches(fragment) {
            return Err(RunnerError::task_invocation(format!(
                "{} already exists with different content; move it aside and rerun `effigy container migrate {container}`",
                fragment.directory.display()
            )));
        }
    }
    let manifest_before = std::fs::read(&plan.manifest)
        .map_err(|error| RunnerError::task_invocation_failed_read(&plan.manifest, error))?;
    let mut written = Vec::new();
    let result = (|| {
        for fragment in &pending {
            std::fs::create_dir_all(&fragment.directory).map_err(|error| {
                RunnerError::task_invocation_failed_write(&fragment.directory, error)
            })?;
            written.push(fragment.directory.clone());
            replace(
                &fragment.directory.join("service.toml"),
                &fragment.service_toml,
            )?;
            replace(
                &fragment.directory.join("compose.fragment.yml"),
                &fragment.compose_fragment,
            )?;
        }
        // Fragments are inert until the manifest names them, so an
        // interrupted run leaves the translated declaration in force.
        replace(&plan.manifest, &plan.manifest_after)?;
        let after = render_outputs(&load_container_policy(repo_root, Some(&container))?);
        if after != before {
            return Err(RunnerError::task_invocation(format!(
                "migrating `{container}` would change its render; the manifest and fragments were restored"
            )));
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::write(&plan.manifest, &manifest_before);
        for directory in written {
            let _ = std::fs::remove_dir_all(directory);
        }
        return Err(error);
    }

    let relative = |path: &Path| {
        path.strip_prefix(repo_root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let fragments = plan
        .migration
        .fragments
        .iter()
        .map(|fragment| {
            serde_json::json!({
                "name": fragment.name,
                "service": fragment.service,
                "path": relative(&fragment.directory),
                "written": pending.iter().any(|pending| pending.name == fragment.name),
            })
        })
        .collect::<Vec<_>>();
    if output_json {
        return Ok(serde_json::json!({
            "schema": "effigy.container.migrate.v1",
            "schema_version": 1,
            "ok": true,
            "environment": container,
            "compose_file": relative(&plan.migration.compose_file),
            "manifest": relative(&plan.manifest),
            "fragments": fragments,
            "render_unchanged": true,
        })
        .to_string());
    }
    let mut output = format!(
        "migrated container `{container}` from {}\n",
        relative(&plan.migration.compose_file)
    );
    for fragment in &plan.migration.fragments {
        output.push_str(&format!(
            "  fragment {}/ for service `{}`\n",
            relative(&fragment.directory),
            fragment.service
        ));
    }
    output.push_str(&format!(
        "  {}: `compose_file` replaced by `[containers.{container}.services]`\nthe render is unchanged; {} is no longer read and can be deleted",
        relative(&plan.manifest),
        relative(&plan.migration.compose_file)
    ));
    Ok(output)
}

/// Everything Compose receives from the render. Declaration inputs differ by
/// construction: migration rewrites the manifest that produced them.
fn render_outputs(policy: &EffectiveContainerPolicy) -> BTreeMap<String, Vec<u8>> {
    policy
        .render
        .iter()
        .flat_map(|render| render.closure.files())
        .filter(|(name, _)| !name.starts_with("inputs/"))
        .map(|(name, bytes)| (name.clone(), bytes.clone()))
        .collect()
}

/// Write beside the target, then rename over it.
fn replace(path: &Path, content: &str) -> Result<(), RunnerError> {
    let staging = path.with_extension("effigy-migrate.tmp");
    std::fs::write(&staging, content)
        .and_then(|()| std::fs::rename(&staging, path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&staging);
            RunnerError::task_invocation_failed_write(path, error)
        })
}
