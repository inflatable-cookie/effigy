//! Test-only observation fault point. Thread-local scope isolates strict fakes.
use crate::observe::{Observed, UnknownReason};
use std::cell::Cell;
thread_local! { static UNKNOWN: Cell<bool> = const { Cell::new(false) }; }
pub(crate) fn injected<T>() -> Option<Observed<T>> {
    UNKNOWN.with(|flag| {
        flag.get()
            .then_some(Observed::Unknown(UnknownReason::Injected))
    })
}
#[test]
fn unknown_injected_at_every_platform_observation_grants_no_fact() {
    UNKNOWN.with(|flag| flag.set(true));
    assert!(matches!(
        crate::ident::boot_identity(),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        crate::ident::process_identity(std::process::id()),
        Observed::Unknown(UnknownReason::Injected)
    ));
    assert!(matches!(
        crate::ident::boot_clock(),
        Observed::Unknown(UnknownReason::Injected)
    ));
    UNKNOWN.with(|flag| flag.set(false));
}
