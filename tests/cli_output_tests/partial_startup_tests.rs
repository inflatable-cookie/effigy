//! Private production CLI fixtures. No real engine, VM or gateway daemon.
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

struct Stack {
    root: PathBuf,
    process: Process,
    session: BufReader<TcpStream>,
    port: u16,
}
struct Fixture {
    root: tempfile::TempDir,
    stacks: Vec<Stack>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stacks.clear();
    }
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("effigy-158-private-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates"))
            .expect("fresh private root");
        fs::create_dir(root.path().join("bin")).expect("bin");
        fs::create_dir(root.path().join("home")).expect("home");
        for name in ["docker", "colima"] {
            let path = root.path().join("bin").join(name);
            let python = Command::new("python3")
                .args(["-c", "import sys;print(sys.executable)"])
                .output()
                .expect("Python interpreter");
            assert!(python.status.success());
            let script = format!(
                "#!{}\n{}",
                String::from_utf8_lossy(&python.stdout).trim(),
                BACKEND
            );
            fs::write(&path, script).expect("exclusive adapter");
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .expect("adapter executable");
        }
        fs::write(root.path().join("bin/lsof"), "#!/bin/sh\nexit 1\n")
            .expect("private host-listener transport");
        fs::set_permissions(
            root.path().join("bin/lsof"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("mode");
        let mut fixture = Self {
            root,
            stacks: Vec::new(),
        };
        for i in 0..3 {
            fixture.add_stack(i);
        }
        fixture
    }
    fn add_stack(&mut self, i: usize) {
        let root = self.root.path().join(format!("stack{i}"));
        fs::create_dir(&root).expect("stack root");
        let ready = root.join("ready");
        let process = Process(
            Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    "--ignored",
                    "--exact",
                    "partial_startup_tests::private_service_child",
                    "--nocapture",
                ])
                .env("PRIVATE_SERVICE_READY", &ready)
                .env("PRIVATE_SERVICE_ID", format!("stack{i}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("own service/exec fixture"),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "private service startup");
            std::thread::sleep(Duration::from_millis(10));
        }
        let port = fs::read_to_string(&ready)
            .expect("actual binding")
            .trim()
            .parse()
            .expect("port");
        let session = BufReader::new(
            TcpStream::connect(("127.0.0.1", port)).expect("persistent exec sentinel connection"),
        );
        session
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("bounded sentinel read");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"[containers]
default = "web"
[containers.web]
primary_service = "db"
working_dir = "/workspace"
profile = "private-{i}"
[containers.web.host]
ports = ["{port}:5432"]
[containers.web.dns]
routes = [{{ domain = "stack{i}.fixture.test", tls = false, service = "db", port = {port} }}]
[containers.web.services.db]
catalog = "postgres"
version = "16"
"#
            ),
        )
        .expect("manifest");
        fs::write(root.join("state.json"),json!({"active":false,"identity":format!("stack{i}"),"id":format!("{:064x}",i+1),"port":port,"mode":"ready","volume":true,"checkout":root}).to_string()).expect("transport state");
        fs::write(root.join("data"), format!("private-data-{i}")).expect("data marker");
        fs::write(root.join("lease-sentinel"), format!("private-lease-{i}"))
            .expect("lease sibling marker");
        for args in [
            vec!["init", "--initial-branch=main"],
            vec!["config", "effigy.runtimeScope", "ephemeral"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .expect("private Git namespace");
            assert!(output.status.success(), "{output:?}");
        }
        self.stacks.push(Stack {
            root,
            process,
            session,
            port,
        });
    }
    fn command(&self, i: usize, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_effigy"));
        c.args(args)
            .current_dir(&self.stacks[i].root)
            .env("HOME", self.root.path().join("home"))
            .env(
                "EFFIGY_TEST_CONTAINERS_HOME",
                self.root.path().join("home/.effigy"),
            )
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("COLIMA_HOME", self.root.path().join("home/colima"))
            .env("DOCKER_CONFIG", self.root.path().join("home/docker"))
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .env("EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER", "1")
            .env("PRIVATE_BACKEND_ROOT", self.root.path())
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.root.path().join("bin").display(),
                    std::env::var("PATH").expect("PATH")
                ),
            )
            .env_remove("EFFIGY_GATEWAY_PRIVATE_STATE_ROOT")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }
    fn run(&self, i: usize, args: &[&str]) -> Output {
        let output = self
            .command(i, args)
            .output()
            .expect("production CLI and positive reap");
        if !self.stacks[i].root.join("commands.jsonl").exists() {
            let human = self
                .command(i, &["container", "up", "--detach"])
                .output()
                .expect("fixture refusal diagnosis");
            panic!("fixture was refused before the private transport: original={output:?}; human={human:?}");
        }
        output
    }
    fn state(&self, i: usize) -> Value {
        serde_json::from_slice(&fs::read(self.stacks[i].root.join("state.json")).expect("state"))
            .expect("JSON")
    }
    fn set(&self, i: usize, key: &str, value: Value) {
        let mut s = self.state(i);
        s[key] = value;
        fs::write(self.stacks[i].root.join("state.json"), s.to_string()).expect("private control");
    }
    fn routes(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.root.path().join("home/.effigy/gateway/routes.json"))
                .expect("private routes"),
        )
        .expect("JSON")
    }
    fn probe(&mut self, i: usize) {
        writeln!(self.stacks[i].session.get_mut(), "still-running").expect("existing exec stream");
        let mut line = String::new();
        self.stacks[i]
            .session
            .read_line(&mut line)
            .expect("live exec ack");
        assert_eq!(line.trim(), format!("stack{i}:still-running"));
        assert!(self.stacks[i]
            .process
            .0
            .try_wait()
            .expect("owned service status")
            .is_none());
    }
    fn assert_no_recreation(&self, i: usize) {
        let log = fs::read_to_string(self.stacks[i].root.join("commands.jsonl"))
            .expect("actual commands");
        let args: Vec<Vec<String>> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("argv receipt"))
            .collect();
        assert_eq!(
            args.iter()
                .filter(|args| args.iter().any(|arg| arg == "up"))
                .count(),
            1,
            "{log}"
        );
        assert!(
            !args.iter().any(|args| args
                .iter()
                .any(|arg| matches!(arg.as_str(), "restart" | "down" | "rm" | "prune"))),
            "{log}"
        );
    }
}

#[test]
fn private_three_scopes_partial_resume_replacement_and_cli_refusals() {
    let mut fixture = Fixture::new();
    // Services and persistent exec connections belong to the test, not to any
    // CLI/inventory process group. All three stacks overlap in time.
    for i in 0..3 {
        fixture.probe(i);
    }
    fixture.set(0, "mode", json!("hang-after-up"));
    let mut launches = Vec::new();
    for i in 0..3 {
        launches.push(Process(
            fixture
                .command(i, &["container", "up", "--detach", "--json"])
                .spawn()
                .expect("concurrent scoped up"),
        ));
    }
    let outputs: Vec<Output> = launches
        .into_iter()
        .map(|mut process| {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            process
                .0
                .stdout
                .take()
                .expect("stdout")
                .read_to_end(&mut stdout)
                .expect("bounded fixture output");
            process
                .0
                .stderr
                .take()
                .expect("stderr")
                .read_to_end(&mut stderr)
                .expect("fixture stderr");
            let status = process.0.wait().expect("reap activation CLI");
            Output {
                status,
                stdout,
                stderr,
            }
        })
        .collect();
    assert!(
        !outputs[0].status.success(),
        "hung inventory cannot be ready"
    );
    let failed: Value =
        serde_json::from_slice(&outputs[0].stdout).expect("one JSON failure envelope");
    assert_eq!(failed["error"]["details"]["state"], "partial", "{failed}");
    assert_eq!(failed["error"]["details"]["readiness"], "unverified");
    for output in &outputs[1..] {
        assert!(output.status.success(), "{output:?}");
    }
    let lease_root = fixture
        .root
        .path()
        .join("home/.effigy/runtime/host-container-leases");
    let mut lease_receipts = Vec::new();
    for i in 0..3 {
        let project = fixture.state(i)["project"].as_str().unwrap().to_owned();
        let lease = lease_root
            .join(format!("private-{i}"))
            .join(&project)
            .join("web.json");
        fs::create_dir_all(lease.parent().unwrap()).unwrap();
        let receipt = json!({"schema":"effigy.host-container-lease.v1","schema_version":1,"token":format!("private-{i}"),"container_name":"web","profile":format!("private-{i}"),"project_name":project,"expires_at_epoch_ms":9999999999999u64}).to_string();
        fs::write(&lease, &receipt).unwrap();
        lease_receipts.push((lease, receipt));
    }
    let mut table = fixture.routes();
    let template = table["routes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    for (name, source) in [
        ("foreign.fixture.test", "manual"),
        ("shared.fixture.test", "container"),
    ] {
        let owner = fixture.root.path().join(format!("{name}-owner"));
        fs::create_dir(&owner).unwrap();
        let mut route = template.clone();
        route["domain"] = json!(name);
        route["project"] = json!(owner);
        route["scope"] = Value::Null;
        route["source"] = json!(source);
        route["target"] = Value::Null;
        route["dns_ip"] = Value::Null;
        route["tcp_port"] = Value::Null;
        route["tcp_target"] = Value::Null;
        table["routes"][name] = route;
    }
    fs::write(
        fixture.root.path().join("home/.effigy/gateway/routes.json"),
        table.to_string(),
    )
    .unwrap();
    let mut siblings = fixture.routes();
    // Route::scope serializes None as absent, including a loaded JSON null.
    // Canonicalize only that field; all ownership and foreign fields stay exact.
    for route in siblings["routes"]
        .as_object_mut()
        .expect("route table")
        .values_mut()
    {
        if route.get("scope") == Some(&Value::Null) {
            route.as_object_mut().expect("route object").remove("scope");
        }
    }
    let before_ids: Vec<Value> = (0..3).map(|i| fixture.state(i)["id"].clone()).collect();
    for i in 0..3 {
        fixture.probe(i);
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("data")).unwrap(),
            format!("private-data-{i}")
        );
    }
    fixture.set(0, "mode", json!("ready"));
    let resumed = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(resumed.status.success(), "{resumed:?}");
    fixture.probe(0);
    assert!(
        !lease_receipts[0].0.exists(),
        "selected temporary lease transfers to detached owner"
    );
    for (lease, receipt) in &lease_receipts[1..] {
        assert_eq!(fs::read_to_string(lease).unwrap(), *receipt);
    }
    let routes = fixture.routes();
    for route in siblings["routes"]
        .as_object()
        .into_iter()
        .flat_map(|routes| routes.iter())
    {
        assert_eq!(routes["routes"][route.0], *route.1, "sibling route changed");
    }
    // Warm detached activation also reconciles without a second compose up.
    let warm = fixture.run(0, &["container", "up", "--detach"]);
    assert!(warm.status.success(), "{warm:?}");
    fixture.assert_no_recreation(0);
    for (i, before_id) in before_ids.iter().enumerate().skip(1) {
        fixture.assert_no_recreation(i);
        assert_eq!(fixture.state(i)["id"], *before_id);
        fixture.probe(i);
    }
    // Same declared backing service, new full ID and actual private binding.
    let ready = fixture.root.path().join("replacement-ready");
    let mut replacement = Process(
        Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--ignored",
                "--exact",
                "partial_startup_tests::private_service_child",
                "--nocapture",
            ])
            .env("PRIVATE_SERVICE_READY", &ready)
            .env("PRIVATE_SERVICE_ID", "replacement")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("own replacement service"),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let new_port: u16 = fs::read_to_string(ready).unwrap().parse().unwrap();
    fixture.set(
        0,
        "id",
        json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
    );
    fixture.set(0, "port", json!(new_port));
    fixture.set(0, "identity", json!("replacement"));
    let replaced = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(replaced.status.success(), "{replaced:?}");
    let replacement_routes = fixture.routes().to_string();
    assert!(
        replacement_routes.contains(&format!("127.0.0.1:{new_port}")),
        "{replacement_routes}"
    );
    assert!(
        !replacement_routes.contains(&format!("127.0.0.1:{}", fixture.stacks[0].port)),
        "old own target retained"
    );
    fixture.assert_no_recreation(0);
    fixture.probe(0);
    let after_replacement = fixture.routes();
    for (domain, route) in siblings["routes"].as_object().unwrap() {
        assert_eq!(after_replacement["routes"][domain], *route);
    }
    // Fresh inspect rejection matrix: no route or service mutation is credited.
    for mode in [
        "foreign", "oneoff", "stopped", "unknown", "drift", "shared", "orphan",
    ] {
        fixture.set(0, "mode", json!(mode));
        let output = fixture.run(0, &["container", "up", "--detach", "--json"]);
        assert!(!output.status.success(), "{mode}: {output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).expect("refusal JSON");
        assert_eq!(result["error"]["details"]["readiness"], "unverified");
        assert_eq!(fixture.routes().to_string(), replacement_routes);
        fixture.probe(0);
    }
    fixture.set(0, "mode", json!("ready"));
    // Establish another partial marker, then change admitted source bytes.
    fixture.set(0, "mode", json!("unknown"));
    assert!(!fixture
        .run(0, &["container", "up", "--detach"])
        .status
        .success());
    fixture.set(0, "mode", json!("ready"));
    // Warm validation rejected before a marker was written: first obtain an
    // admitted marker via a failed exec probe instead.
    fixture.set(0, "mode", json!("exec-fail"));
    let partial = fixture.run(0, &["container", "up", "--detach"]);
    assert!(!partial.status.success());
    assert!(
        String::from_utf8_lossy(&partial.stderr).contains("partial"),
        "{partial:?}"
    );
    let manifest = fixture.stacks[0].root.join("effigy.toml");
    let admitted = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{admitted}\n# drift\n")).unwrap();
    fixture.set(0, "mode", json!("ready"));
    let drift = fixture.run(0, &["container", "up", "--detach", "--json"]);
    assert!(!drift.status.success());
    assert_eq!(fixture.routes().to_string(), replacement_routes);
    fixture.assert_no_recreation(0);
    fs::write(manifest, admitted).unwrap();
    // Noninteractive retire refuses unknown volume ownership and succeeds only
    // for this selected generated scope; actual mutable fixture data is removed.
    fixture.set(0, "mode", json!("volume-unknown"));
    let refused = fixture.run(0, &["container", "retire", "--yes", "--json"]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(fixture.stacks[0].root.join("data").exists());
    fixture.set(0, "mode", json!("ready"));
    let retired = fixture.run(0, &["container", "retire", "--yes", "--json"]);
    assert!(retired.status.success(), "{retired:?}");
    assert!(!fixture.stacks[0].root.join("data").exists());
    let after_retirement = fixture.routes();
    for (domain, route) in siblings["routes"].as_object().unwrap() {
        assert_eq!(after_retirement["routes"][domain], *route);
    }

    assert!(replacement
        .0
        .wait()
        .expect("positive selected replacement reap after retirement")
        .success());
    for (i, before_id) in before_ids.iter().enumerate().skip(1) {
        fixture.probe(i);
        assert_eq!(fixture.state(i)["id"], *before_id);
        assert!(fixture.stacks[i].root.join("data").exists());
        assert_eq!(
            fs::read_to_string(fixture.stacks[i].root.join("lease-sentinel")).unwrap(),
            format!("private-lease-{i}")
        );
    }
    for i in 0..3 {
        println!(
            "scope {i} actual backend receipt:\n{}",
            fs::read_to_string(fixture.stacks[i].root.join("commands.jsonl")).unwrap()
        );
    }
    println!(
        "CLI partial={} resume={} replacement={} retire-refusal={} retire-success={}",
        String::from_utf8_lossy(&outputs[0].stdout),
        String::from_utf8_lossy(&resumed.stdout),
        String::from_utf8_lossy(&replaced.stdout),
        String::from_utf8_lossy(&refused.stdout),
        String::from_utf8_lossy(&retired.stdout)
    );
}

#[test]
fn private_startup_failure_and_interruption_withhold_ready_and_cleanup() {
    let mut fixture = Fixture::new();
    fixture.set(0, "mode", json!("up-fail"));
    fixture.set(1, "mode", json!("interrupt"));
    for i in 0..2 {
        let output = fixture.run(i, &["container", "up", "--detach", "--json"]);
        assert!(!output.status.success(), "{output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).expect("structured failure");
        assert_eq!(result["error"]["details"]["readiness"], "unverified");
        fixture.probe(i);
        fixture.assert_no_recreation(i);
        println!(
            "startup mode {i}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let healthy = fixture.run(2, &["container", "up", "--detach"]);
    assert!(
        healthy.status.success(),
        "healthy activation failed: {healthy:?}"
    );
    fixture.probe(2);
    let named = fixture.run(2, &["container", "web", "retire", "--yes", "--json"]);
    assert!(!named.status.success());
    assert!(fixture.stacks[2].root.join("data").exists());
}

#[test]
fn private_direct_scoped_compose_refuses_fixed_identity_before_effects() {
    let mut fixture = Fixture::new();
    let root = &fixture.stacks[2].root;
    let manifest = fs::read_to_string(root.join("effigy.toml")).unwrap();
    let direct = manifest
        .split("[containers.web.services.db]")
        .next()
        .unwrap()
        .replace(
            "primary_service = \"db\"",
            "compose_file = \"compose.yml\"\nprimary_service = \"db\"",
        );
    fs::write(root.join("effigy.toml"), direct).unwrap();
    fs::write(
        root.join("compose.yml"),
        "services:\n  db:\n    image: fixture.invalid/db\n    ports: [\"4000:5432\"]\n",
    )
    .unwrap();
    let output = fixture
        .command(2, &["container", "up", "--detach", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["error"]["details"]["state"], "unavailable");
    let human = fixture
        .command(2, &["container", "up", "--detach"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&human.stderr).contains("cannot isolate repo-owned compose"),
        "{human:?}"
    );
    assert!(!fixture.stacks[2].root.join("commands.jsonl").exists());
    fixture.probe(2);
    println!(
        "direct scoped identity refusal: {}",
        String::from_utf8_lossy(&human.stderr)
    );
}

#[test]
#[ignore = "exclusive private service child"]
fn private_service_child() {
    let ready = PathBuf::from(std::env::var_os("PRIVATE_SERVICE_READY").expect("ready path"));
    let id = std::env::var("PRIVATE_SERVICE_ID").expect("fixture identity");
    let listener = TcpListener::bind("127.0.0.1:0").expect("private listener");
    let pending = ready.with_extension("pending");
    fs::write(&pending, listener.local_addr().unwrap().port().to_string())
        .expect("binding receipt");
    fs::rename(pending, ready).expect("publish complete binding");
    for stream in listener.incoming() {
        let mut stream = stream.expect("private connection");
        let id = id.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.trim() == "retire" {
                    writeln!(stream, "{id}:retired").unwrap();
                    std::process::exit(0);
                }
                writeln!(stream, "{id}:{}", line.trim()).unwrap();
                line.clear();
            }
        });
    }
}

const BACKEND: &str = r#"import sys,os,json,pathlib,time,socket
root=pathlib.Path(os.environ['PRIVATE_BACKEND_ROOT']); cwd=pathlib.Path.cwd(); args=sys.argv[1:]
if pathlib.Path(sys.argv[0]).name=='colima':
 if args and args[0]=='list': pass
 elif args and args[0]=='status': print('Running')
 else: print('private inert colima transport')
 sys.exit(0)
with open(cwd/'commands.jsonl','a') as f: f.write(json.dumps(args)+'\n')
sfile=cwd/'state.json'; state=json.load(open(sfile)); mode=state['mode']
def save(): sfile.write_text(json.dumps(state))
def record(s):
 for p in (root/'home/.effigy/runtime-scopes').glob('*.json'):
  r=json.load(open(p))
  if r['checkout']==s['checkout']: return r
 return None
def selected(s):
 filters=[a.removeprefix('label=') for a in args if a.startswith('label=')]
 r=record(s)
 labels={'com.docker.compose.project':s.get('project',''),'com.effigy.project':s.get('project',''),'com.effigy.scope':r['token'] if r else ''}
 return all(labels.get(f.split('=',1)[0])==f.split('=',1)[1] for f in filters)
if args and args[0]=='compose':
 sub=next((a for a in args if a in ['up','exec','restart','down','ps']),None)
 if sub=='up':
  if state['active']: state['collateral_recreation']=True
  state['project']=args[args.index('-p')+1];state['active']=True;save()
  if mode=='up-fail': sys.exit(17)
  if mode=='interrupt':
   import signal
   os.kill(os.getppid(),signal.SIGTERM)
 elif sub=='exec':
  if mode=='exec-fail': sys.exit(18)
  with socket.create_connection(('127.0.0.1',state['port']),timeout=1) as c:
   c.sendall(b'production-exec\n');assert c.makefile('rb').readline()==(state['identity']+':production-exec\n').encode()
 elif sub in ['restart','down']: sys.exit(91)
 sys.exit(0)
if args and args[0]=='ps':
 if state['active'] and mode=='hang-after-up' and '--format' in args: time.sleep(120)
 for p in root.glob('stack*/state.json'):
  s=json.load(open(p))
  if not s['active'] or not selected(s): continue
  if '{{.Names}}' in args: print(s['project']+'-db-1')
  elif '--format' in args: print(s['project']+'-db-1\tUp 1 minute\t127.0.0.1:'+str(s['port'])+'->5432/tcp\t'+s['project']+'\t'+s['checkout']+'\t'+('undeclared' if s['mode']=='orphan' else 'db')+'\tfalse')
 sys.exit(0)
if args and args[0]=='inspect':
 labels={'com.docker.compose.project':state['project'],'com.docker.compose.service':'db','com.docker.compose.oneoff':'false','com.docker.compose.project.working_dir':str(cwd),'com.effigy.scope':record(state)['token']}
 if mode=='shared': labels['com.effigy.scope']='foreign-scope'
 if mode=='orphan': labels['com.docker.compose.service']='undeclared'
 if mode=='foreign': labels['com.docker.compose.project']='foreign-project'
 if mode=='oneoff': labels['com.docker.compose.oneoff']='true'
 if mode=='drift': labels['com.docker.compose.project.working_dir']=str(root)
 print(json.dumps([{'Id':state['id'] if mode!='unknown' else 'short','Name':'/'+state['project']+'-db-1','State':{'Status':'running' if mode!='stopped' else 'exited','Running':mode!='stopped'},'Config':{'Labels':labels},'Mounts':[],'NetworkSettings':{'Ports':{'5432/tcp':[{'HostIp':'127.0.0.1','HostPort':str(state['port'])}]}}}]))
 sys.exit(0)
if args and args[0]=='network': sys.exit(0)
if args and args[0]=='volume':
 r=record(state); name=state.get('project','private')+'-fixture-mutable-data'
 if args[1]=='ls' and state['volume'] and selected(state): print(name)
 elif args[1]=='inspect': print(json.dumps([{'Name':name,'Labels':{} if mode=='volume-unknown' else {'com.effigy.scope':r['token'],'com.effigy.project':state['project'],'com.effigy.persist':'false'}}]))
 elif args[1]=='rm': state['volume']=False;save();(cwd/'data').unlink()
 sys.exit(0)
if args and args[0]=='rm':
 with socket.create_connection(('127.0.0.1',state['port']),timeout=2) as c:
  c.sendall(b'retire\n');assert c.makefile('rb').readline()==(state['identity']+':retired\n').encode()
 state['active']=False;save();sys.exit(0)
sys.exit(0)
"#;
