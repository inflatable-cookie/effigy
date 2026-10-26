use super::*;
use crate::policy::load::{load_container_policy, prepare_container_policy};
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (effigy_test_support::Fixture, PathBuf, PathBuf) {
    let fixture = effigy_test_support::Fixture::new("effigy-render-").unwrap();
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        checkout.join("effigy.toml"),
        r#"
[containers]
default="one"
[containers.one]
compose_file="compose.yml"
primary_service="app"
[containers.two]
compose_file="compose.yml"
primary_service="app"
"#,
    )
    .unwrap();
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .unwrap();
    (fixture, checkout, home)
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.insert(path.clone(), vec![]);
            files.extend(snapshot(&path));
        } else {
            files.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    files
}
#[test]
fn resolution_is_a_pure_complete_closure_and_prepare_seals_it() {
    let (fixture, checkout, home) = fixture();
    crate::with_test_effigy_home(&home, || {
        let before = snapshot(fixture.path());
        let observed = load_container_policy(&checkout, None).unwrap();
        assert!(!observed.compose_files[0].exists());
        assert!(observed
            .read_compose(&observed.compose_files[0])
            .unwrap()
            .contains("example:one"));
        assert_eq!(snapshot(fixture.path()), before);
        let prepared = prepare_resolved(&observed).unwrap();
        verify(&prepared).unwrap();
        // An unchanged generation hands Compose the same arguments every time,
        // so a re-up of a running stack recreates nothing (effigy#362).
        let up = crate::compose::compose_up_args(&prepared);
        assert_eq!(up, crate::compose::compose_up_args(&observed));
        assert!(
            !up.iter().any(|argument| argument
                .to_str()
                .is_some_and(|value| value.contains("EFFIGY_RENDER_ROOT"))),
            "no Compose argument carries an Effigy interpolation value"
        );
        // Published objects are ordinary owner-writable files (effigy#344).
        assert_eq!(
            std::fs::metadata(&prepared.compose_files[0])
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!checkout.join(".effigy/runtime/compose").exists());
        assert_eq!(
            prepared.render.as_ref().unwrap().closure.digest(),
            observed.render.as_ref().unwrap().closure.digest()
        );
    });
}
#[test]
fn a_render_generation_matches_across_state_home_paths() {
    let (fixture, checkout, home) = fixture();
    let other_home = fixture.path().join("other-home/.effigy");
    std::fs::create_dir_all(&other_home).unwrap();
    let first = crate::with_test_effigy_home(&home, || {
        prepare_container_policy(&checkout, Some("one")).unwrap()
    });
    let second = crate::with_test_effigy_home(&other_home, || {
        prepare_container_policy(&checkout, Some("one")).unwrap()
    });
    let first_render = first.render.as_ref().unwrap();
    let second_render = second.render.as_ref().unwrap();
    // The render digest never depends on where the state home lives. Q-010
    // moved the published generation into the checkout's own journal, so both
    // renders name the same immutable path instead of one per state home.
    assert_eq!(
        first_render.closure.digest(),
        second_render.closure.digest()
    );
    assert_eq!(first_render.path, second_render.path);
    assert!(
        first_render
            .path
            .starts_with(checkout.join(".effigy/state")),
        "the generation is published in the checkout journal: {}",
        first_render.path.display()
    );
    for bytes in first_render.closure.files().values() {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains(&home.display().to_string()),
            "the immutable closure never embeds the state-home path: {text}"
        );
        assert!(
            !text.contains(&other_home.display().to_string()),
            "the immutable closure never embeds the state-home path: {text}"
        );
    }
}

#[test]
fn reset_preserves_sibling_environment_and_immutable_files() {
    let (_fixture, checkout, home) = fixture();
    crate::with_test_effigy_home(&home, || {
        let one = prepare_container_policy(&checkout, Some("one")).unwrap();
        let two = prepare_container_policy(&checkout, Some("two")).unwrap();
        let bytes = std::fs::read(&two.compose_files[0]).unwrap();
        reset(&one).unwrap();
        assert!(verify(&one).is_err());
        verify(&two).unwrap();
        assert_eq!(std::fs::read(&two.compose_files[0]).unwrap(), bytes);
        assert!(
            one.compose_files[0].exists(),
            "retention owns physical reclamation"
        );
    });
}
#[test]
fn concurrent_engine_publication_preserves_admitted_readers() {
    let (_fixture, checkout, home) = fixture();
    crate::with_test_effigy_home(&home, || {
        let policy = prepare_container_policy(&checkout, Some("one")).unwrap();
        let expected = std::fs::read(&policy.compose_files[0]).unwrap();
        let writer_root = checkout.clone();
        let writer_home = home.clone();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writer_barrier = barrier.clone();
        let writer = std::thread::spawn(move || {
            crate::with_test_effigy_home(&writer_home, || {
                writer_barrier.wait();
                for index in 0..12 {
                    let value = format!("services:\n  app:\n    image: example:{index}\n");
                    effigy_catalog::atomic_write::write_atomic(
                        &writer_root.join("compose.yml"),
                        value.as_bytes(),
                    )
                    .unwrap();
                    prepare_container_policy(&writer_root, Some("one")).unwrap();
                }
            })
        });
        barrier.wait();
        for _ in 0..200 {
            assert_eq!(std::fs::read(&policy.compose_files[0]).unwrap(), expected);
            let observed = load_container_policy(&checkout, Some("one")).unwrap();
            assert!(observed
                .read_compose(&observed.compose_files[0])
                .unwrap()
                .contains("example:"));
        }
        writer.join().unwrap();
    });
}

#[test]
fn declaration_bytes_are_part_of_the_generation() {
    let (_fixture, checkout, home) = fixture();
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, Some("one")).unwrap();
        let manifest = checkout.join("effigy.toml");
        let source =
            std::fs::read_to_string(&manifest).unwrap() + "\n# changed declaration input\n";
        std::fs::write(manifest, source).unwrap();
        let next = load_container_policy(&checkout, Some("one")).unwrap();
        assert_ne!(
            policy.render.unwrap().closure.digest(),
            next.render.unwrap().closure.digest()
        );
    });
}

#[test]
fn internal_path_consumers_resolve_generated_asset_mounts() {
    let (_fixture, checkout, home) = fixture();
    std::fs::write(checkout.join("effigy.toml"), "[containers]\ndefault='web'\n[containers.web]\nprimary_service='web'\n[containers.web.services.app]\ncatalog='php-fpm'\n[containers.web.services.web]\ncatalog='nginx'\nvariant='default'\n").unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).unwrap();
        let render = policy.render.as_ref().unwrap();
        assert!(
            render.closure.files().values().all(|bytes| {
                let text = String::from_utf8_lossy(bytes);
                !text.contains("EFFIGY_RENDER_ROOT") && !text.contains("EFFIGY_RENDER_")
            }),
            "a rendered generation carries no Effigy interpolation value"
        );
        let rendered: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap()).unwrap();
        let mut dockerfiles = rendered["services"]
            .as_mapping()
            .expect("services")
            .values()
            .filter_map(|service| {
                service
                    .get("build")?
                    .get("dockerfile")?
                    .as_str()
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        dockerfiles.sort();
        assert!(!dockerfiles.is_empty(), "GENERATED_DOCKERFILES_MISSING");
        for dockerfile in &dockerfiles {
            assert!(
                dockerfile.starts_with("./")
                    && !dockerfile.contains(&render.path.display().to_string()),
                "the generation names its own files relatively: {dockerfile}"
            );
        }
        let plan = crate::load_workspace_ownership_plan(&policy).unwrap();
        let config = plan
            .targets
            .iter()
            .find(|target| target.path == "/etc/nginx/conf.d/default.conf")
            .expect("actual Compose mount target");
        assert!(
            Path::new(config.source.as_ref().unwrap()).starts_with(&render.path),
            "source: {:?}, generation: {:?}",
            config.source,
            render.path
        );
        assert!(config.read_only);
        let asset = Path::new(config.source.as_ref().unwrap())
            .strip_prefix(&render.path)
            .unwrap();
        assert!(render
            .closure
            .files()
            .contains_key(&asset.to_string_lossy().to_string()));
        assert!(
            !render.path.exists(),
            "reading mount targets does not publish"
        );
    });
}

#[test]
fn prepare_establishes_owned_cache_paths_before_publication() {
    let (fixture, checkout, _) = fixture();
    let real = fixture.path().join("real/home");
    std::fs::create_dir_all(&real).unwrap();
    let alias = fixture.path().join("alias");
    std::os::unix::fs::symlink(real.parent().unwrap(), &alias).unwrap();
    let home = alias.join("home/.effigy");
    std::fs::write(checkout.join("effigy.toml"), "[containers]\ndefault='web'\n[containers.web]\nprimary_service='app'\nworking_dir='/workspace'\n[containers.web.services.app]\ncatalog='php-fpm'\n[systems.dev]\ndefault_workspace='app'\n[systems.dev.workspaces.app]\ncontainer='web'\n").unwrap();
    crate::with_test_effigy_home(&home, || {
        load_container_policy(&checkout, None).unwrap();
        assert!(!home.join("shared/composer-cache").exists());
        let prepared = prepare_container_policy(&checkout, None).unwrap();
        assert!(home.join("shared/composer-cache").exists());
        crate::policy::mounts::prepare_container_mount_sources(&prepared).unwrap();
        crate::policy::render::verify(&prepared).unwrap();
        let fresh = load_container_policy(&checkout, None).unwrap();
        assert_eq!(
            fresh.render.as_ref().unwrap().closure.digest(),
            prepared.render.as_ref().unwrap().closure.digest()
        );
    });
}

/// A rendered health check has to run as written on the backend it names
/// (effigy#324). `nerdctl compose` does not support the `CMD` exec form and
/// joins the argv into one unquoted shell command, so a nerdctl render writes
/// the quoted shell form itself. Docker keeps the exec form it honours.
#[test]
fn a_nerdctl_render_quotes_exec_form_health_checks_and_docker_keeps_them() {
    let (_fixture, checkout, home) = fixture();
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n    healthcheck:\n      test: [\"CMD\", \"node\", \"-e\", \"process.exit(0)\"]\n  shell:\n    image: example:two\n    healthcheck:\n      test: [\"CMD-SHELL\", \"pg_isready -U postgres\"]\n  off:\n    image: example:three\n    healthcheck:\n      test: [\"NONE\"]\n",
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let nerdctl = crate::compose::with_test_compose_backend(
            crate::compose::ComposeBackend::ColimaNerdctl,
            || prepare_container_policy(&checkout, Some("one")).unwrap(),
        );
        let rendered = healthcheck_tests(&nerdctl.read_compose(&nerdctl.compose_files[0]).unwrap());
        assert_eq!(
            rendered["app"],
            ["CMD-SHELL", "'node' '-e' 'process.exit(0)'"],
            "a nerdctl render quotes every exec-form argument"
        );
        assert_eq!(
            rendered["shell"],
            ["CMD-SHELL", "pg_isready -U postgres"],
            "a health check already in shell form is left as written"
        );
        assert_eq!(
            rendered["off"],
            ["NONE"],
            "a disabled health check is left as written"
        );

        let docker = crate::compose::with_test_compose_backend(
            crate::compose::ComposeBackend::Docker,
            || load_container_policy(&checkout, Some("one")).unwrap(),
        );
        let docker_rendered =
            healthcheck_tests(&docker.read_compose(&docker.compose_files[0]).unwrap());
        assert_eq!(
            docker_rendered["app"],
            ["CMD", "node", "-e", "process.exit(0)"],
            "Docker honours the exec form, so its render keeps it"
        );
    });
}

/// The runtime restarts a stopped service: every rendered service carries
/// `restart: unless-stopped` unless the stack declares its own policy.
#[test]
fn a_render_restarts_every_service_that_does_not_declare_a_policy() {
    let (_fixture, checkout, home) = fixture();
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n  db:\n    image: example:two\n    restart: \"no\"\n",
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = prepare_container_policy(&checkout, Some("one")).unwrap();
        let document: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap()).unwrap();
        assert_eq!(
            document["services"]["app"]["restart"],
            serde_yaml::Value::String("unless-stopped".into()),
            "a service without a policy is restarted by the runtime"
        );
        assert_eq!(
            document["services"]["db"]["restart"],
            serde_yaml::Value::String("no".into()),
            "a declared policy wins"
        );
    });
}

/// The `healthcheck.test` of each service in a rendered Compose document, as
/// the words Compose sees.
fn healthcheck_tests(document: &str) -> BTreeMap<String, Vec<String>> {
    let document: serde_yaml::Value = serde_yaml::from_str(document).unwrap();
    document["services"]
        .as_mapping()
        .unwrap()
        .iter()
        .filter_map(|(name, service)| {
            let test = service.get("healthcheck")?.get("test")?;
            Some((
                name.as_str().unwrap().to_owned(),
                match test {
                    serde_yaml::Value::Sequence(words) => words
                        .iter()
                        .map(|word| word.as_str().unwrap().to_owned())
                        .collect(),
                    serde_yaml::Value::String(command) => vec!["CMD-SHELL".into(), command.clone()],
                    other => panic!("unexpected healthcheck test: {other:?}"),
                },
            ))
        })
        .collect()
}
