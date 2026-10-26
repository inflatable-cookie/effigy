//! Checkout location resolution shared by the journal's callers.
use std::path::PathBuf;

/// The checkout a journal belongs to. The location is the identity: no Git
/// generation is resolved, retained or recorded (Q-011).
#[derive(Debug, Clone)]
pub struct CheckoutLocation {
    pub root: PathBuf,
}
