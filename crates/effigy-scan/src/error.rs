use effigy_manifest::ManifestError;

/// Narrow error boundary for the scan engine.
///
/// Produced by option loading, validation, traversal, and execution.
/// Lifts to `RunnerError` at the runner edge via
/// `From<ScanError> for RunnerError` in `src/runner/error.rs`. Matches
/// the Job-8 pattern used by `effigy-process`, `effigy-ui`,
/// `effigy-managed`, `effigy-env`, and `effigy-routing`.
///
/// The initial shape is a single-variant newtype covering the 20
/// `task_invocation`-style call sites that existed pre-extraction. A
/// richer enum (`InvalidGlob`, `ReadFailed`, `ValidateBounds`, etc.)
/// is the natural first refactor inside the crate and can land
/// incrementally without breaking the From impl.
#[derive(Debug)]
pub enum ScanError {
    Invocation(String),
    Manifest(ManifestError),
}

impl ScanError {
    pub fn invocation(message: impl Into<String>) -> Self {
        Self::Invocation(message.into())
    }
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Invocation(message) => write!(f, "{message}"),
            ScanError::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<ManifestError> for ScanError {
    fn from(value: ManifestError) -> Self {
        ScanError::Manifest(value)
    }
}
