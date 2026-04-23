#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffigyTasksError {
    Message(String),
}

impl EffigyTasksError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl std::fmt::Display for EffigyTasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for EffigyTasksError {}

impl From<effigy_ui::UiError> for EffigyTasksError {
    fn from(value: effigy_ui::UiError) -> Self {
        Self::message(value.to_string())
    }
}
