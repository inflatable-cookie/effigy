//! Small, fail-closed client for Nucleus host-run Client protocol v1.
//!
//! This crate only discovers and speaks to a trusted scheduler. It does not
//! select tasks, admit work, or change Effigy's current execution path.

mod identity;
mod secure_fs;
mod token;
mod transport;

pub use identity::{canonical_start_identity, start_identity_matches};
pub use secure_fs::{Authority, HostRunRoot, TrustError};
pub use token::{ParentToken, TokenError, TokenKeys};
pub use transport::{
    container_removed_fact, container_started_fact, journal_facts_offline, nested_fact,
    new_client_request_id, new_fact, override_fact, validate_client_caller,
    validate_client_request_id, AttachEvent, BudgetFallback, ClassSource, ClientError, Clock,
    HostRunClient, HostRunFact, IdentityProvider, OutputStream, Priority, RunClass, Settlement,
    SettlementOutcome, StatusQuery, SubmitRequest, SubmitResult, SystemClock,
    SystemIdentityProvider, WireError,
};

/// Maximum encoded NDJSON frame size, including the newline.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Maximum decoded output payload in one attach event.
pub const MAX_OUTPUT_CHUNK_BYTES: usize = 64 * 1024;
/// Maximum local authority/key file size accepted during discovery.
pub const MAX_LOCAL_FILE_BYTES: usize = 64 * 1024;

#[cfg(test)]
mod protocol_tests;
