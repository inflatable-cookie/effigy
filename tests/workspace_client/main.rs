//! Contract 052 §8.11 against the real channel, session and kernel journal.
//! Only transport and host-engine effects are faked. No live engine is used.

#[cfg(feature = "workspace-serving-fixture")]
mod host;
mod protocol;
mod session;
mod support;
