//! A valid retained snapshot is never a substitute for unavailable fresh I/O.
use super::*;
use effigy_kernel::workspace_session::{ImagePlatform, LoaderFamily};

fn original() -> (
    ContainerIdentity,
    BoundPlatform,
    Observed<Vec<TargetCandidate>>,
) {
    let target = ContainerIdentity {
        container_id: "aa".repeat(32),
        vm_boot_id: "bb".repeat(32),
        backend: "fixture-backend".into(),
    };
    let image = ImagePlatform {
        os: "linux".into(),
        architecture: "arm64".into(),
        variant: String::new(),
        loader: LoaderFamily::Musl,
    };
    let platform = BoundPlatform {
        target: target.clone(),
        platform: image.clone(),
        artifact_platform: image,
        artifact: GuestObject {
            path: "/fixture/artifact".into(),
            digest: Digest::of(b"fixture"),
        },
        artifact_version: "fixture-version".into(),
        execution_available: true,
    };
    let candidates = Observed::Verified(
        vec![TargetCandidate {
            identity: target.clone(),
            project: "fixture-project".into(),
            created_by: None,
        }],
        evidence(),
    );
    (target, platform, candidates)
}

#[test]
fn resume_observation_requires_fresh_target_and_platform_not_only_expected_facts() {
    let (target, platform, original) = original();
    let until = Instant::now() + Duration::from_secs(5);
    for fresh in [
        (
            Observed::Unknown(UnknownReason::GuardUnavailable),
            Observed::Verified(platform.clone(), evidence()),
        ),
        (
            original.clone(),
            Observed::Unknown(UnknownReason::Inconsistent(
                "private-backend-diagnostic".into(),
            )),
        ),
        (
            Observed::Absent(effigy_kernel::observe::AbsenceProof::ContainerGone {
                id: target.container_id.clone(),
            }),
            Observed::Verified(platform.clone(), evidence()),
        ),
    ] {
        let observed = checked_resume_observation(&target, &platform, &original, fresh, until);
        assert!(matches!(observed, Observed::Unknown(_)));
        assert!(!format!("{observed:?}").contains("private-backend-diagnostic"));
    }
    let observed = checked_resume_observation(
        &target,
        &platform,
        &original,
        (
            original.clone(),
            Observed::Verified(platform.clone(), evidence()),
        ),
        until,
    );
    assert!(
        matches!(observed, Observed::Verified((actual, facts), _) if actual == target && facts == platform)
    );
}

#[test]
fn resume_observation_refuses_fresh_boot_platform_ambiguity_and_overshoot() {
    let (target, platform, original) = original();
    let until = Instant::now() + Duration::from_secs(5);
    let Observed::Verified(candidates, _) = &original else {
        panic!("fixture")
    };
    let mut changed_boot = candidates.clone();
    changed_boot[0].identity.vm_boot_id = "cc".repeat(32);
    let mut changed_platform = platform.clone();
    changed_platform.platform.loader = LoaderFamily::Gnu;
    let mut ambiguous = candidates.clone();
    ambiguous.push(candidates[0].clone());
    for (targets, current_platform) in [
        (changed_boot, platform.clone()),
        (candidates.clone(), changed_platform),
        (ambiguous, platform.clone()),
    ] {
        assert!(matches!(
            checked_resume_observation(
                &target,
                &platform,
                &original,
                (
                    Observed::Verified(targets, evidence()),
                    Observed::Verified(current_platform, evidence())
                ),
                until
            ),
            Observed::Unknown(_)
        ));
    }
    assert!(matches!(
        checked_resume_observation(
            &target,
            &platform,
            &original,
            (
                original.clone(),
                Observed::Verified(platform.clone(), evidence())
            ),
            Instant::now() - Duration::from_secs(1)
        ),
        Observed::Unknown(_)
    ));
}
