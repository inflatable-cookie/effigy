use effigy_manifest::{ManifestDemoMode, ManifestDemoStatus, ManifestManagedRun};
use serde_json::{json, Value as JsonValue};

use crate::runtime::{DemoActiveAttempt, DemoActiveTerminalSession, DemoRuntimeBackend};
use crate::{DemoAttemptHistory, DemoHistoricalAttempt, DemoLatestAttempt};

#[derive(Debug, Clone)]
pub struct DemoRecord {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub proof: String,
    pub owner: String,
    pub mode: ManifestDemoMode,
    pub status: ManifestDemoStatus,
    pub covers: Vec<String>,
    pub tags: Vec<String>,
    pub prerequisites: Vec<String>,
    pub dependencies: Vec<String>,
    pub entrypoint: DemoEntrypoint,
    pub sources: Vec<String>,
    pub primary_source: String,
    pub gap_class: &'static str,
    pub runtime_backend: DemoRuntimeBackend,
    pub active_attempt: DemoActiveAttempt,
    pub active_terminal_session: DemoActiveTerminalSession,
    pub latest_attempt: DemoLatestAttempt,
    pub attempt_history: DemoAttemptHistory,
}

impl DemoRecord {
    pub fn effective_status(&self) -> String {
        display_status(self.status, self.latest_attempt.stale, &self.active_attempt)
    }

    pub fn freshness_label(&self) -> &'static str {
        if self.latest_attempt.stale {
            "stale"
        } else {
            "current"
        }
    }

    pub fn actions(&self) -> DemoActionAvailability {
        let can_run = !self.active_attempt.active;
        let can_rerun = !self.active_attempt.active;
        let can_stop = self.active_attempt.active && self.active_attempt.stoppable;
        DemoActionAvailability {
            run_available: can_run,
            run_reason: (!can_run).then(|| {
                "an active attempt already exists; stop it before starting a fresh run".to_owned()
            }),
            stop_available: can_stop,
            stop_reason: if can_stop {
                None
            } else if self.active_attempt.active {
                Some("the active attempt is not stoppable through the current runtime".to_owned())
            } else {
                Some("no active attempt is currently running".to_owned())
            },
            rerun_available: can_rerun,
            rerun_reason: (!can_rerun)
                .then(|| "an active attempt already exists; stop it before rerunning".to_owned()),
        }
    }

    pub fn to_json_summary(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "runtime_backend": self.runtime_backend.to_json(),
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "active_terminal_session": self.active_terminal_session.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
        })
    }

    pub fn to_json_detail(&self) -> JsonValue {
        json!({
            "id": self.id,
            "title": self.title,
            "summary": self.summary,
            "proof": self.proof,
            "owner": self.owner,
            "mode": self.mode.as_str(),
            "status": self.status.as_str(),
            "effective_status": self.effective_status(),
            "freshness": self.freshness_label(),
            "stale": self.latest_attempt.stale,
            "gap_class": self.gap_class,
            "covers": self.covers,
            "tags": self.tags,
            "prerequisites": self.prerequisites,
            "dependencies": self.dependencies,
            "entrypoint": self.entrypoint.to_json(),
            "defined_in": self.primary_source,
            "sources": self.sources,
            "runtime_backend": self.runtime_backend.to_json(),
            "actions": self.actions().to_json(),
            "active_attempt": self.active_attempt.to_json(),
            "active_terminal_session": self.active_terminal_session.to_json(),
            "latest_attempt": self.latest_attempt.to_json(),
            "attempt_history": self.attempt_history.to_json(),
        })
    }

    pub fn matches_filters(
        &self,
        search: Option<&str>,
        owner: Option<&str>,
        tag: Option<&str>,
        mode: Option<&str>,
        cover: Option<&str>,
        status: Option<&str>,
        gap: Option<&str>,
        stale_only: bool,
    ) -> bool {
        if let Some(search) = search {
            let needle = search.to_ascii_lowercase();
            let haystacks = [&self.id, &self.title, &self.summary];
            if !haystacks
                .iter()
                .any(|value| value.to_ascii_lowercase().contains(&needle))
            {
                return false;
            }
        }
        if let Some(owner) = owner {
            if self.owner != owner {
                return false;
            }
        }
        if let Some(tag) = tag {
            if !self.tags.iter().any(|value| value == tag) {
                return false;
            }
        }
        if let Some(mode) = mode {
            if self.mode.as_str() != mode {
                return false;
            }
        }
        if let Some(cover) = cover {
            if !self.covers.iter().any(|value| value == cover) {
                return false;
            }
        }
        if let Some(status) = status {
            if self.browser_status_label() != status {
                return false;
            }
        }
        if let Some(gap) = gap {
            if self.gap_class != gap {
                return false;
            }
        }
        if stale_only && !self.latest_attempt.stale {
            return false;
        }
        true
    }

    pub fn browser_status_label(&self) -> &'static str {
        if self.active_attempt.active {
            return "running";
        }
        self.status.as_str()
    }
}

#[derive(Debug, Clone)]
pub struct DemoActionAvailability {
    pub run_available: bool,
    pub run_reason: Option<String>,
    pub stop_available: bool,
    pub stop_reason: Option<String>,
    pub rerun_available: bool,
    pub rerun_reason: Option<String>,
}

impl DemoActionAvailability {
    pub fn summary_label(&self) -> String {
        let mut actions = Vec::new();
        if self.run_available {
            actions.push("run");
        }
        if self.stop_available {
            actions.push("stop");
        }
        if self.rerun_available {
            actions.push("rerun");
        }
        if actions.is_empty() {
            "none".to_owned()
        } else {
            actions.join(", ")
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "run": {
                "available": self.run_available,
                "reason": self.run_reason,
            },
            "stop": {
                "available": self.stop_available,
                "reason": self.stop_reason,
            },
            "rerun": {
                "available": self.rerun_available,
                "reason": self.rerun_reason,
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct DemoGroup<'a> {
    pub label: String,
    pub demos: Vec<&'a DemoRecord>,
}

impl DemoGroup<'_> {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "label": self.label,
            "count": self.demos.len(),
            "demos": self.demos.iter().map(|demo| demo.to_json_summary()).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DemoRecordGroupBy {
    Owner,
    Tag,
    Mode,
    Cover,
    Status,
    Gap,
}

#[derive(Debug, Clone)]
pub enum DemoEntrypoint {
    Task(String),
    Run(ManifestManagedRun),
}

impl DemoEntrypoint {
    pub fn render_compact(&self) -> String {
        match self {
            Self::Task(task) => format!("task:{task}"),
            Self::Run(run) => format!("run:{}", demo_run_preview(run)),
        }
    }

    pub fn render_full(&self) -> String {
        match self {
            Self::Task(task) => format!("task `{task}`"),
            Self::Run(run) => format!("run `{}`", demo_run_preview(run)),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        match self {
            Self::Task(task) => json!({ "kind": "task", "value": task }),
            Self::Run(run) => json!({ "kind": "run", "value": demo_run_preview(run) }),
        }
    }
}

pub fn history_attempt_to_json(ordinal: usize, attempt: &DemoHistoricalAttempt) -> JsonValue {
    let mut value = attempt.to_json();
    if let Some(object) = value.as_object_mut() {
        object.insert("ordinal".to_owned(), json!(ordinal));
    }
    value
}

pub fn history_attempts_with_outcome<'a>(
    history: &'a DemoAttemptHistory,
    outcome: Option<&str>,
) -> Vec<&'a DemoHistoricalAttempt> {
    history
        .attempts
        .iter()
        .filter(|attempt| {
            outcome
                .map(|value| attempt.outcome == value)
                .unwrap_or(true)
        })
        .collect()
}

pub fn history_attempts_with_limit<'a>(
    attempts: &'a [&'a DemoHistoricalAttempt],
    limit: Option<usize>,
) -> &'a [&'a DemoHistoricalAttempt] {
    let end = limit
        .map(|value| value.min(attempts.len()))
        .unwrap_or(attempts.len());
    &attempts[..end]
}

pub fn find_historical_attempt<'a>(
    attempts: &'a [DemoHistoricalAttempt],
    attempt_id: &str,
) -> Option<&'a DemoHistoricalAttempt> {
    attempts
        .iter()
        .find(|attempt| attempt.attempt_id == attempt_id)
}

pub fn build_demo_groups<'a>(
    demos: &'a [DemoRecord],
    group_by: DemoRecordGroupBy,
) -> Vec<DemoGroup<'a>> {
    let mut groups = std::collections::BTreeMap::<String, Vec<&DemoRecord>>::new();
    for demo in demos {
        match group_by {
            DemoRecordGroupBy::Owner => {
                groups.entry(demo.owner.clone()).or_default().push(demo);
            }
            DemoRecordGroupBy::Tag => {
                if demo.tags.is_empty() {
                    groups
                        .entry("(untagged)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for tag in &demo.tags {
                        groups.entry(tag.clone()).or_default().push(demo);
                    }
                }
            }
            DemoRecordGroupBy::Mode => {
                groups
                    .entry(demo.mode.as_str().to_owned())
                    .or_default()
                    .push(demo);
            }
            DemoRecordGroupBy::Cover => {
                if demo.covers.is_empty() {
                    groups
                        .entry("(unmapped)".to_owned())
                        .or_default()
                        .push(demo);
                } else {
                    for cover in &demo.covers {
                        groups.entry(cover.clone()).or_default().push(demo);
                    }
                }
            }
            DemoRecordGroupBy::Status => {
                groups
                    .entry(demo.effective_status())
                    .or_default()
                    .push(demo);
            }
            DemoRecordGroupBy::Gap => {
                groups
                    .entry(demo.gap_class.to_owned())
                    .or_default()
                    .push(demo);
            }
        }
    }

    groups
        .into_iter()
        .map(|(label, demos)| DemoGroup { label, demos })
        .collect()
}

fn demo_run_preview(run: &ManifestManagedRun) -> String {
    match run {
        ManifestManagedRun::Command(command) => command.clone(),
        ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
    }
}

fn display_status(
    status: ManifestDemoStatus,
    stale: bool,
    active_attempt: &DemoActiveAttempt,
) -> String {
    if active_attempt.active {
        "running".to_owned()
    } else if stale {
        format!("{} (stale)", status.as_str())
    } else {
        status.as_str().to_owned()
    }
}
