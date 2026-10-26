//! Runtime ownership labels.
//!
//! A checkout owns a container, volume or network exactly when the runtime
//! object carries that checkout's labels: the Compose project plus Effigy's
//! checkout and scope labels. Nothing is recorded beside them, so ownership
//! after a deleted worktree is read from the runtime itself (contract 052).

pub const SCOPE_LABEL: &str = "com.effigy.scope";
pub const MANAGED_LABEL: &str = "com.effigy.managed";
pub const PROJECT_LABEL: &str = "com.effigy.project";
pub const PERSIST_LABEL: &str = "com.effigy.persist";
pub const SERVICE_LABEL: &str = "com.effigy.service";
pub const VOLUME_NAME_LABEL: &str = "com.effigy.volume-name";
pub const MOUNT_TARGET_LABEL: &str = "com.effigy.mount-target";
pub const REPO_ROOT_LABEL: &str = "com.effigy.repo-root";
/// Canonical checkout path a generated Compose resource was created for.
pub const CHECKOUT_LABEL: &str = "com.effigy.checkout";
/// Digest of the render a shared service container was created from. The
/// render writes it on its own service, so a later `up` reads from the runtime
/// whether the shared service it is about to start is the one that is there
/// (effigy#377).
pub const SHARED_RENDER_LABEL: &str = "com.effigy.shared-render";
pub const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";

/// Whether a named volume is one of a project's volumes, by the name Compose
/// gives it. `nerdctl compose` copies neither Compose's per-volume `labels:`
/// nor Effigy's labels onto a named volume: it creates
/// `<project>-<volume>` and leaves only `com.docker.compose.project` behind.
/// Docker Compose uses `<project>_<volume>` for the same volume when Effigy
/// does not name it, so both separators are accepted. The name is then the
/// ownership proof, exactly as the Compose project label is for a network.
pub fn named_volume_belongs_to(name: &str, project: &str) -> bool {
    name.strip_prefix(project)
        .and_then(|rest| rest.strip_prefix(['-', '_']))
        .is_some_and(|volume| !volume.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_owns_the_volumes_named_after_it() {
        assert!(named_volume_belongs_to("c2raed464-cache-data", "c2raed464"));
        assert!(named_volume_belongs_to(
            "app-web-wt-abcdef012345_db-data",
            "app-web-wt-abcdef012345"
        ));
        assert!(!named_volume_belongs_to(
            "app-web-wt-abcdef012345",
            "app-web-wt-abcdef012345"
        ));
        assert!(!named_volume_belongs_to(
            "app-web-wt-abcdef012345-",
            "app-web-wt-abcdef012345"
        ));
        assert!(!named_volume_belongs_to("app-webx-data", "app-web"));
        assert!(!named_volume_belongs_to("efv-1234", "c2raed464"));
    }
}
