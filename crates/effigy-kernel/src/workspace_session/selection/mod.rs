//! The durable guest selection one activation commits (052 §8.11).
//!
//! An environment's published generation is not only a row: it selects the
//! exact guest bytes a container-mode client executes. This module owns the
//! record of that selection, so the bytes, the credential, the endpoint and the
//! expected previous pointer values are one durable obligation written by the
//! same transaction that publishes `active(E)`, the session mapping and the
//! session's `Borrow` attachment. There is therefore no window in which a
//! generation is advertised and no record of what it selected, and no
//! transaction spans guest I/O: staging writes no selector, and the selector
//! switch follows the commit and is recorded by `Attachments::selection_switched`
//! (or, after a crash, by `Attachments::selection_recovered`).
//!
//! Nothing here hashes a secret or stores one. A selection names objects by
//! digest and a credential by its own reference.
use super::*;

mod context;
mod obligation;

pub use context::{
    AdvertisedLocator, CredentialSelection, GuestObject, GuestSelection, SelectionObjects,
    SelectionReceipt, SelectionSwitchReceipt, SelectorChange, SelectorReadback, SelectorReader,
    WorkspaceContextDocument, WorkspaceSessionBinding, BINARY_SELECTOR, CONTEXT_SELECTOR,
    ENTRY_SELECTOR, WORKSPACE_CONTEXT_SCHEMA,
};
pub use obligation::{
    committed_selection, selection_abandoned, selection_recovered, unfinished_selection,
    SelectionLiveness, SelectionObligation, SelectionState,
};
pub(crate) use obligation::{
    pending_obligation, read_obligation, record_obligation,
};
