//! Narrow error boundary for the tasks crate.
//!
//! Two failure classes stay distinct: `Message` is a refused selector, plan or
//! inventory request; `Render` is a renderer or JSON encoder refusal carrying
//! the typed [`effigy_ui::UiError`]. The runner edge lifts them to
//! `RunnerError::TaskInvocation` and `RunnerError::Ui` respectively, so the
//! class survives the crate boundary.

#[derive(Debug)]
pub enum EffigyTasksError {
    /// A request was refused. The text names the offending value and the
    /// shape the caller must supply instead.
    Message(String),
    /// Output could not be rendered or encoded.
    Render(effigy_ui::UiError),
}

impl EffigyTasksError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    /// True when the failure came from the renderer or JSON encoder rather
    /// than from a refused request.
    pub fn is_render_failure(&self) -> bool {
        matches!(self, Self::Render(_))
    }
}

impl std::fmt::Display for EffigyTasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Render(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for EffigyTasksError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Message(_) => None,
            Self::Render(error) => Some(error),
        }
    }
}

impl From<effigy_ui::UiError> for EffigyTasksError {
    fn from(value: effigy_ui::UiError) -> Self {
        Self::Render(value)
    }
}

#[cfg(test)]
mod tests {
    use super::EffigyTasksError;
    use effigy_ui::UiError;

    /// A renderer refusal keeps its typed cause, so the caller can branch on
    /// the class instead of matching message text. Fails before the `Render`
    /// variant existed: the cause arrived as an opaque `Message`.
    #[test]
    fn render_failures_keep_the_typed_ui_cause() {
        let error = EffigyTasksError::from(UiError::Encoding("encode refused".to_owned()));
        assert!(
            matches!(
                error,
                EffigyTasksError::Render(UiError::Encoding(ref message))
                    if message == "encode refused"
            ),
            "the UI failure class and its text travel together: {error:?}"
        );
        assert!(error.is_render_failure());
        assert_eq!(error.to_string(), "encode refused");
        assert!(
            std::error::Error::source(&error).is_some(),
            "the typed cause is reachable through source()"
        );
    }

    /// A refused request stays a message: it has no typed cause to chain.
    #[test]
    fn refused_requests_carry_their_actionable_text() {
        let error = EffigyTasksError::message("qa-group selector is empty");
        assert!(!error.is_render_failure());
        assert_eq!(error.to_string(), "qa-group selector is empty");
        assert!(std::error::Error::source(&error).is_none());
    }
}
