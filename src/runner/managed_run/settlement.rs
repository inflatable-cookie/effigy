//! The managed run's own effects and their one settle (contract 052 §5.6,
//! §10.2). The kernel walks a run's effects in reverse; these are the engine
//! effects it hands to the task engine's adapter: the environment the run
//! brought up or borrowed, the remote process groups its exec workloads
//! started, and the gateway it ensured.
//!
//! The environment effect is the run's obligation toward the environment it
//! holds through the reconciler's hold API (contract 053 §4): an owned one is
//! stopped by a reconciler stop that exempts only this run, a borrowed one is
//! retained. The hold itself is the kernel attachment the bring-up's
//! activation recorded; nothing here models holders of its own.

use std::path::{Path, PathBuf};

use effigy_kernel::ctx::{CleanupContext, Interrupt};
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::run::{Effect, Fence, Reconciled};
use serde::{Deserialize, Serialize};

/// The environment the run started (released at settle) or borrowed
/// (retained).
pub(in crate::runner) const ENVIRONMENT: &str = "managed:environment";
/// A remote process group one exec workload started in the environment.
pub(in crate::runner) const EXEC_GROUP: &str = "managed:exec-group";
/// The user gateway the run ensured; never stopped by a run.
pub(in crate::runner) const GATEWAY: &str = "managed:gateway";

/// How the run reaches its environment's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum Binding {
    /// A named `[containers.<name>]` environment, by its selector.
    Named { selector: Option<String> },
    /// A task's inline workspace container, re-resolved from the task.
    Inline { task: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::runner) struct EnvironmentEffect {
    pub(in crate::runner) repo: PathBuf,
    pub(in crate::runner) environment: String,
    pub(in crate::runner) binding: Binding,
    /// The run's settle identity: the down exempts only this run's holders.
    pub(in crate::runner) token: String,
    pub(in crate::runner) borrowed: bool,
    /// The run that holds the environment: its settle's own stop is exempt.
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::runner) struct ExecGroupEffect {
    pub(in crate::runner) repo: PathBuf,
    pub(in crate::runner) binding: Binding,
    pub(in crate::runner) process: String,
    /// Host path of the marker the remote session leader writes.
    pub(in crate::runner) marker: PathBuf,
    /// The marker counts only when it names this effect: a marker left by
    /// an earlier run is never evidence (R3).
    pub(in crate::runner) nonce: String,
}

/// Whether `kind` is a managed engine effect this module settles.
pub(in crate::runner) fn owns(kind: &str) -> bool {
    kind.starts_with("managed:")
}

pub(in crate::runner) fn observe(effect: &Effect, _cx: &CleanupContext) -> Observed<Fence> {
    match effect.spec.kind.as_str() {
        // Ownership is the journal's: the run recorded intent before `up`.
        // The down itself verifies the stack's owned labels before acting.
        ENVIRONMENT => verified_own(effect),
        EXEC_GROUP => match payload::<ExecGroupEffect>(effect).map(|group| marker_pgid(&group)) {
            Ok(Observed::Verified(_, _)) => verified_own(effect),
            Ok(Observed::Absent(proof)) => Observed::Absent(proof),
            Ok(Observed::Unknown(reason)) | Err(reason) => Observed::Unknown(reason),
        },
        GATEWAY => verified_own(effect),
        other => unknown(format!("unknown managed effect `{other}`")),
    }
}

pub(in crate::runner) fn release(effect: &Effect, cx: &CleanupContext) -> Observed<()> {
    let result = match effect.spec.kind.as_str() {
        ENVIRONMENT => {
            payload::<EnvironmentEffect>(effect).and_then(|env| release_environment(&env))
        }
        EXEC_GROUP => payload::<ExecGroupEffect>(effect).and_then(|group| end_exec_group(&group)),
        other => Err(UnknownReason::Inconsistent(format!(
            "managed effect `{other}` has no release"
        ))),
    };
    if let Err(interrupt) = cx.operation().check("managed release") {
        // The interrupt's own sentence is the cause: `Display` names the wait,
        // how long it ran and the phase, so no `Debug` tree reaches a person.
        return Observed::Unknown(match interrupt {
            Interrupt::TimedOut { phase, .. } => UnknownReason::TimedOut { phase },
            other => UnknownReason::Inconsistent(format!("the managed release was {other}")),
        });
    }
    match result {
        Ok(()) => Observed::Verified((), evidence()),
        Err(reason) => Observed::Unknown(reason),
    }
}

pub(in crate::runner) fn reconcile(effect: &Effect, _cx: &CleanupContext) -> Observed<Reconciled> {
    unknown(format!(
        "managed effect `{}` declares no reconcile",
        effect.spec.kind
    ))
}

fn verified_own(effect: &Effect) -> Observed<Fence> {
    Observed::Verified(effect.spec.target.clone(), evidence())
}

fn payload<T: for<'de> Deserialize<'de>>(effect: &Effect) -> Result<T, UnknownReason> {
    serde_json::from_value(effect.spec.payload.clone())
        .map_err(|error| UnknownReason::Unparsable(format!("managed effect payload: {error}")))
}

fn evidence() -> Evidence {
    Evidence::new("managed run journal".to_owned(), None).expect("fixed evidence")
}

fn unknown<T>(reason: String) -> Observed<T> {
    Observed::Unknown(UnknownReason::Inconsistent(reason))
}

/// The run's own stop of the environment: only its own hold is exempt, any
/// other holder refuses (R4).
fn release_environment(env: &EnvironmentEffect) -> Result<(), UnknownReason> {
    let outcome = match &env.binding {
        Binding::Named { selector } => match env.run.as_deref().map(str::parse) {
            Some(Ok(run)) => crate::runner::container_command::release_managed_environment(
                &env.repo,
                selector.as_deref(),
                run,
            ),
            _ => Err(crate::runner::error::RunnerError::task_invocation(
                "the environment effect names no holding run",
            )),
        },
        Binding::Inline { task } => {
            crate::runner::execute::release_inline_managed_environment(&env.repo, task)
        }
    };
    outcome.map(|_| ()).map_err(|error| {
        UnknownReason::Inconsistent(format!(
            "`{}` was not brought down: {error}",
            env.environment
        ))
    })
}

fn end_exec_group(group: &ExecGroupEffect) -> Result<(), UnknownReason> {
    let pgid = match marker_pgid(group) {
        Observed::Verified(pgid, _) => pgid,
        // Nothing started remotely under this effect: nothing to end.
        Observed::Absent(_) => return Ok(()),
        Observed::Unknown(reason) => return Err(reason),
    };
    let policy = policy(&group.repo, &group.binding).map_err(UnknownReason::Inconsistent)?;
    effigy_containers::exec::end_owned_container_process_group(&group.repo, &policy, pgid).map_err(
        |error| {
            UnknownReason::Inconsistent(format!(
                "remote group {pgid} of `{}` was not ended: {error}",
                group.process
            ))
        },
    )
}

/// The effective policy of a run's environment, re-resolved for its settle.
pub(in crate::runner) fn policy(
    repo: &Path,
    binding: &Binding,
) -> Result<effigy_containers::EffectiveContainerPolicy, String> {
    let policy = match binding {
        Binding::Named { selector } => {
            effigy_containers::load_container_policy(repo, selector.as_deref())
                .map(Some)
                .map_err(|error| error.to_string())
        }
        Binding::Inline { task } => crate::runner::execute::inline_managed_policy(repo, task)
            .map_err(|error| error.to_string()),
    };
    match policy {
        Ok(Some(policy)) => Ok(policy),
        Ok(None) => Err("the run's environment no longer resolves".to_owned()),
        Err(error) => Err(format!("the run's environment does not resolve: {error}")),
    }
}

/// The remote group a marker names, only when it carries this effect's
/// nonce. A missing marker is absence: the remote leader never ran.
pub(in crate::runner) fn marker_pgid(group: &ExecGroupEffect) -> Observed<u32> {
    let raw = match std::fs::read_to_string(&group.marker) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Observed::Absent(AbsenceProof::NoFile {
                name: group.marker.display().to_string(),
            })
        }
        Err(error) => {
            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                "exec group marker unreadable: {error}"
            )))
        }
    };
    let mut fields = raw.split_whitespace();
    match (
        fields.next().and_then(|pid| pid.parse::<u32>().ok()),
        fields.next(),
    ) {
        (Some(pgid), Some(nonce)) if pgid > 1 && nonce == group.nonce => {
            Observed::Verified(pgid, evidence())
        }
        (Some(_), Some(nonce)) if nonce == group.nonce => Observed::Unknown(
            UnknownReason::Unparsable("exec group marker names no process group".to_owned()),
        ),
        // An earlier run's marker: this effect's leader never wrote one.
        (Some(_), Some(_)) => Observed::Absent(AbsenceProof::NoFile {
            name: group.marker.display().to_string(),
        }),
        _ => Observed::Unknown(UnknownReason::Unparsable(
            "exec group marker is malformed".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(dir: &Path) -> ExecGroupEffect {
        ExecGroupEffect {
            repo: dir.to_path_buf(),
            binding: Binding::Named { selector: None },
            process: "api".to_owned(),
            marker: dir.join("api.pgid"),
            nonce: "run-1:4".to_owned(),
        }
    }

    #[test]
    fn exec_group_marker_needs_this_effects_nonce() {
        let dir = tempfile::tempdir().expect("marker dir");
        let group = group(dir.path());
        assert!(matches!(marker_pgid(&group), Observed::Absent(_)));
        std::fs::write(&group.marker, "4321 run-0:9\n").expect("stale marker");
        assert!(matches!(marker_pgid(&group), Observed::Absent(_)));
        std::fs::write(&group.marker, "4321 run-1:4\n").expect("marker");
        assert!(matches!(marker_pgid(&group), Observed::Verified(4321, _)));
        std::fs::write(&group.marker, "garbage").expect("bad marker");
        assert!(matches!(marker_pgid(&group), Observed::Unknown(_)));
        std::fs::write(&group.marker, "1 run-1:4").expect("init marker");
        assert!(matches!(marker_pgid(&group), Observed::Unknown(_)));
    }

    #[test]
    fn managed_kinds_are_recognised() {
        assert!(owns(ENVIRONMENT) && owns(EXEC_GROUP) && owns(GATEWAY));
        assert!(!owns("kernel:process"));
    }
}
