use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum CodeGraphError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    Validation(String),
}

impl CodeGraphError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl Display for CodeGraphError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Sql(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Validation(message) => write!(f, "{message}"),
        }
    }
}

impl Error for CodeGraphError {}

impl From<std::io::Error> for CodeGraphError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for CodeGraphError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl From<serde_json::Error> for CodeGraphError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
