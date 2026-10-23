//! Lexical spawn inputs for nested steps. No process-global environment writes.
use super::process::EnvOverride;
use effigy_env::secret::SecretString;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

struct Inputs {
    env: Vec<(String, EnvOverride)>,
    capture: bool,
    deadline: Option<Instant>,
}
thread_local! {
    static SCOPES: RefCell<Vec<Inputs>> = const { RefCell::new(Vec::new()) };
}

pub(in crate::runner) struct SpawnScope;
impl SpawnScope {
    pub(in crate::runner) fn enter(
        env: &BTreeMap<String, String>,
        secrets: Option<&[(&str, &SecretString)]>,
        capture: bool,
        timeout_ms: Option<u64>,
    ) -> Self {
        let entries = env
            .iter()
            .map(|(key, value)| (key.clone(), EnvOverride::Plain(value.clone())))
            .chain(secrets.unwrap_or_default().iter().map(|(key, value)| {
                (
                    (*key).to_owned(),
                    EnvOverride::Secret(value.expose().to_owned()),
                )
            }))
            .collect();
        SCOPES.with(|scopes| {
            scopes.borrow_mut().push(Inputs {
                env: entries,
                capture,
                deadline: timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms)),
            })
        });
        Self
    }
}
impl Drop for SpawnScope {
    fn drop(&mut self) {
        SCOPES.with(|scopes| {
            scopes.borrow_mut().pop();
        });
    }
}
pub(super) fn entries() -> Vec<(String, EnvOverride)> {
    SCOPES.with(|scopes| {
        scopes
            .borrow()
            .iter()
            .flat_map(|inputs| inputs.env.iter().cloned())
            .collect()
    })
}
pub(in crate::runner) fn nested_capture() -> bool {
    SCOPES.with(|scopes| scopes.borrow().last().is_some_and(|inputs| inputs.capture))
}

pub(in crate::runner) fn spawn_plain_env() -> BTreeMap<String, String> {
    entries()
        .into_iter()
        .filter_map(|(key, value)| match value {
            EnvOverride::Plain(value) => Some((key, value)),
            _ => None,
        })
        .collect()
}

pub(in crate::runner) fn remaining() -> Option<Duration> {
    SCOPES.with(|scopes| {
        scopes
            .borrow()
            .iter()
            .filter_map(|inputs| inputs.deadline)
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()))
    })
}
