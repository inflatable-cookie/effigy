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
        let mut inline = Vec::new();
        for service in rendered["services"]
            .as_mapping()
            .expect("services")
            .values()
        {
            let Some(build) = service.get("build").and_then(serde_yaml::Value::as_mapping) else {
                continue;
            };
            assert!(
                !build.contains_key("dockerfile"),
                "Compose resolves a build dockerfile against the context, so the render names its content instead: {build:?}"
            );
            inline.push(
                build["dockerfile_inline"]
                    .as_str()
                    .expect("inline Dockerfile"),
            );
        }
        inline.sort();
        assert!(!inline.is_empty(), "GENERATED_DOCKERFILES_MISSING");
        for dockerfile in &inline {
            assert!(
                dockerfile.contains("FROM "),
                "the generation renders the Dockerfile content itself: {dockerfile}"
            );
        }
        assert!(
            !render
                .closure
                .files()
                .keys()
                .any(|name| name.contains(".effigy-catalog")),
            "the Dockerfile is inlined, so no path copy is published: {:?}",
            render.closure.files().keys().collect::<Vec<_>>()
        );
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

/// The render hands Compose a file that names no restart policy at all, so the
/// runtime's restart manager cannot settle a container between Compose's create
/// and start (effigy#383); a policy the stack declares is what Compose reads.
#[test]
fn a_render_names_no_restart_policy_of_its_own() {
    let (_fixture, checkout, home) = fixture();
    std::fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n  db:\n    image: example:two\n    restart: \"no\"\n",
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = prepare_container_policy(&checkout, Some("one")).unwrap();
        let merged = merged_compose(&policy);
        assert!(
            merged["services"]["app"].get("restart").is_none(),
            "up applies the policy with the runtime's own update, after it started the stack"
        );
        assert_eq!(
            merged["services"]["db"]["restart"],
            serde_yaml::Value::String("no".into()),
            "a declared policy is the stack's own and stays byte for byte"
        );
        for file in &policy.compose_files {
            assert!(
                !policy
                    .read_compose(file)
                    .unwrap()
                    .contains("unless-stopped"),
                "{} must name no restart policy of Effigy's",
                file.display()
            );
        }
    });
}

/// The Compose model Compose itself reads: every `-f` file merged in order,
/// mappings merged key by key and every other value replaced by the later
/// file.
fn merged_compose(policy: &EffectiveContainerPolicy) -> serde_yaml::Value {
    let mut merged = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    for file in &policy.compose_files {
        let document: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(file).unwrap()).unwrap();
        merge(&mut merged, document);
    }
    merged
}

fn merge(into: &mut serde_yaml::Value, from: serde_yaml::Value) {
    match (into, from) {
        (serde_yaml::Value::Mapping(into), serde_yaml::Value::Mapping(from)) => {
            for (key, value) in from {
                match into.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (into, from) => *into = from,
    }
}

/// Every relative path a Compose input names resolves where the file was
/// read, not where the generation publishes it (effigy#371).
#[test]
fn a_render_resolves_every_relative_input_path_against_the_checkout() {
    let (fixture, checkout, home) = fixture();
    std::fs::create_dir_all(checkout.join("app")).unwrap();
    std::fs::write(checkout.join("app/Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::write(checkout.join("app.env"), "A=1\n").unwrap();
    std::fs::write(checkout.join("bind.txt"), "bind\n").unwrap();
    std::fs::write(checkout.join("secret.txt"), "secret\n").unwrap();
    std::fs::write(
        checkout.join("compose.yml"),
        r#"services:
  app:
    build:
      dockerfile: Dockerfile
"#,
    )
    .unwrap();
    // A user override is a later `-f` file: its own relative paths resolve
    // against the project directory too, and it inherits the base's build
    // context rather than naming one.
    std::fs::create_dir_all(checkout.join(".effigy/runtime/compose")).unwrap();
    std::fs::write(
        checkout.join(".effigy/runtime/compose/compose.override.yml"),
        r#"services:
  app:
    build:
      additional_contexts:
        - shared=./app
    env_file:
      - path: ./app.env
        required: false
    volumes:
      - type: bind
        source: ./bind.txt
        target: /bind.txt
secrets:
  token:
    file: ./secret.txt
configs:
  settings:
    file: ./app.env
"#,
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = prepare_container_policy(&checkout, Some("one")).unwrap();
        let document: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap()).unwrap();
        let service = &document["services"]["app"];
        // A build mapping with no context builds from the checkout, never from
        // the generation that holds the Compose file.
        let context = service["build"]["context"]
            .as_str()
            .expect("a missing build context names the directory Compose would have used");
        assert!(
            Path::new(context).is_absolute() && !context.contains(".effigy/state"),
            "a missing build context names the pre-render project directory, not the generation: {context}"
        );
        assert_eq!(
            Path::new(context),
            checkout.join(".effigy/runtime/compose"),
            "the context is the directory the Compose input was read from"
        );
        let override_document: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[1]).unwrap()).unwrap();
        assert_eq!(
            override_document["services"]["app"]["build"]["additional_contexts"][0],
            serde_yaml::Value::String(format!(
                "shared={}",
                checkout.join(".effigy/runtime/compose/app").display()
            )),
            "an additional context resolves against the file that names it"
        );
        assert!(
            override_document["services"]["app"]["build"]
                .get("context")
                .is_none(),
            "an overlay never names a build context of its own"
        );
        assert_eq!(
            override_document["services"]["app"]["env_file"][0]["path"],
            serde_yaml::Value::String(
                checkout
                    .join(".effigy/runtime/compose/app.env")
                    .display()
                    .to_string()
            ),
            "a long-syntax env_file entry resolves against the file that names it"
        );
        assert_eq!(
            override_document["services"]["app"]["volumes"][0]["source"],
            serde_yaml::Value::String(
                checkout
                    .join(".effigy/runtime/compose/bind.txt")
                    .display()
                    .to_string()
            ),
            "a long-syntax bind volume resolves against the file that names it"
        );
        assert_eq!(
            override_document["secrets"]["token"]["file"],
            serde_yaml::Value::String(
                checkout
                    .join(".effigy/runtime/compose/secret.txt")
                    .display()
                    .to_string()
            ),
            "a top-level secret file resolves against the file that names it"
        );
        assert_eq!(
            override_document["configs"]["settings"]["file"],
            serde_yaml::Value::String(
                checkout
                    .join(".effigy/runtime/compose/app.env")
                    .display()
                    .to_string()
            ),
            "a top-level config file resolves against the file that names it"
        );
        let _ = fixture;
    });
}

/// Mapping-form `additional_contexts: { src: ../frontend }` resolves against
/// the declaring file, the same as sequence-form `src=../frontend`.
#[test]
fn mapping_and_sequence_additional_contexts_resolve_against_the_checkout() {
    let (fixture, checkout, home) = fixture();
    std::fs::create_dir_all(checkout.join("app")).unwrap();
    std::fs::write(checkout.join("app/Dockerfile"), "FROM scratch\n").unwrap();
    let frontend = checkout.join(".effigy/runtime/frontend");
    std::fs::create_dir_all(&frontend).unwrap();
    std::fs::write(
        checkout.join("compose.yml"),
        r#"services:
  app:
    build:
      dockerfile: Dockerfile
"#,
    )
    .unwrap();
    std::fs::create_dir_all(checkout.join(".effigy/runtime/compose")).unwrap();
    std::fs::write(
        checkout.join(".effigy/runtime/compose/compose.override.yml"),
        r#"services:
  mapping:
    build:
      additional_contexts:
        src: ../frontend
        image: docker-image://example/base:latest
        abs: /already/absolute
        from: service:app
  sequence:
    build:
      additional_contexts:
        - src=../frontend
        - image=docker-image://example/base:latest
        - abs=/already/absolute
"#,
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = prepare_container_policy(&checkout, Some("one")).unwrap();
        let override_document: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[1]).unwrap()).unwrap();
        let expected = frontend.canonicalize().unwrap();
        let expected = expected.display().to_string();
        assert!(
            !expected.contains(".effigy/state"),
            "the declaring checkout owns the path, not the generation: {expected}"
        );
        assert_eq!(
            override_document["services"]["mapping"]["build"]["additional_contexts"]["src"],
            serde_yaml::Value::String(expected.clone()),
            "mapping-form additional_contexts resolve against the file that names them"
        );
        assert_eq!(
            override_document["services"]["sequence"]["build"]["additional_contexts"][0],
            serde_yaml::Value::String(format!("src={expected}")),
            "sequence-form additional_contexts resolve against the file that names them"
        );
        assert_eq!(
            override_document["services"]["mapping"]["build"]["additional_contexts"]["image"],
            serde_yaml::Value::String("docker-image://example/base:latest".to_owned()),
            "a remote mapping-form context stays as written"
        );
        assert_eq!(
            override_document["services"]["sequence"]["build"]["additional_contexts"][1],
            serde_yaml::Value::String("image=docker-image://example/base:latest".to_owned()),
            "a remote sequence-form context stays as written"
        );
        assert_eq!(
            override_document["services"]["mapping"]["build"]["additional_contexts"]["abs"],
            serde_yaml::Value::String("/already/absolute".to_owned()),
            "an already-absolute mapping-form context stays as written"
        );
        assert_eq!(
            override_document["services"]["sequence"]["build"]["additional_contexts"][2],
            serde_yaml::Value::String("abs=/already/absolute".to_owned()),
            "an already-absolute sequence-form context stays as written"
        );
        assert_eq!(
            override_document["services"]["mapping"]["build"]["additional_contexts"]["from"],
            serde_yaml::Value::String("service:app".to_owned()),
            "a mapping-form service: named context stays as written"
        );
        let _ = fixture;
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

/// A catalog service with a `build:` section builds from the Dockerfile the
/// generation holds. Compose resolves `build.dockerfile` against the build
/// context, not the project directory, so the render names the content instead
/// of a path that would point outside the generation (effigy#364).
#[test]
fn a_catalog_build_service_renders_its_dockerfile_content() {
    let (_fixture, checkout, home) = fixture();
    std::fs::write(
        checkout.join("effigy.toml"),
        r#"
[containers]
default = "stack"
[containers.stack]
primary_service = "workspace"
working_dir = "/workspace-root/app"
[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
"#,
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).unwrap();
        let render = policy.render.as_ref().unwrap();
        let rendered: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap()).unwrap();
        let build = rendered["services"]["workspace"]["build"]
            .as_mapping()
            .expect("the workspace service builds an image");
        assert!(
            !build.contains_key("dockerfile"),
            "a dockerfile path would resolve against the build context, not the generation: {build:?}"
        );
        let inline = build["dockerfile_inline"]
            .as_str()
            .expect("the Dockerfile content is rendered");
        assert!(
            inline.contains("FROM ") && inline.contains("BUN_VERSION"),
            "the rendered content is the catalog Dockerfile: {inline}"
        );
        assert_eq!(
            build["args"][serde_yaml::Value::String("RUST_VERSION".into())].as_str(),
            Some("1.91"),
            "the build arguments are still rendered beside the content"
        );
        assert!(
            !render
                .closure
                .files()
                .keys()
                .any(|name| name.contains(".effigy-catalog")),
            "no Dockerfile path is published: {:?}",
            render.closure.files().keys().collect::<Vec<_>>()
        );
        // The build context stays the checkout, which is where the
        // Dockerfile's own COPY inputs live.
        let context = Path::new(build["context"].as_str().expect("build context"));
        assert_eq!(
            context.canonicalize().unwrap(),
            checkout.canonicalize().unwrap(),
            "the build context is the checkout"
        );
    });
}

/// Compose interpolates every string in the Compose file, `dockerfile_inline`
/// included: the catalog Dockerfile's `WORKDIR ${WORKSPACE_MOUNT}` and its
/// shell `$line` were blanked before BuildKit saw them, and the build failed
/// with `WORKDIR requires exactly one argument` on the Acowtancy stack. Every
/// `$` is doubled, which Compose reads back as one literal dollar (effigy#366).
#[test]
fn a_catalog_dockerfile_keeps_every_dollar_through_compose_interpolation() {
    let (_fixture, checkout, home) = fixture();
    let content = "FROM busybox\nARG APP_DIR=/app\nWORKDIR ${APP_DIR}\nRUN echo \"$line\" >> /tmp/log\nRUN echo \"${bridge_sock}\"\n";
    let catalog = checkout.join("infra/dev/catalog/dollar");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        catalog.join("service.toml"),
        "[service]\nname = \"dollar\"\ndescription = \"a Dockerfile that uses its own dollars\"\n",
    )
    .unwrap();
    std::fs::write(
        catalog.join("compose.fragment.yml"),
        "services:\n  '{{ service_name }}':\n    build:\n      context: '{{ repo_root }}'\n      dockerfile: '{{ catalog_path }}/Dockerfile'\n",
    )
    .unwrap();
    std::fs::write(catalog.join("Dockerfile"), content).unwrap();
    std::fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"stack\"\n[containers.stack]\nprimary_service = \"app\"\n[containers.stack.services.app]\ncatalog = \"dollar\"\n",
    )
    .unwrap();
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).unwrap();
        let rendered: serde_yaml::Value =
            serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap()).unwrap();
        let inline = rendered["services"]["app"]["build"]["dockerfile_inline"]
            .as_str()
            .expect("the Dockerfile content is rendered inline");
        assert_eq!(
            inline.matches('$').count(),
            2 * content.matches('$').count(),
            "every dollar is escaped: {inline}"
        );
        assert!(
            inline.contains("ARG APP_DIR=/app"),
            "the content is otherwise unchanged: {inline}"
        );
        assert_eq!(
            inline.replace("$$", "$"),
            content,
            "Compose reads the escaped content as the catalog's own bytes: {inline}"
        );
    });
}

#[test]
fn workspace_host_alias_render_is_pure_frozen_idempotent_and_refuses_conflicts() {
    let (fixture, checkout, home) = fixture();
    crate::with_test_effigy_home(&home, || {
        let policy = load_container_policy(&checkout, None).expect("policy");
        let before = snapshot(fixture.path());
        for address in ["host-gateway", "192.0.2.12", "2001:db8::12"] {
            let next = policy
                .with_workspace_host_alias(address)
                .expect("explicit observed plan");
            let render = next.render.as_ref().expect("render");
            assert_ne!(
                render.closure.digest(),
                policy.render.as_ref().expect("original").closure.digest()
            );
            assert!(next
                .compose_files
                .iter()
                .all(|file| file.starts_with(&render.path)));
            let metadata: serde_json::Value =
                serde_json::from_slice(&render.closure.files()["inputs/environment.json"])
                    .expect("frozen Compose inventory");
            let names: Vec<String> = serde_json::from_value(metadata["compose_files"].clone())
                .expect("ordered input list");
            assert_eq!(
                next.compose_files,
                names
                    .iter()
                    .map(|name| render.path.join(name))
                    .collect::<Vec<_>>(),
                "the alias append and the sealed publisher must agree on every argv input"
            );
            let override_path = next.compose_files.last().expect("override");
            let document: serde_yaml::Value =
                serde_yaml::from_str(&next.read_compose(override_path).expect("in-memory closure"))
                    .expect("yaml");
            assert_eq!(
                document["services"]["app"]["extra_hosts"]["effigy-host"].as_str(),
                Some(address)
            );
            assert_eq!(
                next.with_workspace_host_alias(address)
                    .expect("retry")
                    .render
                    .expect("render")
                    .closure
                    .digest(),
                render.closure.digest()
            );
            assert!(next.with_workspace_host_alias("192.0.2.99").is_err());
            assert_eq!(
                snapshot(fixture.path()),
                before,
                "render creates no network, files or host authority"
            );
        }
        for address in ["", "guessed-host", "0.0.0.0", "::", "224.0.0.1"] {
            assert!(policy.with_workspace_host_alias(address).is_err());
        }
    });
}

#[test]
fn frozen_backend_keeps_the_recorded_context_when_the_current_context_changes() {
    let _lock = crate::test_env_lock();
    let (_fixture, checkout, home) = fixture();
    let _context = effigy_test_support::env::ScopedEnv::set("DOCKER_CONTEXT", "remote-A");
    crate::with_test_effigy_home(&home, || {
        crate::compose::with_test_compose_backend(crate::compose::ComposeBackend::Docker, || {
            prepare_container_policy(&checkout, None).unwrap();
        });
        std::env::set_var("DOCKER_CONTEXT", "default");
        let policy = load_container_policy(&checkout, None).unwrap();
        let bytes = &policy.render.as_ref().unwrap().closure.files()["inputs/environment.json"];
        let metadata: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(metadata["backend_context"], "remote-A");
        assert_eq!(metadata["backend_id"], "docker-compose");
    });
}
