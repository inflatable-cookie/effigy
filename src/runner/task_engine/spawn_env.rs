//! Lexical spawn inputs for nested steps. No process-global environment writes.
use super::process::EnvOverride;
use effigy_env::secret::SecretString;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(in crate::runner) struct LaunchInputs {
    env: Vec<(String, EnvOverride)>,
    capture: bool,
    deadline: Option<Instant>,
}
thread_local! {
    static SCOPES: RefCell<Vec<LaunchInputs>> = const { RefCell::new(Vec::new()) };
}

pub(in crate::runner) struct SpawnScope;
impl SpawnScope {
    /// Carry exact environment provenance and the absolute parent deadline
    /// across an embedded execution boundary. Never restart a timeout here.
    pub(in crate::runner) fn capture() -> LaunchInputs {
        SCOPES
            .with(|scopes| scopes.borrow().last().cloned())
            .unwrap_or_else(|| LaunchInputs {
                env: Vec::new(),
                capture: false,
                deadline: None,
            })
    }

    pub(in crate::runner) fn inherit(inputs: &LaunchInputs) -> Self {
        SCOPES.with(|scopes| scopes.borrow_mut().push(inputs.clone()));
        Self
    }

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
            .collect::<Vec<_>>();
        let mut inputs = Self::capture();
        inputs.env = inputs
            .env
            .into_iter()
            .chain(entries)
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect();
        inputs.capture = capture;
        inputs.deadline = inputs
            .deadline
            .into_iter()
            .chain(timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms)))
            .min();
        SCOPES.with(|scopes| scopes.borrow_mut().push(inputs));
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
            .last()
            .map(|inputs| inputs.env.clone())
            .unwrap_or_default()
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
            .last()
            .and_then(|inputs| inputs.deadline)
            .map(|at| at.saturating_duration_since(Instant::now()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_launch_keeps_absolute_deadline_and_environment_precedence() {
        let parent_at = Instant::now() + Duration::from_secs(10);
        let parent = LaunchInputs {
            env: vec![("KEY".into(), EnvOverride::Secret("parent".into()))],
            capture: true,
            deadline: Some(parent_at),
        };
        let parent_scope = SpawnScope::inherit(&parent);
        let inherited = SpawnScope::capture();
        drop(parent_scope);
        let _boundary = SpawnScope::inherit(&inherited);
        assert_eq!(SpawnScope::capture().deadline, Some(parent_at));
        for timeout in [1000, 20000] {
            let _child = SpawnScope::enter(
                &BTreeMap::from([("KEY".into(), "child".into())]),
                None,
                true,
                Some(timeout),
            );
            let snapshot = SpawnScope::capture();
            let at = snapshot.deadline.unwrap();
            if timeout == 1000 {
                assert!(at < parent_at);
            } else {
                assert_eq!(at, parent_at);
            }
            assert_eq!(snapshot.env.len(), 1);
            assert!(
                matches!(&snapshot.env[0], (key, EnvOverride::Plain(value)) if key == "KEY" && value == "child")
            );
            assert!(snapshot.capture);
        }
        assert!(
            matches!(&SpawnScope::capture().env[0].1, EnvOverride::Secret(value) if value == "parent")
        );
        assert_eq!(SpawnScope::capture().deadline, Some(parent_at));
    }
}
