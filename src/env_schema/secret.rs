use zeroize::Zeroize;

/// A string value that is zeroized from memory on drop.
///
/// Used for environment variables marked `@sensitive`. The value is never
/// exposed through `Display` or `Debug` — callers must use `expose()` to
/// access the inner value explicitly.
pub struct SecretString {
    inner: String,
}

impl SecretString {
    pub fn new(value: String) -> Self {
        Self { inner: value }
    }

    /// Access the secret value. Use this only at the point where the value
    /// must be passed to an external system (e.g., `Command::env()`).
    pub fn expose(&self) -> &str {
        &self.inner
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretString(****)")
    }
}

impl std::fmt::Display for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

/// A resolved environment value, either plain text or a secret.
#[derive(Debug)]
pub enum ResolvedValue {
    Plain(String),
    Secret(SecretString),
}

impl ResolvedValue {
    /// Get the string value regardless of sensitivity. For secrets, this
    /// calls `expose()` internally.
    pub fn as_str(&self) -> &str {
        match self {
            ResolvedValue::Plain(s) => s,
            ResolvedValue::Secret(s) => s.expose(),
        }
    }

    pub fn is_secret(&self) -> bool {
        matches!(self, ResolvedValue::Secret(_))
    }
}

impl std::fmt::Display for ResolvedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedValue::Plain(s) => write!(f, "{s}"),
            ResolvedValue::Secret(s) => write!(f, "{s}"),
        }
    }
}

#[cfg(test)]
#[path = "secret/tests.rs"]
mod tests;
