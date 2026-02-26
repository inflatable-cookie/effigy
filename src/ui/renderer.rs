use std::fmt::{Display, Formatter};

use crate::ui::widgets::{
    KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts, TableSpec,
};

pub type UiResult<T> = Result<T, UiError>;

#[derive(Debug)]
pub enum UiError {
    Io(std::io::Error),
}

impl Display for UiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for UiError {}

impl From<std::io::Error> for UiError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub trait SpinnerHandle {
    fn set_message(&self, message: &str);
    fn finish_success(&self, message: &str);
    fn finish_error(&self, message: &str);
}

pub trait Renderer {
    fn section(&mut self, title: &str) -> UiResult<()>;
    fn notice(&mut self, level: NoticeLevel, body: &str) -> UiResult<()>;
    fn bullet_list(&mut self, title: &str, items: &[String]) -> UiResult<()>;

    fn success_block(&mut self, block: &MessageBlock) -> UiResult<()>;
    fn error_block(&mut self, block: &MessageBlock) -> UiResult<()>;
    fn warning_block(&mut self, block: &MessageBlock) -> UiResult<()>;

    fn key_values(&mut self, items: &[KeyValue]) -> UiResult<()>;
    fn step(&mut self, label: &str, state: StepState) -> UiResult<()>;
    fn summary(&mut self, counts: SummaryCounts) -> UiResult<()>;

    fn table(&mut self, spec: &TableSpec) -> UiResult<()>;
    fn spinner(&mut self, label: &str) -> UiResult<Box<dyn SpinnerHandle>>;
}
