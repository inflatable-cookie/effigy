//! `data_from` behavior: volume identity, compose output, retirement and
//! reset safety for environments borrowing another environment's service
//! data volumes.

use super::{load_container_policy, temp_repo, with_temp_effigy_home};
use crate::runtime::retire::{plan_retirement, ObservedKind, ObservedResource};
use crate::runtime::scope::ScopeRecord;
use crate::EffectiveContainerPolicy;
use crate::SCOPE_LABEL;
use effigy_catalog::volumes::{classify_for_reset, ManagedVolume};
use std::fs;

const PAIR_MANIFEST: &str = r#"
[containers]
default = "dev"

[containers.dev]
primary_service = "db"

[containers.dev.services.db]
catalog = "postgres"

[containers.hybrid]
primary_service = "db"
data_from = "dev"

[containers.hybrid.services.db]
catalog = "postgres"
"#;

fn pair_manifest_without_data_from() -> String {
    PAIR_MANIFEST.replace("data_from = \"dev\"\n", "")
}

fn write_manifest(root: &std::path::Path, manifest: &str) {
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
}

fn volume_named<'a>(volumes: &'a [ManagedVolume], suffix: &str) -> &'a ManagedVolume {
    volumes
        .iter()
        .find(|volume| volume.name.ends_with(suffix))
        .unwrap_or_else(|| panic!("volume ending in {suffix} not found"))
}

#[test]
fn borrowing_environment_mounts_owner_data_volumes() {
    with_temp_effigy_home("data-from-borrow", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-borrow");
        write_manifest(&root, PAIR_MANIFEST);

        let dev = load_container_policy(&root, Some("dev")).expect("dev policy");
        let hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");

        assert_eq!(hybrid.data_from.as_deref(), Some("dev"));
        let owner_volume = volume_named(&dev.managed_volumes, "-db-data");
        let borrowed = volume_named(&hybrid.borrowed_volumes, "-db-data");
        assert_eq!(
            borrowed.name, owner_volume.name,
            "the borrower must mount the owner's volume name"
        );
        assert!(borrowed.persist, "postgres data volumes persist");
        assert!(
            hybrid
                .managed_volumes
                .iter()
                .all(|volume| volume.name != borrowed.name),
            "borrowed volumes are never listed as owned"
        );

        let compose = hybrid
            .read_compose(&hybrid.compose_files[0])
            .expect("read compose");
        let external_block = format!("{name}:\n    external: true", name = borrowed.name);
        assert!(
            compose.contains(&external_block),
            "borrowed volume must be external in the compose: {compose}"
        );
        let mount_line = format!("{name}:/var/lib/postgresql/data", name = borrowed.name);
        assert!(
            compose.contains(&mount_line),
            "the db service must mount the owner's volume: {compose}"
        );
        assert!(
            !compose.contains(&format!("{}-db-data", hybrid.project_name)),
            "the borrower's project prefix must not name the shared volume: {compose}"
        );

        // Project names stay per environment even though volumes are shared.
        assert_ne!(dev.project_name, hybrid.project_name);
    });
}

#[test]
fn owner_environment_reports_borrower_as_peer_and_keeps_its_volumes() {
    with_temp_effigy_home("data-from-owner-peer", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-owner-peer");
        write_manifest(&root, PAIR_MANIFEST);

        let dev = load_container_policy(&root, Some("dev")).expect("dev policy");
        let hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");

        assert!(dev.data_from.is_none());
        assert!(dev.borrowed_volumes.is_empty());
        let peer = dev
            .data_peers
            .iter()
            .find(|peer| peer.environment == "hybrid")
            .expect("owner sees the borrower as a data peer");
        assert_eq!(peer.project_name, hybrid.project_name);
        assert_eq!(peer.services, vec!["db".to_owned()]);

        let borrower_peer = hybrid
            .data_peers
            .iter()
            .find(|peer| peer.environment == "dev")
            .expect("borrower sees the owner as a data peer");
        assert_eq!(borrower_peer.project_name, dev.project_name);
        assert_eq!(borrower_peer.services, vec!["db".to_owned()]);
    });
}

#[test]
fn linked_worktree_borrows_owner_worktree_scoped_volumes() {
    with_temp_effigy_home("data-from-worktree", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-worktree");
        let (_primary, checkouts) =
            super::compose::create_linked_worktree_fixture(&root, PAIR_MANIFEST, &["one"]);
        let checkout = &checkouts[0];

        let dev = load_container_policy(checkout, Some("dev")).expect("dev policy");
        let hybrid = load_container_policy(checkout, Some("hybrid")).expect("hybrid policy");

        let owner_volume = volume_named(&dev.managed_volumes, "-db-data");
        let borrowed = volume_named(&hybrid.borrowed_volumes, "-db-data");
        assert_eq!(borrowed.name, owner_volume.name);
        assert!(
            owner_volume.name.starts_with(&dev.project_name),
            "a linked worktree borrows the owner's worktree-scoped volume, \
             never the primary checkout's: {}",
            owner_volume.name
        );
    });
}

#[test]
fn adding_data_from_keeps_sibling_generated_compose_byte_identical() {
    with_temp_effigy_home("data-from-byte-identical", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-byte-identical");
        write_manifest(&root, &pair_manifest_without_data_from());
        let dev_without = load_container_policy(&root, Some("dev")).expect("dev policy");
        let compose_without = dev_without
            .read_compose(&dev_without.compose_files[0])
            .expect("read dev compose");
        let names_without = managed_volume_names(&dev_without);

        // Declare the same environments again with `data_from` added to the
        // sibling, in the same checkout, so every path-derived byte matches.
        write_manifest(&root, PAIR_MANIFEST);
        let dev_with = load_container_policy(&root, Some("dev")).expect("dev policy");
        let compose_with = dev_with
            .read_compose(&dev_with.compose_files[0])
            .expect("read dev compose");

        assert_eq!(
            compose_with, compose_without,
            "adding data_from on a sibling must not change this environment's generated compose"
        );
        assert_eq!(managed_volume_names(&dev_with), names_without);
        assert!(dev_with.borrowed_volumes.is_empty());
    });
}

fn managed_volume_names(policy: &EffectiveContainerPolicy) -> Vec<String> {
    let mut names = policy
        .managed_volumes
        .iter()
        .map(|volume| volume.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn environment_without_data_from_keeps_fresh_own_volumes() {
    with_temp_effigy_home("data-from-absent-fresh", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-absent-fresh");
        write_manifest(&root, PAIR_MANIFEST);

        let dev = load_container_policy(&root, Some("dev")).expect("dev policy");
        assert!(dev.borrowed_volumes.is_empty());
        assert!(dev.data_from.is_none());
        for volume in &dev.managed_volumes {
            assert!(
                volume.name.starts_with(&dev.project_name),
                "environment without data_from keeps its own fresh volumes: {}",
                volume.name
            );
        }
    });
}

#[test]
fn scope_record_lists_borrowed_volumes_as_retained_never_owned() {
    let root = temp_repo("data-from-scope-record");
    let mut hybrid = crate::policy::model::test_policy(&root);
    hybrid.name = "hybrid".to_owned();
    hybrid.project_name = "acowtancy-dev-hybrid-wt-abcdef012345".to_owned();
    hybrid.borrowed_volumes = vec![
        ManagedVolume {
            name: "acowtancy-dev-wt-abcdef012345-db-data".to_owned(),
            service: "db".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/var/lib/postgresql/data".to_owned()),
        },
        ManagedVolume {
            name: "acowtancy-dev-wt-abcdef012345-cache".to_owned(),
            service: "db".to_owned(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/cache".to_owned()),
        },
    ];
    hybrid.managed_volumes = vec![ManagedVolume {
        name: "acowtancy-dev-hybrid-wt-abcdef012345-own-cache".to_owned(),
        service: "app".to_owned(),
        persist: false,
        size_bytes: None,
        mount_point: None,
        mount_target: Some("/target".to_owned()),
    }];
    let token = "abcdef0123456789abcdef0123456789";
    let host_map = crate::build_host_map(&[], &[], &[], None, false, None);
    let record = ScopeRecord::from_policy(&hybrid, token, "abcdef01", &host_map, false);

    assert!(
        !record
            .owned_volumes
            .iter()
            .any(|name| name.starts_with("acowtancy-dev-wt-")),
        "borrowed volumes are never owned: {:?}",
        record.owned_volumes
    );
    for borrowed in [
        "acowtancy-dev-wt-abcdef012345-db-data",
        "acowtancy-dev-wt-abcdef012345-cache",
    ] {
        assert!(
            record.retain_volumes.iter().any(|name| name == borrowed),
            "borrowed volume {borrowed} must be retained"
        );
        let labels = [(SCOPE_LABEL.to_owned(), token.to_owned())]
            .into_iter()
            .collect();
        assert!(
            record.volume_is_persistent(borrowed, &labels),
            "retained borrowed volume {borrowed} reads as persistent"
        );
    }
    assert!(
        record
            .owned_volumes
            .iter()
            .any(|name| name == "acowtancy-dev-hybrid-wt-abcdef012345-own-cache"),
        "own volumes stay owned"
    );

    // Retirement planning retains every borrowed volume, mutable or
    // persistent, while still deleting owned mutable volumes.
    let observed = [
        ("acowtancy-dev-wt-abcdef012345-db-data", true),
        ("acowtancy-dev-wt-abcdef012345-cache", false),
        ("acowtancy-dev-hybrid-wt-abcdef012345-own-cache", false),
    ]
    .into_iter()
    .map(|(name, persist)| ObservedResource {
        kind: ObservedKind::Volume,
        name: name.to_owned(),
        scope_label: Some(token.to_owned()),
        project_label: Some("acowtancy-dev-wt-abcdef012345".to_owned()),
        persist,
        external: false,
        profile: Some("effigy".to_owned()),
    })
    .collect::<Vec<_>>();
    let plan = plan_retirement(&record, &observed);
    assert_eq!(
        plan.delete
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>(),
        vec!["acowtancy-dev-hybrid-wt-abcdef012345-own-cache"],
        "only owned mutable volumes delete: {:?}",
        plan.delete
    );
    let retained = plan
        .retain
        .iter()
        .map(|resource| resource.name.as_str())
        .collect::<Vec<_>>();
    assert!(retained.contains(&"acowtancy-dev-wt-abcdef012345-db-data"));
    assert!(retained.contains(&"acowtancy-dev-wt-abcdef012345-cache"));
}

#[test]
fn reset_classification_never_offers_borrowed_volumes_for_removal() {
    with_temp_effigy_home("data-from-reset-safety", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-reset-safety");
        write_manifest(&root, PAIR_MANIFEST);
        let hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");

        let borrowed_names = hybrid
            .borrowed_volumes
            .iter()
            .map(|volume| volume.name.clone())
            .collect::<Vec<_>>();
        assert!(!borrowed_names.is_empty(), "fixture borrows a data volume");
        for keep_data in [true, false] {
            let classification = classify_for_reset(&hybrid.managed_volumes, keep_data);
            for borrowed in &borrowed_names {
                assert!(
                    !classification.remove.contains(borrowed),
                    "reset (keep_data={keep_data}) must never remove borrowed volume {borrowed}"
                );
            }
        }
    });
}
