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
pub const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
