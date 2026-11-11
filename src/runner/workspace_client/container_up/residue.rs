//! What a failed activation's cleanup may claim (effigy#901).
//!
//! Settling an activation's Env run releases process custody and every
//! journalled effect; it does not remove the Compose project the apply may
//! already have created. Claiming "cleanup settled" from custody alone was a
//! fail-open status claim: a sealed apply that attributed nothing still left
//! its container, network and volumes running. "Settled" is now a claim about
//! the owned resources, and it needs the owned project's inventory to answer
//! empty first. Anything else names the residue and the one command an
//! operator can run to remove it.

use std::time::Instant;

use effigy_containers::environment::Workload;
use effigy_kernel::observe::Observed;

use crate::runner::container_command::reserved_input::ReservedPublicationPlan;

/// One bounded, read-only observation after custody has settled. It cannot
/// extend an owner gate: nothing holds Env at this point.
pub(super) const RESIDUE_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// The one down authority for these resources, and the recovery this report
/// names. It is the kernel's own recovery invocation, never a suggested
/// `nerdctl` form that bypasses the environment record.
const RECOVERY: &str = "effigy container down";

/// How many owned container names one message may carry.
const NAMED_LIMIT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CleanupResidue {
    /// The owned project's inventory is verifiably empty.
    Absent,
    /// The owned project still lists these containers.
    Containers(Vec<String>),
    /// The backend could not answer, so absence is unproven.
    Unverified,
}

impl CleanupResidue {
    /// Read-only classification of one project inventory answer.
    pub(super) fn of(observed: Observed<Vec<Workload>>) -> Self {
        match observed {
            Observed::Verified(rows, _) if rows.is_empty() => Self::Absent,
            Observed::Verified(rows, _) => Self::Containers(
                rows.into_iter()
                    .map(|row| row.name)
                    .filter(|name| !name.is_empty())
                    .collect(),
            ),
            // Absence of a verified inventory answer is not absent resources.
            Observed::Absent(_) | Observed::Unknown(_) => Self::Unverified,
        }
    }

    /// One fresh bounded observation of this activation's owned project.
    pub(super) fn observe(plan: &ReservedPublicationPlan) -> Self {
        #[cfg(feature = "workspace-serving-fixture")]
        if super::fake_io::configured() {
            return super::fake_io::residue();
        }
        let policy = plan.policy();
        let until = Instant::now() + RESIDUE_BUDGET;
        let mut backend = plan.backend().adapter();
        Self::of(backend.inventory_until(&policy.repo_root, &policy.project_name, until))
    }
}

/// The exact sentence the failed activation ends with. No message may say
/// "settled" unless the owned resources are verifiably gone.
pub(super) fn message(stage: &'static str, residue: CleanupResidue) -> String {
    match residue {
        CleanupResidue::Absent => format!(
            "workspace activation failed during {stage}; exact owner cleanup settled"
        ),
        CleanupResidue::Containers(names) => {
            let shown = names
                .iter()
                .take(NAMED_LIMIT)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let more = names.len().saturating_sub(NAMED_LIMIT);
            let tail = if more == 0 {
                String::new()
            } else {
                format!(" and {more} more")
            };
            format!(
                "workspace activation failed during {stage}; process custody settled but {} owned container(s) remain: {shown}{tail}; run `{RECOVERY}` to remove them",
                names.len()
            )
        }
        CleanupResidue::Unverified => format!(
            "workspace activation failed during {stage}; process custody settled but the owned residue could not be observed, so its removal is unproven; run `{RECOVERY}` before retrying"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::observe::{Evidence, UnknownReason};

    fn verified(rows: Vec<Workload>) -> Observed<Vec<Workload>> {
        Observed::Verified(
            rows,
            Evidence::new("owned project inventory".into(), None).expect("fixed evidence"),
        )
    }

    fn row(name: &str) -> Workload {
        Workload {
            id: "a".repeat(64),
            name: name.into(),
            running: true,
            labels: Default::default(),
            binds: Vec::new(),
            image: String::new(),
            image_reference: String::new(),
        }
    }

    #[test]
    fn an_empty_owned_inventory_is_the_only_settled_cleanup() {
        assert_eq!(
            CleanupResidue::of(verified(Vec::new())),
            CleanupResidue::Absent
        );
        let settled = message("sealed apply", CleanupResidue::of(verified(Vec::new())));
        assert_eq!(
            settled,
            "workspace activation failed during sealed apply; exact owner cleanup settled"
        );
    }

    #[test]
    fn remaining_owned_containers_are_never_reported_as_settled() {
        let residue = CleanupResidue::of(verified(vec![row("checkout-dev-ec-746e15dc40d7-app-1")]));
        assert_eq!(
            residue,
            CleanupResidue::Containers(vec!["checkout-dev-ec-746e15dc40d7-app-1".into()])
        );
        let reported = message("sealed apply", residue);
        assert!(!reported.contains("cleanup settled"), "{reported}");
        assert!(
            reported.contains("1 owned container(s) remain"),
            "{reported}"
        );
        assert!(
            reported.contains("checkout-dev-ec-746e15dc40d7-app-1"),
            "{reported}"
        );
        assert!(reported.contains("effigy container down"), "{reported}");
    }

    #[test]
    fn an_unreadable_inventory_never_claims_absence() {
        for proof in [
            Observed::Unknown(UnknownReason::Inconsistent("unreadable".into())),
            Observed::Absent(effigy_kernel::observe::AbsenceProof::GateFreeRunClosed),
        ] {
            let reported = message("sealed apply", CleanupResidue::of(proof));
            assert!(!reported.contains("cleanup settled"), "{reported}");
            assert!(reported.contains("unproven"), "{reported}");
            assert!(reported.contains("effigy container down"), "{reported}");
        }
    }

    #[test]
    fn the_named_residue_stays_bounded() {
        let rows = (0..9)
            .map(|index| row(&format!("checkout-dev-ec-746e15dc40d7-app-{index}")))
            .collect();
        let reported = message("sealed apply", CleanupResidue::of(verified(rows)));
        assert!(
            reported.contains("9 owned container(s) remain"),
            "{reported}"
        );
        assert!(reported.contains("and 4 more"), "{reported}");
        assert!(!reported.contains("app-5"), "{reported}");
    }
}
