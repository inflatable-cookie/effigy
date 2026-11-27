//! When the scoped host-to-guest file-event relay runs, and who owns it.
//!
//! Attached commands hold a session relay that ends with them. A detached
//! stack with `fs_events = "on"` owns one kernel-supervised relay process,
//! stopped before its stack goes down. One environment never has two: the
//! holder of its relay lock is the only watcher.

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use effigy_containers::fs_relay::detect::{
    resolve_fs_events_activation, ActivationContext, Backend, FsEventsDecision, FsEventsState,
    Invocation,
};
use effigy_containers::fs_relay::host::{
    observe_fs_relay_backend, start_fs_relay, FsRelaySession, FsRelayStart,
};
use effigy_containers::fs_relay::state::{
    clear_dead_fs_relay_record, fs_events_status, stack_relay_published, FsEventsStatus, RelayOwner,
};
use effigy_containers::{EffectiveAttachMode, EffectiveContainerPolicy, EffectiveFsEventsMode};

use super::error::RunnerError;

/// How often a stack relay retries while an attached session holds the
/// environment's relay.
const TAKEOVER_POLL: Duration = Duration::from_millis(500);

/// How long `container up --detach` waits for a freshly started stack relay to
/// publish its record, so a `container status` run right after reads it.
const STACK_RELAY_SETTLE: Duration = Duration::from_secs(3);

/// How often that wait looks for the record.
const STACK_RELAY_SETTLE_POLL: Duration = Duration::from_millis(40);

/// How the current command uses the environment.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct RelayInvocation {
    pub(in crate::runner) invocation: Invocation,
    /// A client stays with the environment for as long as the relay would run.
    pub(in crate::runner) attached: bool,
    pub(in crate::runner) interactive_tty: bool,
}

impl RelayInvocation {
    /// A command attached to this process's own terminal.
    pub(in crate::runner) fn terminal(invocation: Invocation) -> Self {
        Self {
            invocation,
            attached: true,
            interactive_tty: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        }
    }

    fn detached_up() -> Self {
        Self {
            invocation: Invocation::ContainerUp,
            attached: false,
            interactive_tty: false,
        }
    }
}

/// The activation decision. The runtime is probed for its backend only when
/// the configuration and invocation could activate a relay at all.
pub(in crate::runner) fn decide(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    invocation: RelayInvocation,
) -> FsEventsDecision {
    let mut context = ActivationContext {
        inside_guest: super::container_runtime::inside_container_handoff(),
        backend: Backend::Other,
        invocation: invocation.invocation,
        attached: invocation.attached,
        interactive_tty: invocation.interactive_tty,
        headless: !invocation.interactive_tty,
        ci: std::env::var_os("CI").is_some(),
        ephemeral: matches!(
            effigy_core::worktree_scope::scope_kind(repo_root),
            Ok(Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone))
        ),
    };
    let decision = resolve_fs_events_activation(&policy.fs_events, &context);
    if decision.state != FsEventsState::Active {
        return decision;
    }
    context.backend = observe_fs_relay_backend(repo_root, policy);
    resolve_fs_events_activation(&policy.fs_events, &context)
}

/// One environment's relay as `container status` and `doctor` report it.
pub(in crate::runner) fn observed_status(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> FsEventsStatus {
    let mut status = fs_events_status(policy);
    if status.reason == "not_running" && policy.fs_events.mode != EffectiveFsEventsMode::Off {
        status = status.with_backend(observe_fs_relay_backend(repo_root, policy));
    }
    if unsettled_relay_run(repo_root, policy).is_some() {
        status.state = "interrupted".to_owned();
        status.reason = "relay_run_interrupted".to_owned();
    }
    status
}

/// The stack relay run that never settled: its supervisor is gone, or its
/// journal cannot say.
pub(in crate::runner) fn unsettled_relay_run(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Option<String> {
    crate::runner::host_process::stack_fs_relay_unsettled_run(repo_root, policy)
}

/// Start the relay an attached command owns, when one applies and none is
/// already running. The relay is an aid: a failure warns and the command goes
/// on.
pub(in crate::runner) fn start_session(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    invocation: RelayInvocation,
) -> Option<FsRelaySession> {
    let decision = decide(repo_root, policy, invocation);
    if decision.state != FsEventsState::Active
        || crate::runner::host_process::stack_fs_relay_running(repo_root, policy)
    {
        return None;
    }
    match start_fs_relay(repo_root, policy, RelayOwner::Session, decision.reason) {
        Ok(FsRelayStart::Started(session)) => {
            if session.target_count() > 0 {
                eprintln!(
                    "[fs-events] relaying host file events into `{}` for this session",
                    policy.name
                );
            } else if policy.fs_events.mode == EffectiveFsEventsMode::On {
                eprintln!(
                    "[warn] `{}` has no writable bind mount to relay file events into",
                    policy.name
                );
            }
            Some(session)
        }
        Ok(FsRelayStart::AlreadyRunning) => None,
        Err(error) => {
            eprintln!(
                "[warn] file-event relay for `{}` did not start: {error}",
                policy.name
            );
            None
        }
    }
}

/// After a successful bring-up, make the stack-owned relay match it: a
/// detached `on` environment gets a fresh supervised relay over the containers
/// now running; anything else leaves none behind. Returns a warning when the
/// relay could not be brought to that state.
pub(in crate::runner) fn reconcile_stack_relay(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    attach_mode: EffectiveAttachMode,
) -> Option<String> {
    let wanted = attach_mode == EffectiveAttachMode::Detached
        && decide(repo_root, policy, RelayInvocation::detached_up()).state == FsEventsState::Active;
    if !wanted {
        stop_stack_relay(repo_root, policy);
        return None;
    }
    match crate::runner::host_process::start_stack_fs_relay(repo_root, policy) {
        Ok(()) => {
            await_stack_relay(repo_root, policy, STACK_RELAY_SETTLE);
            None
        }
        Err(error) => Some(format!(
            "file-event relay for `{}` did not start: {error}",
            policy.name
        )),
    }
}

/// Wait until the stack relay publishes its record, its run ends, or the budget
/// runs out. A timeout is not a failure: the relay is an aid, and `container
/// status` reports whatever it finds.
fn await_stack_relay(repo_root: &Path, policy: &EffectiveContainerPolicy, budget: Duration) {
    await_settled(budget, STACK_RELAY_SETTLE_POLL, || {
        stack_relay_published(repo_root, &policy.name)
            || crate::runner::host_process::stack_fs_relay_ended(repo_root, policy)
    });
}

/// Poll `settled` until it holds or `budget` passes. Reports whether it held.
fn await_settled(budget: Duration, poll: Duration, mut settled: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if settled() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(poll);
    }
}

/// Stop the stack-owned relay, if one runs. Called before the stack goes down;
/// a stop that cannot be confirmed is reported and never blocks the teardown.
pub(in crate::runner) fn stop_stack_relay(repo_root: &Path, policy: &EffectiveContainerPolicy) {
    match crate::runner::host_process::stop_stack_fs_relay(repo_root, policy) {
        Ok(lines) => {
            for line in lines {
                eprintln!("{line}");
            }
        }
        Err(error) => eprintln!(
            "[warn] file-event relay for `{}` could not be confirmed stopped: {error}",
            policy.name
        ),
    }
    clear_dead_fs_relay_record(repo_root, &policy.name);
}

/// Stop the stack relays of one checkout before its containers in `projects`,
/// on `profile` when only one is being cleared, are removed without an
/// environment stop.
pub(in crate::runner) fn stop_checkout_stack_relays(
    checkout: &Path,
    projects: &BTreeSet<String>,
    profile: Option<&str>,
) {
    // A checkout whose manifest no longer resolves has no environment to name;
    // retirement reports that on its own path.
    let Ok(policies) = effigy_containers::load_all_container_policies(checkout) else {
        return;
    };
    for policy in policies.iter().filter(|policy| {
        projects.contains(&policy.project_name)
            && profile.is_none_or(|profile| policy.profile == profile)
    }) {
        stop_stack_relay(checkout, policy);
    }
}

/// The hidden `container <NAME> __fs-relay` entry: the stack relay's own
/// process. It relays until its supervisor signals it to stop.
pub(in crate::runner) fn run_stack_relay(
    repo_root: PathBuf,
    name: Option<&str>,
) -> Result<String, RunnerError> {
    let policy = effigy_containers::load_container_policy(&repo_root, name)?;
    if super::container_runtime::inside_container_handoff()
        || policy.fs_events.mode != EffectiveFsEventsMode::On
    {
        // A clean exit: the supervisor restarts only a failed relay, so a
        // manifest that no longer asks for one ends it.
        return Ok(format!(
            "container `{}` does not run a stack-owned file-event relay",
            policy.name
        ));
    }
    let stop = effigy_runtime::signals::install_stop_requested_flag().map_err(RunnerError::from)?;
    let mut session = None;
    while !stop.load(Ordering::Relaxed) {
        if session.is_none() {
            match start_fs_relay(&repo_root, &policy, RelayOwner::Stack, "explicit_on") {
                Ok(FsRelayStart::Started(started)) => {
                    println!(
                        "[fs-events] stack relay for `{}` holds {} mount root(s)",
                        policy.name,
                        started.target_count()
                    );
                    session = Some(started);
                }
                Ok(FsRelayStart::AlreadyRunning) => {}
                // A failed exit lets the supervisor retry with its backoff.
                Err(error) => {
                    return Err(RunnerError::task_invocation(format!(
                        "file-event relay for `{}` did not start: {error}",
                        policy.name
                    )))
                }
            }
        }
        std::thread::sleep(if session.is_some() {
            Duration::from_millis(100)
        } else {
            TAKEOVER_POLL
        });
    }
    drop(session);
    Ok(String::new())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use effigy_containers::fs_relay::state::{
        fs_relay_record_path, RelayRecord, FS_RELAY_RECORD_SCHEMA,
    };
    use std::fs;

    fn publish(repo: &Path, owner: RelayOwner) {
        let record = RelayRecord {
            schema: FS_RELAY_RECORD_SCHEMA.to_owned(),
            container: "web".to_owned(),
            owner,
            pid: std::process::id(),
            state: "active".to_owned(),
            reason: "explicit_on".to_owned(),
            targets: Vec::new(),
        };
        let path = fs_relay_record_path(repo, "web");
        fs::create_dir_all(path.parent().expect("record directory")).expect("directory");
        fs::write(path, serde_json::to_vec(&record).expect("record JSON")).expect("record");
    }

    #[test]
    fn settle_wait_returns_once_the_stack_relay_record_is_visible() {
        let dir = tempfile::tempdir().expect("fixture");
        let repo = dir.path().to_path_buf();
        // A session's record does not settle a stack relay.
        publish(&repo, RelayOwner::Session);
        assert!(!await_settled(
            Duration::from_millis(100),
            Duration::from_millis(10),
            || stack_relay_published(&repo, "web")
        ));
        fs::remove_file(fs_relay_record_path(&repo, "web")).expect("clear");

        let writer = {
            let repo = repo.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200));
                publish(&repo, RelayOwner::Stack);
            })
        };
        let started = Instant::now();
        assert!(await_settled(
            Duration::from_secs(3),
            Duration::from_millis(10),
            || stack_relay_published(&repo, "web")
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        writer.join().expect("writer");
    }

    #[test]
    fn settle_wait_gives_up_at_its_budget_without_failing() {
        let dir = tempfile::tempdir().expect("fixture");
        let started = Instant::now();
        assert!(!await_settled(
            Duration::from_millis(150),
            Duration::from_millis(10),
            || stack_relay_published(dir.path(), "web")
        ));
        assert!(started.elapsed() >= Duration::from_millis(150));
    }
}
