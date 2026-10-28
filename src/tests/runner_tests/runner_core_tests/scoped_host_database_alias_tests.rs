//! The host-native half of the scoped database alias (effigy#264, effigy#431,
//! contract 005).
//!
//! A linked worktree keeps the canonical declared alias in its configuration
//! (`postgres.acme.test`) and answers its own backing service on the scope's
//! effective alias (`postgres.acme-w<key>.test`). A host process resolving the
//! canonical name reaches the gateway route that published it — normally the
//! primary checkout's — so Effigy resolves the declared alias in the
//! environment it delivers to a host run, and refuses when this checkout has
//! no published backing service to resolve it to.
//!
//! The fixtures hold one primary checkout and two marked linked worktrees of
//! one manifest. Nothing starts a container runtime: each scope's published
//! alias route is written to a private gateway route table, which is exactly
//! the record `container up` leaves behind.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::routes::{Route, RouteSource};

use crate::runner::gateway_command::{gateway_dir, set_test_gateway_home};
use crate::runner::tests::prelude::*;

/// One manifest for every checkout: a declared apex, one HTTP route and the
/// catalog `postgres` backing service. The task keeps the canonical declared
/// alias in its `env`, exactly as consumer configuration does.
const MANIFEST: &str = r#"
[containers]
default = "web"

[containers.web]
project_name = "app-dev"
primary_service = "postgres"

[containers.web.dns]
base_domain = "acme.test"
routes = [
  { domain = "app.acme.test", tls = false, service = "postgres" },
]

[containers.web.services.postgres]
catalog = "postgres"

[tasks.check]
run_in = "host"
run = "sh -lc 'printf \"%s\\n\" \"$DATABASE_URL\" > \"$OUT\"'"
env = { DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app", OUT = "{repo}/database-url.txt" }

[tasks.steps]
run_in = "host"
run = [
  { id = "declare", env = { DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" } },
  { id = "record", run = "sh -lc 'printf \"\\n%s\\n\" \"$DATABASE_URL\" > \"$OUT\"'" },
]
env = { OUT = "{repo}/database-url.txt" }

[tasks.remote]
run_in = "container"
run = "sh -lc 'printf \"%s\\n\" \"$DATABASE_URL\" > \"$OUT\"'"
env = { DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app", OUT = "{repo}/database-url.txt" }
"#;

/// One fixture root with a primary checkout that declares the canonical alias.
fn primary_checkout(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    let primary = root.join("primary");
    fs::create_dir_all(primary.join(".git/worktrees")).expect("primary git directory");
    fs::write(primary.join("effigy.toml"), MANIFEST).expect("primary manifest");
    primary
}

/// A marked linked worktree of the primary, with its own scope token, exactly
/// as `git worktree add` plus a first Effigy run leaves it.
fn linked_worktree(primary: &Path, name: &str) -> PathBuf {
    let checkout = primary.parent().expect("fixture root").join(name);
    let private = primary.join(".git/worktrees").join(name);
    fs::create_dir_all(&checkout).expect("worktree checkout");
    fs::create_dir_all(&private).expect("worktree private state");
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", private.display()),
    )
    .expect("worktree git link");
    fs::write(private.join("commondir"), "../..\n").expect("worktree commondir");
    fs::write(checkout.join("effigy.toml"), MANIFEST).expect("worktree manifest");
    effigy_core::worktree_scope::prepare_checkout_token(&checkout).expect("scope token");
    checkout
}

/// The effective apex this checkout's aliases answer on.
fn effective_alias(checkout: &Path, label: &str) -> String {
    effigy_core::worktree_scope::load(checkout)
        .expect("read scope token")
        .expect("a marked checkout has a token");
    let policy = effigy_containers::load_container_policy(checkout, None).expect("policy loads");
    let base = policy
        .dns_effective_base_domain()
        .expect("declared base domain");
    assert_ne!(base, "acme.test", "a scoped checkout rewrites the apex");
    format!("{label}.{base}")
}

/// Publish one alias route for a scope, the record `container up` leaves
/// behind once the backing service has a published host port.
fn publish_alias_route(checkout: &Path, domain: &str, host_port: u16) {
    publish_alias_route_with_endpoint(checkout, domain, 5432, host_port);
}

/// The route `container up` publishes: the checkout's loopback address, the
/// declared container port and the backing service's published host port.
fn publish_alias_route_with_endpoint(
    checkout: &Path,
    domain: &str,
    container_port: u16,
    host_port: u16,
) {
    let table_path = gateway_dir().expect("gateway dir").join("routes.json");
    EndpointStore::for_routes(&table_path)
        .routes::<_, effigy_gateway::GatewayError>(|table| {
            table.upsert(Route {
                domain: domain.to_owned(),
                target: None,
                dns_ip: Some(Ipv4Addr::LOCALHOST),
                tcp_port: Some(container_port),
                tcp_target: Some(format!("127.0.0.1:{host_port}")),
                source: RouteSource::Container,
                project: checkout.display().to_string(),
                managed_listener: None,
                tls: false,
                registered: chrono::Utc::now(),
            });
            Ok(())
        })
        .expect("publish the scope's alias route");
}

/// A real backing service: it answers every connection with its own token, so
/// a client can tell which scope's database it reached.
struct FakeDatabase {
    port: u16,
    token: &'static str,
}

fn fake_database(token: &'static str) -> FakeDatabase {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the fake database");
    let port = listener.local_addr().expect("fake database address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.write_all(format!("{token}\n").as_bytes());
            let _ = stream.flush();
        }
    });
    FakeDatabase { port, token }
}

/// A loopback port nothing holds, for the alias listener the gateway binds.
fn free_loopback_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("probe a free port");
    let port = listener.local_addr().expect("probe address").port();
    drop(listener);
    port
}

/// Start the gateway's own TCP alias manager over the routes this fixture
/// published, so a client reaching a scope's alias address really is forwarded
/// to that scope's backing service.
fn start_tcp_alias_manager() -> tokio::sync::watch::Sender<bool> {
    let table_path = gateway_dir().expect("gateway dir").join("routes.json");
    let table = EndpointStore::for_routes(&table_path)
        .snapshot()
        .and_then(|snapshot| snapshot.routes())
        .expect("read the published routes");
    let table = std::sync::Arc::new(std::sync::RwLock::new(table));
    let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("alias manager runtime");
        let _ = runtime.block_on(effigy_gateway::tcp_alias::run_tcp_alias_manager(
            table,
            shutdown_rx,
        ));
    });
    shutdown
}

/// Ask the gateway what address a domain answers with, then dial the scope's
/// declared container port there. This is the client's own two steps: it
/// resolves the delivered host through the route record and connects to it.
fn reach_database(domain: &str, container_port: u16) -> String {
    let table_path = gateway_dir().expect("gateway dir").join("routes.json");
    let table = EndpointStore::for_routes(&table_path)
        .snapshot()
        .and_then(|snapshot| snapshot.routes())
        .expect("read the published routes");
    let route = table
        .lookup(domain)
        .expect("the scope's published alias route");
    let address = (route.dns_ip.expect("the alias address"), container_port);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect(address) {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut token = String::new();
            let _ = stream.read_to_string(&mut token);
            if !token.trim().is_empty() {
                return token.trim().to_owned();
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the gateway never answered on {address:?}");
}

fn database_url(checkout: &Path) -> String {
    fs::read_to_string(checkout.join("database-url.txt"))
        .expect("the task recorded the URL it received")
        .trim()
        .to_owned()
}

/// A private gateway route table, so no test reads or writes the operator's.
fn with_private_gateway<T>(name: &str, run: impl FnOnce() -> T) -> T {
    let home = temp_workspace(&format!("gateway-home-{name}"));
    let guard = set_test_gateway_home(&home);
    let result = run();
    drop(guard);
    result
}

#[test]
fn each_scoped_host_task_reaches_only_its_own_declared_database_alias() {
    with_private_gateway("scoped-host-alias", || {
        let primary = primary_checkout("scoped-host-alias");
        let one = linked_worktree(&primary, "worktree-one");
        let two = linked_worktree(&primary, "worktree-two");
        let one_alias = effective_alias(&one, "postgres");
        let two_alias = effective_alias(&two, "postgres");
        assert_ne!(one_alias, two_alias, "each scope keeps its own apex");

        // Each scope's own backing service, with a distinct published port.
        publish_alias_route(&one, &one_alias, 55432);
        publish_alias_route(&two, &two_alias, 55433);

        run_task(&one, "check", &[]).expect("the first worktree's task runs");
        run_task(&two, "check", &[]).expect("the second worktree's task runs");

        let one_url = database_url(&one);
        let two_url = database_url(&two);
        assert_eq!(
            one_url,
            format!("postgres://app:secret@{one_alias}:5432/app"),
            "the first worktree's task reached its own alias"
        );
        assert_eq!(
            two_url,
            format!("postgres://app:secret@{two_alias}:5432/app"),
            "the second worktree's task reached its own alias"
        );
        for (name, url, other) in [("one", &one_url, &two_alias), ("two", &two_url, &one_alias)] {
            assert!(
                !url.contains(other.as_str()),
                "worktree {name} never names its sibling's alias: {url}"
            );
            assert!(
                !url.contains("@postgres.acme.test:"),
                "worktree {name} never keeps the canonical name, which would reach the primary: {url}"
            );
        }

        // The primary checkout keeps its declared name: it is the route that
        // name belongs to, so nothing is rewritten there.
        run_task(&primary, "check", &[]).expect("the primary checkout's task runs");
        assert_eq!(
            database_url(&primary),
            "postgres://app:secret@postgres.acme.test:5432/app",
            "a primary checkout keeps its declared alias"
        );
    });
}

/// The reachability half of the proof: each scope's task receives its own
/// alias, the gateway forwards that alias to that scope's own backing service,
/// and no scope's task ever reaches its sibling's database. Two real TCP
/// backing services and the gateway's own TCP alias manager do the forwarding;
/// only DNS and the container runtime are fixtures.
#[test]
fn each_scoped_host_task_reaches_only_its_own_test_database() {
    with_private_gateway("scoped-host-database-reach", || {
        let primary = primary_checkout("scoped-host-database-reach");
        let one = linked_worktree(&primary, "worktree-one");
        let two = linked_worktree(&primary, "worktree-two");
        let one_alias = effective_alias(&one, "postgres");
        let two_alias = effective_alias(&two, "postgres");
        assert_ne!(one_alias, two_alias);

        let one_database = fake_database("database-one");
        let two_database = fake_database("database-two");
        let one_port = free_loopback_port();
        let two_port = free_loopback_port();
        assert_ne!(one_port, two_port);
        publish_alias_route_with_endpoint(&one, &one_alias, one_port, one_database.port);
        publish_alias_route_with_endpoint(&two, &two_alias, two_port, two_database.port);
        let shutdown = start_tcp_alias_manager();

        run_task(&one, "check", &[]).expect("the first worktree's task runs");
        run_task(&two, "check", &[]).expect("the second worktree's task runs");

        for (name, checkout, alias, port, own, sibling) in [
            (
                "one",
                &one,
                &one_alias,
                one_port,
                one_database.token,
                two_database.token,
            ),
            (
                "two",
                &two,
                &two_alias,
                two_port,
                two_database.token,
                one_database.token,
            ),
        ] {
            let delivered = database_url(checkout);
            let host = delivered
                .rsplit('@')
                .next()
                .expect("a URL authority")
                .split(':')
                .next()
                .expect("a host");
            assert_eq!(host, alias, "worktree {name} received its own alias");
            let reached = reach_database(host, port);
            assert_eq!(
                reached, own,
                "worktree {name} reached its own database, not its sibling's"
            );
            assert_ne!(reached, sibling, "worktree {name} never reaches a sibling");
        }
        let _ = shutdown.send(true);
    });
}

#[test]
fn a_scoped_host_task_refuses_before_it_runs_without_a_published_backing_service() {
    with_private_gateway("scoped-host-alias-absent", || {
        let primary = primary_checkout("scoped-host-alias-absent");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");

        let error = run_task(&checkout, "check", &[]).expect_err("the task must refuse");
        assert_task_invocation_error_contains(
            error,
            &[
                "DATABASE_URL",
                "postgres.acme.test",
                &alias,
                "no route for it is recorded",
                "effigy container up web",
            ],
        );
        assert!(
            !checkout.join("database-url.txt").exists(),
            "the refused task never ran its command"
        );
    });
}

#[test]
fn a_scoped_host_task_refuses_an_alias_route_another_checkout_owns() {
    with_private_gateway("scoped-host-alias-sibling", || {
        let primary = primary_checkout("scoped-host-alias-sibling");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        // The route answers, but it records the primary checkout: resolving
        // this alias would reach another scope's database.
        publish_alias_route(&primary, &alias, 55432);

        let error = run_task(&checkout, "check", &[]).expect_err("the task must refuse");
        assert_task_invocation_error_contains(error, &[&alias, "belongs to"]);
        assert!(
            !checkout.join("database-url.txt").exists(),
            "the refused task never ran its command"
        );
    });
}

#[test]
fn a_scoped_host_task_refuses_an_alias_route_with_no_published_port() {
    with_private_gateway("scoped-host-alias-unpublished", || {
        let primary = primary_checkout("scoped-host-alias-unpublished");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        publish_alias_route(&checkout, &alias, 55432);
        let table_path = gateway_dir().expect("gateway dir").join("routes.json");
        EndpointStore::for_routes(&table_path)
            .routes::<_, effigy_gateway::GatewayError>(|table| {
                let mut route = table.lookup(&alias).cloned().expect("the published route");
                route.tcp_target = None;
                table.upsert(route);
                Ok(())
            })
            .expect("withdraw the published port");

        let error = run_task(&checkout, "check", &[]).expect_err("the task must refuse");
        assert_task_invocation_error_contains(error, &[&alias, "names no published upstream port"]);
        assert!(
            !checkout.join("database-url.txt").exists(),
            "the refused task never ran its command"
        );
    });
}

#[test]
fn a_scoped_host_task_resolves_an_env_schema_secret_and_keeps_the_task_env_precedence() {
    with_private_gateway("scoped-host-alias-schema", || {
        let primary = primary_checkout("scoped-host-alias-schema");
        let checkout = linked_worktree(&primary, "worktree-one");
        fs::write(
            checkout.join(".env.schema"),
            "# The declared backing service @type=url\nDATABASE_URL=\n\n# A second backing service @type=url @sensitive\nSECONDARY_DATABASE_URL=\n",
        )
        .expect("env schema");
        // `.env` carries the canonical declared names, exactly as consumer
        // configuration does. The schema's own defaults lose to it.
        fs::write(
            checkout.join(".env"),
            "SECONDARY_DATABASE_URL=postgres://schema@postgres.acme.test:5432/secondary\n\
             DATABASE_URL=postgres://schema@postgres.acme.test:5432/schema-default\n",
        )
        .expect("dotenv");
        let manifest = format!(
            "{MANIFEST}\n[env_schema]\nschema = \".env.schema\"\n\n\
             [tasks.check-schema]\n\
             run_in = \"host\"\n\
             run = \"sh -lc 'printf \\\"%s|%s\\\" \\\"$DATABASE_URL\\\" \\\"$SECONDARY_DATABASE_URL\\\" > \\\"$OUT\\\"'\"\n\
             env = {{ DATABASE_URL = \"postgres://app:secret@postgres.acme.test:5432/app\", OUT = \"{{repo}}/database-url.txt\" }}\n"
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with env schema");
        let alias = effective_alias(&checkout, "postgres");
        publish_alias_route(&checkout, &alias, 55432);

        run_task(&checkout, "check-schema", &[]).expect("the task runs");
        let recorded = database_url(&checkout);
        assert_eq!(
            recorded,
            format!(
                "postgres://app:secret@{alias}:5432/app|postgres://schema@{alias}:5432/secondary"
            ),
            "the task's own env wins over the env-schema default, and the sensitive `.env` value \
             resolves by the same rule"
        );
    });
}

#[test]
fn a_sequence_step_environment_resolves_the_same_declared_alias() {
    with_private_gateway("scoped-host-alias-sequence", || {
        let primary = primary_checkout("scoped-host-alias-sequence");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        publish_alias_route(&checkout, &alias, 55432);

        run_task(&checkout, "steps", &[]).expect("the sequence runs");
        assert_eq!(
            database_url(&checkout),
            format!("postgres://app:secret@{alias}:5432/app"),
            "a step that inherits a declared alias resolves it like the task's own env"
        );
    });
}

/// A managed session (`mode = "tui"`) renders its own process plan, so it
/// resolves the same declared alias: the session's workload receives this
/// checkout's own effective alias, and a missing backing service refuses the
/// session before its workload starts.
#[test]
fn a_managed_session_resolves_the_scoped_database_alias() {
    with_private_gateway("scoped-host-alias-managed", || {
        let primary = primary_checkout("scoped-host-alias-managed");
        let checkout = linked_worktree(&primary, "worktree-one");
        let manifest = format!(
            "{MANIFEST}\n\
             [tasks.dev]\n\
             mode = \"tui\"\n\
             run_in = \"host\"\n\
             run = \"sh -lc 'printf \\\"%s\\\\n\\\" \\\"$DATABASE_URL\\\" > \\\"$OUT\\\"'\"\n\
             env = {{ DATABASE_URL = \"postgres://app:secret@postgres.acme.test:5432/app\", OUT = \"{{repo}}/session-database-url.txt\" }}\n"
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with a managed task");
        let alias = effective_alias(&checkout, "postgres");

        let error = run_task(&checkout, "dev", &["--headless"])
            .expect_err("the session refuses before its workload starts");
        assert_task_invocation_error_contains(
            error,
            &[
                "DATABASE_URL",
                "postgres.acme.test",
                &alias,
                "effigy container up web",
            ],
        );
        assert!(
            !checkout.join("session-database-url.txt").exists(),
            "the refused session never ran its workload"
        );

        publish_alias_route(&checkout, &alias, 55432);
        run_task(&checkout, "dev", &["--headless"]).expect("the managed session runs");
        assert_eq!(
            fs::read_to_string(checkout.join("session-database-url.txt"))
                .expect("the session workload recorded the URL it received")
                .trim(),
            format!("postgres://app:secret@{alias}:5432/app"),
            "the managed session's workload received this checkout's own alias"
        );
    });
}

/// The command Effigy renders for one task, without dispatching anything.
fn rendered_command(root: &Path, selector: &str) -> crate::runner::execute::TaskCommand {
    use crate::runner::execute::api::build_execution_preflight;
    use crate::runner::execute::{build_task_command, resolve_task_selection, SelectionResolution};
    let invocation = effigy_cli::TaskInvocation {
        name: selector.to_owned(),
        args: Vec::new(),
    };
    let preflight =
        build_execution_preflight(&invocation, root.to_path_buf()).expect("read-only preflight");
    let SelectionResolution::Selected { selection, .. } =
        resolve_task_selection(&invocation, &preflight).expect("select the task")
    else {
        panic!("expected a selected fixture task");
    };
    build_task_command(&preflight, &selection, &None).expect("render the task command")
}

#[test]
fn a_container_task_keeps_the_canonical_alias_the_scope_network_answers() {
    with_private_gateway("scoped-host-alias-container", || {
        let primary = primary_checkout("scoped-host-alias-container");
        let checkout = linked_worktree(&primary, "worktree-one");
        // A container-bound task needs a system and workspace binding; the
        // host task keeps its explicit `run_in = "host"`.
        fs::write(
            checkout.join("effigy.toml"),
            format!(
                "{MANIFEST}\n[systems]\ndefault = \"web\"\n\n\
                 [systems.web]\ndefault_workspace = \"postgres\"\n\n\
                 [systems.web.workspaces.postgres]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n"
            ),
        )
        .expect("manifest with a workspace binding");
        let alias = effective_alias(&checkout, "postgres");
        publish_alias_route(&checkout, &alias, 55432);

        let host = rendered_command(&checkout, "check");
        assert!(
            host.command.contains(&format!("@{alias}:5432/app")),
            "the host run receives this scope's alias: {}",
            host.command
        );
        assert!(
            host.host_aliases.is_some(),
            "the host run carries its alias resolution for the secret values it delivers"
        );

        // The container target keeps the canonical declared name: the scope's
        // own Compose network answers it next to the effective alias
        // (`linked_worktree_container_target_resolves_its_own_declared_database_alias`).
        let remote = rendered_command(&checkout, "remote");
        assert!(
            remote.command.contains("@postgres.acme.test:5432/app"),
            "a container task keeps the declared alias: {}",
            remote.command
        );
        assert!(
            remote.host_aliases.is_none(),
            "a container task owes no host-side alias resolution"
        );
    });
}
