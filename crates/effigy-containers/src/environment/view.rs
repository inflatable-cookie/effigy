//! What `container status` and `doctor` show for one environment: the
//! backend it uses and why, and its record. Pure (R2): reading never
//! creates the state root, imports markers or probes the backend.

use std::path::Path;

use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::run::Liveness;
use serde_json::{json, Value};

use super::backend::{effective, project_resources, BackendSelection};
use super::reconcile::EnvRef;
use super::record::{EnvRecord, EnvState, Retention};
use crate::{ContainerCommandReport, EffectiveContainerPolicy};

#[derive(Debug)]
pub struct EnvironmentView {
    pub name: String,
    pub backend: BackendSelection,
    pub record: Observed<EnvRecord>,
}

impl EnvironmentView {
    pub fn read(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Self {
        let record = match EnvRef::for_policy(policy) {
            Ok(env) => {
                let operation = super::reconcile::operation(
                    effigy_kernel::ctx::EffectClass::Observe,
                    effigy_kernel::ctx::CancelToken::default(),
                    std::time::Duration::from_secs(5),
                );
                match operation.observe() {
                    Ok(cx) => match super::observe_root(&cx, &policy.repo_root) {
                        Observed::Verified(root, _) => EnvRecord::read(&cx, &root, &env.resource),
                        Observed::Absent(proof) => Observed::Absent(proof),
                        Observed::Unknown(reason) => Observed::Unknown(reason),
                    },
                    Err(_) => Observed::Unknown(UnknownReason::Inconsistent(
                        "observe capability refused".into(),
                    )),
                }
            }
            // No checkout identity yet: nothing was ever recorded.
            Err(_) => Observed::Absent(effigy_kernel::observe::AbsenceProof::NoRow {
                table: "resources".into(),
            }),
        };
        let recorded = record
            .verified()
            .and_then(|record| record.recorded_backend().cloned());
        Self {
            name: policy.name.clone(),
            backend: effective(repo_root, policy, recorded.as_ref()),
            record,
        }
    }

    /// `backend: Colima (profile `effigy`): <why>` and `record: …`.
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!("backend: {}", self.backend.describe()),
            format!("record: {}", self.record_summary()),
        ]
    }

    fn record_summary(&self) -> String {
        let record = match &self.record {
            Observed::Verified(record, _) => record,
            Observed::Absent(_) => {
                return "none yet; the next bring-up or stop records it".to_owned()
            }
            Observed::Unknown(UnknownReason::HostAuthorityRequired) => {
                return "kept on the host; inside the workspace container it is read through the host"
                    .to_owned()
            }
            Observed::Unknown(reason) => return format!("unreadable ({reason:?})"),
        };
        let mut parts = vec![match record.state {
            EnvState::Active => "active".to_owned(),
            EnvState::Stopped => "stopped".to_owned(),
            EnvState::Pending => "not active".to_owned(),
        }];
        if let Some(engine) = record.engine() {
            parts.push(engine.retention.label().to_owned());
            if engine.imported.is_some() {
                parts.push("imported from v0.14.1 markers".to_owned());
            }
        }
        parts.push(match &record.holders {
            Observed::Verified(holders, _) if holders.is_empty() => "no holders".to_owned(),
            Observed::Verified(holders, _) => format!(
                "held by {}",
                holders
                    .iter()
                    .map(|holder| format!("run {} ({})", holder.run, liveness(&holder.liveness)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "holders unreadable (protects every transition)".to_owned(),
        });
        parts.join(", ")
    }

    pub fn json(&self) -> Value {
        let record = self.record.verified();
        json!({
            "backend": {
                "kind": self.backend.kind,
                "engine": self.backend.engine.as_str(),
                "context": self.backend.context,
                "reason": self.backend.reason.describe(),
            },
            "record": record.map(|record| json!({
                "state": record.state,
                "retention": record.engine().map(|engine| match engine.retention {
                    Retention::Persistent => "persistent",
                    Retention::Held => "held",
                }),
                "generation": record.generation.get(),
                "desired": record.desired,
                "imported": record.engine().is_some_and(|engine| engine.imported.is_some()),
                "holders": match &record.holders {
                    Observed::Verified(holders, _) => json!(holders
                        .iter()
                        .map(|holder| json!({
                            "run": holder.run.to_string(),
                            "liveness": liveness(&holder.liveness),
                        }))
                        .collect::<Vec<_>>()),
                    _ => Value::Null,
                },
            })),
        })
    }

    /// Adds the view to a status report (text lines and `environment` JSON).
    pub fn annotate(&self, report: &mut ContainerCommandReport) {
        for line in self.lines() {
            report.success_text.push('\n');
            report
                .success_text
                .push_str(&format!("[environment] {line}"));
        }
        if let Some(json) = report.json.as_object_mut() {
            json.insert("environment".to_owned(), self.json());
        }
    }
}

fn liveness(liveness: &Observed<Liveness>) -> &'static str {
    match liveness {
        Observed::Verified(Liveness::Live, _) => "live",
        Observed::Verified(Liveness::Interrupted, _) => "interrupted",
        _ => "unknown",
    }
}

/// Why a stop of `policy` provably has nothing to stop, or `None` when the
/// stop must run.
///
/// An environment whose record never published `active` (or was stopped)
/// and whose backend project owns no container,
/// network or volume has nothing to tear down, and its never-assigned ports
/// must never reach Compose: a never-prepared render carries the deliberate
/// [`crate::policy::ports::UNASSIGNED_PORT_EXPRESSION`] refusal (effigy#286).
/// Anything unproven is a stop: an unreadable record, a holding run, a backend
/// that cannot answer the emptiness probe.
pub fn nothing_to_stop(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Option<String> {
    let view = EnvironmentView::read(repo_root, policy);
    let never_prepared = match &view.record {
        // No checkout identity or no record row: nothing was ever recorded.
        Observed::Absent(_) => true,
        // A record that never published `active` describes a stop that has
        // nothing to stop; one that did may own resources Compose must see.
        Observed::Verified(record, _) => {
            record.state != EnvState::Active && !record.protected()
        }
        Observed::Unknown(_) => false,
    };
    if !never_prepared {
        return None;
    }
    match project_resources(&view.backend, repo_root, &policy.project_name) {
        Observed::Verified(resources, _) if resources.empty() => Some(format!(
            "the environment was never prepared and Compose project `{}` owns no container, network or volume",
            policy.project_name
        )),
        _ => None,
    }
}
