use std::ffi::OsStr;

use effigy_manifest::LibraryMount;

use crate::ContainerPolicyError;

use super::RenderedWorkspaceMount;

/// Container path under which user-global library mounts are exposed.
///
/// Stable convention so the legacy `effigy mount` command (which lives inside
/// the decodelabs bundle container) has a predictable place to point at when
/// resolving Composer `path` repositories - e.g. `~/Dev/legacy/libraries/decodelabs`
/// becomes available at `/workspace-libraries/decodelabs/<package>` inside
/// the workspace container.
const WORKSPACE_LIBRARIES_ROOT: &str = "/workspace-libraries";

/// Render user-global library mounts as bind-mount entries on the workspace
/// container. Missing host paths are skipped silently - a developer's
/// `~/.effigy/config.toml` may declare a parent directory that is only
/// present on some machines, and a hard error there would block every
/// `effigy container up` until the file is hand-edited. Hard errors are
/// reserved for genuinely broken state (basename collisions between two
/// declared parents).
pub(crate) fn build_library_mounts(
    container_name: &str,
    library_mounts: &[LibraryMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let mut rendered: Vec<RenderedWorkspaceMount> = Vec::new();
    let mut targets_seen: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for entry in library_mounts {
        let host_path = &entry.host_path;
        if !host_path.exists() {
            // Best-effort: skip silently. A warning channel here would be
            // welcome but doesn't have a natural sink at this layer.
            continue;
        }
        let canonical_source = host_path.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` library mount source `{}` is invalid: {error}",
                entry.raw
            ))
        })?;
        let basename = canonical_source
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "container `{container_name}` library mount `{}` resolves to a path with no basename",
                    entry.raw
                ))
            })?;
        let target = format!("{WORKSPACE_LIBRARIES_ROOT}/{basename}");
        if let Some(previous) = targets_seen.get(&target) {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` library mounts `{previous}` and `{}` both resolve to container path `{target}`; rename one or pick a non-colliding parent directory",
                entry.raw
            )));
        }
        targets_seen.insert(target.clone(), entry.raw.clone());
        let rendered_entry = format!("{}:{target}", canonical_source.display());
        rendered.push(RenderedWorkspaceMount {
            target,
            rendered: rendered_entry,
            source: Some(canonical_source),
            named_volume: None,
        });
    }
    Ok(rendered)
}
