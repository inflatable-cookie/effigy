//! Event filtering, path expansion and bounded per-path coalescing.
use crate::policy::model::DEFAULT_FS_EVENTS_IGNORES;
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{event::ModifyKind, Event, EventKind};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

pub struct EventFilter {
    ignores: GlobSet,
}
impl EventFilter {
    pub fn new(extra: &[String]) -> Result<Self, globset::Error> {
        let mut builder = GlobSetBuilder::new();
        for pattern in DEFAULT_FS_EVENTS_IGNORES
            .iter()
            .copied()
            .chain(extra.iter().map(String::as_str))
        {
            let pattern = pattern.trim_matches('/');
            builder.add(Glob::new(&format!("**/{pattern}"))?);
            builder.add(Glob::new(&format!("**/{pattern}/**"))?);
        }
        Ok(Self {
            ignores: builder.build()?,
        })
    }
    pub fn ignored(&self, path: &Path) -> bool {
        self.ignores.is_match(path)
    }
    pub fn expand(&self, event: &Event) -> Vec<PathBuf> {
        if !matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
                | EventKind::Remove(_)
        ) {
            return Vec::new();
        }
        let mut paths = Vec::new();
        for path in &event.paths {
            if self.ignored(path) {
                continue;
            }
            let exists = path.exists();
            if exists && !matches!(event.kind, EventKind::Remove(_)) {
                paths.push(path.clone());
            }
            if matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(ModifyKind::Name(_))
            ) {
                let mut parent = path.parent();
                while let Some(p) = parent {
                    if p.exists() {
                        if !self.ignored(p) {
                            paths.push(p.to_path_buf());
                        }
                        break;
                    }
                    parent = p.parent();
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }
}
/// How long a sent path keeps the fingerprint it had when sent. Within this
/// window an event that still sees that fingerprint is the echo of our own
/// guest write, not a new host change. FSEvents can report that echo well after
/// the send, so the window is seconds, not the debounce.
pub const ECHO_SUPPRESSION_TTL: Duration = Duration::from_secs(3);

/// What the host sees at one path: the stat fields a guest `chmod` leaves
/// unchanged and a real write, rename, create or delete changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFingerprint {
    Absent,
    Present {
        mtime: Option<SystemTime>,
        len: u64,
        is_dir: bool,
        dev: u64,
        ino: u64,
    },
}

impl PathFingerprint {
    /// The fingerprint of `path` now, without following a symlink.
    pub fn of(path: &Path) -> Self {
        match fs::symlink_metadata(path) {
            Ok(meta) => Self::from_metadata(&meta),
            Err(_) => Self::Absent,
        }
    }

    fn from_metadata(meta: &fs::Metadata) -> Self {
        #[cfg(unix)]
        let (dev, ino) = {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino())
        };
        #[cfg(not(unix))]
        let (dev, ino) = (0, 0);
        Self::Present {
            mtime: meta.modified().ok(),
            len: meta.len(),
            is_dir: meta.is_dir(),
            dev,
            ino,
        }
    }
}

struct Entry {
    due: Instant,
    last_sent: Option<Instant>,
    /// What the host showed at `last_sent`, the state the guest now holds.
    sent: Option<PathFingerprint>,
    pending: bool,
}

/// Debounces host events per path. An event whose path still shows the
/// fingerprint of its last send is dropped: it is the echo of the guest's own
/// `chmod`, and re-sending it would start an oscillation.
pub struct Coalescer {
    debounce: Duration,
    entries: HashMap<PathBuf, Entry>,
}
impl Coalescer {
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            entries: HashMap::new(),
        }
    }

    /// Record an event for `path`. `fingerprint` reads the host's current state
    /// of a path; it is called only for a path that was already sent.
    pub fn push<F>(&mut self, path: PathBuf, now: Instant, fingerprint: F)
    where
        F: Fn(&Path) -> PathFingerprint,
    {
        if let Some(sent) = self.entries.get(&path).and_then(|entry| entry.sent) {
            if fingerprint(&path) == sent {
                return;
            }
        }
        let entry = self.entries.entry(path).or_insert(Entry {
            due: now,
            last_sent: None,
            sent: None,
            pending: false,
        });
        entry.pending = true;
        entry.due = now + self.debounce;
    }

    /// Paths due now, each stamped with the fingerprint it is sent with.
    pub fn drain_ready<F>(&mut self, now: Instant, fingerprint: F) -> Vec<PathBuf>
    where
        F: Fn(&Path) -> PathFingerprint,
    {
        let mut ready = Vec::new();
        self.entries.retain(|path, entry| {
            if entry.pending && entry.due <= now {
                ready.push(path.clone());
                entry.last_sent = Some(now);
                entry.sent = Some(fingerprint(path));
                entry.pending = false;
            }
            entry.pending
                || entry
                    .last_sent
                    .is_some_and(|last| now.duration_since(last) < ECHO_SUPPRESSION_TTL)
        });
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, MetadataKind, RemoveKind, RenameMode};
    use std::time::UNIX_EPOCH;
    #[test]
    fn filters_and_expands_events() {
        let root = tempfile::tempdir().expect("fixture");
        let file = root.path().join("file");
        std::fs::write(&file, "x").expect("write");
        let filter = EventFilter::new(&["cache".into()]).expect("patterns");
        assert!(filter.ignored(&root.path().join("node_modules/a")));
        assert!(filter.ignored(&root.path().join("cache/a")));
        assert!(filter
            .expand(
                &Event::new(EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)))
                    .add_path(file.clone())
            )
            .is_empty());
        assert_eq!(
            filter.expand(
                &Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
                    .add_path(file.clone())
            ),
            vec![file.clone()]
        );
        assert_eq!(
            filter.expand(&Event::new(EventKind::Create(CreateKind::File)).add_path(file.clone())),
            vec![root.path().to_path_buf(), file.clone()]
        );
        assert_eq!(
            filter.expand(
                &Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(file.clone())
            ),
            vec![root.path().to_path_buf(), file.clone()]
        );
        std::fs::remove_file(&file).expect("delete");
        assert_eq!(
            filter.expand(&Event::new(EventKind::Remove(RemoveKind::File)).add_path(file)),
            vec![root.path().to_path_buf()]
        );
    }
    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }
    fn file_fp(len: u64, mtime_secs: u64) -> PathFingerprint {
        PathFingerprint::Present {
            mtime: Some(UNIX_EPOCH + ms(mtime_secs * 1000)),
            len,
            is_dir: false,
            dev: 1,
            ino: 7,
        }
    }
    fn dir_fp(mtime_secs: u64) -> PathFingerprint {
        PathFingerprint::Present {
            mtime: Some(UNIX_EPOCH + ms(mtime_secs * 1000)),
            len: 4096,
            is_dir: true,
            dev: 1,
            ino: 9,
        }
    }

    #[test]
    fn echo_of_own_send_is_dropped_at_any_delay() {
        let p = PathBuf::from("file");
        let f1 = file_fp(5, 100);
        let t0 = Instant::now();
        let mut c = Coalescer::new(ms(100));
        c.push(p.clone(), t0, |_| f1);
        let sent_at = t0 + ms(100);
        assert_eq!(c.drain_ready(sent_at, |_| f1), vec![p.clone()]);
        let mut dispatched = Vec::new();
        for delay in [50, 180, 350, 1000] {
            let at = sent_at + ms(delay);
            c.push(p.clone(), at, |_| f1);
            dispatched.extend(c.drain_ready(at + ms(100), |_| f1));
        }
        assert!(dispatched.is_empty(), "echoes re-sent: {dispatched:?}");
    }

    #[test]
    fn real_rapid_save_passes_through_after_debounce() {
        let p = PathBuf::from("file");
        let (f1, f2) = (file_fp(5, 100), file_fp(9, 101));
        let t0 = Instant::now();
        let mut c = Coalescer::new(ms(100));
        c.push(p.clone(), t0, |_| f1);
        assert_eq!(c.drain_ready(t0 + ms(100), |_| f1), vec![p.clone()]);
        // A save 50 ms after the send changes the fingerprint, so it is accepted.
        let at = t0 + ms(150);
        c.push(p.clone(), at, |_| f2);
        assert!(c.drain_ready(at + ms(99), |_| f2).is_empty());
        assert_eq!(c.drain_ready(at + ms(100), |_| f2), vec![p.clone()]);
        // The echo of that second send is then dropped too.
        c.push(p.clone(), at + ms(200), |_| f2);
        assert!(c.drain_ready(at + ms(400), |_| f2).is_empty());
    }

    #[test]
    fn directory_echo_is_dropped_and_child_change_forwarded() {
        let dir = PathBuf::from("root/dir");
        let (d1, d2) = (dir_fp(100), dir_fp(200));
        let t0 = Instant::now();
        let mut c = Coalescer::new(ms(100));
        // A child create expands to its parent directory, which is sent once.
        c.push(dir.clone(), t0, |_| d1);
        assert_eq!(c.drain_ready(t0 + ms(100), |_| d1), vec![dir.clone()]);
        // A coalesced echo with the directory's mtime unchanged is dropped.
        c.push(dir.clone(), t0 + ms(150), |_| d1);
        assert!(c.drain_ready(t0 + ms(250), |_| d1).is_empty());
        // A second child create changes the directory's mtime and is forwarded.
        c.push(dir.clone(), t0 + ms(400), |_| d2);
        assert_eq!(c.drain_ready(t0 + ms(500), |_| d2), vec![dir]);
    }

    #[cfg(unix)]
    #[test]
    fn guest_chmod_leaves_path_fingerprint_unchanged() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().expect("fixture");
        let file = root.path().join("file");
        fs::write(&file, "hello").expect("write");
        let file_before = PathFingerprint::of(&file);
        let dir_before = PathFingerprint::of(root.path());
        // The guest runs `chmod <current mode>`, which changes ctime only.
        let mode = fs::metadata(&file).expect("stat").permissions().mode() & 0o7777;
        std::thread::sleep(ms(20));
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).expect("chmod");
        assert_eq!(PathFingerprint::of(&file), file_before);
        assert_eq!(PathFingerprint::of(root.path()), dir_before);
        // Real changes are visible.
        fs::write(&file, "hello, again").expect("write");
        assert_ne!(PathFingerprint::of(&file), file_before);
        fs::write(root.path().join("new"), "").expect("create");
        assert_ne!(PathFingerprint::of(root.path()), dir_before);
        assert_eq!(
            PathFingerprint::of(&root.path().join("gone")),
            PathFingerprint::Absent
        );
    }
}
