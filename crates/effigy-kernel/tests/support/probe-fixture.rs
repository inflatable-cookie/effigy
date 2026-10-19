//! Socket-only workload telemetry lets the O10 oracle snapshot every fixture
//! file. No probe (or its guard) writes a pid file, marker, or test log.
use effigy_kernel::{
    ctx::*, custody::EnvValue, ident, observe::Observed, probe::*, supervise::Executable,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
fn op(cancel: CancelToken, class: EffectClass, budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        cancel,
        Limits {
            capture_tail: 4096,
            line_max: 4096,
            log_rotate: 0,
            log_keep: 0,
            queue_bytes: 4096,
            probe_concurrency: 2,
        },
        class,
        Placement::Host,
    )
}
fn announce(address: &OsString) {
    let mut socket = TcpStream::connect(address.to_str().unwrap()).unwrap();
    writeln!(
        socket,
        "{}",
        serde_json::to_string(
            &ident::process_identity(std::process::id())
                .verified()
                .unwrap()
        )
        .unwrap()
    )
    .unwrap();
}
pub fn run(entry: &str, args: &[OsString], cancel: CancelToken) {
    if entry == "probe-workload" {
        let mode = args[1].to_str().unwrap();
        let address = &args[2];
        announce(address);
        if mode != "hang" {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    OsString::from("probe-descendant"),
                    mode.into(),
                    address.clone(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if mode.contains("clear") {
                command.env_clear();
            }
            if mode.starts_with("escape") {
                // SAFETY: setsid only, no allocation or Rust locks after fork.
                unsafe {
                    command.pre_exec(|| {
                        if libc::setsid() < 0 {
                            Err(std::io::Error::last_os_error())
                        } else {
                            Ok(())
                        }
                    });
                }
            }
            #[expect(
                clippy::zombie_processes,
                reason = "The proof requires a descendant that outlives its leader; the harness owns its exact pid."
            )]
            let child = command.spawn().unwrap();
            let _ = child.id();
            if mode == "descendant-live" || mode == "escape" {
                std::thread::sleep(Duration::from_secs(30));
            }
            // Keep the leader alive until the descendant connected to telemetry.
            // The harness controls timing by waiting for both announcements.
            std::thread::sleep(Duration::from_millis(150));
            return;
        }
        std::thread::sleep(Duration::from_secs(30));
        return;
    }
    if entry == "probe-descendant" {
        announce(&args[2]);
        std::thread::sleep(Duration::from_secs(30));
        return;
    }
    if entry == "probe-maintain" {
        let result = maintain(
            &op(cancel, EffectClass::Mutate, Duration::from_secs(5))
                .effects()
                .unwrap(),
        )
        .unwrap();
        println!(
            "{}",
            json!({"signals": result.signals.len(), "uncertain": result.uncertain.len()})
        );
        return;
    }
    let home = Path::new(&args[1]);
    let runtime = ProbeRuntime::install(Executable::current().unwrap()).unwrap();
    let cx = op(cancel, EffectClass::Observe, Duration::from_secs(10))
        .observe()
        .unwrap()
        .with_probes(runtime.clone());
    let warmup = ProbeCommand {
        argv: vec!["/usr/bin/true".into()],
        cwd: home.into(),
        env: BTreeMap::new(),
        deadline: Duration::from_secs(2),
        max_output: 4096,
    };
    assert!(matches!(cx.probe_process(warmup), Observed::Verified(..)));
    println!(
        "{}",
        json!({"observer": ident::process_identity(std::process::id()).verified().unwrap(), "guard": runtime.guard_identity(&cx).verified().unwrap()})
    );
    std::io::stdout().flush().unwrap();
    // Harness may kill or stop the guard before allowing registration.
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    let mode = args[2].to_str().unwrap();
    let mut env = BTreeMap::new();
    env.insert(
        "HOME".into(),
        EnvValue::Plain(if mode == "git" {
            home.parent().unwrap().as_os_str().into()
        } else {
            home.as_os_str().into()
        }),
    );
    let argv = match mode {
        "git" => vec![
            "/usr/bin/git".into(),
            "--no-optional-locks".into(),
            "status".into(),
            "--porcelain".into(),
        ],
        "missing" => vec![home.join("missing-tool").into_os_string()],
        "capture" => {
            env.insert(
                "SECRET".into(),
                EnvValue::Secret(
                    effigy_kernel::custody::SecretValue::new(b"probe-secret".to_vec()).unwrap(),
                ),
            );
            vec!["/bin/sh".into(), "-c".into(), "printf '%s' \"$SECRET\"; printf '-ok'; i=0; while [ $i -lt 1000 ]; do printf x; i=$((i + 1)); done".into()]
        }
        _ => vec![
            std::env::current_exe().unwrap().into_os_string(),
            "probe-workload".into(),
            mode.into(),
            args[3].clone(),
        ],
    };
    let output = cx.probe_process(ProbeCommand {
        argv,
        cwd: home.into(),
        env,
        deadline: Duration::from_millis(1200),
        max_output: if mode == "capture" { 64 } else { 4096 },
    });
    match output {
        Observed::Verified(value, _) => println!(
            "{}",
            json!({"verified": true, "exit": value.exit.code, "output": String::from_utf8_lossy(&value.output), "truncated": value.truncated, "warnings": value.warnings})
        ),
        Observed::Unknown(reason) => println!("{}", json!({"unknown": reason})),
        other => panic!("unexpected observation: {other:?}"),
    }
}
