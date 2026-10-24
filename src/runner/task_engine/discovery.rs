//! Bounded, read-only candidates. A search root is not a checkout registry or
//! mutation authority; each consumer verifies the exact identity again.
use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::observe::Observed;
use effigy_kernel::record::{read_record, Record};
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

const MAX_CHECKOUTS: usize = 1000;
const MAX_DEPTH: usize = 8;
const MAX_DIRECTORIES: usize = 20_000;

#[derive(Debug, Serialize)]
pub(in crate::runner) struct Omitted {
    pub(in crate::runner) path: PathBuf,
    pub(in crate::runner) reason: String,
}
#[derive(Debug, Default, Serialize)]
pub(in crate::runner) struct Discovery {
    pub(in crate::runner) roots_searched: Vec<PathBuf>,
    pub(in crate::runner) checkouts: BTreeSet<PathBuf>,
    pub(in crate::runner) omitted: Vec<Omitted>,
}
impl Discovery {
    fn omit(&mut self, path: &Path, reason: impl ToString) {
        self.omitted.push(Omitted {
            path: path.into(),
            reason: reason.to_string(),
        });
    }
    fn candidate(&mut self, path: &Path) -> Option<PathBuf> {
        if self.checkouts.len() >= MAX_CHECKOUTS {
            self.omit(path, "checkout candidate limit (1000)");
            return None;
        }
        match effigy_kernel::fs::DirFd::open(path) {
            Ok(dir) => {
                self.checkouts.insert(dir.path().to_owned());
                Some(dir.path().to_owned())
            }
            Err(error) => {
                self.omit(path, error);
                None
            }
        }
    }
}

pub(in crate::runner) fn collect(
    cx: &ObserveContext,
    current: &Path,
    search_roots: &[PathBuf],
) -> Discovery {
    let mut out = Discovery::default();
    if let Some(current) = out.candidate(current) {
        git_worktrees(cx, &current, &mut out);
    }
    let mut queue: VecDeque<_> = search_roots.iter().cloned().map(|root| (root, 0)).collect();
    let mut seen = BTreeSet::new();
    while let Some((path, depth)) = queue.pop_front() {
        if let Err(error) = cx.check("checkout discovery") {
            out.omit(&path, format!("view budget: {error:?}"));
            break;
        }
        if seen.len() >= MAX_DIRECTORIES || out.checkouts.len() >= MAX_CHECKOUTS {
            out.omit(&path, "discovery directory or checkout limit");
            break;
        }
        if !path.is_absolute() {
            out.omit(&path, "search roots must be absolute");
            continue;
        }
        if depth == 0 {
            out.roots_searched.push(path.clone());
        }
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) => {
                out.omit(&path, error);
                continue;
            }
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            out.omit(&path, "search root is not a real directory");
            continue;
        }
        let dir = match effigy_kernel::fs::DirFd::open(&path) {
            Ok(dir) => dir,
            Err(error) => {
                out.omit(&path, error);
                continue;
            }
        };
        if !seen.insert(dir.path().to_owned()) {
            continue;
        }
        if path.join("effigy.toml").is_file() || path.join(".effigy/state").is_dir() {
            let _ = out.candidate(dir.path());
            git_worktrees(cx, dir.path(), &mut out);
        }
        if depth == MAX_DEPTH {
            out.omit(&path, "discovery depth limit (8)");
            continue;
        }
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                out.omit(&path, error);
                continue;
            }
        };
        for entry in entries.take(MAX_DIRECTORIES + 1) {
            if cx.check("checkout discovery entries").is_err() {
                out.omit(&path, "view budget");
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    out.omit(&path, error);
                    continue;
                }
            };
            if matches!(
                entry.file_name().to_str(),
                Some(".git" | ".effigy" | "target" | "node_modules" | "vendor" | ".cache")
            ) {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => queue.push_back((entry.path(), depth + 1)),
                Err(error) => out.omit(&entry.path(), error),
                _ => {}
            }
            if queue.len() >= MAX_DIRECTORIES {
                out.omit(&path, "discovery directory limit");
                break;
            }
        }
    }
    out
}

#[derive(Debug)]
struct GitPointer(String);
impl Record for GitPointer {
    const MAX_BYTES: u64 = 4096;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value = std::str::from_utf8(bytes)
            .map_err(|error| error.to_string())?
            .trim();
        if value.is_empty() {
            return Err("empty Git worktree pointer".into());
        }
        Ok(Self(value.to_owned()))
    }
}

fn git_worktrees(cx: &ObserveContext, current: &Path, out: &mut Discovery) {
    out.roots_searched.push(current.to_owned());
    if std::fs::symlink_metadata(current.join(".git"))
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        out.omit(
            &current.join(".git"),
            "Git directory link is not a discovery root",
        );
        return;
    }
    let linked = effigy_core::git_worktree::detect_linked_worktree(current);
    let common = linked
        .as_ref()
        .map_or_else(|| current.join(".git"), |git| git.common_git_dir.clone());
    if let Some(primary) = linked.and_then(|git| git.primary_checkout_root) {
        let _ = out.candidate(&primary);
    }
    let worktrees = common.join("worktrees");
    if !worktrees.exists() {
        return;
    }
    if let Err(error) = effigy_kernel::fs::DirFd::open(&worktrees) {
        out.omit(&worktrees, error);
        return;
    }
    let entries = match std::fs::read_dir(&worktrees) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            out.omit(&common, error);
            return;
        }
    };
    for entry in entries.take(MAX_CHECKOUTS + 1) {
        if cx.check("Git worktree discovery").is_err() {
            out.omit(&common, "view budget");
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                out.omit(&common, error);
                continue;
            }
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let pointer = entry.path().join("gitdir");
        let dir = match effigy_kernel::fs::DirFd::open(&entry.path()) {
            Ok(dir) => dir,
            Err(error) => {
                out.omit(&pointer, error);
                continue;
            }
        };
        match read_record::<GitPointer>(
            cx,
            &dir,
            &effigy_kernel::fs::FileName::new("gitdir").expect("fixed Git metadata name"),
        ) {
            Observed::Verified(GitPointer(value), _) => {
                let gitfile = Path::new(&value);
                if let Some(root) = gitfile.parent().filter(|root| root.is_absolute()) {
                    let _ = out.candidate(root);
                } else {
                    out.omit(&pointer, "relative Git worktree pointer");
                }
            }
            other => out.omit(
                &pointer,
                format!("Git worktree pointer is unavailable: {other:?}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::kernel;
    #[test]
    fn runtime_discovery_never_follows_directory_links_and_reports_missing_roots() {
        let root = effigy_test_support::Fixture::new("q010-discovery-").unwrap();
        let external = root.path().join("unsearched");
        let search = root.path().join("search");
        std::fs::create_dir_all(external.join("child")).unwrap();
        std::fs::create_dir_all(search.join("a")).unwrap();
        std::fs::write(search.join("a/effigy.toml"), "").unwrap();
        std::fs::write(external.join("child/effigy.toml"), "").unwrap();
        std::os::unix::fs::symlink(&external, search.join("link")).unwrap();
        let before = std::fs::read_dir(search.join("a")).unwrap().count();
        let result = collect(
            &kernel::observe(),
            &search,
            &[search.clone(), root.path().join("missing")],
        );
        assert!(result.checkouts.contains(&search.join("a")));
        assert!(!result.checkouts.contains(&external.join("child")));
        assert!(result
            .omitted
            .iter()
            .any(|row| row.path.ends_with("missing")));
        assert_eq!(std::fs::read_dir(search.join("a")).unwrap().count(), before);
    }
}
