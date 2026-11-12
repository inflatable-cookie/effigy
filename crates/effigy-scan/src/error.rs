use effigy_manifest::ManifestError;

/// Narrow error boundary for the scan engine.
///
/// Produced by option loading, validation, traversal, and execution.
/// Lifts to `RunnerError` at the runner edge via
/// `From<ScanError> for RunnerError` in `src/runner/error.rs`. Matches
/// the Job-8 pattern used by `effigy-process`, `effigy-ui`,
/// `effigy-managed`, `effigy-env`, and `effigy-routing`.
///
/// `BudgetExhausted` is the typed budget-stop signal the doctor inventory
/// reports when its deadline expires mid-walk: callers match on the variant
/// instead of substring-matching the rendered message.
#[derive(Debug)]
pub enum ScanError {
    Invocation(String),
    Manifest(ManifestError),
    BudgetExhausted { phase: String },
}

impl ScanError {
    pub fn invocation(message: impl Into<String>) -> Self {
        Self::Invocation(message.into())
    }

    pub fn budget_exhausted(phase: impl Into<String>) -> Self {
        Self::BudgetExhausted {
            phase: phase.into(),
        }
    }

    /// Typed budget-stop predicate: true only for the `BudgetExhausted`
    /// variant, never for a message substring.
    pub fn is_budget_exhausted(&self) -> bool {
        matches!(self, Self::BudgetExhausted { .. })
    }
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Invocation(message) => write!(f, "{message}"),
            ScanError::Manifest(error) => write!(f, "{error}"),
            ScanError::BudgetExhausted { phase } => {
                write!(f, "doctor scan budget exhausted during {phase}")
            }
        }
    }
}

impl std::error::Error for ScanError {}

impl From<ManifestError> for ScanError {
    fn from(value: ManifestError) -> Self {
        ScanError::Manifest(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_exhausted_matches_by_type_not_substring() {
        // The doctor inventory maps the typed variant; a plain invocation
        // whose message happens to mention a budget must not match.
        let typed = ScanError::budget_exhausted("inventory");
        assert!(typed.is_budget_exhausted());
        assert!(typed.to_string().contains("budget exhausted"));
        let lookalike =
            ScanError::invocation("inventory notes: budget exhausted (historical)");
        assert!(!lookalike.is_budget_exhausted());
    }
}
