//! `data_from` behavior: volume identity, compose output and reset safety for
//! environments borrowing another environment's service data volumes.

use super::{load_container_policy, temp_repo, with_temp_effigy_home, with_test_compose_backend};
use crate::compose::ComposeBackend;
use crate::ContainerPolicyError;
use crate::EffectiveContainerPolicy;
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

        // A linked worktree borrows the owner's worktree-scoped volume,
        // never the primary checkout's: the same borrowed-name identity
        // holds under a worktree scope.
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
fn owner_environment_reports_borrower_as_peer_and_keeps_its_volumes() {
    with_temp_effigy_home("data-from-owner-peer", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-owner-peer");
        write_manifest(&root, PAIR_MANIFEST);

        let dev = load_container_policy(&root, Some("dev")).expect("dev policy");
        let hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");

        assert!(dev.data_from.is_none());
        assert!(dev.borrowed_volumes.is_empty());
        for volume in &dev.managed_volumes {
            assert!(
                volume.name.starts_with(&dev.project_name),
                "environment without data_from keeps its own fresh volumes: {}",
                volume.name
            );
        }
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
fn adding_data_from_keeps_sibling_generated_compose_byte_identical() {
    let owner = tempfile::tempdir().expect("private sibling render fixture");
    let root = owner.path().join("checkout");
    fs::create_dir(&root).unwrap();
    crate::with_test_effigy_home(&owner.path().join("home/.effigy"), || {
        let _lock = crate::test_env_lock();
        let generated_bytes = |policy: &EffectiveContainerPolicy| {
            let render = policy.render.as_ref().expect("immutable host render");
            let name = policy.compose_files[0].strip_prefix(&render.path).unwrap();
            render.closure.files()[name.to_string_lossy().as_ref()].clone()
        };
        write_manifest(&root, &pair_manifest_without_data_from());
        let dev_without = load_container_policy(&root, Some("dev")).expect("dev policy");
        let compose_without = generated_bytes(&dev_without);
        let names_without = managed_volume_names(&dev_without);

        // The changed declaration creates a new generation. Compare the exact
        // immutable bytes, before internal readers expand its absolute root.
        write_manifest(&root, PAIR_MANIFEST);
        let dev_with = load_container_policy(&root, Some("dev")).expect("dev policy");
        let compose_with = generated_bytes(&dev_with);
        assert_ne!(
            dev_with.render.as_ref().unwrap().closure.digest(),
            dev_without.render.as_ref().unwrap().closure.digest()
        );
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

// Acow shape (effigy#730): one `stack` environment owns the Postgres, Silo and
// DbGate data volumes; a `hybrid` environment borrows them through
// `data_from`. Mailpit shares a service but declares no named volume.
const ACOW_SHAPED_MANIFEST: &str = r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "postgres"

[containers.stack.services.postgres]
catalog = "postgres"

[containers.stack.services.silo]
catalog = "silo"

[containers.stack.services.mailpit]
catalog = "mailpit"

[containers.stack.services.dbgate]
catalog = "dbgate"

[containers.hybrid]
primary_service = "postgres"
data_from = "stack"

[containers.hybrid.services.postgres]
catalog = "postgres"

[containers.hybrid.services.silo]
catalog = "silo"

[containers.hybrid.services.mailpit]
catalog = "mailpit"

[containers.hybrid.services.dbgate]
catalog = "dbgate"
"#;

fn borrowed_volume_names(policy: &EffectiveContainerPolicy) -> Vec<String> {
    let mut names = policy
        .borrowed_volumes
        .iter()
        .map(|volume| volume.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Run the Colima nerdctl preflight against a fixture root. Fixture checkouts
/// live under a temp directory, so the Colima host-path preflight is skipped
/// for the same reason the other mount-budget tests skip it.
fn with_colima_nerdctl_preflight<T>(run: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
    unsafe {
        std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
    }
    let result = with_test_compose_backend(ComposeBackend::ColimaNerdctl, run);
    match previous {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
        },
        None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
    }
    result
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

/// The generated borrowing Compose declares each borrowed volume exactly as
/// `{owner_volume_name: {external: true}}`. The Colima nerdctl mount-budget
/// preflight must admit that supported external form — under the owner's own
/// platform volume identity — instead of refusing Effigy's own generated
/// input, and it must do so by reading only, without claiming the volumes.
#[test]
fn colima_mount_budget_admits_generated_external_data_from_volumes() {
    with_temp_effigy_home("data-from-colima-budget", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-colima-budget");
        write_manifest(&root, ACOW_SHAPED_MANIFEST);

        let stack = load_container_policy(&root, Some("stack")).expect("stack policy");
        let hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");

        let owner_names = managed_volume_names(&stack);
        assert!(
            owner_names.len() >= 3,
            "the Acow shape owns the postgres, silo and dbgate data volumes: {owner_names:?}"
        );
        assert_eq!(
            borrowed_volume_names(&hybrid),
            owner_names,
            "the borrowing environment mounts exactly the owner's volume names"
        );
        for name in &owner_names {
            assert!(
                hybrid
                    .managed_volumes
                    .iter()
                    .all(|volume| &volume.name != name),
                "borrowed volume {name} is never listed as owned"
            );
        }

        // The generated input carries only the supported external form.
        let compose_path = hybrid.compose_files[0].clone();
        let compose = hybrid.read_compose(&compose_path).expect("read compose");
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&compose).expect("generated compose is valid YAML");
        for name in &owner_names {
            let definition = &parsed["volumes"][name];
            assert_eq!(
                definition["external"].as_bool(),
                Some(true),
                "borrowed volume {name} must be declared external: {compose}"
            );
            assert_eq!(
                definition.as_mapping().map(serde_yaml::Mapping::len),
                Some(1),
                "the generated borrowed entry carries only `external: true`: {compose}"
            );
        }

        let render_digest = hybrid
            .render
            .as_ref()
            .expect("immutable host render")
            .closure
            .digest();
        let borrowed_before = borrowed_volume_names(&hybrid);
        let managed_before = managed_volume_names(&hybrid);

        with_colima_nerdctl_preflight(|| crate::validate_compose_backend_runtime(&root, &hybrid))
            .expect(
                "the generated `data_from` external volumes must pass the Colima nerdctl preflight",
            );

        // Admission is a size estimate over the rendered inputs: it mutates
        // nothing, claims no borrowed volume and runs no volume command.
        assert_eq!(
            hybrid
                .render
                .as_ref()
                .expect("immutable host render")
                .closure
                .digest(),
            render_digest,
            "the preflight must not rewrite the generated Compose"
        );
        assert_eq!(borrowed_volume_names(&hybrid), borrowed_before);
        assert_eq!(managed_volume_names(&hybrid), managed_before);
        assert_eq!(
            hybrid.read_compose(&compose_path).expect("re-read compose"),
            compose,
            "the rendered Compose bytes are unchanged by admission"
        );

        // The owner keeps its own managed volumes and stays admissible.
        with_colima_nerdctl_preflight(|| crate::validate_compose_backend_runtime(&root, &stack))
            .expect("the owner stack keeps its own managed volumes");
    });
}

/// Everything outside the exact generated external form keeps its unmodelled
/// refusal: unknown external identities, aliases, `external: false` or
/// malformed external values, driver options, conflicting definitions and
/// unresolved interpolation all refuse before any apply.
#[test]
fn colima_mount_budget_refuses_unmodelled_external_volume_forms() {
    with_temp_effigy_home("data-from-colima-refusals", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("data-from-colima-refusals");
        write_manifest(&root, ACOW_SHAPED_MANIFEST);

        let mut hybrid = load_container_policy(&root, Some("hybrid")).expect("hybrid policy");
        let owner_volume = borrowed_volume_names(&hybrid)
            .into_iter()
            .find(|name| name.ends_with("-postgres-data"))
            .expect("the postgres data volume is borrowed");
        let overlay = root.join("infra/dev/compose.external-overlay.yml");
        fs::create_dir_all(overlay.parent().expect("overlay parent")).expect("mkdir overlay dir");
        hybrid.compose_files.push(overlay.clone());

        let refuse = |content: &str| {
            fs::write(&overlay, content).expect("write external-volume overlay");
            with_colima_nerdctl_preflight(|| {
                crate::validate_compose_backend_runtime(&root, &hybrid)
            })
            .expect_err("an unsupported external volume definition must refuse")
        };

        let unknown_identity = refuse("services:\n  postgres:\n    volumes:\n      - stack-postgres-data:/var/lib/postgresql/data\nvolumes:\n  stack-postgres-data:\n    external: true\n");
        assert!(
            unknown_identity
                .to_string()
                .contains("primary service mounts a named volume with an `external` volume that is not a `data_from` borrowed volume"),
            "unexpected refusal: {unknown_identity}"
        );

        // Composing `external: true` from a boolean is not enough: the alias
        // must name the owner's own platform volume.
        let aliased = refuse(&format!(
            "services:\n  postgres:\n    volumes:\n      - {owner_volume}:/var/lib/postgresql/data\nvolumes:\n  {owner_volume}:\n    external: true\n    name: local-postgres-alias\n"
        ));
        assert!(
            aliased
                .to_string()
                .contains("outside the supported `data_from` form"),
            "unexpected refusal: {aliased}"
        );

        for (label, definition) in [
            ("false external", "    external: false"),
            ("string external", "    external: \"true\"\n"),
            ("mapped external", "    external:\n      name: {owner}\n"),
            (
                "escaped name",
                "    external: true\n    name: ../../../etc\n",
            ),
            (
                "interpolated name",
                "    external: true\n    name: ${VOLUME_NAME}\n",
            ),
            (
                "driver options",
                "    external: true\n    driver_opts:\n      type: nfs\n",
            ),
        ] {
            let escaped = definition.replace("{owner}", &owner_volume);
            let error = refuse(&format!(
                "services:\n  postgres:\n    volumes:\n      - {owner_volume}:/var/lib/postgresql/data\nvolumes:\n  {owner_volume}:\n{escaped}"
            ));
            assert!(
                error.to_string().contains("named volume"),
                "{label} must stay unmodelled: {error}"
            );
            assert!(
                !error.to_string().contains("/var/lib/postgresql/data"),
                "{label} diagnostics stay path-free: {error}"
            );
        }

        // A later Compose input that redefines a borrowed volume as a managed
        // one leaves two candidate backing identities; refuse instead of
        // picking one.
        let conflicting = refuse(&format!(
            "services:\n  postgres:\n    volumes:\n      - {owner_volume}:/var/lib/postgresql/data\nvolumes:\n  {owner_volume}:\n    driver: local\n"
        ));
        assert!(
            conflicting
                .to_string()
                .contains("conflicting top-level volume definitions across Compose inputs"),
            "unexpected refusal: {conflicting}"
        );

        // Unresolved interpolation in the mount itself stays unmodelled.
        let interpolated_mount = refuse(
            "services:\n  postgres:\n    volumes:\n      - ${DATA_VOLUME}:/var/lib/postgresql/data\n",
        );
        assert!(
            interpolated_mount
                .to_string()
                .contains("unresolved Compose interpolation"),
            "unexpected refusal: {interpolated_mount}"
        );

        // The same inputs remain admissible on the unchanged Docker route,
        // which never uses the nerdctl label estimate.
        fs::write(
            &overlay,
            "services:\n  postgres:\n    volumes:\n      - stack-postgres-data:/var/lib/postgresql/data\nvolumes:\n  stack-postgres-data:\n    external: true\n",
        )
        .expect("write external-volume overlay");
        with_test_compose_backend(ComposeBackend::Docker, || {
            crate::validate_compose_backend_runtime(&root, &hybrid)
                .expect("Docker must not inherit Colima mount-budget refusals")
        });
    });
}

/// Enough borrowed external volumes still exceed the nerdctl mount-label
/// budget, so the guard refuses an oversized borrowing mount set before any
/// native effect instead of admitting every `external: true` entry.
#[test]
fn colima_mount_budget_refuses_oversized_borrowed_volume_mounts() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("data-from-colima-oversized");
    let compose_file = root.join("infra/dev/docker-compose.yml");
    fs::create_dir_all(compose_file.parent().expect("compose parent")).expect("mkdir compose dir");

    let mut policy = crate::policy::model::test_policy(&root);
    policy.borrowed_volumes = (0..24)
        .map(|index| ManagedVolume {
            name: format!("acow-dev-{index:02}-postgres-data"),
            service: "postgres".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some(format!("/var/lib/postgresql/data/{index:02}")),
        })
        .collect();
    let mounts = policy
        .borrowed_volumes
        .iter()
        .map(|volume| {
            format!(
                "      - {}:{}",
                volume.name,
                volume.mount_target.as_deref().expect("mount target")
            )
        })
        .collect::<Vec<_>>();
    let definitions = policy
        .borrowed_volumes
        .iter()
        .map(|volume| format!("  {}:\n    external: true\n", volume.name))
        .collect::<Vec<_>>();
    fs::write(
        &compose_file,
        format!(
            "services:\n  app:\n    image: example.invalid/synthetic\n    volumes:\n{}\nvolumes:\n{}",
            mounts.join("\n"),
            definitions.join("")
        ),
    )
    .expect("write borrowed-volume compose");
    policy.compose_files = vec![compose_file];

    let error =
        with_colima_nerdctl_preflight(|| crate::validate_compose_backend_runtime(&root, &policy))
            .expect_err("an oversized borrowed mount set must exceed the budget");
    match error {
        ContainerPolicyError::TaskInvocation(message) => {
            assert!(
                message.contains("estimated nerdctl/mounts label size"),
                "unexpected refusal: {message}"
            );
            assert!(
                !message.contains("/var/lib/postgresql/data"),
                "the budget refusal must not leak borrowed mount paths: {message}"
            );
        }
        other => panic!("expected the budget refusal, got {other:?}"),
    }

    // A small borrowed mount set stays inside the budget with the owner's
    // platform volume names, not this project's prefix.
    policy.borrowed_volumes.truncate(3);
    let mounts = policy
        .borrowed_volumes
        .iter()
        .map(|volume| {
            format!(
                "      - {}:{}",
                volume.name,
                volume.mount_target.as_deref().expect("mount target")
            )
        })
        .collect::<Vec<_>>();
    let definitions = policy
        .borrowed_volumes
        .iter()
        .map(|volume| format!("  {}:\n    external: true\n", volume.name))
        .collect::<Vec<_>>();
    fs::write(
        &policy.compose_files[0],
        format!(
            "services:\n  app:\n    image: example.invalid/synthetic\n    volumes:\n{}\nvolumes:\n{}",
            mounts.join("\n"),
            definitions.join("")
        ),
    )
    .expect("write small borrowed-volume compose");
    with_colima_nerdctl_preflight(|| crate::validate_compose_backend_runtime(&root, &policy))
        .expect("a within-budget borrowed mount set is admissible");
}
