//! Invocation holds on selected source generations (contract 020).
//!
//! A command that resolves source paths holds the objects its checkout lock
//! names when it starts. Pruning keeps every generation a live holder names, so
//! a refresh by another invocation never removes bytes a running command uses.
//!
//! A hold is a symlink in `.effigy/cache/source-holds/` named `<pid>.<n>` whose
//! target is the held object, relative to `.effigy`. The holder removes its own
//! links when it exits. A link whose pid is gone is stale and the next prune
//! removes it. A reused pid only keeps bytes longer, never fewer.
use super::*;
use std::collections::BTreeSet;

const HOLDS_DIR: &str = "cache/source-holds";
/// The lock is read again after the holds are written; a selection that moved
/// in between is held again, so this bounds that retry.
const HOLD_ATTEMPTS: usize = 3;

/// The objects one invocation holds. Dropping it releases them.
#[derive(Debug)]
pub struct SourceHold {
    links: Vec<PathBuf>,
}

impl Drop for SourceHold {
    fn drop(&mut self) {
        for link in &self.links {
            let _ = std::fs::remove_file(link);
        }
    }
}

/// Hold every object the checkout lock selects now, for the life of the caller.
///
/// The lock is read, held, and read again: a refresh that publishes between
/// the two reads is seen by the second read, and a refresh that publishes after
/// the holds are written sees them when it prunes. Inside a workspace container
/// nothing is held, because a container pid is meaningless to the host.
pub fn hold_selected_sources(manifest: &Path) -> Result<Option<SourceHold>, ManifestError> {
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return Ok(None);
    }
    for _ in 0..HOLD_ATTEMPTS {
        let Some(lock) = read_lock(manifest)? else {
            return Ok(None);
        };
        let wanted = lock
            .sources
            .values()
            .map(|pin| pin.object.clone())
            .collect::<BTreeSet<_>>();
        if wanted.is_empty() {
            return Ok(None);
        }
        let hold = SourceHold::write(manifest, &wanted)?;
        let settled = read_lock(manifest)?.map(|again| {
            again
                .sources
                .values()
                .map(|pin| pin.object.clone())
                .collect()
        });
        if settled.as_ref() == Some(&wanted) {
            return Ok(Some(hold));
        }
        // The selection moved while these holds were written; dropping `hold`
        // removes them, and the next pass holds the new selection.
    }
    Err(error(
        manifest,
        "source selection kept changing while it was held; retry the command",
    ))
}

impl SourceHold {
    fn write(manifest: &Path, objects: &BTreeSet<PathBuf>) -> Result<Self, ManifestError> {
        let dir = holds_dir(manifest);
        std::fs::create_dir_all(&dir).map_err(|err| io_error(&dir, err))?;
        let pid = std::process::id();
        let mut hold = Self { links: Vec::new() };
        for (index, object) in objects.iter().enumerate() {
            let link = dir.join(format!("{pid}.{index}"));
            // A link left by an earlier process that reused this pid is replaced.
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(Path::new("../..").join(object), &link)
                .map_err(|err| io_error(&link, err))?;
            hold.links.push(link);
        }
        Ok(hold)
    }
}

/// The generation names live holders keep. Stale links are removed here.
pub(super) fn held_generations(manifest: &Path) -> BTreeSet<String> {
    let mut held = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(holds_dir(manifest)) else {
        return held;
    };
    for entry in entries.flatten() {
        let link = entry.path();
        let pid = entry
            .file_name()
            .to_str()
            .and_then(|name| name.split('.').next())
            .and_then(|pid| pid.parse::<i32>().ok());
        if !pid.is_some_and(process_is_alive) {
            let _ = std::fs::remove_file(&link);
            continue;
        }
        let Ok(target) = std::fs::read_link(&link) else {
            continue;
        };
        let mut parts = target.components().map(|part| part.as_os_str());
        if parts.by_ref().any(|part| part == "sources") {
            if let Some(generation) = parts.next().and_then(|name| name.to_str()) {
                held.insert(generation.to_owned());
            }
        }
    }
    held
}

fn holds_dir(manifest: &Path) -> PathBuf {
    parent(manifest).join(".effigy").join(HOLDS_DIR)
}

/// Signal 0 probes existence. EPERM still means the process exists.
fn process_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 performs the permission and existence check only; it
    // delivers nothing to the target process.
    let sent = unsafe { libc::kill(pid, 0) };
    sent == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
