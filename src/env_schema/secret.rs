#[cfg(test)]
use zeroize::Zeroize;
use zeroize::Zeroizing;

/// A string value that is zeroized from memory on drop.
///
/// Used for environment variables marked `@sensitive`. The value is never
/// exposed through `Display` or `Debug` — callers must use `expose()` to
/// access the inner value explicitly.
#[derive(Clone)]
pub struct SecretString {
    inner: Zeroizing<String>,
}

impl SecretString {
    pub fn new(value: String) -> Self {
        Self {
            inner: Zeroizing::new(value),
        }
    }

    /// Access the secret value. Use this only at the point where the value
    /// must be passed to an external system (e.g., `Command::env()`).
    pub fn expose(&self) -> &str {
        &self.inner
    }

    #[cfg(test)]
    pub(crate) fn zeroize_for_test(&mut self) {
        self.inner.zeroize();
    }

    #[cfg(test)]
    pub(crate) fn bytes_for_test(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    #[cfg(test)]
    pub(crate) fn raw_bytes_for_test(&self, len: usize) -> &[u8] {
        assert!(len <= self.inner.capacity());
        // Inspect the still-owned buffer after zeroization without relying on
        // the post-zeroize string length, which may be reset to 0.
        unsafe { std::slice::from_raw_parts(self.inner.as_ptr(), len) }
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
#[derive(Debug, Clone)]
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
