//! Bounded newest-bytes capture shared by the task engine.
//!
//! Single owner of the `Tail` mechanism: `process` (plain host commands)
//! and `remote` (container sessions) both keep one `Tail` per stream so
//! JSON output holds the newest `CAPTURE_TAIL` bytes. The bound lives here
//! with the buffer so the two call sites cannot drift apart.

/// JSON output keeps this much of each stream, newest bytes last.
pub(super) const CAPTURE_TAIL: usize = 8 << 20;

/// A bounded capture that keeps the newest bytes.
#[derive(Default)]
pub(super) struct Tail {
    pub(super) bytes: Vec<u8>,
}

impl Tail {
    pub(super) fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > CAPTURE_TAIL {
            let excess = self.bytes.len() - CAPTURE_TAIL;
            self.bytes.drain(..excess);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.bytes.len()
    }

    #[cfg(test)]
    pub(super) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_only_the_newest_bytes() {
        let mut tail = Tail::default();
        tail.push(b"hello ");
        tail.push(b"world");
        assert_eq!(tail.as_slice(), b"hello world");
        // Push past the bound in one chunk: only the newest CAPTURE_TAIL
        // bytes survive.
        let big = vec![b'x'; CAPTURE_TAIL + 10];
        tail.push(&big);
        assert_eq!(tail.len(), CAPTURE_TAIL);
        assert!(tail.as_slice().iter().all(|byte| *byte == b'x'));
    }
}
