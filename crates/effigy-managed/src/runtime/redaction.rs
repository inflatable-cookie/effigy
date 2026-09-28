pub(super) fn redact_managed_output(value: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|secret| !secret.is_empty())
        .fold(value.to_owned(), |output, secret| {
            output.replace(secret, "[REDACTED]")
        })
}

pub(super) struct SecretStreamRedactor {
    secrets: Vec<Vec<u8>>,
    max_secret_len: usize,
    pending: Vec<u8>,
}

impl SecretStreamRedactor {
    pub(super) fn new(secrets: &[String]) -> Self {
        let secrets = secrets
            .iter()
            .filter(|secret| !secret.is_empty())
            .map(|secret| secret.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let max_secret_len = secrets.iter().map(Vec::len).max().unwrap_or_default();
        Self {
            secrets,
            max_secret_len,
            pending: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        let safe_len = self
            .pending
            .len()
            .saturating_sub(self.max_secret_len.saturating_sub(1));
        self.redact_prefix(safe_len)
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        self.redact_prefix(self.pending.len())
    }

    fn redact_prefix(&mut self, safe_len: usize) -> Vec<u8> {
        let mut output = Vec::new();
        let mut consumed = 0;
        while consumed < safe_len {
            let matching_secret = self
                .secrets
                .iter()
                .filter(|secret| self.pending[consumed..].starts_with(secret))
                .max_by_key(|secret| secret.len());
            if let Some(secret) = matching_secret {
                output.extend_from_slice(b"[REDACTED]");
                consumed += secret.len();
            } else {
                output.push(self.pending[consumed]);
                consumed += 1;
            }
        }
        self.pending.drain(..consumed);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{redact_managed_output, SecretStreamRedactor};

    #[test]
    fn managed_output_redacts_all_secret_values() {
        let secrets = vec!["first-secret".to_owned(), "second-secret".to_owned()];
        assert_eq!(
            redact_managed_output("first-secret and second-secret", &secrets),
            "[REDACTED] and [REDACTED]"
        );
    }

    #[test]
    fn stream_redactor_hides_values_split_across_chunks() {
        let mut redactor = SecretStreamRedactor::new(&["secret-value".to_owned()]);
        let mut output = redactor.push(b"before secret-");
        output.extend(redactor.push(b"value after"));
        output.extend(redactor.finish());

        assert_eq!(output, b"before [REDACTED] after");
    }
}
