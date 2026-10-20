//! Remote custody on the Compose backends (052 §5.10): the production
//! `RemoteExecAdapter` for Colima and Docker.
//!
//! - **Identity.** A container's identity names the exact engine and context
//!   it runs on, so settlement during sentinel recovery reaches the same
//!   backend without re-guessing (`engine:full-id@context`).
//! - **Environment.** Values never travel in argv (K12). A fixed host relay
//!   sends them as `KEY=value` lines read from its own private environment:
//!   Docker reads them as an env file on a private descriptor; Colima's VM
//!   shell reads them from stdin and hands them to `nerdctl` by name.
//! - **Stop and observe.** A fixed POSIX helper runs inside the registered
//!   container as root, inventories `/proc` and rechecks identity at
//!   delivery. The kernel's `session` rules decide what may be signalled.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{CleanupContext, EffectContext, OperationContext};
use effigy_kernel::custody::{EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::id::{Generation, ResourceId};
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::remote::session::{self, Member, SignalTarget, Snapshot, Transport};
use effigy_kernel::remote::{
    AdapterError, ContainerIdentity, ContainerTarget, Nonce, RemoteCommand, RemoteExecAdapter,
    RemoteRegistration, RemoteTerminal, SessionState, Signal,
};
use effigy_kernel::run::Fence;

use super::backend::{BackendKind, BackendSelection};
use crate::BackendId;

/// TERM to KILL for the local exec client.
const CLIENT_GRACE: Duration = Duration::from_secs(3);
/// Values of forwarded variables in the relay's private environment.
const VALUE_PREFIX: &str = "EFFIGY_REMOTE_V";

/// The exact container one registration binds: engine, context and full id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteContainer {
    pub kind: BackendKind,
    pub engine: BackendId,
    /// The Colima profile, or the Docker context or host the CLI targets.
    pub context: String,
    pub id: String,
}

impl RemoteContainer {
    /// `context` must already be exact: [`ComposeRemote::resolve`] resolves
    /// Docker's ambient default context to its name.
    pub fn new(selection: &BackendSelection, context: String, full_id: &str) -> Self {
        Self {
            kind: selection.kind,
            engine: selection.engine.clone(),
            context,
            id: full_id.to_owned(),
        }
    }

    pub fn target(&self) -> ContainerTarget {
        ContainerTarget(self.encode())
    }

    pub fn encode(&self) -> String {
        format!("{}:{}@{}", self.engine.as_str(), self.id, self.context)
    }

    pub fn parse(encoded: &str) -> Option<Self> {
        let (engine, rest) = encoded.split_once(':')?;
        let (id, context) = rest.split_once('@')?;
        let engine = BackendId::new(engine);
        let kind = if engine == BackendId::colima_nerdctl() {
            BackendKind::Colima
        } else if engine == BackendId::docker_compose() {
            super::backend::docker_kind()
        } else {
            return None;
        };
        let full = id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit());
        (full && !context.is_empty()).then(|| Self {
            kind,
            engine,
            context: context.to_owned(),
            id: id.to_owned(),
        })
    }

    /// One runtime CLI invocation on exactly this backend.
    fn runtime(&self, tail: Vec<OsString>) -> (&'static str, Vec<OsString>) {
        match self.kind {
            BackendKind::Colima => {
                let mut args = ["nerdctl", "--profile", self.context.as_str(), "--"]
                    .map(OsString::from)
                    .to_vec();
                args.extend(tail);
                ("colima", args)
            }
            BackendKind::DockerDesktop | BackendKind::DockerEngine => {
                let mut args = docker_target(&self.context);
                args.extend(tail);
                ("docker", args)
            }
        }
    }
}

fn docker_target(context: &str) -> Vec<OsString> {
    let flag = if context.contains("://") {
        "--host"
    } else {
        "--context"
    };
    vec![OsString::from(flag), OsString::from(context)]
}

/// One bounded backend CLI call's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The backend CLI seam. `input` arrives on stdin, never in argv; the call
/// ends by `deadline`. An error means the call's effect is unknown.
pub trait BackendCli {
    fn run(
        &mut self,
        program: &str,
        args: &[OsString],
        input: &[u8],
        deadline: Instant,
    ) -> Result<CliOutput, String>;
}

/// The host CLIs, through the crate's bounded capture.
#[derive(Debug, Clone)]
pub struct HostCli {
    pub workdir: PathBuf,
}

impl BackendCli for HostCli {
    fn run(
        &mut self,
        program: &str,
        args: &[OsString],
        input: &[u8],
        deadline: Instant,
    ) -> Result<CliOutput, String> {
        crate::exec::run_input_capture_until(&self.workdir, program, args, input, deadline)
            .map(|output| CliOutput {
                code: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            })
            .map_err(|error| error.to_string())
    }
}

/// How the workload runs inside the container.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub user: Option<String>,
    pub workdir: Option<String>,
    /// Host directory the exec client runs in.
    pub cwd: PathBuf,
    /// The exec client's own environment (the host's), without any custody
    /// nonce. Forwarded variables come from the remote command only.
    pub client_env: BTreeMap<OsString, EnvValue>,
    pub lifetime: Lifetime,
    pub label: String,
}

/// The production adapter. A recovery adapter (no exec options) observes,
/// signals and finds by nonce; it never launches.
#[derive(Debug)]
pub struct ComposeRemote<C: BackendCli = HostCli> {
    cli: C,
    exec: Option<ExecOptions>,
}

impl ComposeRemote<HostCli> {
    pub fn recovery() -> Self {
        Self::with_cli(
            HostCli {
                workdir: std::env::temp_dir(),
            },
            None,
        )
    }

    pub fn launcher(exec: ExecOptions) -> Self {
        Self::with_cli(
            HostCli {
                workdir: exec.cwd.clone(),
            },
            Some(exec),
        )
    }
}

impl<C: BackendCli> ComposeRemote<C> {
    pub fn with_cli(cli: C, exec: Option<ExecOptions>) -> Self {
        Self { cli, exec }
    }

    pub fn cli(&mut self) -> &mut C {
        &mut self.cli
    }

    /// The exact target for `full_id` on `selection`. Docker's ambient
    /// context is resolved to its name once, here, the way the CLI resolves
    /// it, so every later call (recovery in another process included)
    /// targets the same engine.
    pub fn resolve(
        &self,
        selection: &BackendSelection,
        full_id: &str,
    ) -> Result<RemoteContainer, String> {
        let ambient = matches!(
            selection.kind,
            BackendKind::DockerDesktop | BackendKind::DockerEngine
        ) && selection.context == "default"
            && std::env::var_os("DOCKER_CONTEXT").is_none_or(|value| value.is_empty())
            && std::env::var_os("DOCKER_HOST").is_none_or(|value| value.is_empty());
        let context = if ambient {
            crate::exec::docker_config_current_context()
                .filter(|name| !name.contains(char::is_whitespace))
                .ok_or_else(|| {
                    "the Docker CLI config cannot be read, so its current context is unknown"
                        .to_owned()
                })?
        } else {
            selection.context.clone()
        };
        Ok(RemoteContainer::new(selection, context, full_id))
    }

    fn inspect(&mut self, container: &RemoteContainer, cx: &OperationContext) -> Observed<State> {
        let deadline = match deadline(cx) {
            Ok(deadline) => deadline,
            Err(reason) => return Observed::Unknown(reason),
        };
        let (program, args) = container.runtime(vec![
            OsString::from("inspect"),
            OsString::from(&container.id),
        ]);
        let output = match self.cli.run(program, &args, &[], deadline) {
            Ok(output) => output,
            Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error)),
        };
        if output.code == Some(0) {
            return match parse_inspect(&output.stdout, &container.id) {
                Ok(state) => Observed::Verified(state, evidence("backend inspect")),
                Err(reason) => Observed::Unknown(UnknownReason::Unparsable(reason)),
            };
        }
        // A failed inspect is not absence. Only a complete inventory without
        // the id proves the container is gone.
        let (program, args) = container.runtime(
            ["ps", "--all", "--no-trunc", "--format", "{{.ID}}"]
                .map(OsString::from)
                .to_vec(),
        );
        match self.cli.run(program, &args, &[], deadline) {
            Ok(listed) if listed.code == Some(0) => {
                let present = String::from_utf8_lossy(&listed.stdout)
                    .lines()
                    .any(|line| line.trim() == container.id);
                if present {
                    Observed::Unknown(UnknownReason::Inconsistent(format!(
                        "inspect failed for a listed container: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )))
                } else {
                    Observed::Absent(AbsenceProof::ContainerGone {
                        id: container.encode(),
                    })
                }
            }
            Ok(listed) => Observed::Unknown(UnknownReason::Inconsistent(
                String::from_utf8_lossy(&listed.stderr).trim().to_owned(),
            )),
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error)),
        }
    }

    /// Runs the fixed helper in the container as root. The nonce, when any,
    /// is its first stdin line.
    fn helper(
        &mut self,
        container: &RemoteContainer,
        args: &[&str],
        nonce: Option<&Nonce>,
        cx: &OperationContext,
    ) -> Result<String, UnknownReason> {
        let deadline = deadline(cx)?;
        let mut tail = ["exec", "-i", "-u", "0"].map(OsString::from).to_vec();
        tail.push(OsString::from(&container.id));
        tail.extend(["sh", "-c", HELPER, "effigy-custody"].map(OsString::from));
        tail.extend(args.iter().map(OsString::from));
        let (program, args) = container.runtime(tail);
        let input = format!("{}\n", nonce.map(Nonce::as_str).unwrap_or_default());
        let output = self
            .cli
            .run(program, &args, input.as_bytes(), deadline)
            .map_err(UnknownReason::Inconsistent)?;
        match output.code {
            Some(0) => String::from_utf8(output.stdout)
                .map_err(|_| UnknownReason::Unparsable("helper output".into())),
            // The helper's own refusals: /proc unreadable in the guest.
            Some(3) => Err(UnknownReason::PermissionDenied),
            _ => Err(UnknownReason::Inconsistent(format!(
                "custody helper failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    fn snapshot_of(
        &mut self,
        container: &RemoteContainer,
        identity: &ContainerIdentity,
        nonce: &Nonce,
        cx: &OperationContext,
    ) -> Observed<Inventory> {
        let state = match self.inspect(container, cx) {
            Observed::Verified(state, _) => state,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        if !state.running {
            // A stopped container holds no process: every session in it ended.
            return Observed::Verified(
                Inventory {
                    identity: ContainerIdentity {
                        created: state.created,
                        ..identity.clone()
                    },
                    pid_ns: None,
                    processes: Vec::new(),
                },
                evidence("stopped container"),
            );
        }
        let text = match self.helper(container, &["snap"], Some(nonce), cx) {
            Ok(text) => text,
            Err(reason) => return Observed::Unknown(reason),
        };
        match parse_snapshot(&text) {
            Ok((boot, pid_ns, processes)) => Observed::Verified(
                Inventory {
                    identity: ContainerIdentity {
                        id: identity.id.clone(),
                        created: state.created,
                        vm_boot_id: boot,
                    },
                    pid_ns: Some(pid_ns),
                    processes,
                },
                evidence("container process inventory"),
            ),
            Err(reason) => Observed::Unknown(UnknownReason::Unparsable(reason)),
        }
    }

    fn launch_spec(
        &self,
        container: &RemoteContainer,
        cmd: RemoteCommand,
    ) -> Result<ProcessSpec, AdapterError> {
        let exec = self
            .exec
            .as_ref()
            .ok_or_else(|| inconsistent("a recovery adapter never launches"))?;
        let mut env = exec.client_env.clone();
        let mut names = Vec::new();
        for (index, (name, value)) in cmd.env.iter().enumerate() {
            let name = name
                .to_str()
                .filter(|name| valid_name(name))
                .ok_or_else(|| inconsistent("forwarded variable name is not portable"))?;
            let single = match value {
                EnvValue::Plain(value) => !value.as_encoded_bytes().contains(&b'\n'),
                EnvValue::Secret(value) => value.is_single_line(),
            };
            if !single {
                return Err(inconsistent(&format!(
                    "variable `{name}` spans lines; an env file cannot carry it"
                )));
            }
            names.push(OsString::from(name));
            env.insert(
                OsString::from(format!("{VALUE_PREFIX}{index}")),
                value.clone(),
            );
        }
        let mut flags = Vec::new();
        let colima = container.kind == BackendKind::Colima;
        // `terminal` reported Colima's downgrade, so the kernel frames on
        // stderr; a terminal reaching here would frame on the wrong stream.
        match (cmd.terminal, colima) {
            (RemoteTerminal::None, _) => {}
            (RemoteTerminal::Stdin, _) => flags.push(OsString::from("-i")),
            (RemoteTerminal::Tty, true) => {
                return Err(inconsistent("Colima cannot carry a backend terminal"))
            }
            (RemoteTerminal::Tty, false) => {
                flags.extend(["-i", "-t"].map(OsString::from));
            }
        }
        if let Some(user) = &exec.user {
            flags.extend([OsString::from("-u"), OsString::from(user)]);
        }
        if let Some(workdir) = &exec.workdir {
            flags.extend([OsString::from("-w"), OsString::from(workdir)]);
        }
        let mut argv = vec![OsString::from("/bin/sh"), OsString::from("-c")];
        if colima {
            argv.push(OsString::from(COLIMA_RELAY));
            argv.push(OsString::from("effigy-remote"));
            argv.push(OsString::from(names.len().to_string()));
            argv.extend(names.iter().cloned());
            argv.push(host_program("colima"));
            argv.push(OsString::from(&container.context));
            argv.push(OsString::from(if cmd.terminal == RemoteTerminal::None {
                "none"
            } else {
                "stdin"
            }));
            argv.push(OsString::from(COLIMA_VM));
            argv.push(OsString::from("exec"));
            argv.extend(flags);
            for name in &names {
                argv.extend([OsString::from("-e"), name.clone()]);
            }
        } else {
            argv.push(OsString::from(DOCKER_RELAY));
            argv.push(OsString::from("effigy-remote"));
            argv.push(OsString::from(names.len().to_string()));
            argv.extend(names.iter().cloned());
            argv.push(host_program("docker"));
            argv.extend(docker_target(&container.context));
            argv.push(OsString::from("exec"));
            argv.extend(flags);
            argv.extend(["--env-file", "/dev/fd/4"].map(OsString::from));
        }
        argv.push(OsString::from(&container.id));
        argv.extend(cmd.wrapper_argv());
        Ok(ProcessSpec {
            argv,
            cwd: exec.cwd.clone(),
            env,
            grace: CLIENT_GRACE,
            lifetime: exec.lifetime.clone(),
            terminal: Terminal::None,
            label: exec.label.clone(),
        })
    }
}

/// Inspect facts the adapter relies on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    created: String,
    running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Process {
    pid: u32,
    zombie: bool,
    ppid: u32,
    pgrp: u32,
    sid: u32,
    start: u64,
    /// `Some(true)` carries the nonce, `Some(false)` provably does not.
    nonce: Option<bool>,
}

#[derive(Debug, Clone)]
struct Inventory {
    identity: ContainerIdentity,
    /// `None` for a stopped container, which has no live namespace.
    pid_ns: Option<u64>,
    processes: Vec<Process>,
}

impl Inventory {
    fn live(&self) -> impl Iterator<Item = &Process> {
        self.processes.iter().filter(|process| !process.zombie)
    }

    fn member(process: &Process, nonce: &Nonce, descendant: bool) -> Member {
        Member {
            pid: process.pid,
            start_ticks: process.start,
            pgid: process.pgrp,
            sid: process.sid,
            nonce: match process.nonce {
                Some(true) => Observed::Verified(nonce.as_str().to_owned(), evidence("environ")),
                Some(false) => Observed::Verified(String::new(), evidence("environ")),
                None => Observed::Unknown(UnknownReason::PermissionDenied),
            },
            descendant,
        }
    }

    /// The registration's view: the complete session when isolated, else the
    /// leader's group plus its verified descendants (I-RC4).
    fn snapshot(&self, reg: &RemoteRegistration) -> Snapshot {
        let leader = self
            .live()
            .find(|process| process.pid == reg.leader_pid)
            .map(|process| Self::member(process, &reg.nonce, false));
        let mut members = Vec::new();
        if reg.session_isolated {
            members.extend(
                self.live()
                    .filter(|process| process.sid == reg.sid)
                    .map(|process| Self::member(process, &reg.nonce, false)),
            );
        } else {
            let mut tree = vec![reg.leader_pid];
            let mut index = 0;
            while index < tree.len() {
                let parent = tree[index];
                tree.extend(
                    self.live()
                        .filter(|process| process.ppid == parent && !tree.contains(&process.pid))
                        .map(|process| process.pid)
                        .collect::<Vec<_>>(),
                );
                index += 1;
            }
            let group = leader.as_ref().map(|leader| leader.pgid);
            members.extend(
                self.live()
                    .filter(|process| {
                        (process.sid == reg.sid && Some(process.pgrp) == group)
                            || tree.contains(&process.pid)
                    })
                    .map(|process| {
                        let descendant =
                            process.pid != reg.leader_pid && tree.contains(&process.pid);
                        Self::member(process, &reg.nonce, descendant)
                    }),
            );
        }
        Snapshot {
            container: self.identity.clone(),
            pid_ns: self.pid_ns.unwrap_or(reg.pid_ns),
            leader,
            members,
        }
    }
}

struct Session<'a, C: BackendCli> {
    adapter: &'a mut ComposeRemote<C>,
    container: RemoteContainer,
}

impl<C: BackendCli> Transport for Session<'_, C> {
    fn snapshot(&mut self, reg: &RemoteRegistration, cx: &OperationContext) -> Observed<Snapshot> {
        match self
            .adapter
            .snapshot_of(&self.container, &reg.container, &reg.nonce, cx)
        {
            Observed::Verified(inventory, evidence) => {
                Observed::Verified(inventory.snapshot(reg), evidence)
            }
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }

    fn deliver(
        &mut self,
        reg: &RemoteRegistration,
        target: &SignalTarget,
        sig: Signal,
        cx: &CleanupContext,
    ) -> Observed<()> {
        let sig = match sig {
            Signal::Term => "TERM",
            Signal::Continue => "CONT",
            Signal::Kill => "KILL",
        };
        let ns = format!("pid:[{}]", reg.pid_ns);
        let (args, pid) = match target {
            SignalTarget::Session {
                sid,
                leader_pid,
                start_ticks,
            } => (
                vec![
                    "kill".to_owned(),
                    reg.container.vm_boot_id.clone(),
                    ns,
                    sig.to_owned(),
                    "session".to_owned(),
                    sid.to_string(),
                    leader_pid.to_string(),
                    start_ticks.to_string(),
                ],
                *leader_pid,
            ),
            SignalTarget::Member {
                pid, start_ticks, ..
            } => (
                vec![
                    "kill".to_owned(),
                    reg.container.vm_boot_id.clone(),
                    ns,
                    sig.to_owned(),
                    "member".to_owned(),
                    pid.to_string(),
                    start_ticks.to_string(),
                ],
                *pid,
            ),
        };
        let args = args.iter().map(String::as_str).collect::<Vec<_>>();
        match self
            .adapter
            .helper(&self.container, &args, Some(&reg.nonce), cx.operation())
        {
            Ok(text) => match text.trim() {
                "sent" => Observed::Verified((), evidence("custody helper delivery")),
                "gone" => Observed::Absent(AbsenceProof::NoSuchProcess { pid }),
                "moved" => Observed::Unknown(inconsistent_reason(
                    "the container's boot or namespace changed before delivery",
                )),
                other => Observed::Unknown(UnknownReason::Unparsable(other.to_owned())),
            },
            Err(reason) => Observed::Unknown(reason),
        }
    }
}

impl<C: BackendCli> RemoteExecAdapter for ComposeRemote<C> {
    /// Colima's VM shell carries the forwarded values on stdin, so it cannot
    /// also hold a terminal: a terminal request there attaches stdin only.
    fn terminal(&self, id: &ContainerIdentity, requested: RemoteTerminal) -> RemoteTerminal {
        match RemoteContainer::parse(&id.id) {
            Some(container)
                if container.kind == BackendKind::Colima && requested == RemoteTerminal::Tty =>
            {
                RemoteTerminal::Stdin
            }
            _ => requested,
        }
    }

    fn identity(
        &mut self,
        target: &ContainerTarget,
        cx: &OperationContext,
    ) -> Observed<ContainerIdentity> {
        let Some(container) = RemoteContainer::parse(&target.0) else {
            return Observed::Unknown(inconsistent_reason("not an exact container target"));
        };
        let state = match self.inspect(&container, cx) {
            Observed::Verified(state, _) => state,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        if !state.running {
            return Observed::Unknown(inconsistent_reason("the container is not running"));
        }
        match self.helper(&container, &["boot"], None, cx) {
            Ok(text) => match text.trim() {
                boot if !boot.is_empty() && !boot.contains(char::is_whitespace) => {
                    Observed::Verified(
                        ContainerIdentity {
                            id: container.encode(),
                            created: state.created,
                            vm_boot_id: boot.to_owned(),
                        },
                        evidence("backend identity"),
                    )
                }
                _ => Observed::Unknown(UnknownReason::Unparsable("boot id".into())),
            },
            Err(reason) => Observed::Unknown(reason),
        }
    }

    fn launch(
        &mut self,
        id: &ContainerIdentity,
        cmd: RemoteCommand,
        cx: &EffectContext,
    ) -> Result<ProcessSpec, AdapterError> {
        cx.operation().check("remote launch").map_err(|_| {
            AdapterError(UnknownReason::TimedOut {
                phase: "remote launch".into(),
            })
        })?;
        let container = RemoteContainer::parse(&id.id)
            .ok_or_else(|| inconsistent("not an exact container identity"))?;
        self.launch_spec(&container, cmd)
    }

    fn signal_session(
        &mut self,
        reg: &RemoteRegistration,
        sig: Signal,
        cx: &CleanupContext,
    ) -> Observed<SessionState> {
        let Some(container) = RemoteContainer::parse(&reg.container.id) else {
            return Observed::Unknown(inconsistent_reason("not an exact container identity"));
        };
        session::signal(
            &mut Session {
                adapter: self,
                container,
            },
            reg,
            sig,
            cx,
        )
    }

    fn observe_session(
        &mut self,
        reg: &RemoteRegistration,
        cx: &OperationContext,
    ) -> Observed<SessionState> {
        let Some(container) = RemoteContainer::parse(&reg.container.id) else {
            return Observed::Unknown(inconsistent_reason("not an exact container identity"));
        };
        let snapshot = Session {
            adapter: self,
            container,
        }
        .snapshot(reg, cx);
        session::observe(reg, snapshot)
    }

    fn find_by_nonce(
        &mut self,
        id: &ContainerIdentity,
        nonce: &Nonce,
        cx: &OperationContext,
    ) -> Observed<Option<RemoteRegistration>> {
        let Some(container) = RemoteContainer::parse(&id.id) else {
            return Observed::Unknown(inconsistent_reason("not an exact container identity"));
        };
        let inventory = match self.snapshot_of(&container, id, nonce, cx) {
            Observed::Verified(inventory, _) => inventory,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        if inventory.identity.created != id.created {
            return Observed::Unknown(inconsistent_reason(
                "a replacement container carries the recorded id",
            ));
        }
        let Some(pid_ns) = inventory.pid_ns else {
            // Stopped: no process can carry the nonce, or ever start under it.
            return Observed::Verified(None, evidence("stopped container"));
        };
        if inventory.identity.vm_boot_id != id.vm_boot_id {
            return Observed::Absent(AbsenceProof::BootChanged {
                recorded: id.vm_boot_id.clone(),
                current: inventory.identity.vm_boot_id,
            });
        }
        if inventory.live().any(|process| process.nonce.is_none()) {
            return Observed::Unknown(UnknownReason::PermissionDenied);
        }
        let carriers = inventory
            .live()
            .filter(|process| process.nonce == Some(true))
            .collect::<Vec<_>>();
        if carriers.is_empty() {
            // The complete live inventory of the registered boot has no
            // carrier: the workload never started (R5).
            return Observed::Verified(None, evidence("complete nonce inventory"));
        }
        let leader = carriers
            .iter()
            .find(|process| process.sid == process.pid)
            .copied()
            .or_else(|| {
                let tops = carriers
                    .iter()
                    .filter(|process| !carriers.iter().any(|other| other.pid == process.ppid))
                    .collect::<Vec<_>>();
                (tops.len() == 1).then(|| *tops[0])
            });
        let Some(leader) = leader else {
            return Observed::Unknown(inconsistent_reason("no single nonce session leader"));
        };
        Observed::Verified(
            Some(RemoteRegistration {
                // The kernel binds its intent's fence; the adapter cannot know it.
                env: Fence {
                    resource: match ResourceId::gateway("remote-recovery") {
                        Ok(resource) => resource,
                        Err(_) => return Observed::Unknown(inconsistent_reason("fence")),
                    },
                    generation: Generation::new(0),
                },
                container: id.clone(),
                pid_ns,
                sid: leader.sid,
                leader_pid: leader.pid,
                leader_start_ticks: leader.start,
                nonce: nonce.clone(),
                session_isolated: leader.sid == leader.pid,
            }),
            evidence("nonce inventory"),
        )
    }
}

fn host_program(program: &str) -> OsString {
    crate::compose::resolve_host_cli_program(program)
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

fn deadline(cx: &OperationContext) -> Result<Instant, UnknownReason> {
    cx.check("remote backend call")
        .map_err(|_| UnknownReason::TimedOut {
            phase: "remote backend call".into(),
        })?;
    let remaining = cx.remaining().ok_or_else(|| UnknownReason::TimedOut {
        phase: "unbounded remote backend call".into(),
    })?;
    Ok(Instant::now() + remaining)
}

fn evidence(source: &str) -> Evidence {
    Evidence::new(source.into(), None).expect("fixed evidence")
}

fn inconsistent_reason(message: &str) -> UnknownReason {
    UnknownReason::Inconsistent(message.into())
}

fn inconsistent(message: &str) -> AdapterError {
    AdapterError(inconsistent_reason(message))
}

fn parse_inspect(bytes: &[u8], full_id: &str) -> Result<State, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Row {
        id: String,
        created: String,
        state: RowState,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct RowState {
        running: bool,
    }
    let rows: Vec<Row> =
        serde_json::from_slice(bytes).map_err(|error| format!("unparsable inspect: {error}"))?;
    let [row] = rows.as_slice() else {
        return Err("inspect did not name exactly one container".into());
    };
    if row.id != full_id {
        return Err("inspect named another container".into());
    }
    if row.created.is_empty() || row.created.contains(char::is_whitespace) {
        return Err("unusable creation time".into());
    }
    Ok(State {
        created: row.created.clone(),
        running: row.state.running,
    })
}

fn parse_snapshot(text: &str) -> Result<(String, u64, Vec<Process>), String> {
    let mut boot = None;
    let mut pid_ns = None;
    let mut processes = Vec::new();
    for line in text.lines() {
        let fields = line.split(' ').collect::<Vec<_>>();
        match fields.as_slice() {
            ["boot", value] if !value.is_empty() => boot = Some((*value).to_owned()),
            ["ns", value] => {
                pid_ns = value
                    .strip_prefix("pid:[")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .and_then(|number| number.parse().ok());
            }
            ["p", pid, state, ppid, pgrp, sid, start, nonce] => {
                let number = |field: &str| field.parse::<u64>().map_err(|_| line.to_owned());
                let small = |field: &str| field.parse::<u32>().map_err(|_| line.to_owned());
                processes.push(Process {
                    pid: small(pid)?,
                    zombie: matches!(*state, "Z" | "X"),
                    ppid: small(ppid)?,
                    pgrp: small(pgrp)?,
                    sid: small(sid)?,
                    start: number(start)?,
                    nonce: match *nonce {
                        "y" => Some(true),
                        "n" => Some(false),
                        _ => None,
                    },
                });
            }
            _ => return Err(format!("unexpected helper line `{line}`")),
        }
    }
    match (boot, pid_ns) {
        (Some(boot), Some(pid_ns)) => Ok((boot, pid_ns, processes)),
        _ => Err("helper omitted the boot or namespace".into()),
    }
}

/// The in-container helper: POSIX `sh`, `/proc`, `tr` and `kill`. It reads
/// the nonce from its first stdin line and never writes anything.
/// `snap` lists every live process with its session facts and whether its
/// environment carries the nonce (`y`, `n`, or `?` when unreadable). `kill`
/// rechecks boot, namespace and the target's identity immediately before it
/// signals (I-P4): a session only while its leader lives with the
/// registered start ticks, a member only with its start ticks and nonce.
pub(crate) const HELPER: &str = r#"mode=$1; shift
IFS= read -r nonce || nonce=
IFS= read -r boot < /proc/sys/kernel/random/boot_id || exit 3
if [ "$mode" = boot ]; then printf '%s\n' "$boot"; exit 0; fi
ns=$(readlink /proc/self/ns/pid) || exit 3
me=$$
facts() {
    IFS= read -r st < "/proc/$1/stat" 2>/dev/null || return 1
    st=${st##*) }
    set -f; set -- $st; set +f
    f_state=$1 f_ppid=$2 f_pgrp=$3 f_sid=$4
    shift 19
    f_start=$1
}
carries() {
    e=$(tr '\0' '\n' < "/proc/$1/environ" 2>/dev/null) || { printf '?'; return; }
    case "
$e
" in
    *"
EFFIGY_CUSTODY_NONCE=$nonce
"*) printf y ;;
    *) printf n ;;
    esac
}
case $mode in
snap)
    printf 'boot %s\nns %s\n' "$boot" "$ns"
    for d in /proc/[0-9]*; do
        p=${d#/proc/}
        [ "$p" = "$me" ] && continue
        facts "$p" || continue
        [ "$f_ppid" = "$me" ] && continue
        n=-
        [ -n "$nonce" ] && n=$(carries "$p")
        printf 'p %s %s %s %s %s %s %s\n' "$p" "$f_state" "$f_ppid" "$f_pgrp" "$f_sid" "$f_start" "$n"
    done
    ;;
kill)
    if [ "$boot" != "$1" ] || [ "$ns" != "$2" ]; then echo moved; exit 0; fi
    sig=$3 kind=$4
    shift 4
    if [ "$kind" = session ]; then
        sid=$1 leader=$2 start=$3
        if ! facts "$leader" || [ "$f_start" != "$start" ] || [ "$f_sid" != "$sid" ] || [ "$f_state" = Z ]; then
            echo gone; exit 0
        fi
        for d in /proc/[0-9]*; do
            p=${d#/proc/}
            [ "$p" = "$me" ] && continue
            facts "$p" || continue
            if [ "$f_sid" = "$sid" ] && [ "$f_state" != Z ]; then kill -s "$sig" "$p" 2>/dev/null; fi
        done
        echo sent
    else
        pid=$1 start=$2
        if ! facts "$pid" || [ "$f_start" != "$start" ] || [ "$f_state" = Z ] || [ "$(carries "$pid")" != y ]; then
            echo gone; exit 0
        fi
        if kill -s "$sig" "$pid" 2>/dev/null; then echo sent; else echo gone; fi
    fi
    ;;
*) exit 2 ;;
esac
"#;

/// Host relay for Docker: forwarded values reach the CLI as an env file on
/// a private descriptor, so stdin stays the operator's and nothing secret
/// enters argv. `$1` is the variable count, then the names, then the CLI.
pub(crate) const DOCKER_RELAY: &str = r#"n=$1; shift
names=
while [ "$n" -gt 0 ]; do names="$names $1"; shift; n=$((n - 1)); done
emit() {
    i=0
    for name in $names; do
        printf '%s=' "$name"
        printenv "EFFIGY_REMOTE_V$i" || printf '\n'
        i=$((i + 1))
    done
}
exec 3<&0
emit | { exec 4<&0 <&3 3<&-; exec "$@"; }
"#;

/// Host relay for Colima: the VM shell reads the values from stdin first;
/// with `stdin` the operator's input follows them. `$1` is the variable
/// count, then the names, the `colima` path, profile, mode and VM script.
pub(crate) const COLIMA_RELAY: &str = r#"n=$1; shift
names=
count=$n
while [ "$n" -gt 0 ]; do names="$names $1"; shift; n=$((n - 1)); done
colima=$1 profile=$2 mode=$3 vm=$4
shift 4
emit() {
    i=0
    for name in $names; do
        printf '%s=' "$name"
        printenv "EFFIGY_REMOTE_V$i" || printf '\n'
        i=$((i + 1))
    done
}
if [ "$mode" != stdin ]; then
    emit | "$colima" ssh --profile "$profile" -- sh -c "$vm" sh "$count" $names "$@"
    exit $?
fi
exec 3<&0
{ emit; exec cat <&3; } | "$colima" ssh --profile "$profile" -- sh -c "$vm" sh "$count" $names "$@" &
pid=$!
trap 'trap "" TERM INT HUP; kill -TERM 0 2>/dev/null; exit 143' TERM INT HUP
wait "$pid"
status=$?
trap '' TERM
kill -TERM 0 2>/dev/null
exit "$status"
"#;

/// Colima VM side: exports the forwarded values read from stdin and hands
/// them to `nerdctl` by name, so they never appear in the VM's argv either.
pub(crate) const COLIMA_VM: &str = r#"n=$1; shift
names=
while [ "$n" -gt 0 ]; do names="$names $1"; shift; n=$((n - 1)); done
keep=
for name in $names; do
    IFS= read -r line || exit 125
    case $line in "$name="*) ;; *) exit 125 ;; esac
    export "$line"
    keep="${keep:+$keep,}$name"
done
exec sudo --preserve-env="$keep" nerdctl "$@"
"#;

#[cfg(test)]
mod tests;
