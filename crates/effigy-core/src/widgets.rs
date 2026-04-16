//! Shared UI widget data types.
//!
//! These types are used by both the runner's rendering layer and domain
//! crates that produce display-ready projections. Moving them here allows
//! domain crates (effigy-demo, effigy-doctor, etc.) to build key-value
//! lists and table specs without depending on the runner's UI module.

/// Notice severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Step execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Done,
    Failed,
}

/// A titled text block with optional hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageBlock {
    pub title: String,
    pub body: String,
    pub hint: Option<String>,
}

impl MessageBlock {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// A key-value pair for structured display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Summary counts for a check or scan result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryCounts {
    pub ok: usize,
    pub warn: usize,
    pub err: usize,
}

/// Table specification with headers and rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSpec {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl TableSpec {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self { headers, rows }
    }
}
