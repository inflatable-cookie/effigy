use super::*;
#[cfg(test)]
use std::process::Command;

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("launch git fixture command");
    assert!(
        output.status.success(),
        "git {} failed in {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        repo.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One compose file parsed into the model Compose reads, so an assertion names
/// a service, a key and a value rather than bytes of YAML text.
fn parsed_compose(policy: &crate::EffectiveContainerPolicy, index: usize) -> serde_yaml::Value {
    let rendered = policy
        .read_compose(&policy.compose_files[index])
        .expect("read compose");
    serde_yaml::from_str(&rendered).expect("parse compose")
}

/// The value one service's `environment` gives a key, whether the document
/// spells the block as a mapping or as a list of `KEY=value` entries.
fn service_environment(document: &serde_yaml::Value, service: &str, key: &str) -> Option<String> {
    let environment = &document["services"][service]["environment"];
    if let Some(mapping) = environment.as_mapping() {
        return mapping
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_owned);
    }
    environment.as_sequence().and_then(|entries| {
        entries
            .iter()
            .filter_map(|entry| entry.as_str())
            .find_map(|entry| {
                let (name, value) = entry.split_once('=')?;
                (name == key).then(|| value.to_owned())
            })
    })
}

/// The mount sources and targets one service declares.
fn service_volumes(document: &serde_yaml::Value, service: &str) -> Vec<String> {
    document["services"][service]["volumes"]
        .as_sequence()
        .map(|volumes| {
            volumes
                .iter()
                .filter_map(|volume| volume.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The host-gateway entries (`domain:address`) an override gives one service.
fn host_gateway_domains(document: &serde_yaml::Value, service: &str) -> Vec<String> {
    document["services"][service]["extra_hosts"]
        .as_sequence()
        .map(|hosts| {
            hosts
                .iter()
                .filter_map(|host| host.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The `networks.default.aliases` an override gives one service.
fn service_aliases(document: &serde_yaml::Value, service: &str) -> Vec<String> {
    document["services"][service]["networks"]["default"]["aliases"]
        .as_sequence()
        .map(|aliases| {
            aliases
                .iter()
                .filter_map(|alias| alias.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn isolated_dirs_survive_primary_and_linked_worktree_render_closures() {
    let fixture = tempfile::tempdir().unwrap();
    crate::with_test_effigy_home(&fixture.path().join("home"), || {
        let manifest = r#"
[containers]
default = "stack"
[containers.stack]
project_name = "acowtancy-dev"
primary_service = "workspace"
working_dir = "/workspace-root/acowtancy"
[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "acowtancy"
isolated_dirs = ["node_modules", "packages/froyo/node_modules", "apps/farmyard/target", "platform/silo/target"]
[systems.dev]
container = "stack"
working_dir = "/workspace-root/acowtancy"
user = "dev"
home = "/home/dev"
"#;
        let (primary, worktrees) =
            create_linked_worktree_fixture(fixture.path(), manifest, &["task"]);
        let mut volume_sets = Vec::new();
        for checkout in [primary, worktrees[0].clone()] {
            let policy = load_container_policy(&checkout, None).unwrap();
            let render = policy.render.as_ref().unwrap();
            assert!(!render.path.exists(), "render is pure");
            let document: serde_yaml::Value =
                serde_yaml::from_str(&policy.read_compose(&policy.compose_files[0]).unwrap())
                    .unwrap();
            let mounts = document["services"]["workspace"]["volumes"]
                .as_sequence()
                .unwrap();
            let mut names = std::collections::BTreeSet::new();
            for dir in [
                "node_modules",
                "packages/froyo/node_modules",
                "apps/farmyard/target",
                "platform/silo/target",
            ] {
                let target = format!(":/workspace-root/acowtancy/{dir}");
                let matches = mounts
                    .iter()
                    .filter_map(serde_yaml::Value::as_str)
                    .filter(|mount| mount.ends_with(&target))
                    .collect::<Vec<_>>();
                assert_eq!(matches.len(), 1, "{dir}: {mounts:?}");
                let name = matches[0].split(':').next().unwrap();
                assert!(
                    names.insert(name.to_owned()),
                    "one volume per isolated directory"
                );
                let declaration = &document["volumes"][name];
                assert_eq!(declaration["name"].as_str(), Some(name));
                assert_eq!(
                    declaration["labels"]["com.effigy.managed"].as_str(),
                    Some("true")
                );
            }
            volume_sets.push(names);
        }
        assert!(
            volume_sets[0].is_disjoint(&volume_sets[1]),
            "worktree volumes are isolated from primary"
        );
    });
}

pub(super) fn create_linked_worktree_fixture(
    root: &Path,
    manifest: &str,
    names: &[&str],
) -> (PathBuf, Vec<PathBuf>) {
    let primary = root.join("primary");
    fs::create_dir_all(&primary).expect("create primary checkout");
    fs::write(primary.join("effigy.toml"), manifest).expect("write primary manifest");
    run_git(&primary, &["init", "--quiet"]);
    run_git(
        &primary,
        &["config", "--local", "user.name", "Effigy Fixture"],
    );
    run_git(
        &primary,
        &[
            "config",
            "--local",
            "user.email",
            "effigy-fixture@example.invalid",
        ],
    );
    run_git(&primary, &["add", "effigy.toml"]);
    run_git(&primary, &["commit", "--quiet", "-m", "fixture manifest"]);

    let checkouts = names
        .iter()
        .map(|name| {
            let checkout = root.join(name);
            run_git(
                &primary,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "-b",
                    name,
                    checkout.to_str().expect("fixture path is UTF-8"),
                ],
            );
            effigy_core::worktree_scope::prepare_checkout_token(&checkout).unwrap();
            checkout
        })
        .collect();
    let common_git_dir = primary.join(".git");
    assert_eq!(
        effigy_core::git_worktree::resolve_common_git_dir(&primary)
            .expect("resolve fixture primary git directory")
            .as_deref(),
        Some(common_git_dir.as_path())
    );
    assert!(effigy_core::git_worktree::detect_linked_worktree(&primary).is_none());
    (primary, checkouts)
}

fn assert_linked_worktree_git_layout(primary: &Path, checkout: &Path, name: &str) {
    let layout = effigy_core::git_worktree::detect_linked_worktree(checkout)
        .expect("Git-created linked-worktree metadata");
    let expected_worktree_git_dir =
        fs::canonicalize(primary.join(".git/worktrees").join(name)).unwrap();
    let expected_common_git_dir = fs::canonicalize(primary.join(".git")).unwrap();
    assert_eq!(
        fs::canonicalize(layout.worktree_git_dir).unwrap(),
        expected_worktree_git_dir
    );
    assert_eq!(
        fs::canonicalize(layout.common_git_dir).unwrap(),
        expected_common_git_dir
    );
    assert_eq!(
        fs::canonicalize(
            effigy_core::git_worktree::resolve_common_git_dir(checkout)
                .expect("resolve fixture common git directory")
                .expect("fixture common git directory")
        )
        .unwrap(),
        expected_common_git_dir
    );
}

fn managed_volume_names(policy: &crate::EffectiveContainerPolicy) -> Vec<String> {
    policy
        .managed_volumes
        .iter()
        .map(|volume| volume.name.clone())
        .collect()
}

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

fn load_prepared_policy(
    root: &Path,
    name: Option<&str>,
) -> Result<crate::EffectiveContainerPolicy, crate::ContainerPolicyError> {
    crate::load_container_policy_for_creation(root, name).map(|(policy, _)| policy)
}

#[test]
fn linked_worktrees_generate_distinct_compose_resources_and_source_mounts() {
    with_temp_effigy_home("linked-worktree-resources", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("linked-worktree-resources");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "shared-dev"
primary_service = "app"
[containers.web.services.app]
catalog = "workspace-rust-bun"
host_ports = ["41001:41001"]
"#;
        let (primary, checkouts) = create_linked_worktree_fixture(&root, manifest, &["one", "two"]);
        let mut policies = Vec::new();
        for (name, checkout) in ["one", "two"].into_iter().zip(&checkouts) {
            assert_linked_worktree_git_layout(&primary, checkout, name);
            let policy = load_prepared_policy(checkout, None).unwrap();
            let compose = policy.read_compose(&policy.compose_files[0]).unwrap();
            assert!(
                compose.contains(&checkout.display().to_string()),
                "{compose}"
            );
            assert!(policy
                .managed_volumes
                .iter()
                .all(|volume| volume.name.starts_with(&policy.project_name)));
            policies.push(policy);
        }
        assert_ne!(policies[0].project_name, policies[1].project_name);
        // Both checkouts ask Compose for an OS-assigned port; what keeps them
        // apart is the project name and the alias, not a shared port number.
        assert_eq!(policies[0].declared_ports, policies[1].declared_ports);
        assert_ne!(
            managed_volume_names(&policies[0]),
            managed_volume_names(&policies[1])
        );
        let again = load_container_policy(&policies[0].repo_root, None).unwrap();
        assert_eq!(again.project_name, policies[0].project_name);
        assert_eq!(again.declared_ports, policies[0].declared_ports);
        assert_eq!(
            managed_volume_names(&again),
            managed_volume_names(&policies[0])
        );
    });
}

#[test]
fn linked_worktrees_get_distinct_effective_hosts_and_primary_keeps_declared() {
    with_temp_effigy_home("linked-worktree-hosts", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("linked-worktree-hosts");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "app-dev"
primary_service = "app"
[containers.web.dns]
routes = [
  { domain = "app.test", tls = true, service = "web" },
  { domain = "mail.app.test", tls = false, service = "mail" },
]
[containers.web.services.app]
catalog = "workspace-rust-bun"
"#;
        let (primary, checkouts) = create_linked_worktree_fixture(&root, manifest, &["one", "two"]);
        let primary_policy = load_prepared_policy(&primary, None).unwrap();
        assert_eq!(primary_policy.dns_domain.as_deref(), Some("app.test"));
        assert_eq!(primary_policy.dns_routes[0].declared(), "app.test");
        assert_eq!(primary_policy.dns_routes[0].domain, "app.test");
        let mut effective = Vec::new();
        let mut preceding_policies = vec![primary_policy.clone()];
        for (name, checkout) in ["one", "two"].into_iter().zip(&checkouts) {
            assert_linked_worktree_git_layout(&primary, checkout, name);
            let policy = load_prepared_policy(checkout, None).unwrap();
            assert_eq!(policy.dns_routes[0].declared(), "app.test");
            assert_ne!(policy.dns_routes[0].domain, "app.test");
            assert!(policy.dns_routes[0].domain.contains("-w"));
            assert!(policy.dns_routes[1].domain.starts_with("mail.app-w"));
            assert!(policy.read_compose(&policy.compose_files[0]).is_ok());
            let compose = policy.read_compose(&policy.compose_files[0]).unwrap();
            assert!(compose.contains("com.effigy.scope"), "{compose}");
            effective.push(policy.dns_routes[0].domain.clone());
            preceding_policies.push(policy);
        }
        assert_ne!(
            preceding_policies[1].project_name,
            preceding_policies[2].project_name
        );
        assert_ne!(effective[0], effective[1]);
        assert_ne!(effective[0], "app.test");

        let again = load_container_policy(&checkouts[0], None).unwrap();
        assert_eq!(again.project_name, preceding_policies[1].project_name);
        assert_eq!(again.dns_routes[0].domain, effective[0]);
    });
}

fn listener_route(policy: &crate::EffectiveContainerPolicy) -> String {
    policy.host_processes[0]
        .listener
        .as_ref()
        .expect("managed listener")
        .route_domain
        .clone()
}

#[test]
fn linked_worktrees_scope_managed_listener_routes_with_declared_hosts() {
    with_temp_effigy_home("linked-worktree-listener-routes", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("linked-worktree-listener-routes");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "app-dev"
primary_service = "app"
[containers.web.services.app]
catalog = "workspace-rust-bun"
[[containers.web.host_processes]]
name = "api"
run = "exec ./scripts/api"
[containers.web.host_processes.listener]
bind = "127.0.0.1:0"
[containers.web.host_processes.listener.readiness]
path = "/health"
status = 200
[containers.web.host_processes.listener.route]
domain = "api.acme.test"
tls = true
"#;
        let (primary, checkouts) = create_linked_worktree_fixture(&root, manifest, &["one", "two"]);
        let primary_policy = load_container_policy(&primary, None).unwrap();
        assert_eq!(listener_route(&primary_policy), "api.acme.test");
        let mut preceding_policies = vec![primary_policy];
        let mut effective = Vec::new();
        for checkout in &checkouts {
            let policy = load_container_policy(checkout, None).unwrap();
            let domain = listener_route(&policy);
            assert!(domain.starts_with("api.acme-w"), "got `{domain}`");
            assert!(domain.ends_with(".test"), "got `{domain}`");
            effective.push(domain);
            preceding_policies.push(policy);
        }
        assert_ne!(effective[0], effective[1]);

        let again = load_container_policy(&checkouts[0], None).unwrap();
        assert_eq!(
            listener_route(&again),
            effective[0],
            "repeated resolution must not scope the listener twice"
        );
    });
}

#[test]
fn marked_clones_and_shared_identity_scope_listener_routes_like_dns() {
    with_temp_effigy_home("scoped-listener-dns-match", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("scoped-listener-dns-match");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "app-dev"
primary_service = "app"
[containers.web.dns]
routes = [{ domain = "web.acme.test", tls = true, service = "app" }]
[containers.web.services.app]
catalog = "workspace-rust-bun"
[[containers.web.host_processes]]
name = "api"
run = "exec ./scripts/api"
[containers.web.host_processes.listener]
bind = "127.0.0.1:0"
[containers.web.host_processes.listener.readiness]
path = "/health"
status = 200
[containers.web.host_processes.listener.route]
domain = "api.acme.test"
tls = true
"#;
        let (primary, checkouts) = create_linked_worktree_fixture(&root, manifest, &["one", "two"]);
        let primary_policy = load_container_policy(&primary, None).unwrap();
        assert_eq!(primary_policy.dns_routes[0].domain, "web.acme.test");
        assert_eq!(listener_route(&primary_policy), "api.acme.test");
        let mut preceding_policies = vec![primary_policy];
        // DNS and listener names stay distinct but share one scoped apex.
        let apex = |name: &str| name.split_once('.').expect("dotted host").1.to_owned();

        // A marked ephemeral clone owns its Git directory and follows the
        // same scope rule as a linked worktree.
        let clone = root.join("clone");
        run_git(
            &root,
            &[
                "clone",
                "--quiet",
                primary.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        run_git(
            &clone,
            &["config", "--local", "effigy.runtimeScope", "ephemeral"],
        );
        effigy_core::worktree_scope::prepare_checkout_token(&clone).unwrap();
        let clone_policy = load_container_policy(&clone, None).unwrap();
        let clone_listener = listener_route(&clone_policy);
        assert!(
            clone_listener.starts_with("api.acme-w"),
            "got `{clone_listener}`"
        );
        assert_eq!(
            apex(&clone_policy.dns_routes[0].domain),
            apex(&clone_listener)
        );
        preceding_policies.push(clone_policy);

        let worktree = load_container_policy(&checkouts[0], None).unwrap();
        let worktree_listener = listener_route(&worktree);
        assert!(worktree_listener.starts_with("api.acme-w"));
        assert_ne!(worktree_listener, clone_listener);
        assert_eq!(
            apex(&worktree.dns_routes[0].domain),
            apex(&worktree_listener)
        );
        assert!(worktree.dns_routes[0].domain.starts_with("web.acme-w"));

        // Identical DNS and listener names are still one static conflict.
        fs::write(
            checkouts[1].join("effigy.toml"),
            manifest.replace("domain = \"api.acme.test\"", "domain = \"web.acme.test\""),
        )
        .unwrap();
        let conflict = load_container_policy(&checkouts[1], None);
        let conflict = conflict.expect_err("identical static and listener hosts must be refused");
        assert!(
            conflict.to_string().contains("conflicts with a static"),
            "got `{conflict}`"
        );

        // An explicit shared runtime identity keeps the declared host, exactly
        // like its DNS route, so the foreign-claim check still applies.
        let shared_manifest = manifest.replace(
            "[containers.web]\n",
            "[containers.web]\nshare_runtime_identity = true\n",
        );
        fs::write(checkouts[1].join("effigy.toml"), shared_manifest).unwrap();
        let shared = load_container_policy(&checkouts[1], None).unwrap();
        assert_eq!(shared.dns_routes[0].domain, "web.acme.test");
        assert_eq!(listener_route(&shared), "api.acme.test");
    });
}

#[test]
fn invalid_linked_worktree_admin_directory_is_refused() {
    let root = temp_repo("invalid-linked-worktree-admin");
    let checkout = root.join("checkout");
    let private = root.join("private");
    let common = root.join("common");
    fs::create_dir_all(&checkout).unwrap();
    fs::create_dir_all(&private).unwrap();
    fs::create_dir_all(&common).unwrap();
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", private.display()),
    )
    .unwrap();
    fs::write(private.join("commondir"), format!("{}\n", common.display())).unwrap();

    let error = effigy_core::git_worktree::resolve_common_git_dir(&checkout)
        .expect_err("missing Git admin HEAD must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("not a git directory"));
}

#[test]
fn marked_ephemeral_clones_get_distinct_compose_resources_alongside_unmarked_checkout() {
    with_temp_effigy_home("ephemeral-clone-resources", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("ephemeral-clone-resources");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "shared-dev"
primary_service = "app"
[containers.web.services.app]
catalog = "workspace-rust-bun"
host_ports = ["41001:41001"]
"#;
        // The operator's own unmarked checkout keeps primary behavior.
        let operator = root.join("operator");
        fs::create_dir_all(operator.join(".git")).unwrap();
        fs::write(operator.join(".git/config"), "[core]\n").unwrap();
        fs::write(operator.join("effigy.toml"), manifest).unwrap();
        let operator_policy = load_prepared_policy(&operator, None).unwrap();
        assert_eq!(operator_policy.project_name, "shared-dev");

        let mut identities = Vec::new();
        for name in ["clone-one", "clone-two"] {
            let checkout = root.join(name);
            fs::create_dir_all(checkout.join(".git")).unwrap();
            fs::write(
                checkout.join(".git/config"),
                "[effigy]\n\truntimeScope = ephemeral\n",
            )
            .unwrap();
            fs::write(checkout.join("effigy.toml"), manifest).unwrap();
            let policy = load_prepared_policy(&checkout, None).unwrap();
            assert!(
                policy.project_name.starts_with("shared-dev-ec-"),
                "{}",
                policy.project_name
            );
            assert!(policy
                .managed_volumes
                .iter()
                .all(|volume| volume.name.starts_with(&policy.project_name)));
            let compose = policy.read_compose(&policy.compose_files[0]).unwrap();
            assert!(compose.contains("com.effigy.scope"), "{compose}");
            identities.push((policy.project_name.clone(), policy.declared_ports.clone()));
        }
        // Distinct from the operator checkout and from each other.
        assert_ne!(identities[0].0, identities[1].0);
        assert_ne!(identities[0].0, "shared-dev");
        assert_eq!(
            identities[0].1,
            vec!["127.0.0.1::41001".to_owned()],
            "a scoped clone takes an OS-assigned port instead of the pin"
        );
        assert_eq!(identities[0].1, identities[1].1);

        // Repeated commands reuse the identity; a recreated clone gets a new
        // generation with its own name and ports.
        let again = load_container_policy(&root.join("clone-one"), None).unwrap();
        assert_eq!(again.project_name, identities[0].0);
        assert_eq!(again.declared_ports, identities[0].1);
        // Move the sealed old fixture aside, then replace the whole checkout.
        // Replacing only Git metadata cannot adopt its existing journal.
        fs::rename(root.join("clone-one"), root.join("retired-clone-one")).unwrap();
        fs::create_dir_all(root.join("clone-one/.git")).unwrap();
        fs::write(
            root.join("clone-one/.git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .unwrap();
        fs::write(root.join("clone-one/effigy.toml"), manifest).unwrap();
        let recreated = load_prepared_policy(&root.join("clone-one"), None).unwrap();
        assert_ne!(recreated.project_name, identities[0].0);
        // Host ports are OS-assigned, so the identity and volumes carry the
        // difference, not a shared port number.
        assert_eq!(recreated.declared_ports, identities[0].1);
    });
}

#[test]
fn linked_worktree_container_target_resolves_its_own_declared_database_alias() {
    // Two scoped checkouts of one manifest. Each container target must resolve
    // the canonical declared TCP alias to its own backing service, because the
    // Compose network (and its project) belongs to that scope.
    with_temp_effigy_home("scoped-database-aliases", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("scoped-database-aliases");
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "app-dev"
primary_service = "workspace"
[containers.web.dns]
base_domain = "acme.test"
routes = [
  { domain = "app.acme.test", tls = false, service = "workspace" },
]
[containers.web.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "scoped-database-aliases"
[containers.web.services.postgres]
catalog = "postgres"
"#;
        let mut scopes = Vec::new();
        for name in ["clone-one", "clone-two"] {
            let checkout = root.join(name);
            fs::create_dir_all(checkout.join(".git")).unwrap();
            fs::write(
                checkout.join(".git/config"),
                "[effigy]\n\truntimeScope = ephemeral\n",
            )
            .unwrap();
            fs::write(checkout.join("effigy.toml"), manifest).unwrap();
            effigy_core::worktree_scope::prepare_checkout_token(&checkout).unwrap();
            let policy = load_container_policy(&checkout, None).unwrap();
            assert!(
                policy.compose_files.len() >= 2,
                "generated render plus DNS override"
            );
            let dns_override = parsed_compose(&policy, 1);
            let effective_base = policy
                .dns_routes
                .first()
                .expect("declared HTTP route")
                .domain
                .strip_prefix("app.")
                .expect("scoped route keeps the declared label")
                .to_owned();
            let generated = policy.read_compose(&policy.compose_files[0]).unwrap();
            assert_eq!(
                policy.dns_effective_base_domain().as_deref(),
                Some(effective_base.as_str()),
                "gateway and Compose aliases share the scoped declared apex"
            );
            assert!(
                generated.contains("networks:") && generated.contains("com.effigy.scope"),
                "the scope owns its Compose network: {generated}"
            );
            let aliases = service_aliases(&dns_override, "postgres");
            assert!(
                aliases
                    .iter()
                    .any(|alias| alias == &format!("postgres.{effective_base}")),
                "the scoped alias stays reachable: {dns_override:?}"
            );
            assert!(
                aliases.iter().any(|alias| alias == "postgres.acme.test"),
                "the canonical declared alias resolves inside this scope's target: {dns_override:?}"
            );
            assert!(
                !host_gateway_domains(&dns_override, "postgres")
                    .iter()
                    .any(|host| host.starts_with("postgres.acme.test:")),
                "the canonical alias is a network alias, never the host gateway: {dns_override:?}"
            );
            scopes.push((policy.project_name.clone(), effective_base));
        }
        assert_ne!(
            scopes[0].0, scopes[1].0,
            "each scope keeps its own Compose project network"
        );
        assert_ne!(
            scopes[0].1, scopes[1].1,
            "each scope keeps its own host apex"
        );
    });
}

/// One translated compose file with a fixed host port, in each checkout
/// scope: a linked worktree and a marked ephemeral clone take an OS-assigned
/// port rather than competing for the primary's, and an unmarked clone leaves
/// the host side to Prepare instead of claiming the declared 8080.
#[test]
fn translated_fixed_port_is_assigned_per_scope_kind() {
    fn linked_worktree(checkout: &Path, root: &Path) {
        let private = root.join("primary/.git/worktrees/worker");
        fs::create_dir_all(checkout).unwrap();
        fs::create_dir_all(&private).unwrap();
        fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", private.display()),
        )
        .unwrap();
        fs::write(private.join("commondir"), "../..\n").unwrap();
    }
    fn ephemeral_clone(checkout: &Path, _root: &Path) {
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::write(
            checkout.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .unwrap();
    }
    fn ordinary_checkout(checkout: &Path, _root: &Path) {
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::write(checkout.join(".git/config"), "[core]\n").unwrap();
    }

    type Row = (&'static str, fn(&Path, &Path), Option<&'static str>);
    let cases: [Row; 3] = [
        ("a linked worktree", linked_worktree, Some("127.0.0.1::80")),
        (
            "a marked ephemeral clone",
            ephemeral_clone,
            Some("127.0.0.1::80"),
        ),
        ("an unmarked clone", ordinary_checkout, None),
    ];
    for (name, mark, assigned) in cases {
        let root = temp_repo("direct-fixed-port");
        let checkout = root.join("worker");
        fs::create_dir_all(&checkout).unwrap();
        mark(&checkout, &root);
        fs::write(
            checkout.join("compose.yml"),
            "services:\n  app:\n    image: alpine\n    ports: ['8080:80']\n",
        )
        .unwrap();
        fs::write(
            checkout.join("effigy.toml"),
            r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
"#,
        )
        .unwrap();

        let policy = load_container_policy(&checkout, None).unwrap();
        match assigned {
            Some(port) => {
                assert_eq!(policy.declared_ports, vec![port.to_owned()], "{name}");
                let compose = policy.read_compose(&policy.compose_files[0]).unwrap();
                assert!(compose.contains(port), "{name}: {compose}");
            }
            // A translated port is a catalog port: Prepare assigns the host
            // side, so no checkout claims the fixed 8080.
            None => {
                assert!(
                    !policy.declared_ports.contains(&"8080:80".to_owned()),
                    "{name}: {:?}",
                    policy.declared_ports
                );
                assert!(
                    policy
                        .declared_ports
                        .iter()
                        .any(|port| port.ends_with(":80")),
                    "{name}: {:?}",
                    policy.declared_ports
                );
            }
        }
    }
}

/// A manifest that still names the retired built-in `minio` service is refused
/// with an actionable message naming its replacement. It is never remapped
/// onto `silo` (effigy#424).
#[test]
fn retired_minio_catalog_service_refuses_and_names_silo() {
    let root = temp_repo("retired-minio-catalog");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"

[containers.stack.services.minio]
catalog = "minio"
"#,
    )
    .expect("write manifest");

    let error = load_container_policy(&root, None).expect_err("minio must refuse");
    let message = error.to_string();
    assert!(message.contains("minio"), "{message}");
    assert!(
        message.contains("catalog = \"silo\""),
        "the refusal must name the replacement: {message}"
    );
    assert!(
        message.contains("does not remap"),
        "the refusal must say nothing is remapped: {message}"
    );
}

#[test]
fn load_container_policy_generates_compose_from_catalog_services() {
    with_temp_effigy_home("catalog-services", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-services");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "clientname.test", tls = true, port = 8080 }]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_prepared_policy(&root, None).expect("policy");

        assert_eq!(policy.primary_service, "app");
        assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
        assert_eq!(policy.dns_domain.as_deref(), Some("clientname.test"));
        assert!(policy.dns_tls);
        assert_eq!(policy.dns_port, Some(8080));
        assert_eq!(policy.compose_files.len(), 2);
        assert!(
            policy
                .compose_file_display
                .contains("web.generated.compose.yml"),
            "display should reference generated compose, got {}",
            policy.compose_file_display
        );

        let compose_path = policy.compose_files[0].clone();
        assert_eq!(policy.compose_files[0], compose_path);
        assert!(compose_path.exists(), "generated compose file should exist");
        let dns_override = parsed_compose(&policy, 1);
        for service in ["app", "web"] {
            assert!(
                dns_override["services"][service]["dns"].is_sequence(),
                "dns override should target {service}: {dns_override:?}"
            );
            assert!(
                host_gateway_domains(&dns_override, service)
                    .iter()
                    .any(|host| host == "clientname.test:192.168.5.2"),
                "dns override should route project hostnames to the Colima gateway for {service}: {dns_override:?}"
            );
        }

        let compose = policy
            .read_compose(&compose_path)
            .expect("read generated compose");
        assert!(compose.contains("services:"));
        assert!(compose.contains("app:"));
        assert!(compose.contains("web:"));
        assert_eq!(
            policy.secret_delivery,
            effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv
        );
        assert!(!policy.ports_declared_explicitly);
        assert!(
            policy.declared_ports.contains(&"127.0.0.1::80".to_owned()),
            "{:?}",
            policy.declared_ports
        );
        assert!(
            compose.contains("127.0.0.1::80"),
            "generated compose lets the OS assign the published port, bound to loopback: {compose}"
        );
    });
}

#[test]
fn load_container_policy_preserves_secret_runtime_file_delivery() {
    with_temp_effigy_home("catalog-secret-runtime-files", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-secret-runtime-files");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "/run/effigy/secrets"
source_for_deferrals = true

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");

        assert_eq!(
            policy.secret_delivery,
            effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles
        );
        assert_eq!(
            policy.secret_runtime_dir.as_deref(),
            Some("/run/effigy/secrets")
        );
        assert!(policy.source_secret_runtime_for_deferrals);
    });
}

#[test]
fn generated_compose_workspace_app_shape_keeps_runtime_paths_and_external_mounts_stable() {
    with_temp_effigy_home("workspace-app-generated-compose-paths", |_| {
        let parent = tempfile::tempdir().expect("workspace app parent tempdir");
        let root = parent.path().join("workspace-app-reference");
        let platform = parent.path().join("platform");
        fs::create_dir_all(&root).expect("mkdir root");
        fs::create_dir_all(&platform).expect("mkdir workspace app sibling");
        let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../effigy-manifest/tests/fixtures/workspace-app-bundle");
        let bundle_dir = root.join("bundles/workspace-app");
        copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
[bundle]
base = {{ type = "path", dir = "bundles/workspace-app" }}
host = "workspace-app.test"
databases = ["workspace-app"]
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"

[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace-root/workspace-app-reference"

[containers.web.host]
mounts = [
  {{ host = "{}",
     container = "/workspace-root/platform",
     external = true }},
]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
                platform.display()
            ),
        )
        .expect("write manifest");

        let policy = {
            let _lock = crate::test_env_lock();
            load_container_policy(&root, None).expect("policy")
        };
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("read compose");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_platform = platform.canonicalize().expect("canonical platform");
        let runtime_compose_dir = policy
            .render
            .as_ref()
            .unwrap()
            .path
            .join("checkout/.effigy/runtime/compose");

        assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
        assert_eq!(policy.project_name, "workspace-app-reference-web-dev");
        assert!(policy.compose_files[0].starts_with(&runtime_compose_dir));
        assert!(
            !policy.compose_files[0].exists(),
            "observed runtime compose path is not published"
        );
        assert!(
            policy
                .compose_file_display
                .contains(".effigy/runtime/compose/"),
            "display should stay under runtime compose dir: {}",
            policy.compose_file_display
        );
        assert!(
            compose.contains(&format!(
                "{}:/workspace-root/workspace-app-reference",
                canonical_root.display()
            )),
            "generated compose should mount the target repo at the workspace app workspace path: {compose}"
        );
        assert!(
            compose.contains(&format!(
                "{}:/workspace-root/platform",
                canonical_platform.display()
            )),
            "generated compose should preserve the external workspace app sibling mount: {compose}"
        );
    });
}

#[test]
fn generated_compose_uses_explicit_host_ports_when_declared() {
    with_temp_effigy_home("catalog-explicit-host-ports", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-explicit-host-ports");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
ports = ["18080:80", "13306:3306"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("compose");

        assert!(compose.contains("127.0.0.1:18080:80"), "{compose}");
        assert!(compose.contains("127.0.0.1:13306:3306"), "{compose}");
        assert!(policy.ports_declared_explicitly);
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "18080:80"));
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "13306:3306"));
    });
}

#[test]
fn generated_compose_publish_address_can_opt_back_into_public_binding() {
    with_temp_effigy_home("catalog-public-publish-address", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-public-publish-address");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
ports = ["18080:80"]
publish_address = "0.0.0.0"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_prepared_policy(&root, None).expect("policy");
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("compose");

        assert!(compose.contains("0.0.0.0:18080:80"), "{compose}");
        assert!(
            !compose.contains("127.0.0.1:18080:80"),
            "public publish address should replace the loopback default: {compose}"
        );
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "18080:80"));
    });
}

#[test]
fn generated_compose_rejects_invalid_publish_address() {
    with_temp_effigy_home("catalog-invalid-publish-address", |_| {
        let root = temp_repo("catalog-invalid-publish-address");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
publish_address = "not-an-ip"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
"#,
        )
        .expect("write manifest");

        let error = load_container_policy(&root, None).expect_err("should fail");
        assert!(
            error.to_string().contains("host.publish_address"),
            "unexpected error: {error}"
        );
    });
}

#[test]
fn direct_compose_prefers_manifest_host_ports_over_inferred_ports() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("direct-compose-explicit-ports");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[containers.web.host]
ports = ["18080:80"]
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
      - "8080:80"
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert!(policy.ports_declared_explicitly);
    assert_eq!(policy.declared_ports, vec!["18080:80".to_owned()]);
}

/// Catalog ports are never pinned by the render: every published host port is
/// OS-assigned, one per service, and a second resolution does not move them.
#[test]
fn os_assigned_ports_per_catalog() {
    #[derive(Clone, Copy)]
    enum Ports {
        /// One HTTP service and one database.
        Stack,
        /// Two HTTP services, each with its own OS-assigned host port.
        TwoHttpServices,
        /// TCP-alias services stay published on loopback with an OS-host port.
        TcpAliases,
    }

    let stack = r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#;
    let two_http = r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.dbadmin]
catalog = "phpmyadmin"
"#;
    let tcp_aliases = r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
host_ports = ["41001:41001"]

[containers.stack.services.postgres]
catalog = "postgres"
database = "acme"
password = "postgres"

[containers.stack.services.mailpit]
catalog = "mailpit"

[containers.stack.services.silo]
catalog = "silo"
"#;

    let cases: [(&str, &str, Ports, &[&str]); 3] = [
        (
            "one HTTP service and one database",
            stack,
            Ports::Stack,
            &["127.0.0.1::80", "127.0.0.1::3306"],
        ),
        (
            "two HTTP services",
            two_http,
            Ports::TwoHttpServices,
            &["127.0.0.1::80", "127.0.0.1::80"],
        ),
        (
            "TCP-alias services",
            tcp_aliases,
            Ports::TcpAliases,
            &["127.0.0.1::5432", "127.0.0.1::1025", "127.0.0.1::9000"],
        ),
    ];
    for (name, manifest, kind, expected) in cases {
        with_temp_effigy_home("catalog-auto-ports", |_| {
            let _lock = crate::test_env_lock();
            let root = temp_repo("catalog-auto-ports");
            fs::write(root.join("effigy.toml"), manifest).expect("write manifest");

            let policy = load_prepared_policy(&root, None).expect("policy");
            assert!(!policy.ports_declared_explicitly, "{name}");
            let compose = policy
                .read_compose(&policy.compose_files[0])
                .expect("read generated compose");
            for published in expected {
                assert!(
                    policy.declared_ports.contains(&published.to_string()),
                    "{name}: expected {published} beside {:?}",
                    policy.declared_ports
                );
                assert!(
                    compose.contains(&format!("- {published}")),
                    "{name}: {published} is published:
{compose}"
                );
            }

            match kind {
                Ports::Stack => {
                    // A second resolution, without Prepare, keeps the same
                    // ports.
                    let again = load_container_policy(&root, None).expect("second policy");
                    assert_eq!(policy.declared_ports, again.declared_ports, "{name}");
                }
                Ports::TwoHttpServices => {
                    let http_bindings = policy
                        .declared_ports
                        .iter()
                        .filter(|value| value.ends_with(":80"))
                        .count();
                    assert_eq!(http_bindings, 2, "{name}: {compose}");
                    assert_eq!(
                        compose.matches("- 127.0.0.1::80").count(),
                        2,
                        "{name}: both HTTP services publish an OS-assigned host port:\n{compose}"
                    );
                }
                Ports::TcpAliases => {
                    // The alias address is not the published host port: no
                    // service claims a fixed address:port pair.
                    for container_port in ["5432", "1025", "9000"] {
                        assert!(
                            !compose
                                .contains(&format!("127.1.0.1:{container_port}:{container_port}")),
                            "{name}: {compose}"
                        );
                    }
                    assert!(compose.contains(":8025"), "{name}: {compose}");
                    assert!(compose.contains(":9001"), "{name}: {compose}");
                    let again = load_prepared_policy(&root, None).expect("regenerated policy");
                    assert_eq!(again.project_name, policy.project_name, "{name}");
                }
            }
        });
    }
}

#[test]
fn generated_php_workspace_defaults_to_non_root_identity() {
    with_temp_effigy_home("catalog-php-workspace-user", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-php-workspace-user");
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
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("policy");
        assert_eq!(policy.workspace_user.as_deref(), Some("dev"));
        assert_eq!(policy.workspace_home.as_deref(), Some("/home/dev"));
    });
}

#[test]
fn generated_services_infer_working_dir_from_container_config() {
    with_temp_effigy_home("catalog-container-working-dir", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-container-working-dir");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/var/www/html"

[containers.web.services.app]
catalog = "php-fpm"
document_root = "."
version = "8.4"

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = "/vendor/genesis.php"
error_page_404 = "/vendor/genesis.php"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("policy");
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("read compose");

        assert!(
            compose.contains("working_dir: /var/www/html")
                || compose.contains("working_dir: \"/var/www/html\""),
            "generated compose should include the container working_dir: {compose}"
        );
        assert!(
            compose.contains(&format!("{}:/var/www/html", root.display())),
            "generated compose should mount the repo at the container working_dir: {compose}"
        );
    });
}

/// One PHP workspace, three composer-state modes: an enabled host composer
/// home replaces the shared one, the default mounts the shared cache and home,
/// and disabling both shared mounts removes them.
#[test]
fn composer_mount_modes() {
    #[derive(Clone, Copy)]
    enum ComposerMount {
        HostHome,
        Shared,
        Disabled,
    }

    let cases: [(&str, ComposerMount, &str); 3] = [
        (
            "the host composer home is enabled",
            ComposerMount::HostHome,
            "mount_host_composer_home = true",
        ),
        ("the default shared mounts", ComposerMount::Shared, ""),
        (
            "both shared mounts are disabled",
            ComposerMount::Disabled,
            "mount_shared_composer_auth = false\nmount_shared_composer_cache = false",
        ),
    ];
    for (name, mode, params) in cases {
        with_temp_effigy_home("catalog-php-composer", |home| {
            let _lock = crate::test_env_lock();
            let root = temp_repo("catalog-php-composer");
            let host_composer_home = root.join("host-composer-home");
            fs::create_dir_all(&host_composer_home).expect("mkdir host composer home");
            fs::write(
                root.join("effigy.toml"),
                format!(
                    r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
{params}

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#
                ),
            )
            .expect("write manifest");

            let policy = with_test_host_composer_home(Some(&host_composer_home), || {
                load_container_policy(&root, Some("web")).expect("policy")
            });
            if matches!(mode, ComposerMount::Shared) {
                assert!(!home.join("shared/composer-cache").exists(), "{name}");
                crate::policy::mounts::prepare_container_mount_sources(&policy)
                    .expect("prepare cache");
                assert!(home.join("shared/composer-cache").is_dir(), "{name}");
            }
            let rewritten = policy
                .read_compose(&policy.compose_files[0])
                .expect("read compose");

            let host_mount = format!(
                "{}:/home/dev/.config/composer",
                host_composer_home.display()
            );
            let shared_home = ":/home/dev/.config/composer";
            let shared_cache = "/shared/composer-cache:/home/dev/.cache/composer";
            match mode {
                ComposerMount::HostHome => {
                    assert!(rewritten.contains(&host_mount), "{name}: {rewritten}");
                }
                ComposerMount::Shared => {
                    assert!(!rewritten.contains(&host_mount), "{name}: {rewritten}");
                    assert!(rewritten.contains(shared_home), "{name}: {rewritten}");
                    assert!(rewritten.contains(shared_cache), "{name}: {rewritten}");
                }
                ComposerMount::Disabled => {
                    assert!(!rewritten.contains(shared_home), "{name}: {rewritten}");
                    assert!(!rewritten.contains(shared_cache), "{name}: {rewritten}");
                }
            }
        });
    }
}

#[test]
fn generated_php_workspace_host_integration_and_shared_service_stack_proof_stays_stable() {
    with_temp_effigy_home("catalog-php-host-integration-proof", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-php-host-integration-proof");
        let host_composer_home = root.join("host-composer-home");
        let ssh_dir = root.join("ssh-home");
        let sibling = tempfile::tempdir().expect("external sibling tempdir");
        fs::create_dir_all(&host_composer_home).expect("mkdir host composer home");
        fs::create_dir_all(&ssh_dir).expect("mkdir ssh dir");
        fs::create_dir_all(sibling.path().join("public")).expect("mkdir sibling public");
        fs::write(ssh_dir.join("config"), "Host gideonreeling.co.uk\n").expect("write ssh config");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
mounts = [
  {{ host = "{}",
     container = "/var/www/mortcalc",
     external = true }},
]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
mount_host_composer_home = true
ssh_dir_path = "{}"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
                sibling.path().display(),
                ssh_dir.display()
            ),
        )
        .expect("write manifest");

        let policy = with_test_host_composer_home(Some(&host_composer_home), || {
            load_prepared_policy(&root, Some("web")).expect("policy")
        });
        let document = parsed_compose(&policy, 0);
        let volumes = service_volumes(&document, "app");
        assert!(
            volumes.contains(&format!(
                "{}:/home/dev/.config/composer",
                host_composer_home.display()
            )),
            "rewritten compose should mount the host composer home: {document:?}"
        );
        assert!(
            !volumes.contains(&"effigy-shared-composer-home:/home/dev/.config/composer".to_owned()),
            "workspace proof should not fall back to the shared composer-home volume when host composer home is enabled: {document:?}"
        );
        assert!(
            volumes.contains(&format!("{}:/home/dev/.ssh:ro", ssh_dir.display())),
            "rewritten compose should mount the explicit host ssh dir read-only: {document:?}"
        );
        assert!(
            volumes.contains(&format!(
                "{}:/var/www/mortcalc",
                sibling.path().canonicalize().unwrap().display()
            )),
            "rewritten compose should preserve the external host mount: {document:?}"
        );

        assert_eq!(policy.shared_services.len(), 1);
        let shared = &policy.shared_services[0];
        assert_eq!(
            shared.standard_env_vars(),
            vec![
                ("DB_HOST".to_owned(), "host.docker.internal".to_owned()),
                ("MYSQL_HOST".to_owned(), "host.docker.internal".to_owned()),
                ("DB_PORT".to_owned(), shared.host_port.to_string()),
                ("MYSQL_PORT".to_owned(), shared.host_port.to_string()),
            ]
        );
        for key in ["DB_HOST", "MYSQL_HOST"] {
            assert_eq!(
                service_environment(&document, "app", key).as_deref(),
                Some("host.docker.internal"),
                "{key} names the shared service host: {document:?}"
            );
        }
        for key in ["DB_PORT", "MYSQL_PORT"] {
            assert_eq!(
                service_environment(&document, "app", key),
                Some(shared.host_port.to_string()),
                "{key} names the shared service port: {document:?}"
            );
        }
    });
}

#[test]
fn generated_compose_rewrites_shared_backing_services() {
    with_temp_effigy_home("catalog-shared-db", |home| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-shared-db");
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

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_prepared_policy(&root, None).expect("policy");
        assert_eq!(policy.shared_services.len(), 1);
        let shared = &policy.shared_services[0];
        assert_eq!(shared.service_name, "db");
        assert_eq!(shared.catalog, "mariadb");
        assert_eq!(shared.container_port, 3306);

        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("compose");
        assert!(!compose.contains("\n  db:\n"));
        assert!(compose.contains("DB_HOST: host.docker.internal"));
        assert!(compose.contains(&format!("DB_PORT: '{}'", shared.host_port)));

        // A shared render is published by its ensure, not by `prepare`.
        for render in &policy.render.as_ref().expect("rendered").shared {
            crate::policy::render::publish_shared_render(render).expect("publish shared");
        }
        assert!(shared.compose_file.exists());
        let shared_compose = policy
            .read_compose(&shared.compose_file)
            .expect("read shared compose");
        assert!(
            shared_compose.contains(&format!("127.0.0.1:{}:3306", shared.host_port)),
            "shared compose should bind the runtime host port to loopback: {shared_compose}"
        );
        // A shared service belongs to no checkout: its render lives at one
        // machine-wide, content-addressed directory, not in this checkout's
        // generation (effigy#377).
        assert_eq!(
            shared.compose_file,
            home.join("shared-services")
                .join(&shared.project_name)
                .join(&shared.render_digest)
                .join(format!("{}.generated.compose.yml", shared.project_name))
        );
        assert!(
            !shared.compose_file.starts_with(&root),
            "the shared render names no checkout: {}",
            shared.compose_file.display()
        );
    });
}

/// A shared backing service publishes the init SQL its fragment binds, at one
/// machine-wide content-addressed directory, so a fresh home directory works, a
/// later `databases` change reaches the server, and every checkout hands Compose
/// the same bytes (effigy#371, effigy#377).
#[test]
fn a_shared_service_publishes_the_init_sql_it_binds() {
    with_temp_effigy_home("catalog-shared-db-init-sql", |home| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-shared-db-init-sql");
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

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"
databases = ["app", "legacy"]
"#,
        )
        .expect("write manifest");

        let policy = load_prepared_policy(&root, None).expect("policy");
        // A shared render is published by its ensure, not by Prepare (task 414):
        // the test publishes it the way the ensure's first creation does.
        for render in &policy.render.as_ref().expect("rendered").shared {
            crate::policy::render::publish_shared_render(render).expect("publish shared");
        }
        let shared = &policy.shared_services[0];
        assert!(
            home.join("shared-services")
                .join(&shared.project_name)
                .join(&shared.render_digest)
                .join(format!("{}.generated.compose.yml", shared.project_name))
                .is_file(),
            "the shared render is published under the machine-wide home"
        );
        let shared_compose = policy
            .read_compose(&shared.compose_file)
            .expect("read shared compose");
        let document: serde_yaml::Value =
            serde_yaml::from_str(&shared_compose).expect("shared compose is YAML");
        let volumes = document["services"]["db"]["volumes"]
            .as_sequence()
            .expect("the shared service mounts its init SQL");
        let init_sql = volumes
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .find(|mount| mount.contains("/docker-entrypoint-initdb.d/"))
            .expect("the fragment binds an init SQL file");
        let source = Path::new(init_sql.split(':').next().expect("source"));
        assert!(
            source.is_absolute() && source.starts_with(shared.compose_file.parent().unwrap()),
            "the bind source is an absolute file in the shared render, not a checkout path: {init_sql}"
        );
        assert!(
            !init_sql.contains(&root.display().to_string()),
            "the shared render names no checkout: {init_sql}"
        );
        let sql = fs::read_to_string(source).expect("the bind source is a written file");
        assert!(sql.contains("CREATE DATABASE IF NOT EXISTS `app`"), "{sql}");
        assert!(
            sql.contains("CREATE DATABASE IF NOT EXISTS `legacy`"),
            "a declared database reaches the init SQL: {sql}"
        );
        // The render names itself on the service it creates, so a later `up`
        // reads from the runtime whether the shared service is this render
        // (effigy#377).
        assert_eq!(
            document["services"]["db"]["labels"][crate::SHARED_RENDER_LABEL].as_str(),
            Some(shared.render_digest.as_str()),
            "the shared service carries the digest of the render that wrote it: {shared_compose}"
        );
        // Another checkout of the same declaration publishes the same bytes at
        // the same absolute path, so Compose keeps one shared container instead
        // of recreating it per checkout (effigy#377).
        let second_checkout = temp_repo("catalog-shared-db-init-sql-other");
        fs::copy(
            root.join("effigy.toml"),
            second_checkout.join("effigy.toml"),
        )
        .expect("copy manifest");
        let other = load_prepared_policy(&second_checkout, None).expect("other policy");
        let other_shared = &other.shared_services[0];
        assert_eq!(other_shared.render_digest, shared.render_digest);
        assert_eq!(other_shared.compose_file, shared.compose_file);
        assert_eq!(
            fs::read_to_string(&other_shared.compose_file).expect("other shared compose"),
            shared_compose,
            "the shared render is byte-identical from another checkout"
        );
    });
}

#[test]
fn generated_compose_rejects_unsupported_shared_catalog() {
    with_temp_effigy_home("catalog-shared-unsupported", |_| {
        let root = temp_repo("catalog-shared-unsupported");
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
shared = true
"#,
        )
        .expect("write manifest");

        let error = load_container_policy(&root, None).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("unsupported shared catalog `php-fpm`"));
    });
}

#[test]
fn shared_service_env_reaches_clients_and_local_services_keep_their_own() {
    with_temp_effigy_home("catalog-shared-client-scope", |_home| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("catalog-shared-client-scope");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"

[containers.stack.services.dbgate]
catalog = "dbgate"

[containers.stack.services.mysql]
catalog = "mariadb"
database = "acowtancy"

[containers.stack.services.legacy_mysql]
catalog = "mariadb"
database = "acowtancy"
shared = true
"#,
        )
        .expect("write manifest");

        let policy = load_prepared_policy(&root, None).expect("policy");
        let shared = &policy.shared_services[0];
        let compose = policy
            .read_compose(&policy.compose_files[0])
            .expect("compose");
        let document: serde_yaml::Value = serde_yaml::from_str(&compose).expect("parse compose");
        let service = |name: &str| &document["services"][name];
        let environment = |name: &str| -> Vec<String> {
            service(name)["environment"]
                .as_mapping()
                .map(|mapping| {
                    mapping
                        .keys()
                        .filter_map(serde_yaml::Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let gateway = crate::runtime::dns::resolve_runtime_gateway_address("effigy")
            .expect("Colima gateway address");
        let extra_hosts = serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(format!(
            "host.docker.internal:{gateway}"
        ))]);

        assert!(
            !environment("mysql").iter().any(|name| name == "DB_HOST"
                || name == "MYSQL_HOST"
                || name == "DB_PORT"
                || name == "MYSQL_PORT"),
            "a local MariaDB catalog service must not read another MariaDB's health check: {}",
            environment("mysql").join(", ")
        );
        assert!(service("mysql").get("extra_hosts").is_none());

        assert!(
            !environment("dbgate")
                .iter()
                .any(|name| name == "DB_HOST" || name == "MYSQL_HOST"),
            "a catalog appliance keeps its own declared connection: {}",
            environment("dbgate").join(", ")
        );
        assert!(service("dbgate").get("extra_hosts").is_none());

        let workspace_env = environment("workspace");
        for name in ["DB_HOST", "MYSQL_HOST", "DB_PORT", "MYSQL_PORT"] {
            assert!(
                workspace_env.iter().any(|entry| entry == name),
                "the workspace client takes the shared service env: {}",
                workspace_env.join(", ")
            );
        }
        assert_eq!(
            service("workspace")["environment"]["DB_HOST"],
            "host.docker.internal"
        );
        assert_eq!(
            service("workspace")["environment"]["DB_PORT"],
            shared.host_port.to_string()
        );
        assert_eq!(service("workspace")["extra_hosts"], extra_hosts);
    });
}

#[test]
fn load_container_policy_rejects_compose_file_and_services_together() {
    let root = temp_repo("mixed-compose-source");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
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
    assert!(error
        .to_string()
        .contains("cannot combine `compose_file` with"));
}

#[test]
fn direct_compose_policy_writes_runtime_dns_override_for_declared_services() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("direct-compose-runtime-dns");
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
        r#"
services:
  app:
    image: "node:22"
  db:
    image: "postgres:16"
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    // Translated `compose_file` renders like any catalog environment: one
    // generated document carrying ownership labels, plus the DNS override.
    assert_eq!(policy.compose_files.len(), 2);
    assert!(policy.compose_files[0].ends_with(".effigy/runtime/compose/web.generated.compose.yml"));
    assert!(policy.compose_files[1].ends_with(".effigy/runtime/dns/web.compose.override.yml"));
    let dns_override = parsed_compose(&policy, 1);
    for service in ["app", "db"] {
        let servers: Vec<&str> = dns_override["services"][service]["dns"]
            .as_sequence()
            .map(|servers| {
                servers
                    .iter()
                    .filter_map(|server| server.as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            servers,
            vec!["1.1.1.1", "8.8.8.8"],
            "dns override should carry both resolvers for {service}: {dns_override:?}"
        );
        assert!(
            host_gateway_domains(&dns_override, service).is_empty(),
            "dns override should not invent route parity hosts without domains for {service}: {dns_override:?}"
        );
    }
}

#[test]
fn generated_compose_runtime_dns_override_uses_explicit_alias_base_domain() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("generated-compose-route-parity");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.dns]
base_domain = "acme.test"
routes = [
  { domain = "s3.acme.test", tls = false, service = "workspace" },
  { domain = "api.s3.acme.test", tls = false, service = "workspace", port = 41001 },
]

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "generated-compose-route-parity"

[containers.stack.services.postgres]
catalog = "postgres"

[containers.stack.services.mailpit]
catalog = "mailpit"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.compose_files.len(), 2);
    let dns_override = parsed_compose(&policy, 1);
    let gateway_hosts = host_gateway_domains(&dns_override, "workspace");
    for domain in ["s3.acme.test", "api.s3.acme.test"] {
        assert!(
            gateway_hosts
                .iter()
                .any(|host| host == &format!("{domain}:192.168.5.2")),
            "dns override should route {domain} to the gateway: {dns_override:?}"
        );
    }
    for (service, alias) in [
        ("postgres", "postgres.acme.test"),
        ("mailpit", "smtp.acme.test"),
    ] {
        assert!(
            service_aliases(&dns_override, service).iter().any(|entry| entry == alias),
            "dns override should alias {service} as {alias} under the configured base domain: {dns_override:?}"
        );
        assert!(
            !gateway_hosts
                .iter()
                .any(|host| host.starts_with(&format!("{alias}:"))),
            "TCP service aliases must not point back at the host gateway: {dns_override:?}"
        );
        assert!(
            !host_gateway_domains(&dns_override, service)
                .iter()
                .any(|host| host.starts_with(&format!("{alias}:"))),
            "TCP service aliases must not point back at the host gateway: {dns_override:?}"
        );
    }
}

#[test]
fn generated_compose_infers_an_undeclared_alias_base_domain() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("generated-compose-inferred-apex");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.dns]
routes = [
  { domain = "s3.acme.test", tls = false, service = "workspace" },
  { domain = "admin.acme.test", tls = false, service = "workspace" },
  { domain = "acme.test", tls = false, service = "workspace" },
]

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "generated-compose-inferred-apex"

[containers.stack.services.postgres]
catalog = "postgres"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(
        policy.dns_effective_base_domain().as_deref(),
        Some("acme.test"),
        "the shallowest host the primary service serves is the alias apex"
    );
    let dns_override = parsed_compose(&policy, 1);
    let aliases = service_aliases(&dns_override, "postgres");
    assert!(
        aliases.iter().any(|alias| alias == "postgres.acme.test"),
        "Compose aliases use the inferred apex: {dns_override:?}"
    );
    assert!(
        !aliases.iter().any(|alias| alias == "postgres.s3.acme.test"),
        "the front route does not become the alias apex: {dns_override:?}"
    );
}

#[test]
fn direct_compose_policy_rewrites_workspace_mounts_from_manifest_contract() {
    let _lock = crate::test_env_lock();
    let parent = effigy_test_support::Fixture::process_lifetime(&format!(
        "effigy-direct-compose-workspace-mounts-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
    .expect("fixture directory");
    let root = parent.join("workspace-app-reference");
    let platform = parent.join("platform");
    let poodle = parent.join("poodle");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    fs::create_dir_all(&platform).expect("mkdir workspace app");
    fs::create_dir_all(&poodle).expect("mkdir poodle");
    fs::write(
        poodle.join("effigy.toml"),
        r#"
[isolation]
paths = ["node_modules", ".svelte-kit"]
"#,
    )
    .expect("write poodle manifest");
    fs::write(
        root.join("infra/dev/workspace.Dockerfile"),
        "FROM node:22\n",
    )
    .expect("dockerfile");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[systems.dev]
container = "stack"
user = "dev"
home = "/home/dev"
working_dir = "/workspace-root/workspace-app-reference"
mounts = ["../platform", "../poodle"]

[containers.stack]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    build:
      context: ../..
      dockerfile: infra/dev/workspace.Dockerfile
    volumes:
      - ../../../:/workspace-root
      - stack-cache:/cache
volumes:
  stack-cache: {}
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let rewritten = policy
        .read_compose(&policy.compose_files[0])
        .expect("read rewritten workspace compose");
    let expected_rewrite_path = policy
        .render
        .as_ref()
        .unwrap()
        .path
        .join("checkout/.effigy/runtime/compose/stack.workspace.compose.yml");
    let canonical_root = root.canonicalize().expect("canonical repo root");

    assert_eq!(policy.compose_files[0], expected_rewrite_path);
    assert!(
        !expected_rewrite_path.exists(),
        "observation does not publish the rewrite"
    );
    assert!(
        rewritten.contains(":/workspace-root/workspace-app-reference"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/platform"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/poodle"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains("efi-iso-")
            && rewritten.contains(":/workspace-root/poodle/node_modules"),
        "rewritten compose should overlay poodle node_modules isolation with a named volume: {rewritten}"
    );
    assert!(
        rewritten.contains("efi-iso-")
            && rewritten.contains(":/workspace-root/poodle/.svelte-kit"),
        "rewritten compose should overlay poodle .svelte-kit isolation with a named volume: {rewritten}"
    );
    assert!(
        !rewritten.contains("../../../:/workspace-root"),
        "rewritten compose should remove broad parent mount: {rewritten}"
    );
    assert!(
        rewritten.contains(":/cache") && rewritten.contains("name: efv-"),
        "rewritten compose should preserve and compact named volumes: {rewritten}"
    );
    assert!(
        rewritten.contains("name: efi-iso-"),
        "rewritten compose should declare poodle node_modules isolation volume: {rewritten}"
    );
    assert!(
        !rewritten.contains("user: dev"),
        "rewritten compose should not force runtime user: {rewritten}"
    );
    assert!(
        !rewritten.contains("HOME: /home/dev"),
        "rewritten compose should not force runtime HOME: {rewritten}"
    );
    assert!(
        rewritten.contains(&format!("context: {}", canonical_root.display())),
        "rewritten compose should preserve relocated build context: {rewritten}"
    );
    assert!(
        rewritten.contains("dockerfile: infra/dev/workspace.Dockerfile"),
        "rewritten compose should preserve dockerfile path relative to build context: {rewritten}"
    );
    assert_eq!(policy.workspace_user.as_deref(), Some("dev"));
    assert_eq!(policy.workspace_home.as_deref(), Some("/home/dev"));
}

#[test]
fn generated_php_nginx_stack_compacts_mirrored_named_volumes_across_services() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("catalog-php-nginx-shared-volume-compact");
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
document_root = "."
isolated_dirs = ["vendor", "node_modules"]

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = "/vendor/genesis.php"
error_page_404 = "/vendor/genesis.php"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, Some("web")).expect("policy");
    let rewritten = policy
        .read_compose(&policy.compose_files[0])
        .expect("read rewritten workspace compose");

    assert!(
        rewritten.contains("efv-"),
        "rewritten compose should compact named volume names: {rewritten}"
    );
    assert!(
        !rewritten.contains("catalog-php-nginx-shared-volume-compact-web-app-var-www-html-vendor"),
        "rewritten compose should not leave the long vendor volume name behind: {rewritten}"
    );
    assert!(
        !rewritten
            .contains("catalog-php-nginx-shared-volume-compact-web-app-var-www-html-node-modules"),
        "rewritten compose should not leave the long node_modules volume name behind: {rewritten}"
    );
    assert!(
        rewritten.contains(":/var/www/html/vendor:ro"),
        "rewritten compose should keep the nginx read-only vendor mirror mount: {rewritten}"
    );
    assert!(
        rewritten.contains(":/var/www/html/node_modules:ro"),
        "rewritten compose should keep the nginx read-only node_modules mirror mount: {rewritten}"
    );
}

/// An environment that opts into `stable_ports` gets one concrete host port
/// per published container port, derived from its checkout path.
#[test]
fn stable_ports_opt_in_renders_derived_ports() {
    with_temp_effigy_home("stable-ports-opt-in", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("stable-ports-opt-in");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"
[containers.web]
primary_service = "app"
[containers.web.host]
stable_ports = true
[containers.web.services.app]
catalog = "node"
"#,
        )
        .expect("manifest");

        let prepared = load_prepared_policy(&root, None).expect("prepare");
        let port = prepared.declared_ports[0]
            .split(':')
            .next()
            .and_then(|port| port.parse::<u16>().ok())
            .expect("a concrete host port");
        assert!(port >= 20000, "{}", prepared.declared_ports[0]);
        let again = load_prepared_policy(&root, None).expect("repeat prepare");
        assert_eq!(again.declared_ports, prepared.declared_ports);
    });
}

#[test]
fn fresh_worktree_creation_scopes_local_resources_but_keeps_shared_service() {
    let fixture = tempfile::tempdir().unwrap();
    fs::create_dir(fixture.path().join("home")).unwrap();
    crate::with_test_effigy_home(&fixture.path().join("home"), || {
        let manifest = r#"
[containers]
default = "web"
[containers.web]
project_name = "fixture-dev"
primary_service = "app"
[containers.web.dns]
base_domain = "acme.test"
routes = [{ domain = "app.acme.test", service = "app", tls = false }]
[containers.web.services.app]
catalog = "workspace-rust-bun"
[containers.web.services.postgres]
catalog = "postgres"
[containers.web.services.shared_db]
catalog = "mariadb"
shared = true
version = "10.11"
"#;
        let (primary, worktrees) =
            create_linked_worktree_fixture(fixture.path(), manifest, &["linked"]);
        let linked = &worktrees[0];
        let token_dir = effigy_core::worktree_scope::resolve(linked)
            .unwrap()
            .unwrap()
            .token_dir;
        fs::remove_file(token_dir.join(effigy_core::worktree_scope::SCOPE_FILE)).unwrap();
        let primary_policy = load_prepared_policy(&primary, None).unwrap();
        assert!(effigy_core::worktree_scope::load_scoped(linked)
            .unwrap()
            .is_none());
        let linked_policy = load_prepared_policy(linked, None).unwrap();
        assert_ne!(primary_policy.project_name, linked_policy.project_name);
        assert!(linked_policy.project_name.starts_with("fixture-dev-wt-"));
        assert_ne!(
            primary_policy.dns_routes[0].domain,
            linked_policy.dns_routes[0].domain
        );
        assert!(primary_policy.managed_volumes.iter().all(|v| linked_policy
            .managed_volumes
            .iter()
            .all(|other| v.name != other.name)));
        assert_eq!(
            primary_policy.shared_services[0].project_name,
            linked_policy.shared_services[0].project_name
        );
        let aliases = service_aliases(&parsed_compose(&linked_policy, 1), "postgres");
        assert!(aliases
            .iter()
            .any(|alias| alias.starts_with("postgres.acme-w")));
    });
}
