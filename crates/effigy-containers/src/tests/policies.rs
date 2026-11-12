use super::*;

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

fn setup_workspace_app_path_bundle(root: &std::path::Path) {
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../effigy-manifest/tests/fixtures/workspace-app-bundle");
    let bundle_dir = root.join("bundles/workspace-app");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
}

fn policy_with_runtime_dns_overlay(
    name: &str,
    compose_yaml: &str,
) -> (
    std::path::PathBuf,
    crate::policy::model::EffectiveContainerPolicy,
) {
    let root = temp_repo(name);
    let compose_file = root.join("infra/dev/docker-compose.yml");
    fs::create_dir_all(compose_file.parent().expect("compose parent")).expect("mkdir compose dir");
    fs::write(&compose_file, compose_yaml).expect("write synthetic Compose input");

    let mut policy = crate::policy::model::test_policy(&root);
    let mut compose_files = vec![compose_file];
    let mut draft = crate::policy::render::Draft::default();
    crate::runtime::dns::materialize_runtime_dns_override(
        &root,
        &policy.name,
        &policy.profile,
        &crate::runtime::dns::RuntimeDnsOverrideRoutes::default(),
        &mut compose_files,
        &mut draft,
    )
    .expect("render runtime DNS override");

    let overlay = compose_files.last().expect("generated DNS override");
    let rendered_overlay = draft.read(overlay).expect("read generated DNS override");
    fs::create_dir_all(overlay.parent().expect("DNS override parent"))
        .expect("mkdir DNS override dir");
    fs::write(overlay, rendered_overlay).expect("materialize generated DNS override in fixture");
    policy.compose_files = compose_files;
    (root, policy)
}

/// One selection: the container a manifest resolves for a requested name, or
/// the reason it refuses to guess.
#[test]
fn container_selection_cases() {
    #[derive(Clone, Copy)]
    enum Selection {
        /// The selected container keeps this name.
        Resolves(&'static str),
        /// The selection is refused and its message names this token.
        Refused(&'static str),
    }

    fn default_render(policy: &crate::EffectiveContainerPolicy) {
        assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
        assert_eq!(
            policy.compose_file_display(),
            ".effigy/runtime/compose/web.generated.compose.yml, .effigy/runtime/dns/web.compose.override.yml"
        );
        assert_eq!(policy.dns_domain, None);
        assert!(!policy.dns_tls);
        assert_eq!(policy.dns_port, None);
        let render = policy.render.as_ref().unwrap();
        assert_eq!(
            policy.compose_files,
            vec![
                render
                    .path
                    .join("checkout/.effigy/runtime/compose/web.generated.compose.yml"),
                render
                    .path
                    .join("checkout/.effigy/runtime/dns/web.compose.override.yml"),
            ]
        );
    }

    type Check = fn(&crate::EffectiveContainerPolicy);
    type Row = (
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
        Option<&'static str>,
        Selection,
        Option<Check>,
    );
    let cases: [Row; 6] = [
        (
            "the container default",
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
            &[(
                "infra/dev/docker-compose.yml",
                "services:\n  app:\n    image: example\n",
            )],
            None,
            Selection::Resolves("web"),
            Some(default_render),
        ),
        (
            "the system workspace binding when no container default exists",
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/workspace"
"#,
            &[(
                "infra/dev/docker-compose.yml",
                "services:\n  app:\n    image: example\n",
            )],
            None,
            Selection::Resolves("web"),
            None,
        ),
        (
            "the container default over the system binding",
            r#"
[containers]
default = "api"

[containers.api]
compose_file = "infra/dev/api-compose.yml"
primary_service = "app"
working_dir = "/workspace"

[containers.web]
compose_file = "infra/dev/web-compose.yml"
primary_service = "app"
working_dir = "/workspace"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
"#,
            &[
                (
                    "infra/dev/api-compose.yml",
                    "services:\n  app:\n    image: example\n",
                ),
                (
                    "infra/dev/web-compose.yml",
                    "services:\n  app:\n    image: example\n",
                ),
            ],
            None,
            Selection::Resolves("api"),
            None,
        ),
        (
            "a named container without a default",
            r#"
[containers]

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
            &[(
                "infra/dev/docker-compose.yml",
                "services:\n  app:\n    image: example\n",
            )],
            Some("web"),
            Selection::Resolves("web"),
            None,
        ),
        (
            "an omitted name with neither default",
            r#"
[containers]

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
            &[(
                "infra/dev/docker-compose.yml",
                "services:\n  app:\n    image: example\n",
            )],
            None,
            Selection::Refused("container name omitted but neither"),
            None,
        ),
        (
            "a manifest with no container registry",
            "[tasks]\n",
            &[],
            None,
            Selection::Refused("[containers]"),
            None,
        ),
    ];
    for (name, manifest, files, requested, expected, check) in cases {
        let root = temp_repo("container-selection");
        fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
        for (path, body) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().expect("compose parent")).expect("mkdir compose dir");
            fs::write(path, body).expect("write compose");
        }
        match expected {
            Selection::Resolves(container) => {
                let policy = load_container_policy(&root, requested).expect(name);
                assert_eq!(policy.name, container, "{name}");
                if let Some(check) = check {
                    check(&policy);
                }
            }
            Selection::Refused(token) => {
                let error = load_container_policy(&root, requested).expect_err(name);
                assert!(
                    matches!(error, ContainerPolicyError::TaskInvocation(_)),
                    "{name}: {error:?}"
                );
                assert!(error.to_string().contains(token), "{name}: {error}");
            }
        }
    }
}

/// One compose project name: the catalog alias, a sanitized component, an
/// explicit name, the checkout name when only bundle defaults carry an alias,
/// and two containers that must not collide.
#[test]
fn colima_mount_budgets_refuse_over_budget_payloads() {
    // Phase 1: a direct-compose mount payload over budget is refused, with
    // path-free contributors, while Docker is unaffected.
    {
        let _lock = crate::test_env_lock();
        let root = temp_repo("colima-mount-budget");
        let volumes = (0..72)
            .map(|index| {
                format!(
                    "      - workspace-volume-{index:02}:/workspace-root/acme/some/really/long/path/{index:02}/node_modules\n"
                )
            })
            .collect::<String>();
        let declared = (0..72)
            .map(|index| format!("  workspace-volume-{index:02}: {{}}\n"))
            .collect::<String>();
        fs::write(
            root.join("effigy.toml"),
            r#"
    [containers]
    default = "web"

    [containers.web]
    compose_file = "infra/dev/docker-compose.yml"
    primary_service = "app"
    "#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(
            root.join("infra/dev/docker-compose.yml"),
            format!(
                "services:\n  app:\n    image: alpine\n    volumes:\n{volumes}volumes:\n{declared}"
            ),
        )
        .expect("compose");

        let mut policy = load_container_policy(&root, None).expect("policy");
        let overlay = root.join("infra/dev/compose.overlay.yml");
        fs::write(
            &overlay,
            "services:\n  app:\n    volumes:\n      - type: bind\n        source: ./replacement-cache\n        target: /workspace-root/acme/some/really/long/path/00/node_modules\n        read_only: true\n",
        )
        .expect("overlay");
        policy.compose_files.push(overlay);
        let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
        unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
        }
        let error = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
            crate::validate_compose_backend_runtime(&root, &policy).expect_err("should fail")
        });
        match previous {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
            },
            None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
        }

        assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
        assert!(
            error
                .to_string()
                .contains("estimated nerdctl/mounts label size"),
            "unexpected refusal: {error}"
        );
        assert!(error
            .to_string()
            .contains("reduce optional mounts or workspace isolation mounts"));
        assert!(error
            .to_string()
            .contains("Largest contributors (paths omitted)"));
        assert!(!error
            .to_string()
            .contains("/workspace-root/acme/some/really/long/path"));

        with_test_compose_backend(ComposeBackend::Docker, || {
            crate::validate_compose_backend_runtime(&root, &policy)
                .expect("Docker backend must not inherit nerdctl label limit");
        });
    }
    // Phase 2: a null-DNS overlay passes under budget and refuses over
    // budget or when scalar, and the generated overlay stays valid YAML.
    {
        let _lock = crate::test_env_lock();

        let validate = |name: &str, mount_count: usize| {
            // Match the measured 21-entry synthetic shape for the passing case;
            // the larger fixture proves the same generated overlay still reaches
            // the mount-budget refusal instead of being mistaken for bad input.
            let source = format!("/{}", "s".repeat(61));
            let volumes = (0..mount_count)
                .map(|index| {
                    let target = format!("/{}{index:02}", "d".repeat(25));
                    format!(
                        "      - type: bind\n        source: {source}\n        target: {target}\n        read_only: {}\n",
                        index + 1 == mount_count
                    )
                })
                .collect::<String>();
            let compose = format!(
                "services:\n  app:\n    image: example.invalid/synthetic\n    network_mode: none\n    volumes:\n{volumes}"
            );
            let (root, policy) = policy_with_runtime_dns_overlay(name, &compose);
            let dns_overlay = policy
                .compose_files
                .last()
                .expect("generated runtime DNS override");
            let rendered_overlay = policy
                .read_compose(dns_overlay)
                .expect("read generated runtime DNS override");
            let parsed: serde_yaml::Value =
                serde_yaml::from_str(&rendered_overlay).expect("generated overlay is valid YAML");
            assert!(
                parsed["services"]["app"].is_null(),
                "network_mode without applicable DNS or route fields emits a null service: {rendered_overlay}"
            );

            let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
            unsafe {
                std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
            }
            let result = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
                crate::validate_compose_backend_runtime(&root, &policy)
            });
            match previous {
                Some(value) => unsafe {
                    std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
                },
                None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
            }
            result
        };

        validate("colima-null-dns-under-budget", 21)
            .expect("a supported under-budget mount set must pass preflight");
        let over_budget = validate("colima-null-dns-over-budget", 30)
            .expect_err("an over-budget mount set must still refuse before apply");
        assert!(over_budget
            .to_string()
            .contains("estimated nerdctl/mounts label size"));

        // A null generated service is an empty mapping, but malformed non-null
        // service values remain fail-closed for the Colima fallback.
        let (root, policy) = policy_with_runtime_dns_overlay(
            "colima-nonmapping-primary-service",
            "services:\n  app: 42\n",
        );
        let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
        unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
        }
        let malformed = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
            crate::validate_compose_backend_runtime(&root, &policy)
                .expect_err("a scalar primary service must still refuse")
        });
        match previous {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
            },
            None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
        }
        assert!(malformed
            .to_string()
            .contains("primary service definition is not a mapping"));
    }
    // Phase 3: primary-service tmpfs entries count in the nerdctl label budget.
    {
        let _lock = crate::test_env_lock();
        let root = temp_repo("colima-tmpfs-budget");
        fs::write(
            root.join("effigy.toml"),
            r#"
    [containers]
    default = "web"

    [containers.web]
    compose_file = "infra/dev/docker-compose.yml"
    primary_service = "app"
    "#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");

        let validate_colima = |content: &str| {
            fs::write(root.join("infra/dev/docker-compose.yml"), content).expect("write compose");
            let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
            unsafe {
                std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
            }
            let result = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
                let policy = load_container_policy(&root, None).expect("policy");
                crate::validate_compose_backend_runtime(&root, &policy)
            });
            match previous {
                Some(value) => unsafe {
                    std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
                },
                None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
            }
            result
        };

        let generated_secret_runtime = "services:\n  app:\n    image: example.invalid/synthetic\n    tmpfs:\n      - /run/effigy/secrets\n";
        validate_colima(generated_secret_runtime)
            .expect("a small generated tmpfs runtime stays inside the budget");

        let oversized = (0..40)
            .map(|index| {
                format!(
                    "      - /run/effigy/secrets/runtime/cache/segment/{index:02}/node-modules\n"
                )
            })
            .collect::<String>();
        let oversized_compose = format!(
            "services:\n  app:\n    image: example.invalid/synthetic\n    tmpfs:\n{oversized}"
        );
        let error = validate_colima(&oversized_compose)
            .expect_err("enough tmpfs entries must exceed the nerdctl label budget");
        assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
        assert!(
            error
                .to_string()
                .contains("estimated nerdctl/mounts label size"),
            "unexpected refusal: {error}"
        );
        assert!(!error.to_string().contains("/run/effigy/secrets"));

        with_test_compose_backend(ComposeBackend::Docker, || {
            let policy = load_container_policy(&root, None).expect("policy");
            crate::validate_compose_backend_runtime(&root, &policy)
                .expect("Docker must not inherit Colima mount-budget refusals");
        });
    }
}

#[test]
fn creation_policy_resolves_workspace_mounts_for_creation() {
    // Phase 1: late and absolute extra mounts resolve with decisions.
    {
        let root = temp_repo("workspace-mount-create-direct");
        let absolute_source = root.join("absolute-extra");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
    [containers]
    default = "web"

    [containers.web]
    compose_file = "compose.yml"
    primary_service = "app"

    [systems.dev]
    default_workspace = "app"

    [systems.dev.workspaces.app]
    container = "web"
    working_dir = "/workspace-root/app"
    mounts = [
      "late:/workspace-root/late:ro,cached",
      {{ source = "{}", target = "/workspace-root/absolute-extra", options = ["ro"] }}
    ]
    "#,
                absolute_source.display()
            ),
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let initially_loaded = load_container_policy(&root, Some("web")).expect("initial policy");
        let initial_compose = initially_loaded
            .read_compose(&initially_loaded.compose_files[0])
            .expect("initial workspace compose");
        assert!(!initial_compose.contains("/workspace-root/late"));
        assert!(!initial_compose.contains("/workspace-root/absolute-extra"));

        fs::create_dir(root.join("late")).expect("preparation creates late source");
        fs::create_dir(&absolute_source).expect("preparation creates absolute source");

        let (creation_policy, decisions) =
            load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
        let created_compose = creation_policy
            .read_compose(&creation_policy.compose_files[0])
            .expect("creation workspace compose");
        let canonical_late = root.join("late").canonicalize().expect("canonical late");
        let canonical_absolute = absolute_source.canonicalize().expect("canonical absolute");

        assert!(created_compose.contains(&format!(
            "{}:/workspace-root/late:ro,cached",
            canonical_late.display()
        )));
        assert!(created_compose.contains(&format!(
            "{}:/workspace-root/absolute-extra:ro",
            canonical_absolute.display()
        )));
        assert_eq!(decisions.len(), 2);
        assert!(decisions.iter().all(|decision| {
            decision.decision == WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
        }));
        assert!(decisions.iter().any(|decision| {
            decision.source == canonical_late.display().to_string()
                && decision.target == "/workspace-root/late"
        }));
        assert!(decisions.iter().any(|decision| {
            decision.source == canonical_absolute.display().to_string()
                && decision.target == "/workspace-root/absolute-extra"
        }));
    }
    // Phase 2: a symlink mount source canonicalizes into compose.
    {
        use std::os::unix::fs::symlink;

        let root = temp_repo("workspace-mount-create-symlink");
        let real_source = root.join("actual-source");
        let link_source = root.join("linked-source");
        fs::create_dir(&real_source).expect("create source");
        symlink(&real_source, &link_source).expect("create source symlink");
        fs::write(
            root.join("effigy.toml"),
            r#"
    [containers]
    default = "web"

    [containers.web]
    compose_file = "compose.yml"
    primary_service = "app"

    [systems.dev]
    default_workspace = "app"

    [systems.dev.workspaces.app]
    container = "web"
    working_dir = "/workspace-root/app"
    mounts = ["linked-source:/workspace-root/linked:ro"]
    "#,
        )
        .expect("write manifest");
        fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let (policy, decisions) =
            load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("read compose");
        let canonical = real_source.canonicalize().expect("canonical source");

        assert!(compose.contains(&format!(
            "{}:/workspace-root/linked:ro",
            canonical.display()
        )));
        assert!(!compose.contains(&format!(
            "{}:/workspace-root/linked:ro",
            link_source.display()
        )));
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].source, canonical.display().to_string());
    }
    // Phase 3: generated compose picks up the late extra mount.
    {
        with_temp_effigy_home("workspace-mount-create-generated", |_| {
            let _lock = crate::test_env_lock();
            let root = temp_repo("workspace-mount-create-generated");
            fs::write(
                root.join("effigy.toml"),
                r#"
    [containers]
    default = "web"

    [containers.web]
    primary_service = "app"

    [containers.web.services.app]
    catalog = "php-fpm"
    version = "8.3"

    [systems.dev]
    default_workspace = "app"

    [systems.dev.workspaces.app]
    container = "web"
    working_dir = "/var/www/html"
    mounts = ["late:/var/www/late:ro"]
    "#,
            )
            .expect("write manifest");

            let initial = load_container_policy(&root, Some("web")).expect("initial policy");
            assert_eq!(initial.compose_source, EffectiveComposeSource::Generated);
            fs::create_dir(root.join("late")).expect("preparation creates source");

            let (creation_policy, decisions) =
                load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
            let compose = creation_policy
                .read_compose(&creation_policy.compose_files[0])
                .expect("creation workspace compose");
            let canonical = root.join("late").canonicalize().expect("canonical source");
            let backend = crate::ContainerManager::defaults();
            let request = crate::ContainerManagerRequest::new(&root)
                .backend_override(crate::BackendId::docker_compose());
            let plan = backend
                .compose_invocation_plan(
                    &request,
                    &crate::ContainerBackendDetection::new(false),
                    &creation_policy.profile,
                    &crate::compose::compose_up_args(&creation_policy),
                    crate::ContainerAction::Activate,
                    "docker compose up",
                )
                .expect("generated compose backend plan");
            let captured_compose = plan
                .args
                .windows(2)
                .find(|args| args[0] == "-f")
                .map(|args| PathBuf::from(&args[1]))
                .expect("backend plan captures generated compose file");

            assert!(compose.contains(&format!("{}:/var/www/late:ro", canonical.display())));
            assert_eq!(captured_compose, creation_policy.compose_files[0]);
            assert!(fs::read_to_string(captured_compose)
                .expect("read captured generated compose")
                .contains(&format!("{}:/var/www/late:ro", canonical.display())));
            assert_eq!(decisions.len(), 1);
            assert_eq!(
                decisions[0].decision,
                WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
            );
        });
    }
}

#[test]
fn project_name_cases() {
    #[derive(Clone, Copy)]
    enum Expects {
        /// A single container resolves to this project name.
        One(&'static str),
        /// The alias comes only from bundle defaults, so the checkout names it.
        RepoName,
        /// Two containers keep these distinct project names.
        Two(&'static str, &'static str),
    }

    fn single_container(alias: &str, project_name: &str) -> String {
        format!(
            r#"
[catalog]
alias = "{alias}"

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
{project_name}
"#
        )
    }

    type Row = (&'static str, fn(&Path) -> String, Expects);
    let cases: [Row; 5] = [
        (
            "a catalog alias names a single container",
            |_| single_container("workspace-app-reference", ""),
            Expects::One("workspace-app-reference-dev"),
        ),
        (
            "default project name components are sanitized",
            |_| single_container("R7-webCore", ""),
            Expects::One("r7-webcore-dev"),
        ),
        (
            "an explicit project name is sanitized",
            |_| {
                single_container(
                    "workspace-app-reference",
                    "project_name = \"r7-webCore-decodelabs-library\"",
                )
            },
            Expects::One("r7-webcore-decodelabs-library"),
        ),
        (
            "an alias that only comes from bundle defaults leaves the checkout name",
            |root| {
                let bundle = root.with_file_name("bundle-defaults-alias-bundle");
                fs::create_dir_all(&bundle).expect("mkdir bundle");
                fs::write(
                    bundle.join("bundle.toml"),
                    r#"
[bundle]
name = "legacy-site"
description = "legacy site fixture"
"#,
                )
                .expect("write bundle descriptor");
                fs::write(
                    bundle.join("effigy.toml"),
                    r#"
[catalog]
alias = "root"
"#,
                )
                .expect("write bundle defaults");
                format!(
                    r#"
[bundle]
base = {{ type = "path", dir = "{}" }}

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
                    bundle.display()
                )
            },
            Expects::RepoName,
        ),
        (
            "two containers keep distinct project names",
            |_| {
                r#"
[catalog]
alias = "workspace-app-reference"

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.worker]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "jobs"
"#
                .to_owned()
            },
            Expects::Two(
                "workspace-app-reference-web-dev",
                "workspace-app-reference-worker-dev",
            ),
        ),
    ];
    for (name, manifest, expects) in cases {
        let root = temp_repo("project-name");
        fs::write(root.join("effigy.toml"), manifest(&root)).expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(
            root.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .expect("compose");

        match expects {
            Expects::One(project_name) => {
                let policy = {
                    let _lock = crate::test_env_lock();
                    load_container_policy(&root, None).expect(name)
                };
                assert_eq!(policy.project_name, project_name, "{name}");
            }
            Expects::RepoName => {
                let policy = {
                    let _lock = crate::test_env_lock();
                    load_container_policy(&root, None).expect(name)
                };
                // A compose project name is the fixture's repo name, lowercased.
                let expected = format!(
                    "{}-dev",
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap()
                        .to_lowercase()
                );
                assert_eq!(policy.project_name, expected, "{name}");
            }
            Expects::Two(web, worker) => {
                let policies = load_all_container_policies(&root).expect(name);
                assert_eq!(policies.len(), 2, "{name}");
                assert_eq!(policies[0].name, "web", "{name}");
                assert_eq!(policies[0].project_name, web, "{name}");
                assert_eq!(policies[1].name, "worker", "{name}");
                assert_eq!(policies[1].project_name, worker, "{name}");
            }
        }
    }
    // A duplicate effective project name is refused by name.
    {
        let root = temp_repo("duplicate-project-names");
        fs::write(
            root.join("effigy.toml"),
            r#"
    [catalog]
    alias = "workspace-app-reference"

    [containers]
    default = "web"

    [containers.web]
    compose_file = "infra/dev/docker-compose.yml"
    primary_service = "app"
    project_name = "workspace-app-reference-dev"

    [containers.worker]
    compose_file = "infra/dev/docker-compose.yml"
    primary_service = "jobs"
    project_name = "workspace-app-reference-dev"
    "#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(
            root.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .expect("compose");

        let error = load_container_policy(&root, None).expect_err("should fail");
        match error {
            ContainerPolicyError::TaskInvocation(message) => assert_eq!(
                message,
                "containers must resolve to unique `project_name` values when more than one container is declared; duplicate effective project names: `workspace-app-reference-dev` for containers `web`, `worker`"
            ),
            other => panic!("expected the uniqueness refusal, got {other:?}"),
        }
    }
}

#[test]
fn creation_policy_skips_removed_optional_source_without_creating_it() {
    let root = temp_repo("workspace-mount-create-removed");
    let source = root.join("early");
    fs::create_dir(&source).expect("create initial source");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["early:/workspace-root/early:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let _initial = load_container_policy(&root, Some("web")).expect("initial policy");
    fs::remove_dir_all(&source).expect("preparation removes source");

    let (creation_policy, decisions) =
        load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
    let created_compose = creation_policy
        .read_compose(&creation_policy.compose_files[0])
        .expect("creation workspace compose");

    assert!(
        !source.exists(),
        "resolver must not recreate the host source"
    );
    assert!(!created_compose.contains("/workspace-root/early"));
    assert_eq!(decisions.len(), 1);
    assert_eq!(
        decisions[0].decision,
        WorkspaceExtraMountDecisionKind::SkippedSourceAbsent
    );
    assert_eq!(
        decisions[0].source,
        source
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(source.file_name().unwrap())
            .display()
            .to_string()
    );
    assert_eq!(decisions[0].target, "/workspace-root/early");
}

#[test]
fn a_creation_policy_load_reports_each_preparation_stage_in_order() {
    let root = temp_repo("creation-policy-stages");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let mut stages = Vec::new();
    let (_policy, _decisions) =
        crate::load_container_policy_for_creation_observed(&root, Some("web"), &mut |stage| {
            stages.push(stage)
        })
        .expect("creation policy");

    assert_eq!(
        stages,
        vec![
            crate::CreationPolicyStage::CheckoutIdentity,
            crate::CreationPolicyStage::Declaration,
            crate::CreationPolicyStage::RenderInputs,
            crate::CreationPolicyStage::ResolvedDeclaration,
            crate::CreationPolicyStage::Render,
            crate::CreationPolicyStage::MountSources,
        ],
        "each preparation stage is reported once, in the order it runs"
    );
    assert_eq!(
        stages[0].description(),
        "recording the checkout identity",
        "a stage names what it is doing"
    );
}

#[cfg(unix)]
#[test]
fn creation_policy_refuses_unreadable_optional_source_instead_of_skipping() {
    let root = temp_repo("workspace-mount-create-unreadable");
    fs::write(root.join("blocked"), "not a directory").expect("write blocking file");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["blocked/source:/workspace-root/blocked:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let error = load_container_policy_for_creation(&root, Some("web"))
        .expect_err("ENOTDIR is not evidence that the optional source is absent");

    assert!(error.to_string().contains("workspace extra mount source"));
    assert!(!root.join("blocked/source").exists());
}

#[test]
fn load_container_exec_working_dir_ignores_host_task_defaults_for_workspace_inference() {
    let root = temp_repo("bundle-host-default-workdir");
    setup_workspace_app_path_bundle(&root);
    let manifest_path = root.join("effigy.toml");
    fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"
databases = ["acme"]

[systems.dev]
mounts = ["../platform"]

[task_defaults]
run_in = "host"
"#,
    )
    .expect("write manifest");
    fs::create_dir(root.join(".git")).expect("git dir");

    let working_dir = load_container_exec_working_dir(&root, Some("stack")).expect("working dir");

    assert_eq!(
        working_dir,
        PathBuf::from("/workspace-root/workspace-app-reference")
    );
}

#[test]
fn load_container_policy_resolves_named_member_mounts() {
    let root = temp_repo("named-member-mount");
    setup_workspace_app_path_bundle(&root);
    fs::create_dir_all(root.join("underlay")).expect("member dir");
    fs::write(
        root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"
databases = ["acme"]

[catalog.members]
underlay = "underlay"

[systems.dev]
mounts = [{ member = "underlay", target = "/workspace-root/underlay" }]
"#,
    )
    .expect("write manifest");
    fs::create_dir(root.join(".git")).expect("git dir");

    let policy = {
        let _lock = crate::test_env_lock();
        load_container_policy(&root, Some("stack")).expect("policy")
    };

    let rewritten = policy
        .read_compose(&policy.compose_files[0])
        .expect("rewritten compose");
    assert!(
        rewritten.contains(
            &root
                .join("underlay")
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        ),
        "named member source should be rendered into compose: {rewritten}"
    );
}

#[test]
fn translated_compose_ports_follow_catalog_port_policy() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("direct-compose-inferred-ports");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    image: alpine
    ports:
      - "41001:41001"
      - "127.0.0.1:41002:41002"
  mailpit:
    image: axllent/mailpit:latest
    ports:
      - target: 8025
        published: 8025
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    // Translated ports follow catalog port policy: every published container
    // port is kept, and Prepare assigns the host side.
    assert!(!policy.ports_declared_explicitly);
    let mut declared = policy.declared_ports.clone();
    declared.sort();
    assert_eq!(
        declared,
        vec![
            "127.0.0.1::41001".to_owned(),
            "127.0.0.1::41002".to_owned(),
            "127.0.0.1::8025".to_owned(),
        ]
    );
}

#[test]
fn load_inline_workspace_container_policy_writes_compose_and_derives_exec_dir() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("inline-workspace-policy");
    fs::create_dir(root.join(".git")).expect("git dir");
    let inline = ManifestInlineWorkspaceContainerConfig {
        image: Some("node:22".to_owned()),
        mount: Some("./:/workspace".to_owned()),
        extra: Default::default(),
    };

    let policy =
        load_inline_workspace_container_policy(&root, "dev__app", &inline, None).expect("policy");
    assert!(!root.join(".gitignore").exists());
    assert!(
        !root.join(".git/info/exclude").exists(),
        "resolution must not edit Git exclusions"
    );
    let working_dir =
        resolve_inline_workspace_exec_working_dir(&root, "dev__app", &inline, None).expect("cwd");

    assert_eq!(policy.name, "dev__app");
    assert_eq!(policy.primary_service, "workspace");
    assert_eq!(policy.compose_source, EffectiveComposeSource::Direct);
    assert_eq!(working_dir, PathBuf::from("/workspace"));
    assert!(
        !policy.compose_files[0].exists(),
        "inline resolution must not publish files"
    );
    let compose = policy
        .read_compose(&policy.compose_files[0])
        .expect("read compose");
    assert!(compose.contains("image: node:22"), "compose: {compose}");
    assert!(
        compose.contains("working_dir: /workspace"),
        "compose: {compose}"
    );
    assert!(
        compose.contains(&root.canonicalize().unwrap().display().to_string()),
        "compose should use absolute host path: {compose}"
    );
    let dns_override = policy
        .read_compose(&policy.compose_files[1])
        .expect("read dns override");
    assert!(
        dns_override.contains("workspace:\n    dns:"),
        "dns override should target workspace service: {dns_override}"
    );
}

#[test]
fn validate_container_policy_rejects_mounts_outside_repo() {
    let parent = effigy_test_support::Fixture::process_lifetime(&format!(
        "effigy-mount-validation-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
    .expect("fixture directory");
    let root = parent.join("repo");
    let outside = parent.join("outside");
    fs::create_dir_all(&root).expect("mkdir repo");
    fs::create_dir_all(&outside).expect("mkdir outside");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = ["../outside:/workspace/outside"]
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    // Host-mount resolution moved to intake (see `mount_spec`), so the
    // repo-escape check now fires at policy-load time, not at later
    // `validate_container_policy`.
    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(error.to_string().contains("escapes the repo root"));
}

#[test]
fn validate_compose_backend_runtime_rejects_temp_root_repo_for_colima_nerdctl() {
    // Backend resolution reads `HOME` and `EFFIGY_COMPOSE_BACKEND`, which
    // sibling tests swap process-globally. Take the same lock they hold.
    let _lock = crate::test_env_lock();
    let root = temp_repo("temp-root-colima-compose");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let error = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
        crate::validate_compose_backend_runtime(&root, &policy).expect_err("should fail")
    });

    assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
    assert!(error
        .to_string()
        .contains("under a temp directory that Colima may not share"));
    assert!(error.to_string().contains("/Users/..."));
}

#[test]
fn validate_compose_backend_runtime_checks_staged_source_instead_of_temp_checkout() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("staged-colima-path-preflight");
    let temporary = root.join("temporary");
    let source = temporary.join("source");
    // Only model paths under this existing, non-temporary mount. The preflight
    // does not publish files there; all fixture writes stay under `root`.
    // A sibling of `source` would still be under /tmp on hosted runners.
    let shared = std::env::current_dir()
        .expect("current directory")
        .canonicalize()
        .expect("existing shared mount");
    let colima = root.join("colima");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(colima.join("private")).expect("profile");
    let _env = effigy_test_support::env::ScopedEnv::set_many(&[
        ("TMPDIR", temporary.as_os_str().to_owned()),
        ("COLIMA_HOME", colima.as_os_str().to_owned()),
    ]);
    fs::write(source.join("effigy.toml"),
        "[containers.web]\ncompose_file = 'compose.yml'\nprimary_service = 'app'\nprofile = 'private'\n")
        .expect("manifest");
    fs::write(
        source.join("compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");
    fs::write(
        colima.join("private/colima.yaml"),
        format!(
            "mounts:\n- location: {}\n  writable: true\n",
            shared.display()
        ),
    )
    .expect("profile mounts");
    let mut policy = load_container_policy(&source, Some("web")).expect("policy");
    // Rebase only the in-memory snapshot's logical address. Validation still
    // reads the real rendered bytes, without publishing into the modeled mount.
    let render = policy.render.as_mut().expect("Compose snapshot");
    let original = render.path.clone();
    render.path = shared;
    policy.compose_files = policy
        .compose_files
        .iter()
        .map(|path| {
            render
                .path
                .join(path.strip_prefix(&original).expect("snapshot input"))
        })
        .collect();
    policy.workspace_source = crate::artifact_source::WorkspaceSource::Staged;
    with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
        crate::validate_compose_backend_runtime(&source, &policy).expect("reachable staged source");
        policy.workspace_source = crate::artifact_source::WorkspaceSource::Checkout;
        assert!(crate::validate_compose_backend_runtime(&source, &policy).is_err());
    });
}

#[test]
fn validate_compose_backend_runtime_refuses_unmodelled_mount_inputs_for_colima_only() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("colima-unmodelled-mount-input");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example.invalid/synthetic\n",
    )
    .expect("compose");
    let mut policy = load_container_policy(&root, None).expect("policy");
    let overlay = root.join("infra/dev/compose.overlay.yml");
    policy.compose_files.push(overlay.clone());

    let refuse = |content: &str| {
        fs::write(&overlay, content).expect("write synthetic Compose overlay");
        let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
        unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
        }
        let result = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
            crate::validate_compose_backend_runtime(&root, &policy)
        });
        match previous {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
            },
            None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
        }
        result.expect_err("unmodelled mount input must refuse before apply")
    };

    let malformed_entry = refuse("services:\n  app:\n    volumes:\n      - 42\n");

    let inherited_mounts = refuse("services:\n  app:\n    extends:\n      service: base\n");
    let configured_volume = refuse(
        "services:\n  app:\n    volumes:\n      - cache:/workspace/cache\nvolumes:\n  cache:\n    name: unmodelled-volume-name\n",
    );
    assert!(malformed_entry.to_string().contains("volumes[1]"));
    assert!(malformed_entry
        .to_string()
        .contains("paths and values are omitted"));

    let unsupported_option = refuse(
        "services:\n  app:\n    volumes:\n      - type: bind\n        source: /private/mount/DO_NOT_PRINT\n        target: /workspace/cache\n        read_only: true\n        bind:\n          propagation: rshared\n",
    );
    assert!(unsupported_option
        .to_string()
        .contains("unmodelled fields or options"));
    assert!(!unsupported_option
        .to_string()
        .contains("/private/mount/DO_NOT_PRINT"));

    // An option-carrying tmpfs entry changes nerdctl's serialization and
    // stays refused; its path is omitted from the diagnostic.
    let tmpfs_options =
        refuse("services:\n  app:\n    tmpfs:\n      - /workspace/cache:size=1073741824\n");
    assert!(tmpfs_options
        .to_string()
        .contains("`tmpfs` entry has unmodelled options"));
    assert!(!tmpfs_options.to_string().contains("/workspace/cache"));

    // A tmpfs mapping form carries per-mount options and stays refused.
    let tmpfs_mapping =
        refuse("services:\n  app:\n    tmpfs:\n      /workspace/cache: \"size=1g\"\n");
    assert!(tmpfs_mapping
        .to_string()
        .contains("`tmpfs` is neither empty nor a list of option-less paths"));
    assert!(!tmpfs_mapping.to_string().contains("/workspace/cache"));

    // An option-less tmpfs list is a modelled mount input (the php-fpm
    // catalog renders exactly this shape on the primary service), so it no
    // longer refuses as an unmodelled field.
    fs::write(
        &overlay,
        "services:\n  app:\n    tmpfs:\n      - /run/effigy/secrets\n",
    )
    .expect("write synthetic Compose overlay");
    let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
    unsafe {
        std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
    }
    with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
        crate::validate_compose_backend_runtime(&root, &policy)
            .expect("option-less primary-service tmpfs is a modelled mount input")
    });
    match previous {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
        },
        None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
    }

    assert!(malformed_entry.to_string().contains("volumes[1]"));
    assert!(inherited_mounts
        .to_string()
        .contains("unmodelled mount input `extends`"));
    assert!(configured_volume
        .to_string()
        .contains("primary service mounts a named volume with unmodelled top-level options"));

    with_test_compose_backend(ComposeBackend::Docker, || {
        crate::validate_compose_backend_runtime(&root, &policy)
            .expect("Docker must not inherit Colima mount-budget refusals");
    });
}

/// The budget counts modelled primary-service tmpfs rows like any other
/// mount: a small generated secret runtime stays admissible, while enough
/// option-less tmpfs entries exceed the preflight budget and refuse with
/// paths omitted.
#[test]
fn effective_attach_mode_respects_flags_before_policy() {
    let root = temp_repo("attach-mode");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
startup = "detached"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    assert_eq!(
        effective_attach_mode(&policy, false, false),
        EffectiveAttachMode::Detached
    );
    assert_eq!(
        effective_attach_mode(&policy, true, false),
        EffectiveAttachMode::Attached
    );
}
