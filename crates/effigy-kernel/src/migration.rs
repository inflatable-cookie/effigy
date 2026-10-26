//! Checkout location resolution shared by the journal's callers.
use std::path::PathBuf;

/// The caller resolves Git metadata; the kernel holds both directory
/// generations through publication and persists their facts in the local
/// binding.
#[derive(Debug, Clone)]
pub struct CheckoutLocation {
    pub root: PathBuf,
    pub git_directory: Option<PathBuf>,
}
