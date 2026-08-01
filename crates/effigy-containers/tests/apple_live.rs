use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_catalog::stack_plan::{
    EffectiveStackPlan, StackBuildPlan, StackCommandPlan, StackDependencyPlan, StackMountKind,
    StackMountPlan, StackPortPlan, StackReadinessPlan, StackResourcePlan, StackServicePlan,
};
use effigy_containers::apple::{AppleError, AppleStackExecutor};

const PROJECT: &str = "effigy-apple-live-probe";

fn dependency(service: &str) -> StackDependencyPlan {
    StackDependencyPlan {
        service: service.to_owned(),
        condition: "service-started".to_owned(),
    }
}

fn service(name: &str, image: &str) -> StackServicePlan {
    StackServicePlan {
        name: name.to_owned(),
        image: Some(image.to_owned()),
        build: None,
        command: None,
        environment: BTreeMap::new(),
        user: None,
        working_dir: None,
        mounts: Vec::new(),
        tmpfs: Vec::new(),
        ports: Vec::new(),
        dependencies: Vec::new(),
        readiness: None,
        resources: StackResourcePlan {
            memory: Some("512M".to_owned()),
            cpus: Some("2".to_owned()),
        },
    }
}

fn live_stack(root: &Path) -> EffectiveStackPlan {
    let mut db = service("db", "postgres:17-alpine");
    db.environment = BTreeMap::from([
        (
            "PGDATA".to_owned(),
            "/var/lib/postgresql/data/pgdata".to_owned(),
        ),
        ("POSTGRES_DB".to_owned(), "effigy".to_owned()),
        ("POSTGRES_PASSWORD".to_owned(), "effigy".to_owned()),
    ]);
    db.mounts.push(StackMountPlan {
        kind: StackMountKind::Volume,
        source: Some(format!("{PROJECT}-db-data")),
        target: "/var/lib/postgresql/data".to_owned(),
        read_only: false,
        options: Vec::new(),
    });
    db.readiness = Some(StackReadinessPlan {
        command: StackCommandPlan::Exec(vec![
            "CMD-SHELL".to_owned(),
            "pg_isready -U postgres".to_owned(),
        ]),
        interval: Some("1s".to_owned()),
        timeout: Some("3s".to_owned()),
        retries: Some(30),
        start_period: Some("1s".to_owned()),
    });

    let mut cache = service("cache", "redis:7-alpine");
    cache.readiness = Some(StackReadinessPlan {
        command: StackCommandPlan::Exec(vec![
            "CMD".to_owned(),
            "redis-cli".to_owned(),
            "ping".to_owned(),
        ]),
        interval: Some("1s".to_owned()),
        timeout: Some("3s".to_owned()),
        retries: Some(30),
        start_period: Some("1s".to_owned()),
    });

    let mut app = service("app", "");
    app.image = None;
    app.build = Some(StackBuildPlan {
        context: root.display().to_string(),
        dockerfile: Some(root.join("Dockerfile").display().to_string()),
        args: BTreeMap::new(),
        target: None,
    });
    app.working_dir = Some("/workspace".to_owned());
    app.mounts.push(StackMountPlan {
        kind: StackMountKind::Bind,
        source: Some(root.display().to_string()),
        target: "/workspace".to_owned(),
        read_only: true,
        options: vec!["ro".to_owned()],
    });
    app.dependencies = vec![dependency("db"), dependency("cache")];
    app.readiness = Some(StackReadinessPlan {
        command: StackCommandPlan::Exec(vec!["CMD".to_owned(), "true".to_owned()]),
        interval: Some("1s".to_owned()),
        timeout: Some("3s".to_owned()),
        retries: Some(10),
        start_period: None,
    });

    let mut web = service("web", "nginx:alpine");
    web.ports.push(StackPortPlan {
        host_ip: Some("127.0.0.1".to_owned()),
        host_port: Some(18088),
        container_port: 80,
        protocol: "tcp".to_owned(),
    });
    web.dependencies.push(dependency("app"));
    web.readiness = Some(StackReadinessPlan {
        command: StackCommandPlan::Exec(vec![
            "CMD-SHELL".to_owned(),
            "wget -qO- http://127.0.0.1/ >/dev/null".to_owned(),
        ]),
        interval: Some("1s".to_owned()),
        timeout: Some("3s".to_owned()),
        retries: Some(30),
        start_period: Some("1s".to_owned()),
    });

    EffectiveStackPlan {
        project_name: PROJECT.to_owned(),
        network_name: format!("{PROJECT}-default"),
        services: [
            ("app".to_owned(), app),
            ("web".to_owned(), web),
            ("db".to_owned(), db),
            ("cache".to_owned(), cache),
        ]
        .into_iter()
        .collect(),
    }
}

fn require(condition: bool, reason: impl Into<String>) -> Result<(), AppleError> {
    condition
        .then_some(())
        .ok_or_else(|| AppleError::InvalidPlan {
            reason: reason.into(),
        })
}

fn wait_for_host_http(url: &str) -> Result<(), AppleError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let output = std::process::Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--max-time", "2", url])
            .output()?;
        if output.status.success() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let last_error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(AppleError::InvalidPlan {
                reason: format!(
                    "published port did not become reachable ({last_error}); verify Local Network access is enabled for container-runtime-linux"
                ),
            });
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
#[ignore = "requires Apple Containers 1.2 on Apple silicon and mutates prototype resources"]
fn apple_native_four_service_lifecycle() {
    let root = tempfile::tempdir().expect("temp root");
    std::fs::write(
        root.path().join("Dockerfile"),
        "FROM alpine:3.22\nCMD [\"sleep\", \"300\"]\n",
    )
    .expect("Dockerfile");
    std::fs::write(root.path().join("proof.txt"), "bind-mounted\n").expect("proof file");
    let stack = live_stack(root.path());
    let executor = AppleStackExecutor::default();
    let _ = executor.stop(&stack, true);

    let result = (|| {
        let first_start = Instant::now();
        let report = executor.start(&stack, root.path())?;
        eprintln!(
            "apple_first_start_seconds={:.2}",
            first_start.elapsed().as_secs_f64()
        );
        require(
            report.container_ids.len() == 4,
            "expected four container IDs",
        )?;
        require(
            report.ipv4_addresses.len() == 4,
            "expected four IPv4 addresses",
        )?;

        let discovery = executor.exec(
            &stack,
            "app",
            &[
                "sh",
                "-lc",
                "getent hosts db cache web && nc -z db 5432 && nc -z cache 6379 && wget -qO- http://web/ >/dev/null && grep -q bind-mounted /workspace/proof.txt",
            ],
        )?;
        require(discovery.status.success(), "service discovery probe failed")?;

        wait_for_host_http("http://127.0.0.1:18088/")?;
        executor.logs(&stack, "web", 20)?;
        let stats = std::process::Command::new("container")
            .arg("stats")
            .args(report.container_ids.values())
            .args(["--no-stream", "--format", "json"])
            .output()?;
        require(stats.status.success(), "Apple stats snapshot failed")?;
        eprintln!(
            "apple_stats={}",
            String::from_utf8_lossy(&stats.stdout).trim()
        );

        let io_start = Instant::now();
        executor.exec(
            &stack,
            "db",
            &[
                "sh",
                "-lc",
                "dd if=/dev/zero of=\"$PGDATA/effigy-io-probe\" bs=1M count=64 conv=fsync >/dev/null 2>&1; rm -f \"$PGDATA/effigy-io-probe\"",
            ],
        )?;
        eprintln!("apple_io_seconds={:.2}", io_start.elapsed().as_secs_f64());
        executor.exec(
            &stack,
            "db",
            &[
                "psql",
                "-U",
                "postgres",
                "-d",
                "effigy",
                "-c",
                "CREATE TABLE lifecycle_probe(value text); INSERT INTO lifecycle_probe VALUES ('persisted');",
            ],
        )?;

        executor.stop(&stack, false)?;
        let second_start = Instant::now();
        executor.start(&stack, root.path())?;
        eprintln!(
            "apple_second_start_seconds={:.2}",
            second_start.elapsed().as_secs_f64()
        );
        let persisted = executor.exec(
            &stack,
            "db",
            &[
                "psql",
                "-U",
                "postgres",
                "-d",
                "effigy",
                "-tAc",
                "SELECT value FROM lifecycle_probe LIMIT 1",
            ],
        )?;
        require(
            String::from_utf8_lossy(&persisted.stdout).trim() == "persisted",
            "named volume did not survive stop/start",
        )?;

        let stale_delete = std::process::Command::new("container")
            .args(["delete", "--force", "effigy-apple-live-probe-web"])
            .output()?;
        require(
            stale_delete.status.success(),
            "failed to create interrupted-stack fixture",
        )?;
        let recovery_start = Instant::now();
        executor.start(&stack, root.path())?;
        wait_for_host_http("http://127.0.0.1:18088/")?;
        eprintln!(
            "apple_interrupted_recovery_seconds={:.2}",
            recovery_start.elapsed().as_secs_f64()
        );

        let system_stop = std::process::Command::new("container")
            .args(["system", "stop"])
            .output()?;
        require(system_stop.status.success(), "Apple system stop failed")?;
        let system_start = std::process::Command::new("container")
            .args(["system", "start"])
            .output()?;
        require(system_start.status.success(), "Apple system start failed")?;
        let runtime_recovery = Instant::now();
        executor.start(&stack, root.path())?;
        wait_for_host_http("http://127.0.0.1:18088/")?;
        let persisted_after_runtime_restart = executor.exec(
            &stack,
            "db",
            &[
                "psql",
                "-U",
                "postgres",
                "-d",
                "effigy",
                "-tAc",
                "SELECT value FROM lifecycle_probe LIMIT 1",
            ],
        )?;
        require(
            String::from_utf8_lossy(&persisted_after_runtime_restart.stdout).trim() == "persisted",
            "named volume did not survive Apple runtime restart",
        )?;
        eprintln!(
            "apple_runtime_restart_recovery_seconds={:.2}",
            runtime_recovery.elapsed().as_secs_f64()
        );
        Ok::<(), effigy_containers::apple::AppleError>(())
    })();
    let cleanup = executor.stop(&stack, true);

    result.expect("live Apple stack proof");
    cleanup.expect("live Apple stack cleanup");
}
