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

use std::io::Write;
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
    publish_alias_route_with_endpoint(checkout, domain, Ipv4Addr::LOCALHOST, 5432, host_port);
}

/// The route `container up` publishes: the checkout's loopback address, the
/// declared container port and the backing service's published host port.
fn publish_alias_route_with_endpoint(
    checkout: &Path,
    domain: &str,
    address: Ipv4Addr,
    container_port: u16,
    host_port: u16,
) {
    let table_path = gateway_dir().expect("gateway dir").join("routes.json");
    EndpointStore::for_routes(&table_path)
        .routes::<_, effigy_gateway::GatewayError>(|table| {
            table.upsert(Route {
                domain: domain.to_owned(),
                target: None,
                dns_ip: Some(address),
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

/// A private DNS resolver and TCP-alias gateway over this fixture's own
/// routes: the same two hops a client takes, without touching the operator's
/// gateway or machine-wide DNS.
struct PrivateGateway {
    shutdown: tokio::sync::watch::Sender<bool>,
}

fn start_private_gateway(dns_port: u16) -> PrivateGateway {
    let table_path = gateway_dir().expect("gateway dir").join("routes.json");
    let table = EndpointStore::for_routes(&table_path)
        .snapshot()
        .and_then(|snapshot| snapshot.routes())
        .expect("read the published routes");
    let table = std::sync::Arc::new(std::sync::RwLock::new(table));
    let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
    let dns_shutdown = shutdown_rx.clone();
    let alias_table = std::sync::Arc::clone(&table);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("private gateway runtime");
        runtime.block_on(async move {
            let config = effigy_gateway::dns::DnsConfig {
                bind_addr: (Ipv4Addr::LOCALHOST, dns_port).into(),
                tld: "test".to_owned(),
                resolve_to: Ipv4Addr::LOCALHOST,
            };
            let dns = tokio::spawn(effigy_gateway::dns::run_dns_server(
                config,
                table,
                std::sync::Arc::new(effigy_gateway::stats::GatewayStats::new()),
                std::sync::Arc::new(effigy_gateway::dns::DnsCache::new(Duration::from_millis(
                    50,
                ))),
                dns_shutdown,
            ));
            let _ =
                effigy_gateway::tcp_alias::run_tcp_alias_manager(alias_table, shutdown_rx).await;
            dns.abort();
        });
    });
    PrivateGateway { shutdown }
}

impl PrivateGateway {
    /// Wait until every scope address answers on the declared container port,
    /// so a task's connect is the gateway's forwarding and not a race.
    fn wait_until_ready(&self, addresses: &[Ipv4Addr]) {
        let deadline = Instant::now() + Duration::from_secs(20);
        for address in addresses {
            loop {
                if TcpStream::connect((*address, 5432)).is_ok() {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "the TCP alias listener never bound {address}:5432"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

impl Drop for PrivateGateway {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

/// A free alias address: the pool's own loopback range, probed on the declared
/// container port so no live listener is asked to share it. The highest
/// addresses are tried first, away from the pool's lowest-first allocation.
fn free_alias_addresses(count: usize) -> Vec<Ipv4Addr> {
    let mut found = Vec::new();
    for last in (1..=50).rev() {
        let address = Ipv4Addr::new(127, 1, 0, last);
        if TcpListener::bind((address, 5432)).is_ok() {
            found.push(address);
        }
        if found.len() == count {
            break;
        }
    }
    assert_eq!(
        found.len(),
        count,
        "three free loopback alias addresses on port 5432: found {found:?}"
    );
    found
}

/// A free UDP port for the private resolver.
fn free_udp_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("probe a free UDP port");
    let port = socket.local_addr().expect("probe address").port();
    drop(socket);
    port
}

fn database_url(checkout: &Path) -> String {
    fs::read_to_string(checkout.join("database-url.txt"))
        .expect("the task recorded the URL it received")
        .trim()
        .to_owned()
}

/// A private gateway route table and a short scratch root: a detached run's
/// control socket is bounded, and a reviewer's plain selector run has no say
/// over `TMPDIR`, so this suite pins a short one for its own fixtures.
fn with_short_scratch<T>(name: &str, run: impl FnOnce() -> T) -> T {
    let short = tempfile::Builder::new()
        .prefix("ef431-")
        .tempdir_in("/tmp")
        .expect("short fixture root");
    let _env = EnvGuard::set_many(&[("TMPDIR", Some(short.path().display().to_string()))]);
    let home = temp_workspace(&format!("gateway-home-{name}"));
    let guard = set_test_gateway_home(&home);
    let result = run();
    drop(guard);
    result
}

#[test]
fn each_scoped_host_task_reaches_only_its_own_declared_database_alias() {
    with_short_scratch("scoped-host-alias", || {
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
/// Write the task that connects: it resolves the host of the URL Effigy
/// delivered through the private resolver, then connects to that host on the
/// URL's own port and records what it reached.
fn write_connect_manifest(checkout: &Path, dns_port: u16) {
    let out = checkout.join("reached.txt");
    let manifest = format!(
        r#"{MANIFEST}
[tasks.connect]
run_in = "host"
run = "sh -lc 'url=$DATABASE_URL; rest=$(printf %s $url | cut -d@ -f2); host=$(printf %s $rest | cut -d: -f1); port=$(printf %s $rest | cut -d: -f2 | cut -d/ -f1); ip=$(dig +short +time=2 +tries=3 @127.0.0.1 -p $DNS_PORT $host | head -1); echo $host > $OUT; echo $port >> $OUT; echo $ip >> $OUT; if [ -z $ip ]; then exit 4; fi; exec 3<>/dev/tcp/$ip/$port || exit 5; head -c 128 <&3 >> $OUT'"
env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app", DNS_PORT = "{dns_port}", OUT = "{}" }}
"#,
        out.display()
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with a connecting task");
}

/// Read what a connecting task recorded: the host and port it used, the
/// address it resolved, and the token its backing service answered with.
fn reached(checkout: &Path) -> (String, String, String, String) {
    let recorded = fs::read_to_string(checkout.join("reached.txt"))
        .expect("the task recorded what it reached");
    let fields = recorded.lines().collect::<Vec<_>>();
    assert_eq!(fields.len(), 4, "host, port, address and token: {recorded}");
    (
        fields[0].to_owned(),
        fields[1].to_owned(),
        fields[2].to_owned(),
        fields[3].trim().to_owned(),
    )
}

/// Each scope's host task connects with the URL Effigy delivered it — the same
/// host, and the declared port 5432 — and reaches only its own backing
/// service. The primary checkout keeps the declared name; the two linked
/// worktrees use their own effective aliases. The private resolver and the
/// gateway's own TCP alias manager are the two hops a client takes; only the
/// container runtime is a fixture.
#[test]
fn each_scoped_host_task_connects_to_only_its_own_test_database() {
    with_short_scratch("scoped-host-database-reach", || {
        let primary = primary_checkout("scoped-host-database-reach");
        let one = linked_worktree(&primary, "worktree-one");
        let two = linked_worktree(&primary, "worktree-two");
        let primary_alias = "postgres.acme.test".to_owned();
        let one_alias = effective_alias(&one, "postgres");
        let two_alias = effective_alias(&two, "postgres");
        assert_ne!(one_alias, two_alias, "each scope keeps its own apex");

        let addresses = free_alias_addresses(3);
        let databases = [
            fake_database("database-primary"),
            fake_database("database-one"),
            fake_database("database-two"),
        ];
        for (((checkout, alias), address), database) in [
            (&primary, &primary_alias),
            (&one, &one_alias),
            (&two, &two_alias),
        ]
        .iter()
        .zip(addresses.iter())
        .zip(databases.iter())
        {
            publish_alias_route_with_endpoint(checkout, alias, *address, 5432, database.port);
        }
        let dns_port = free_udp_port();
        let gateway = start_private_gateway(dns_port);
        gateway.wait_until_ready(&addresses);
        for checkout in [&primary, &one, &two] {
            write_connect_manifest(checkout, dns_port);
        }

        for (name, checkout, alias, own, siblings) in [
            (
                "the primary checkout",
                &primary,
                &primary_alias,
                databases[0].token,
                [databases[1].token, databases[2].token],
            ),
            (
                "worktree one",
                &one,
                &one_alias,
                databases[1].token,
                [databases[0].token, databases[2].token],
            ),
            (
                "worktree two",
                &two,
                &two_alias,
                databases[2].token,
                [databases[0].token, databases[1].token],
            ),
        ] {
            run_task(checkout, "connect", &[])
                .unwrap_or_else(|error| panic!("{name}'s task runs: {error}"));
            let (host, port, _address, token) = reached(checkout);
            assert_eq!(host, *alias, "{name} connected with its own alias");
            assert_eq!(
                port, "5432",
                "{name} connected on the port its delivered URL declared"
            );
            assert_eq!(
                token, own,
                "{name} reached its own backing service, not a sibling's"
            );
            for sibling in siblings {
                assert_ne!(token, sibling, "{name} never reaches a sibling");
            }
        }
    });
}

#[test]
fn a_scoped_host_task_refuses_before_it_runs_without_a_published_backing_service() {
    with_short_scratch("scoped-host-alias-absent", || {
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
    with_short_scratch("scoped-host-alias-sibling", || {
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
    with_short_scratch("scoped-host-alias-unpublished", || {
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
    with_short_scratch("scoped-host-alias-schema", || {
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
    with_short_scratch("scoped-host-alias-sequence", || {
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
/// resolves the same declared alias: its workload receives this checkout's own
/// effective alias.
#[test]
fn a_managed_session_resolves_the_scoped_database_alias() {
    with_short_scratch("scoped-host-alias-managed", || {
        let primary = primary_checkout("scoped-host-alias-managed");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        publish_alias_route(&checkout, &alias, 55432);
        write_managed_session_manifest(&checkout);

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

/// The same session refuses before its workload starts when this checkout has
/// no published backing service for the alias it would deliver.
#[test]
fn a_managed_session_refuses_without_a_published_backing_service() {
    with_short_scratch("scoped-host-alias-managed-absent", || {
        let primary = primary_checkout("scoped-host-alias-managed-absent");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        write_managed_session_manifest(&checkout);

        let error = run_task(&checkout, "dev", &["--headless"])
            .expect_err("the session refuses before its workload starts");
        assert_task_invocation_error_contains(
            error,
            &["postgres.acme.test", &alias, "effigy container up web"],
        );
        assert!(
            !checkout.join("session-database-url.txt").exists(),
            "the refused session never ran its workload"
        );
    });
}

/// A managed session needs a `concurrent` entry; this one renders a host task
/// through a task reference, which is the shape a real session uses for a host
/// workload.
fn write_managed_session_manifest(checkout: &Path) {
    let manifest = format!(
        "{MANIFEST}\n\
         [tasks.record-db-url]\n\
         run_in = \"host\"\n\
         run = \"sh -lc 'printf \\\"%s\\\\n\\\" \\\"$DATABASE_URL\\\" > \\\"$OUT\\\"'\"\n\
         env = {{ DATABASE_URL = \"postgres://app:secret@postgres.acme.test:5432/app\", OUT = \"{{repo}}/session-database-url.txt\" }}\n\
         \n\
         [tasks.dev]\n\
         mode = \"tui\"\n\
         concurrent = [\n\
           {{ name = \"record\", task = \"record-db-url\", run_in = \"host\", start = 1 }},\n\
         ]\n"
    );
    fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with a managed session");
}

/// A URL that only carries the declared name in its credentials, or that names
/// an unrelated server, is neither rewritten nor proved: it keeps its bytes and
/// reaches the server it names, with no route published at all.
#[test]
fn a_task_targeting_an_unrelated_server_keeps_its_url() {
    with_short_scratch("scoped-host-alias-credentials", || {
        let primary = primary_checkout("scoped-host-alias-credentials");
        let checkout = linked_worktree(&primary, "worktree-one");
        let out = checkout.join("credentials.txt");
        let credential_url =
            "postgres://postgres.acme.test:secret@database.elsewhere.test:5432/app";
        let password_url = "postgres://user:postgres.acme.test@database.elsewhere.test:5432/app";
        let username_url = "postgres://postgres.acme.test@database.elsewhere.test:5432/app";
        let manifest = format!(
            r#"{MANIFEST}
[tasks.elsewhere]
run_in = "host"
run = "sh -lc 'echo $CREDENTIAL_URL > {out}; echo $PASSWORD_URL >> {out}; echo $USERNAME_URL >> {out}'"
env = {{ CREDENTIAL_URL = "{credential_url}", PASSWORD_URL = "{password_url}", USERNAME_URL = "{username_url}" }}
"#,
            out = out.display(),
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with an unrelated task");

        run_task(&checkout, "elsewhere", &[]).expect("the task runs without a published alias");
        assert_eq!(
            fs::read_to_string(&out)
                .expect("the task recorded the URLs it received")
                .trim_end(),
            format!("{credential_url}\n{password_url}\n{username_url}"),
            "a URL whose credentials contain the declared name keeps its exact bytes"
        );
    });
}

/// A configured test suite is a host environment too: its own `env`, its
/// rendered command and its `setup`/`teardown` steps receive this checkout's
/// effective alias, and an unpublished one refuses before any suite starts.
/// A suite this run does not select is neither proved nor run.
#[test]
fn a_configured_test_suite_resolves_and_proves_its_scoped_alias() {
    with_short_scratch("scoped-host-alias-suite", || {
        let primary = primary_checkout("scoped-host-alias-suite");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        let suite = checkout.join("suite-database-url.txt");
        let setup = checkout.join("setup-database-url.txt");
        let teardown = checkout.join("teardown-database-url.txt");
        let unselected = checkout.join("unselected-database-url.txt");
        let manifest = format!(
            r#"{MANIFEST}
[test.suites.unit]
run = "sh -lc 'printf %s $DATABASE_URL > {suite}'"
env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" }}
setup = [
  {{ id = "seed", run = "sh -lc 'printf %s $DATABASE_URL > {setup}'", env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" }} }},
]
teardown = [
  {{ id = "clear", run = "sh -lc 'printf %s $DATABASE_URL > {teardown}'", env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" }} }},
]

[test.suites.unselected]
run = "sh -lc 'printf %s $DATABASE_URL > {unselected}'"
env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" }}
"#,
            suite = suite.display(),
            setup = setup.display(),
            teardown = teardown.display(),
            unselected = unselected.display(),
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with test suites");

        let error = run_task(&checkout, "test", &["unit"]).expect_err("the suite must refuse");
        assert_task_invocation_error_contains(
            error,
            &[
                "test.suites.unit",
                "postgres.acme.test",
                &alias,
                "effigy container up web",
            ],
        );
        for path in [&suite, &setup, &teardown, &unselected] {
            assert!(
                !path.exists(),
                "the refused suite never ran {}",
                path.display()
            );
        }

        publish_alias_route(&checkout, &alias, 55432);
        run_task(&checkout, "test", &["unit"]).expect("the suite runs");
        let expected = format!("postgres://app:secret@{alias}:5432/app");
        for (name, path) in [
            ("suite", &suite),
            ("setup", &setup),
            ("teardown", &teardown),
        ] {
            assert_eq!(
                fs::read_to_string(path)
                    .unwrap_or_else(|_| panic!("the {name} recorded the URL it received"))
                    .trim(),
                expected,
                "the {name} environment received this checkout's own alias"
            );
        }
        assert!(
            !unselected.exists(),
            "a suite this run did not select neither ran nor was proved"
        );
    });
}

/// The proof follows the delivered endpoints, not the text around them: a
/// suite that merely mentions the effective alias in prose runs without a
/// published route, and a suite this run did not select is never proved.
#[test]
fn a_suite_alias_in_prose_or_in_unselected_suite_is_not_proved() {
    with_short_scratch("scoped-host-alias-suite-prose", || {
        let primary = primary_checkout("scoped-host-alias-suite-prose");
        let checkout = linked_worktree(&primary, "worktree-one");
        let alias = effective_alias(&checkout, "postgres");
        let prose = checkout.join("prose.txt");
        let unselected = checkout.join("unselected-database-url.txt");
        let manifest = format!(
            r#"{MANIFEST}
[test.suites.prose]
run = "sh -lc 'printf %s searching-{alias}-now > {prose}'"

[test.suites.other]
run = "sh -lc 'printf %s $DATABASE_URL > {unselected}'"
env = {{ DATABASE_URL = "postgres://app:secret@postgres.acme.test:5432/app" }}
"#,
            alias = alias,
            prose = prose.display(),
            unselected = unselected.display(),
        );
        fs::write(checkout.join("effigy.toml"), manifest).expect("manifest with prose suite");

        run_task(&checkout, "test", &["prose"]).expect("a prose mention refuses nothing");
        assert_eq!(
            fs::read_to_string(&prose)
                .expect("the prose suite ran")
                .trim(),
            format!("searching-{alias}-now"),
            "text that merely names the alias keeps its bytes and proves nothing"
        );
        assert!(
            !unselected.exists(),
            "the unselected suite was neither proved nor run"
        );
    });
}

/// The command Effigy renders for one task, without dispatching anything./// The command Effigy renders for one task, without dispatching anything.
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
    with_short_scratch("scoped-host-alias-container", || {
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
