//! Container-mode workspace client.
//!
//! Context, identity and staged guest-probe declarations. The workspace-client
//! owner joins these sources with its serving supervisor and guest client.

pub(crate) mod context;
pub(crate) mod guest_probe;
pub(crate) mod identity;

#[cfg(test)]
pub(crate) mod guest_fake;
