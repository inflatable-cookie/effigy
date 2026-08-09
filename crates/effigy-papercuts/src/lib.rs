//! Project and portfolio discovery for conventional root `PAPERCUTS.md` queues.

use fs2::FileExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const PAPERCUTS_SCHEMA: &str = "effigy.papercuts.v1";
pub const PAPERCUTS_SCHEMA_VERSION: u32 = 1;
pub const PAPERCUTS_ADD_SCHEMA: &str = "effigy.papercuts.add.v1";
const QUEUE_FILE: &str = "PAPERCUTS.md";
const STARTER: &str = "# Papercuts\n\nSmall, actionable friction found during agent work. Agents append entries when\nthey hit a solvable hurdle; they do not stop the current task to fix one.\n\n## Open\n\n<!-- Keep entries short. Append newest entries at the top. Do not include secrets. -->\n";

#[derive(Debug)]
pub enum PapercutsError {
    InvalidScope {
        path: PathBuf,
        detail: String,
    },
    CollectionAdd {
        path: PathBuf,
    },
    DuplicateOpenTitle {
        title: String,
    },
    LockBusy {
        path: PathBuf,
    },
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for PapercutsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope { path, detail } => {
                write!(f, "invalid papercuts scope `{}`: {detail}", path.display())
            }
            Self::CollectionAdd { path } => write!(
                f,
                "`papercuts add` requires one project scope; `{}` resolves as a collection",
                path.display()
            ),
            Self::DuplicateOpenTitle { title } => {
                write!(f, "an open papercut titled `{title}` already exists")
            }
            Self::LockBusy { path } => write!(
                f,
                "papercuts queue is being updated by another process (lock `{}`)",
                path.display()
            ),
            Self::Io {
                action,
                path,
                source,
            } => {
                write!(f, "failed to {action} `{}`: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for PapercutsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeMode {
    Project,
    Collection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PapercutStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PapercutEntry {
    pub project: String,
    pub project_root: PathBuf,
    pub source_path: PathBuf,
    pub source_line: usize,
    pub status: PapercutStatus,
    pub title: String,
    pub date: String,
    pub friction: String,
    pub impact: String,
    pub possible_fix: String,
    pub surface: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PapercutDiagnostic {
    pub source_path: PathBuf,
    pub source_line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PapercutSummary {
    pub projects_scanned: usize,
    pub files_found: usize,
    pub open: usize,
    pub closed: usize,
    pub diagnostics: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PapercutReport {
    pub schema: &'static str,
    pub schema_version: u32,
    pub scope: PathBuf,
    pub mode: ScopeMode,
    pub summary: PapercutSummary,
    pub entries: Vec<PapercutEntry>,
    pub diagnostics: Vec<PapercutDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PapercutDraft {
    pub title: String,
    pub friction: String,
    pub impact: String,
    pub possible_fix: String,
    pub surface: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PapercutAddReport {
    pub schema: &'static str,
    pub schema_version: u32,
    pub entry: PapercutEntry,
}

impl PapercutAddReport {
    pub fn new(entry: PapercutEntry) -> Self {
        Self {
            schema: PAPERCUTS_ADD_SCHEMA,
            schema_version: PAPERCUTS_SCHEMA_VERSION,
            entry,
        }
    }
}

pub fn discover(scope: &Path, include_closed: bool) -> Result<PapercutReport, PapercutsError> {
    let resolved = resolve_scope(scope)?;
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let mut files_found = 0;

    for root in &resolved.project_roots {
        let queue = root.join(QUEUE_FILE);
        if !queue.is_file() {
            continue;
        }
        files_found += 1;
        let content = fs::read_to_string(&queue).map_err(|source| PapercutsError::Io {
            action: "read papercuts queue",
            path: queue.clone(),
            source,
        })?;
        let (mut parsed, mut parsed_diagnostics) = parse_queue(root, &queue, &content);
        entries.append(&mut parsed);
        diagnostics.append(&mut parsed_diagnostics);
    }

    if !include_closed {
        entries.retain(|entry| entry.status == PapercutStatus::Open);
    }
    sort_entries(&mut entries);
    let open = entries
        .iter()
        .filter(|entry| entry.status == PapercutStatus::Open)
        .count();
    let closed = entries.len() - open;

    Ok(PapercutReport {
        schema: PAPERCUTS_SCHEMA,
        schema_version: PAPERCUTS_SCHEMA_VERSION,
        scope: resolved.scope,
        mode: resolved.mode,
        summary: PapercutSummary {
            projects_scanned: resolved.project_roots.len(),
            files_found,
            open,
            closed,
            diagnostics: diagnostics.len(),
        },
        entries,
        diagnostics,
    })
}

pub fn add(
    scope: &Path,
    date: &str,
    draft: &PapercutDraft,
) -> Result<PapercutEntry, PapercutsError> {
    let resolved = resolve_scope(scope)?;
    if resolved.mode != ScopeMode::Project || resolved.project_roots.len() != 1 {
        return Err(PapercutsError::CollectionAdd {
            path: resolved.scope,
        });
    }
    let root = &resolved.project_roots[0];
    let queue = root.join(QUEUE_FILE);
    let lock_dir = std::env::temp_dir().join("effigy-papercuts-locks");
    fs::create_dir_all(&lock_dir).map_err(|source| PapercutsError::Io {
        action: "create papercuts lock directory",
        path: lock_dir.clone(),
        source,
    })?;
    let lock_path = lock_dir.join(format!(
        "{}.lock",
        fingerprint(&[queue.to_string_lossy().as_ref()])
    ));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| PapercutsError::Io {
            action: "open papercuts lock",
            path: lock_path.clone(),
            source,
        })?;
    lock.try_lock_exclusive().map_err(|source| {
        if source.kind() == std::io::ErrorKind::WouldBlock {
            PapercutsError::LockBusy {
                path: lock_path.clone(),
            }
        } else {
            PapercutsError::Io {
                action: "acquire papercuts lock",
                path: lock_path.clone(),
                source,
            }
        }
    })?;

    let existing = if queue.is_file() {
        fs::read_to_string(&queue).map_err(|source| PapercutsError::Io {
            action: "read papercuts queue",
            path: queue.clone(),
            source,
        })?
    } else {
        STARTER.to_owned()
    };
    let (entries, _) = parse_queue(root, &queue, &existing);
    let normalized_title = normalize_title(&draft.title);
    if entries.iter().any(|entry| {
        entry.status == PapercutStatus::Open && normalize_title(&entry.title) == normalized_title
    }) {
        return Err(PapercutsError::DuplicateOpenTitle {
            title: draft.title.clone(),
        });
    }

    let block = render_entry(date, draft);
    let insertion = insertion_offset(&existing).ok_or_else(|| PapercutsError::InvalidScope {
        path: queue.clone(),
        detail: "queue is missing the `## Open` section".to_owned(),
    })?;
    let mut updated = String::with_capacity(existing.len() + block.len() + 2);
    updated.push_str(&existing[..insertion]);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push('\n');
    updated.push_str(&block);
    updated.push_str(&existing[insertion..]);
    atomic_write(&queue, updated.as_bytes())?;
    drop(lock);

    let (entries, _) = parse_queue(root, &queue, &updated);
    entries
        .into_iter()
        .find(|entry| entry.status == PapercutStatus::Open && entry.title == draft.title)
        .ok_or_else(|| PapercutsError::InvalidScope {
            path: queue,
            detail: "new entry could not be read after insertion".to_owned(),
        })
}

struct ResolvedScope {
    scope: PathBuf,
    mode: ScopeMode,
    project_roots: Vec<PathBuf>,
}

fn resolve_scope(scope: &Path) -> Result<ResolvedScope, PapercutsError> {
    let scope = scope.canonicalize().map_err(|source| PapercutsError::Io {
        action: "resolve papercuts scope",
        path: scope.to_path_buf(),
        source,
    })?;
    if !scope.is_dir() {
        return Err(PapercutsError::InvalidScope {
            path: scope,
            detail: "scope must be a directory".to_owned(),
        });
    }
    if let Some(root) = nearest_project_root(&scope) {
        return Ok(ResolvedScope {
            scope,
            mode: ScopeMode::Project,
            project_roots: vec![root],
        });
    }

    let mut roots = Vec::new();
    let children = fs::read_dir(&scope).map_err(|source| PapercutsError::Io {
        action: "read papercuts collection",
        path: scope.clone(),
        source,
    })?;
    for child in children {
        let child = child.map_err(|source| PapercutsError::Io {
            action: "read papercuts collection entry",
            path: scope.clone(),
            source,
        })?;
        let file_type = child.file_type().map_err(|source| PapercutsError::Io {
            action: "inspect papercuts collection entry",
            path: child.path(),
            source,
        })?;
        if file_type.is_dir() && !file_type.is_symlink() && is_project_root(&child.path()) {
            roots.push(child.path());
        }
    }
    roots.sort();
    Ok(ResolvedScope {
        scope,
        mode: ScopeMode::Collection,
        project_roots: roots,
    })
}

fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|path| is_project_root(path))
        .map(Path::to_path_buf)
}

fn is_project_root(path: &Path) -> bool {
    path.join(".git").exists() || path.join("effigy.toml").is_file()
}

#[derive(Default)]
struct ParsedFields {
    friction: String,
    impact: String,
    possible_fix: String,
    surface: String,
    resolution: String,
}

#[derive(Clone, Copy)]
enum Field {
    Friction,
    Impact,
    PossibleFix,
    Surface,
    Resolution,
}

fn parse_queue(
    root: &Path,
    queue: &Path,
    content: &str,
) -> (Vec<PapercutEntry>, Vec<PapercutDiagnostic>) {
    let project = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_owned();
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let lines = content.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].starts_with("### [") {
            index += 1;
            continue;
        }
        let source_line = index + 1;
        let Some((status, title, date)) = parse_heading(lines[index]) else {
            diagnostics.push(PapercutDiagnostic {
                source_path: queue.to_path_buf(),
                source_line,
                message: "invalid papercut heading; expected `### [ ] <title> — YYYY-MM-DD`"
                    .to_owned(),
            });
            index += 1;
            continue;
        };
        index += 1;
        let mut fields = ParsedFields::default();
        let mut active = None;
        while index < lines.len() && !lines[index].starts_with("### [") {
            let line = lines[index];
            if line.starts_with("# ") || line.starts_with("## ") {
                break;
            }
            if let Some((field, value)) = parse_field(line) {
                active = Some(field);
                append_field(&mut fields, field, value);
            } else if let Some(field) = active {
                let continuation = line.trim();
                if !continuation.is_empty() {
                    append_field(&mut fields, field, continuation);
                }
            }
            index += 1;
        }
        if status == PapercutStatus::Open {
            for (name, value) in [
                ("Friction", fields.friction.as_str()),
                ("Impact", fields.impact.as_str()),
                ("Possible fix", fields.possible_fix.as_str()),
                ("Surface", fields.surface.as_str()),
            ] {
                if value.is_empty() {
                    diagnostics.push(PapercutDiagnostic {
                        source_path: queue.to_path_buf(),
                        source_line,
                        message: format!("papercut entry is missing `{name}`"),
                    });
                }
            }
        }
        let fingerprint = fingerprint(&[
            &project,
            status_name(status),
            &title,
            &date,
            &fields.friction,
            &fields.impact,
            &fields.possible_fix,
            &fields.surface,
            &fields.resolution,
        ]);
        entries.push(PapercutEntry {
            project: project.clone(),
            project_root: root.to_path_buf(),
            source_path: queue.to_path_buf(),
            source_line,
            status,
            title,
            date,
            friction: fields.friction,
            impact: fields.impact,
            possible_fix: fields.possible_fix,
            surface: fields.surface,
            resolution: (!fields.resolution.is_empty()).then_some(fields.resolution),
            fingerprint,
        });
    }
    (entries, diagnostics)
}

fn parse_heading(line: &str) -> Option<(PapercutStatus, String, String)> {
    let rest = line.strip_prefix("### [")?;
    let (marker, rest) = rest.split_once("] ")?;
    let status = match marker {
        " " => PapercutStatus::Open,
        "x" | "X" => PapercutStatus::Closed,
        _ => return None,
    };
    let (title, date) = rest.rsplit_once(" — ")?;
    let title = title.trim();
    let date = date.trim();
    if title.is_empty() || !valid_date(date) {
        return None;
    }
    Some((status, title.to_owned(), date.to_owned()))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn parse_field(line: &str) -> Option<(Field, &str)> {
    let line = line.trim_start();
    for (prefix, field) in [
        ("- Friction:", Field::Friction),
        ("- Impact:", Field::Impact),
        ("- Possible fix:", Field::PossibleFix),
        ("- Surface:", Field::Surface),
        ("- Resolution:", Field::Resolution),
    ] {
        if let Some(value) = line.strip_prefix(prefix) {
            return Some((field, value.trim()));
        }
    }
    None
}

fn append_field(fields: &mut ParsedFields, field: Field, value: &str) {
    let target = match field {
        Field::Friction => &mut fields.friction,
        Field::Impact => &mut fields.impact,
        Field::PossibleFix => &mut fields.possible_fix,
        Field::Surface => &mut fields.surface,
        Field::Resolution => &mut fields.resolution,
    };
    if !target.is_empty() {
        target.push(' ');
    }
    target.push_str(value);
}

fn sort_entries(entries: &mut [PapercutEntry]) {
    entries.sort_by(|left, right| {
        left.project
            .cmp(&right.project)
            .then_with(|| status_rank(left.status).cmp(&status_rank(right.status)))
            .then_with(|| right.date.cmp(&left.date))
            .then_with(|| left.title.cmp(&right.title))
    });
}

fn status_rank(status: PapercutStatus) -> u8 {
    match status {
        PapercutStatus::Open => 0,
        PapercutStatus::Closed => 1,
    }
}

fn status_name(status: PapercutStatus) -> &'static str {
    match status {
        PapercutStatus::Open => "open",
        PapercutStatus::Closed => "closed",
    }
}

fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn insertion_offset(content: &str) -> Option<usize> {
    let open = content.find("## Open")?;
    let after_heading = content[open..].find('\n').map(|offset| open + offset + 1)?;
    let tail = &content[after_heading..];
    if let Some(comment_start) = tail.find("<!--") {
        let before_comment = &tail[..comment_start];
        if before_comment.trim().is_empty() {
            let comment_end = tail[comment_start..].find("-->")? + comment_start + 3;
            return Some(
                after_heading + comment_end + usize::from(tail[comment_end..].starts_with('\n')),
            );
        }
    }
    Some(after_heading)
}

fn render_entry(date: &str, draft: &PapercutDraft) -> String {
    format!(
        "### [ ] {} — {}\n- Friction: {}\n- Impact: {}\n- Possible fix: {}\n- Surface: {}\n\n",
        draft.title, date, draft.friction, draft.impact, draft.possible_fix, draft.surface
    )
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), PapercutsError> {
    let parent = path.parent().ok_or_else(|| PapercutsError::InvalidScope {
        path: path.to_path_buf(),
        detail: "queue has no parent directory".to_owned(),
    })?;
    let temp = parent.join(format!(".{QUEUE_FILE}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temp);
        return Err(PapercutsError::Io {
            action: "write papercuts queue",
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn project(parent: &Path, name: &str, queue: Option<&str>) -> PathBuf {
        let root = parent.join(name);
        fs::create_dir_all(root.join(".git")).unwrap();
        if let Some(queue) = queue {
            fs::write(root.join(QUEUE_FILE), queue).unwrap();
        }
        root
    }

    const QUEUE: &str = "# Papercuts\n\n## Open\n\n<!-- Keep entries short. -->\n\n### [ ] Slow graph — 2026-08-09\n- Friction: graph output\n  needed another pass\n- Impact: repeat work\n- Possible fix: refresh once\n- Surface: graph\n\n### [x] Old cut — 2026-08-08\n- Friction: old\n- Impact: old impact\n- Possible fix: fixed\n- Surface: docs\n- Resolution: done\n";

    #[test]
    fn project_discovery_defaults_to_open_and_preserves_multiline_fields() {
        let temp = TempDir::new().unwrap();
        let root = project(temp.path(), "alpha", Some(QUEUE));
        let nested = root.join("src");
        fs::create_dir(&nested).unwrap();

        let report = discover(&nested, false).unwrap();
        assert_eq!(report.mode, ScopeMode::Project);
        assert_eq!(report.summary.open, 1);
        assert_eq!(report.summary.closed, 0);
        assert_eq!(
            report.entries[0].friction,
            "graph output needed another pass"
        );
    }

    #[test]
    fn collection_reads_immediate_project_roots_and_skips_nested_templates() {
        let temp = TempDir::new().unwrap();
        let alpha = project(temp.path(), "alpha", Some(QUEUE));
        let beta = project(temp.path(), "beta", Some(QUEUE));
        let template = alpha.join("skills/template");
        fs::create_dir_all(&template).unwrap();
        fs::write(template.join(QUEUE_FILE), QUEUE).unwrap();
        fs::create_dir(temp.path().join("not-a-project")).unwrap();

        let report = discover(temp.path(), true).unwrap();
        assert_eq!(report.mode, ScopeMode::Collection);
        assert_eq!(report.summary.projects_scanned, 2);
        assert_eq!(report.summary.files_found, 2);
        assert_eq!(report.entries.len(), 4);
        let alpha = alpha.canonicalize().unwrap();
        let beta = beta.canonicalize().unwrap();
        assert!(report
            .entries
            .iter()
            .all(|entry| entry.project_root == alpha || entry.project_root == beta));
    }

    #[test]
    fn malformed_entries_report_diagnostics_without_hiding_valid_entries() {
        let temp = TempDir::new().unwrap();
        let queue = "# Papercuts\n\n## Open\n\n### [?] Broken\n### [ ] Valid — 2026-08-09\n- Friction: found\n- Impact: repeat\n- Surface: docs\n";
        project(temp.path(), "alpha", Some(queue));
        let report = discover(temp.path(), false).unwrap();
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.diagnostics.len(), 2);
    }

    #[test]
    fn add_creates_queue_then_rejects_normalized_open_duplicate() {
        let temp = TempDir::new().unwrap();
        let root = project(temp.path(), "alpha", None);
        let draft = PapercutDraft {
            title: "Slow graph".to_owned(),
            friction: "noisy".to_owned(),
            impact: "repeat".to_owned(),
            possible_fix: "refresh".to_owned(),
            surface: "graph".to_owned(),
        };
        let added = add(&root, "2026-08-09", &draft).unwrap();
        assert_eq!(added.title, "Slow graph");
        assert!(root.join(QUEUE_FILE).is_file());
        assert!(!root.join(".PAPERCUTS.md.effigy.lock").exists());

        let duplicate = PapercutDraft {
            title: "  SLOW   GRAPH ".to_owned(),
            ..draft
        };
        assert!(matches!(
            add(&root, "2026-08-09", &duplicate),
            Err(PapercutsError::DuplicateOpenTitle { .. })
        ));
    }

    #[test]
    fn add_rejects_collection_without_writing() {
        let temp = TempDir::new().unwrap();
        project(temp.path(), "alpha", None);
        let draft = PapercutDraft {
            title: "Cut".to_owned(),
            friction: "f".to_owned(),
            impact: "i".to_owned(),
            possible_fix: "x".to_owned(),
            surface: "s".to_owned(),
        };
        assert!(matches!(
            add(temp.path(), "2026-08-09", &draft),
            Err(PapercutsError::CollectionAdd { .. })
        ));
    }
}
