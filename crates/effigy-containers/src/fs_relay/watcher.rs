//! Event filtering, path expansion and bounded per-path coalescing.
use crate::policy::model::DEFAULT_FS_EVENTS_IGNORES;
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{event::ModifyKind, Event, EventKind};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
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
struct Entry {
    due: Instant,
    last_sent: Option<Instant>,
    pending: bool,
}
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
    pub fn push(&mut self, path: PathBuf, now: Instant) {
        let entry = self.entries.entry(path).or_insert(Entry {
            due: now + self.debounce,
            last_sent: None,
            pending: true,
        });
        entry.pending = true;
        entry.due = now + self.debounce;
        if let Some(last) = entry.last_sent {
            entry.due = entry.due.max(last + Duration::from_millis(300));
        }
    }
    pub fn drain_ready(&mut self, now: Instant) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        self.entries.retain(|path, entry| {
            if entry.pending && entry.due <= now {
                ready.push(path.clone());
                entry.last_sent = Some(now);
                entry.pending = false;
            }
            entry.pending
                || entry
                    .last_sent
                    .is_none_or(|last| now.duration_since(last) < Duration::from_millis(300))
        });
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, MetadataKind, RemoveKind, RenameMode};
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
    #[test]
    fn cooldown_reschedules_a_second_save() {
        let now = Instant::now();
        let p = PathBuf::from("file");
        let mut c = Coalescer::new(Duration::from_millis(100));
        c.push(p.clone(), now);
        assert!(c.drain_ready(now + Duration::from_millis(99)).is_empty());
        assert_eq!(
            c.drain_ready(now + Duration::from_millis(100)),
            vec![p.clone()]
        );
        c.push(p.clone(), now + Duration::from_millis(150));
        assert!(c.drain_ready(now + Duration::from_millis(399)).is_empty());
        assert_eq!(
            c.drain_ready(now + Duration::from_millis(400)),
            vec![p.clone()]
        );
        let mut slow = Coalescer::new(Duration::from_millis(500));
        slow.push(p.clone(), now);
        assert_eq!(
            slow.drain_ready(now + Duration::from_millis(500)),
            vec![p.clone()]
        );
        slow.push(p.clone(), now + Duration::from_millis(550));
        assert!(slow
            .drain_ready(now + Duration::from_millis(850))
            .is_empty());
        assert_eq!(slow.drain_ready(now + Duration::from_millis(1050)), vec![p]);
    }
}
