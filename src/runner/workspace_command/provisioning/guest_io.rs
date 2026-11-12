//! Perform filesystem and identity I/O against the exact verified guest.

use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::exec_command::{
    copy_file_into_service, copy_file_into_service_bounded, run_command_capture_until,
    BoundedCapture, BoundedCommandBudget, BoundedCommandPhase,
};
use crate::runner::workspace_client::context::{
    GuestReader, GuestWorkspace, WorkspaceContainerIdentity, HOST_REPREPARE_COMMAND,
};
use crate::runner::RunnerError;
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
const CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX: &str = "/tmp/effigy-host";
static WORKSPACE_EFFIGY_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);
/// One identity probe reads four short `/proc` fields and two `ls` lines.
const WORKSPACE_IDENTITY_STDOUT_BYTES: u64 = 64 * 1024;
const WORKSPACE_IDENTITY_STDERR_BYTES: u64 = 8 * 1024;

pub(super) fn render_workspace_effigy_staging_path() -> String {
    let counter = WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{CONTAINER_WORKSPACE_EFFIGY_STAGING_PATH_PREFIX}-{}-{counter}",
        std::process::id()
    )
}

fn run_workspace_compose_capture_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    args: Vec<OsString>,
    budget: &mut BoundedCommandBudget,
) -> Result<BoundedCapture, RunnerError> {
    let plan = guest_exec_plan(repo_root, policy, target_id, args, "workspace preparation")
        .map_err(|_| {
            bounded_phase_error(
                policy,
                policy.primary_service.as_str(),
                budget,
                BoundedCommandPhase::WorkspacePreparation,
                "workspace_preparation_failed",
                "bounded container command could not resolve workspace preparation",
                None,
            )
        })?;
    budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, policy.primary_service.as_str(), budget, failure)
        })
}

/// Compose resolves the backend; every exec replaces the service alias with
/// the verified full container ID before selecting its runtime command.
fn guest_exec_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    target_id: &str,
    args: Vec<OsString>,
    label: &str,
) -> Result<effigy_containers::ContainerRuntimeInvocationPlan, RunnerError> {
    let compose = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        repo_root,
        policy,
        args,
        effigy_containers::ContainerAction::Exec,
        label,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let args = crate::runner::exec_command::verified_target_exec_args(&compose, target_id)?;
    effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &args,
        effigy_containers::ContainerAction::Exec,
        label,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

/// The container-mode guest filesystem, reached through Compose exec and copy.
enum GuestCommand<'a> {
    Unbounded {
        target_id: String,
    },
    Target {
        target_id: &'a str,
        budget: &'a mut BoundedCommandBudget,
    },
}

pub(super) struct ComposeWorkspaceGuest<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    command: GuestCommand<'a>,
}

impl<'a> ComposeWorkspaceGuest<'a> {
    pub(super) fn unbounded(
        repo_root: &'a Path,
        policy: &'a EffectiveContainerPolicy,
    ) -> Result<Self, RunnerError> {
        let binding = crate::runner::container_runtime_prep::verify_service_binding(
            repo_root,
            policy,
            policy.primary_service.as_str(),
            None,
        )?;
        Ok(Self {
            repo_root,
            policy,
            command: GuestCommand::Unbounded {
                target_id: binding.id().to_owned(),
            },
        })
    }

    fn target_id(&self) -> &str {
        match &self.command {
            GuestCommand::Unbounded { target_id } => target_id,
            GuestCommand::Target { target_id, .. } => target_id,
        }
    }

    pub(super) fn bounded(
        repo_root: &'a Path,
        policy: &'a EffectiveContainerPolicy,
        target_id: &'a str,
        budget: &'a mut BoundedCommandBudget,
    ) -> Self {
        Self {
            repo_root,
            policy,
            command: GuestCommand::Target { target_id, budget },
        }
    }

    pub(super) fn run_script(
        &mut self,
        script: &str,
        as_root: bool,
    ) -> Result<BoundedCapture, RunnerError> {
        let mut compose_args =
            effigy_containers::compose::compose_args(self.policy, ["exec", "-T"]);
        if as_root {
            compose_args.extend([OsString::from("-u"), OsString::from("0")]);
        }
        compose_args.extend([
            OsString::from(self.policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(script),
        ]);
        match &mut self.command {
            GuestCommand::Unbounded { target_id } => {
                let plan = guest_exec_plan(
                    self.repo_root,
                    self.policy,
                    target_id,
                    compose_args,
                    "workspace guest command",
                )?;
                let output = run_command_capture_until(
                    &plan.repo_root,
                    plan.program.as_os_str(),
                    &plan.args,
                    None,
                    None,
                )?;
                Ok(BoundedCapture {
                    status: output.status,
                    stdout: output.stdout,
                    stderr: output.stderr,
                })
            }
            GuestCommand::Target { target_id, budget } => run_workspace_compose_capture_bounded(
                self.repo_root,
                self.policy,
                target_id,
                compose_args,
                budget,
            ),
        }
    }

    fn stage(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        match &mut self.command {
            GuestCommand::Unbounded { target_id } => copy_file_into_service(
                self.repo_root,
                self.policy,
                target_id,
                host_path,
                guest_path,
            ),
            GuestCommand::Target { target_id, budget } => copy_file_into_service_bounded(
                self.repo_root,
                self.policy,
                self.policy.primary_service.as_str(),
                target_id,
                host_path,
                guest_path,
                budget,
            ),
        }
    }
}

// Colima/nerdctl collapses nonzero guest exits into host exit 1. Optional
// reads therefore carry explicit presence framing in a successful guest reply;
// transport failure never supplies absence. The body remains binary-safe.
fn selector_parent(path: &str) -> Result<String, RunnerError> {
    Path::new(path)
        .parent()
        .and_then(Path::to_str)
        .map(shell_quote)
        .ok_or_else(|| RunnerError::task_invocation("guest selector has no parent directory"))
}

fn optional_guest_read(bytes: &[u8]) -> Result<Option<&[u8]>, RunnerError> {
    match bytes {
        b"A\n" => Ok(None),
        [b'P', b'\n', body @ ..] => Ok(Some(body)),
        _ => Err(RunnerError::task_invocation(
            "guest optional read returned malformed presence framing",
        )),
    }
}

#[cfg(test)]
mod guest_read_protocol {
    use super::*;
    #[test]
    fn selector_writes_prepare_missing_parents_through_the_bounded_guest_transport() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};
        let _lock = crate::contract_test_support::lock_test();
        let fixture = effigy_test_support::Fixture::new("guest-selector-parent-").unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        let executable = bin.join("colima");
        std::fs::write(
            &executable,
            r#"#!/bin/sh
set -e
[ "$1" = nerdctl ] && [ "$2" = --profile ] && [ "$3" = fixture ] && [ "$4" = -- ]
shift 4
[ "$1" = exec ]
previous=''
while [ "$#" -gt 0 ] && [ "$1" != sh ]; do previous="$1"; shift; done
[ "$previous" = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]
exec "$@"
"#,
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(format!("{}:/usr/bin:/bin", bin.display()))),
            ("HOME", Some(home.display().to_string())),
            ("COLIMA_HOME", Some(root.join("c").display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("colima".into())),
        ]);
        std::fs::write(root.join("effigy.toml"), "[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\nprofile='fixture'\n").unwrap();
        std::fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: fixture:one\n",
        )
        .unwrap();
        let policy = effigy_containers::load_container_policy(&root, Some("web")).unwrap();
        let now = Instant::now();
        let mut budget = BoundedCommandBudget::new(now, now + Duration::from_secs(10), 4096, 4096);
        let id = "a".repeat(64);
        let mut guest = ComposeWorkspaceGuest::bounded(&root, &policy, &id, &mut budget);
        let pointer = root.join("guest fresh ; literal/run/bin/current");
        guest
            .replace_pointer(pointer.to_str().unwrap(), "42")
            .unwrap();
        assert_eq!(std::fs::read_to_string(&pointer).unwrap(), "42\n");
        // This double executes host utilities. The Linux guest's GNU mv -T
        // branch is exercised here on Linux and by native smoke on macOS.
        #[cfg(target_os = "linux")]
        {
            let link = root.join("another fresh/usr/local/bin/effigy");
            guest
                .replace_link(link.to_str().unwrap(), "/owned/object")
                .unwrap();
            assert_eq!(
                std::fs::read_link(link).unwrap(),
                Path::new("/owned/object")
            );
        }
    }

    #[test]
    fn presence_framing_distinguishes_absence_empty_and_binary_contents() {
        assert_eq!(optional_guest_read(b"A\n").unwrap(), None);
        assert_eq!(optional_guest_read(b"P\n").unwrap(), Some(&b""[..]));
        assert_eq!(
            optional_guest_read(b"P\nA\n\0\xff").unwrap(),
            Some(&b"A\n\0\xff"[..])
        );
        for bad in [&b""[..], &b"A\nextra"[..], &b"diagnostic"[..], &b"P"[..]] {
            assert!(optional_guest_read(bad).is_err());
        }
    }
}

impl GuestReader for ComposeWorkspaceGuest<'_> {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        let script = format!(
            "p={}\nif [ -e \"$p\" ]; then printf 'P\\n'; cat \"$p\"; else printf 'A\\n'; fi",
            shell_quote(path)
        );
        // Root reads staged objects, including credentials before ownership handoff.
        let output = self.run_script(&script, true)?;
        if output.status.success() {
            return optional_guest_read(&output.stdout).map(|value| value.map(<[u8]>::to_vec));
        }
        Err(RunnerError::task_invocation(format!(
            "cannot read `{path}` in workspace container `{}`: {}",
            self.policy.name,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }

    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        // One digest line, never the file: a bounded caller must not pay for
        // the executable's size in its own capture budget.
        let script = format!(
            "p={path}\n[ -f \"$p\" ] || {{ printf 'A\\n'; exit 0; }}\nprintf 'P\\n'\n\
             if command -v sha256sum >/dev/null 2>&1; then sha256sum \"$p\"\n\
             elif command -v shasum >/dev/null 2>&1; then shasum -a 256 \"$p\"\n\
             elif command -v openssl >/dev/null 2>&1; then openssl dgst -sha256 \"$p\"\n\
             else exit 4\nfi",
            path = shell_quote(path)
        );
        // Root reads staged objects, including credentials before ownership handoff.
        let output = self.run_script(&script, true)?;
        match output.status.code() {
            Some(0) => {}
            Some(4) => {
                return Err(RunnerError::task_invocation(format!(
                    "workspace container `{}` has no SHA-256 utility, so the pinned guest \
                     executable cannot be authenticated; run `{HOST_REPREPARE_COMMAND}` with an \
                     image that provides sha256sum, shasum or openssl",
                    self.policy.name
                )))
            }
            _ => {
                return Err(RunnerError::task_invocation(format!(
                    "cannot hash `{path}` in workspace container `{}`: {}",
                    self.policy.name,
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
        }
        let Some(bytes) = optional_guest_read(&output.stdout)? else {
            return Ok(None);
        };
        let rendered = String::from_utf8_lossy(bytes);
        rendered
            .split_whitespace()
            .find(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(|token| token.to_ascii_lowercase())
            .map(Some)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "SHA-256 tool in workspace container `{}` returned no digest for `{path}`",
                    self.policy.name
                ))
            })
    }
}

impl GuestWorkspace for ComposeWorkspaceGuest<'_> {
    fn read_link(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        let script = format!(
            "if [ -L {path} ]; then printf 'P\\n'; readlink {path}; else printf 'A\\n'; fi",
            path = shell_quote(path)
        );
        let output = self.run_script(&script, false)?;
        match output.status.code() {
            Some(0) => optional_guest_read(&output.stdout)?
                .map(|bytes| String::from_utf8(bytes.to_vec())
                    .map(|target| target.trim_end_matches('\n').to_owned())
                    .map_err(|_| RunnerError::task_invocation("guest link read returned invalid UTF-8")))
                .transpose(),
            _ => Err(RunnerError::task_invocation(format!(
                "cannot read workspace entry `{path}`: {}; run `{HOST_REPREPARE_COMMAND}` on the host",
                String::from_utf8_lossy(&output.stderr).trim()
            ))),
        }
    }

    fn ensure_dir(&mut self, path: &str, mode: u32) -> Result<(), RunnerError> {
        let script = format!(
            "mkdir -p {path} && chmod {mode:04o} {path}",
            path = shell_quote(path)
        );
        self.run_root(&script, "make a guest directory")
    }

    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        let staging = render_workspace_effigy_staging_path();
        let host_file = stage_host_bytes(bytes)?;
        let staged = self.stage(host_file.path(), &staging);
        drop(host_file);
        staged?;
        let script = format!(
            "install -m {mode:04o} {staging} {path} && rm -f {staging}",
            staging = shell_quote(&staging),
            path = shell_quote(path),
        );
        self.run_root(&script, "publish a guest file")
    }

    fn write_no_replace(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        let staging = render_workspace_effigy_staging_path();
        let host_file = stage_host_bytes(bytes)?;
        self.stage(host_file.path(), &staging)?;
        self.install_immutable(&staging, path, mode)
    }

    fn install_no_replace(&mut self, staging: &str, path: &str) -> Result<(), RunnerError> {
        self.install_immutable(staging, path, 0o555)
    }

    fn set_owner(&mut self, path: &str, owner: &str) -> Result<(), RunnerError> {
        let script = format!(
            "chown {owner} {path}",
            owner = shell_quote(owner),
            path = shell_quote(path)
        );
        self.run_root(&script, "give a guest file to the workspace user")
    }

    fn stage_host_file(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        self.stage(host_path, guest_path)
    }

    fn install_executable(&mut self, staging: &str, path: &str) -> Result<(), RunnerError> {
        let script = format!(
            "install -m 0555 {staging} {path} && rm -f {staging}",
            staging = shell_quote(staging),
            path = shell_quote(path),
        );
        self.run_root(&script, "install the pinned guest executable")
    }

    fn replace_pointer(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let parent = selector_parent(path)?;
        let script = format!(
            "mkdir -p -m 0755 -- {parent} && printf '%s\\n' {target} > {path}.tmp && mv -f {path}.tmp {path}",
            target = shell_quote(target),
            path = shell_quote(path),
        );
        self.run_root(&script, "switch a guest pointer")
    }

    fn replace_link(&mut self, path: &str, target: &str) -> Result<(), RunnerError> {
        let parent = selector_parent(path)?;
        let script = format!(
            "mkdir -p -m 0755 -- {parent} && ln -sfn {target} {path}.tmp && mv -Tf {path}.tmp {path}",
            target = shell_quote(target),
            path = shell_quote(path),
        );
        self.run_root(&script, "switch the guest entry executable")
    }

    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        // Placement uses the verified full backend ID, never guest hostname/cgroup claims.
        let output = self.run_script(
            "printf 'boot=%s\\n' \"$(cat /proc/sys/kernel/random/boot_id)\"\n\
             printf 'platform=%s\\n' \"$(uname -m)\"\n\
             printf 'libc_musl=%s\\n' \"$(ls /lib/ld-musl-*.so.1 2>/dev/null | head -n1)\"\n\
             printf 'libc_glibc=%s\\n' \"$(ls /lib/*-linux-gnu*/libc.so.6 /lib/libc.so.6 /lib64/libc.so.6 2>/dev/null | head -n1)\"",
            false,
        )?;
        if !output.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "cannot read the workspace container identity for `{}`: {}",
                self.policy.name,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        parse_container_identity(&String::from_utf8_lossy(&output.stdout), self.target_id())
            .map_err(|_| {
                RunnerError::task_invocation(format!(
                    "workspace container `{}` did not report its identity; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                    self.policy.name
                ))
            })
    }
}

impl ComposeWorkspaceGuest<'_> {
    fn install_immutable(
        &mut self,
        staging: &str,
        path: &str,
        mode: u32,
    ) -> Result<(), RunnerError> {
        // Use same-filesystem link(2) for no-replace publication despite external races.
        let script = format!(
            "object=$(mktemp {template}) || exit 1; trap 'rm -f \"$object\" {staging}' EXIT; install -m {mode:04o} {staging} \"$object\" && ln \"$object\" {path}",
            template = shell_quote(&format!("{path}.stage.XXXXXX")),
            staging = shell_quote(staging), path = shell_quote(path),
        );
        self.run_root(&script, "create an immutable guest object")
    }

    fn run_root(&mut self, script: &str, what: &str) -> Result<(), RunnerError> {
        let output = self.run_script(script, true)?;
        if output.status.success() {
            return Ok(());
        }
        Err(RunnerError::task_invocation(format!(
            "cannot {what} in workspace container `{}`: {}",
            self.policy.name,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Read guest-local identity through the selected backend and exact full ID.
/// This does not re-resolve a Compose service or select a backend from a path.
pub(in crate::runner) fn observed_identity_on_backend(
    root: &effigy_kernel::root::StateRoot,
    selected: &effigy_containers::environment::BackendSelection,
    target_id: &str,
    deadline: std::time::Instant,
) -> Result<WorkspaceContainerIdentity, RunnerError> {
    if target_id.len() != 64
        || !target_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(RunnerError::task_invocation(
            "guest identity needs an exact full container id",
        ));
    }
    let checkout = root.owning_directory();
    let expected = checkout
        .identity()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let check = || -> Result<(), RunnerError> {
        let current = effigy_kernel::fs::DirFd::open(checkout.path())
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if current
            .identity()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            != expected
        {
            return Err(RunnerError::task_invocation(
                "workspace checkout identity changed",
            ));
        }
        Ok(())
    };
    check()?;
    let script = "printf 'boot=%s\\n' \"$(cat /proc/sys/kernel/random/boot_id)\"\n\
                  printf 'platform=%s\\n' \"$(uname -m)\"\n\
                  printf 'libc_musl=%s\\n' \"$(ls /lib/ld-musl-*.so.1 2>/dev/null | head -n1)\"\n\
                  printf 'libc_glibc=%s\\n' \"$(ls /lib/*-linux-gnu*/libc.so.6 /lib/libc.so.6 /lib64/libc.so.6 2>/dev/null | head -n1)\"";
    let args = ["exec", target_id, "sh", "-lc", script].map(OsString::from);
    let (program, args) = selected
        .compose_invocation(&args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let output = effigy_containers::exec::run_backend_invocation_until(
        checkout.path(),
        &program,
        &args,
        "workspace guest identity",
        &[],
        deadline,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if output.stdout.len() as u64 > WORKSPACE_IDENTITY_STDOUT_BYTES
        || output.stderr.len() as u64 > WORKSPACE_IDENTITY_STDERR_BYTES
    {
        return Err(RunnerError::task_invocation(
            "workspace guest identity output exceeds its capture budget",
        ));
    }
    check()?;
    if std::time::Instant::now() >= deadline {
        return Err(RunnerError::task_invocation(
            "workspace guest identity deadline expired",
        ));
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| RunnerError::task_invocation("workspace guest identity is not UTF-8"))?;
    parse_container_identity(text, target_id)
}

/// Combine exact-target probe facts with the host-verified full backend ID.
/// Guest hostname or cgroup claims never grant placement.
pub(super) fn parse_container_identity(
    rendered: &str,
    verified_target_id: &str,
) -> Result<WorkspaceContainerIdentity, RunnerError> {
    let field = |name: &str| {
        rendered
            .lines()
            .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix('='))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let container_id = (verified_target_id.len() == 64
        && verified_target_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()))
    .then(|| verified_target_id.to_ascii_lowercase());
    let vm_boot_id = field("boot");
    let platform = field("platform");
    // The loader a guest can see tells the host which C library the image has,
    // and therefore which artifact family can run in it (§8.9). An image with
    // neither marker is unproven, not glibc by default.
    let libc = field("libc").unwrap_or_else(|| {
        if field("libc_musl").is_some() {
            "musl".to_owned()
        } else if field("libc_glibc").is_some() {
            "gnu".to_owned()
        } else {
            String::new()
        }
    });
    match (container_id, vm_boot_id, platform) {
        (Some(container_id), Some(vm_boot_id), Some(platform)) => Ok(WorkspaceContainerIdentity {
            container_id,
            vm_boot_id,
            platform,
            libc,
        }),
        _ => Err(RunnerError::task_invocation(
            "the container identity probe requires a verified full backend id, boot id and platform",
        )),
    }
}

/// Stage a host copy in an exclusively created owner-only file.
/// Its guard removes the file and owned directory on every outcome.
pub(super) fn stage_host_bytes(bytes: &[u8]) -> Result<StagedHostFile, RunnerError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let directory = std::env::temp_dir().join(format!(
        "effigy-workspace-stage-{pid}",
        pid = std::process::id()
    ));
    match std::fs::create_dir(&directory) {
        Ok(()) => {
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).map_err(
                |error| {
                    RunnerError::task_invocation(format!(
                        "cannot make workspace staging directory `{}` private: {error}",
                        directory.display()
                    ))
                },
            )?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(&directory).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot inspect workspace staging directory `{}`: {error}",
                    directory.display()
                ))
            })?;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
                return Err(RunnerError::task_invocation(format!(
                    "workspace staging directory `{}` is not a private directory this run owns",
                    directory.display()
                )));
            }
        }
        Err(error) => {
            return Err(RunnerError::task_invocation(format!(
                "cannot create workspace staging directory `{}`: {error}",
                directory.display()
            )))
        }
    }
    let path = directory.join(format!(
        "stage-{counter}",
        counter = WORKSPACE_EFFIGY_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    // `create_new` refuses a pre-existing path, so a planted symlink can never
    // receive the bytes; 0600 is the only mode the file ever has.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot stage workspace bytes at `{}`: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot write staged workspace bytes at `{}`: {error}",
            path.display()
        ))
    })?;
    drop(file);
    Ok(StagedHostFile { path, directory })
}

/// A host staging file that removes itself and its run-owned directory.
pub(super) struct StagedHostFile {
    path: PathBuf,
    directory: PathBuf,
}

impl StagedHostFile {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedHostFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod verified_target_tests {
    use super::*;
    use crate::contract_test_support::EnvGuard;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn unbounded_guest_probes_and_copies_keep_the_verified_target() {
        let fixture = effigy_test_support::Fixture::new("workspace-verified-target-").unwrap();
        let bin = fixture.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let capture = fixture.path().join("args");
        let backend = bin.join("docker");
        std::fs::write(
            &backend,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf 'boot=boot-a\\nplatform=x86_64\\nlibc=gnu\\nhostname=spoofed\\n'\n",
                shell_quote(&capture.display().to_string()),
            ),
        ).unwrap();
        std::fs::set_permissions(&backend, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        let mut policy = crate::runner::test_support::effective_container_policy(
            "dev",
            "dev",
            "app",
            fixture.path().join("compose.yml"),
        );
        policy.repo_root = fixture.path().to_owned();
        let target = "a1".repeat(32);
        let mut guest = ComposeWorkspaceGuest {
            repo_root: fixture.path(),
            policy: &policy,
            command: GuestCommand::Unbounded {
                target_id: target.clone(),
            },
        };
        assert_eq!(
            guest.probe_container_identity().unwrap().container_id,
            target
        );
        let args = std::fs::read_to_string(&capture).unwrap();
        assert!(args.lines().any(|arg| arg == target));
        assert!(!args.lines().any(|arg| arg == "app"));
        let source = fixture.path().join("artifact");
        std::fs::write(&source, "pinned bytes").unwrap();
        guest.stage(&source, "/run/effigy/staged").unwrap();
        let args = std::fs::read_to_string(&capture).unwrap();
        assert!(args
            .lines()
            .any(|arg| arg == format!("{target}:/run/effigy/staged")));
        assert!(!args.contains("app:/run/effigy/staged"));
    }
}
