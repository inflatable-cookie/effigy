//! Proving that applying this render cannot recreate a live predecessor of the
//! same environment, from Compose's own config hashes.

use std::time::Instant;

use std::path::Path;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

use super::compose_hash::compose_service_config_hashes;

/// What proves that applying one rendered policy cannot recreate a live
/// predecessor of this environment.
///
/// The environment record's historical `desired` digest is *not* live evidence: it
/// says what Effigy last published, not what the running container was created
/// from. The one fact that does is Compose's own config hash: the container carries
/// the hash of the service config it was created from, and the render this
/// activation is about to apply can be asked for the hash it would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum PredecessorCompatibility {
    /// This project has no running container, so nothing can be recreated.
    NoPredecessor,
    /// Every running container of this project was created from exactly the
    /// service config this render produces: `(full container id, config hash)`.
    Matches(Vec<(String, String)>),
}

impl PredecessorCompatibility {
    /// How many live containers were compared.
    pub(in crate::runner) fn compared(&self) -> usize {
        match self {
            Self::NoPredecessor => 0,
            Self::Matches(containers) => containers.len(),
        }
    }
}

/// Observe the live recreate risk of one rendered policy, within one deadline.
///
/// `policy` must be the one a prior [`seal_render_for_preflight`] produced: its
/// Compose inputs name the retained private seal, so this observation only reads
/// and never stages a second tree.
///
/// Unknown, a changed container id, a changed config hash and a backend that
/// cannot report Compose config hashes all refuse; only a live stack whose every
/// service config is byte-identical to what this render produces is compatible.
/// NoPredecessor is a verified empty listing. An unavailable inventory stays
/// Unknown and grants nothing.
///
/// Interpolation is not replayed: the hash is computed from the frozen rendered
/// closure and the project, so a service config that depends on apply-time
/// environment values can differ and refuses, which is the safe direction.
/// The backend reads the live-predecessor guard performs.
///
/// Production reads the runtime; a test supplies a deterministic fixture, so the
/// adapter → Compose config hash → fenced reconcile path can be exercised without
/// a live container runtime. Every method is bounded by the caller's deadline.
pub(in crate::runner) trait PredecessorBackend {
    fn inventory(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>;
    fn inspect(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>>;
    fn config_hashes(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError>;
}

/// The production reads: the runtime inventory, its inspect and `compose
/// config --hash`.
#[derive(Debug, Default)]
pub(in crate::runner) struct RuntimePredecessorBackend;

impl PredecessorBackend for RuntimePredecessorBackend {
    fn inventory(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>
    {
        match effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            repo_root,
            policy,
            Some(deadline),
        ) {
            Ok(rows) => effigy_kernel::observe::Observed::Verified(
                rows,
                effigy_kernel::observe::Evidence::new("project inventory".into(), None)
                    .expect("fixed evidence"),
            ),
            Err(error) => effigy_kernel::observe::Observed::Unknown(
                effigy_kernel::observe::UnknownReason::Inconsistent(format!(
                    "the project inventory is unavailable: {error}"
                )),
            ),
        }
    }

    fn inspect(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        name: &str,
        deadline: Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>> {
        use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
        match effigy_containers::exec::inspect_containers_for_policy(
            repo_root,
            policy,
            std::slice::from_ref(&name.to_owned()),
            Some(deadline),
        ) {
            Ok(output) => Observed::Verified(
                output.stdout,
                Evidence::new("container inspect".into(), None).expect("fixed evidence"),
            ),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(format!(
                "the predecessor `{name}` could not be inspected: {error}"
            ))),
        }
    }

    fn config_hashes(
        &mut self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
        compose_service_config_hashes(repo_root, policy, deadline)
    }
}

pub(in crate::runner) fn observed_predecessor_compatibility(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> effigy_kernel::observe::Observed<PredecessorCompatibility> {
    let mut backend = RuntimePredecessorBackend;
    observed_predecessor_compatibility_with(repo_root, policy, deadline, &mut backend)
}

/// The same observation over an injected backend seam. Production passes
/// [`RuntimePredecessorBackend`]; a test supplies a deterministic fixture.
pub(in crate::runner) fn observed_predecessor_compatibility_with(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
    backend: &mut dyn PredecessorBackend,
) -> effigy_kernel::observe::Observed<PredecessorCompatibility> {
    use effigy_kernel::observe::{Evidence, Observed, UnknownReason};

    let unknown = |reason: String| Observed::Unknown(UnknownReason::Inconsistent(reason));
    // Ownership resolution canonicalizes the checkout and reads its scope token:
    // both are filesystem work that can stall, so it runs on the publication's
    // bounded worker and an expired budget is unknown, never a held caller.
    let owned_root = repo_root.to_path_buf();
    let owned_policy = policy.clone();
    let ownership = match crate::runner::workspace_command::provisioning::bounded_publication_read(
        "workspace-predecessor-ownership",
        deadline,
        move || {
            effigy_containers::exec::OwnershipContext::resolve(&owned_root, &owned_policy)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        },
    ) {
        Ok(ownership) => ownership,
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut) => {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "predecessor ownership".into(),
            })
        }
        Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::Unavailable(
            reason,
        )) => return unknown(format!("the checkout's ownership is unproven: {reason}")),
    };
    let rows = match backend.inventory(repo_root, policy, deadline) {
        effigy_kernel::observe::Observed::Verified(rows, _) => rows,
        effigy_kernel::observe::Observed::Unknown(reason) => return Observed::Unknown(reason),
        effigy_kernel::observe::Observed::Absent(_) => {
            return unknown("the project inventory is absent".into())
        }
    };
    let running = rows
        .into_iter()
        .filter(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && !row.oneoff
                && row.status.is_running()
        })
        .collect::<Vec<_>>();
    // Verified empty listing only. Unknown inventory already returned above:
    // an unavailable backend is not NoPredecessor.
    if running.is_empty() {
        return Observed::Verified(
            PredecessorCompatibility::NoPredecessor,
            Evidence::new("project inventory".into(), None).expect("fixed evidence"),
        );
    }
    let intended = match backend.config_hashes(repo_root, policy, deadline) {
        Ok(hashes) => hashes,
        Err(error) => {
            return unknown(format!(
                "this backend cannot report the render's Compose config hashes: {error}"
            ));
        }
    };
    let mut compared = Vec::new();
    for row in running {
        let inspected = match backend.inspect(repo_root, policy, &row.container_name, deadline) {
            effigy_kernel::observe::Observed::Verified(bytes, _) => bytes,
            _ => {
                return unknown(format!(
                    "the predecessor `{}` inspect is unavailable",
                    row.container_name
                ))
            }
        };
        let verified = match ownership.verify_inspected(repo_root, policy, &row, &inspected) {
            Ok(verified) => verified,
            Err(error) => {
                return unknown(format!(
                    "the predecessor `{}` ownership or identity changed: {error}",
                    row.container_name
                ));
            }
        };
        let (service, live_hash) = match inspected_config_hash(&inspected) {
            Ok(facts) => facts,
            Err(reason) => {
                return unknown(format!(
                    "the predecessor `{}` reports no Compose config hash: {reason}",
                    row.container_name
                ));
            }
        };
        let Some(expected) = intended.get(&service) else {
            return unknown(format!(
                "the live predecessor runs service `{service}`, which this render does not declare"
            ));
        };
        if expected != &live_hash {
            return unknown(format!(
                "the live predecessor for service `{service}` was created from config `{live_hash}`, \
                 not the render's `{expected}`: applying would recreate it"
            ));
        }
        compared.push((verified.id().to_owned(), live_hash));
    }
    // Listing order is not evidence: two reads of the same stack must compare
    // equal, so the frozen facts are ordered by container id.
    compared.sort();
    Observed::Verified(
        PredecessorCompatibility::Matches(compared),
        Evidence::new("Compose config hash".into(), None).expect("fixed evidence"),
    )
}

/// The Compose service and config hash one inspected container records.
pub(super) fn inspected_config_hash(stdout: &[u8]) -> Result<(String, String), String> {
    let raw: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|error| error.to_string())?;
    let labels = raw
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("Config"))
        .and_then(|config| config.get("Labels"))
        .ok_or_else(|| "the inspect names no labels".to_owned())?;
    let label = |name: &str| {
        labels
            .get(name)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("the inspect names no `{name}` label"))
    };
    Ok((
        label("com.docker.compose.service")?,
        label("com.docker.compose.config-hash")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::inspected_config_hash;

    #[test]
    fn the_inspected_config_hash_needs_a_service_and_a_hash() {
        let inspect = br#"[{"Config": {"Labels": {"com.docker.compose.service": "app", "com.docker.compose.config-hash": "abc123"}}}]"#;
        assert_eq!(
            inspected_config_hash(inspect).expect("both labels"),
            ("app".to_owned(), "abc123".to_owned())
        );
        let no_hash = br#"[{"Config": {"Labels": {"com.docker.compose.service": "app"}}}]"#;
        assert!(inspected_config_hash(no_hash).is_err());
        assert!(inspected_config_hash(b"[]").is_err());
        assert!(inspected_config_hash(b"not json").is_err());
    }
}
