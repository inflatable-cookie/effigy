//! Explicit feature-only external I/O double. No kernel receipt, reservation,
//! seal, selection or serving owner is fabricated here. Never native evidence.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use effigy_containers::environment::{BackendSelection, EnvError};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::id::Digest;
use effigy_kernel::observe::{Evidence, Observed, UnknownReason};
use effigy_kernel::workspace_session::{ContainerIdentity, GuestObject};
use serde::Deserialize;

use crate::runner::container_command::reserved_input::{
    PublicationDeadline, ReservedAddressing, ReservedBackendObservation, VerifiedBinding,
};
use crate::runner::container_runtime_prep::{ObservedImagePlatform, PredecessorCompatibility};
use crate::runner::error::RunnerError;
use crate::runner::workspace_client::context::v2::WorkspaceContextV2;
use crate::runner::workspace_client::context::{
    GuestReader, GuestWorkspace, WorkspaceContainerIdentity,
};
use crate::runner::workspace_client::guest_fake::FakeGuest;
use crate::runner::workspace_client::identity::GuestSelfObservation;
use crate::runner::workspace_command::provisioning::reserved_guest::WorkspaceGuestProbe;
use crate::runner::workspace_command::provisioning::LinuxWorkspaceTarget;
use crate::runner::workspace_transport::qualification::{
    GuestChannelProbe, GuestDialEvidence, ResolvedAlias,
};
use crate::runner::workspace_transport::TlsChannelTransport;

const CONFIG: &str = "EFFIGY_WORKSPACE_ACTIVATION_FAKE";
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    artifact: PathBuf,
    applied: PathBuf,
    export: PathBuf,
    container_id: String,
    boot_id: String,
    machine: String,
    architecture: String,
    libc: String,
    #[serde(default)]
    fail_handoff: bool,
    #[serde(default)]
    fail_adoption_cleanup: bool,
    #[serde(default)]
    delay_final_revalidation: bool,
    #[serde(default)]
    fail_publication: bool,
    #[serde(default)]
    fail_binding: bool,
    #[serde(default)]
    fail_liveness: bool,
    #[serde(default)]
    fail_binary_staging: bool,
}

pub(super) fn configured() -> bool {
    std::env::var_os(CONFIG).is_some()
}
pub(super) fn fail_handoff() -> bool {
    configured() && config().is_ok_and(|config| config.fail_handoff)
}
pub(super) fn fail_adoption_cleanup() -> bool {
    configured() && config().is_ok_and(|config| config.fail_adoption_cleanup)
}
pub(super) fn binding_failure() -> bool {
    config().is_ok_and(|c| c.fail_binding)
}
pub(super) fn integration_budget() -> Duration {
    if config().is_ok_and(|c| c.delay_final_revalidation) {
        Duration::from_secs(2)
    } else {
        super::integrations::DEFAULT_INTEGRATION_BUDGET
    }
}
pub(super) fn delay_final_revalidation(until: std::time::Instant) {
    if let Ok(c) = config() {
        if c.delay_final_revalidation {
            let _ = std::fs::write(
                c.export.with_extension("revalidation"),
                b"entered with live integration budget",
            );
            std::thread::sleep(
                until.saturating_duration_since(std::time::Instant::now())
                    + Duration::from_millis(1),
            );
        }
    }
}
pub(super) fn record_unwind(
    step: crate::runner::container_command::workspace_integrations::WorkspaceIntegrationStep,
) {
    use std::io::Write;
    if let Ok(c) = config() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(c.export.with_extension("unwind"))
        {
            let _ = writeln!(f, "{step:?}");
        }
    }
}
fn failure() -> RunnerError {
    RunnerError::task_invocation("workspace fake I/O unavailable")
}
pub(super) fn diagnostic(message: &'static str) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(config) = std::env::var_os(CONFIG) {
        let path = PathBuf::from(config).with_extension("failure");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}
fn config() -> Result<Config, RunnerError> {
    let path = std::env::var_os(CONFIG)
        .ok_or_else(|| RunnerError::task_invocation("fake configuration environment absent"))?;
    serde_json::from_slice(&std::fs::read(path).map_err(|_| failure())?).map_err(|_| failure())
}
fn evidence() -> Evidence {
    Evidence::new("explicit workspace fake backend I/O".into(), None).expect("fixed evidence")
}
fn unknown<T>() -> Observed<T> {
    Observed::Unknown(UnknownReason::Inconsistent(
        "workspace fake backend observation unavailable".into(),
    ))
}
fn verified<T>(value: T) -> Observed<T> {
    Observed::Verified(value, evidence())
}
fn identity(config: &Config) -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: config.container_id.clone(),
        vm_boot_id: config.boot_id.clone(),
        platform: config.machine.clone(),
        libc: config.libc.clone(),
    }
}

struct GuestState {
    guest: FakeGuest,
    written: BTreeMap<String, Vec<u8>>,
    export: PathBuf,
}
type Shared = Arc<Mutex<GuestState>>;
fn shared() -> Result<Shared, RunnerError> {
    static GUEST: OnceLock<Shared> = OnceLock::new();
    if let Some(guest) = GUEST.get() {
        return Ok(Arc::clone(guest));
    }
    let config = config()?;
    let guest = Arc::new(Mutex::new(GuestState {
        guest: FakeGuest::new().with_container(identity(&config)),
        written: BTreeMap::new(),
        export: config.export,
    }));
    Ok(Arc::clone(GUEST.get_or_init(|| guest)))
}

pub(super) struct Observation {
    selected: BackendSelection,
    guest: Shared,
}
impl Observation {
    pub(super) fn new(policy: &EffectiveContainerPolicy) -> Result<Self, RunnerError> {
        let checkout = super::cli_preflight::read_checkout(&policy.repo_root)?;
        Ok(Self {
            selected: super::cli_preflight::read_backend(policy, checkout)?,
            guest: shared()?,
        })
    }
}
impl ReservedBackendObservation for Observation {
    fn selection(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: PublicationDeadline,
    ) -> Observed<BackendSelection> {
        verified(self.selected.clone())
    }
    fn addressing(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &BackendSelection,
        _: PublicationDeadline,
    ) -> Observed<ReservedAddressing> {
        verified(ReservedAddressing {
            proxy_bind: Some("127.0.0.1".parse().expect("loopback")),
            colima_gateway: Some("192.168.5.2".parse().expect("fixture gateway")),
        })
    }
    fn predecessor(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: PublicationDeadline,
    ) -> Observed<PredecessorCompatibility> {
        verified(PredecessorCompatibility::NoPredecessor)
    }
    fn primary(
        &mut self,
        policy: &EffectiveContainerPolicy,
        _: &BackendSelection,
        deadline: PublicationDeadline,
    ) -> Observed<Option<VerifiedBinding>> {
        let Ok(config) = config() else {
            return unknown();
        };
        if !config.applied.exists() {
            return verified(None);
        }
        match self.binding(policy, deadline) {
            Observed::Verified(binding, e) => Observed::Verified(Some(binding), e),
            _ => unknown(),
        }
    }
    fn binding(
        &mut self,
        policy: &EffectiveContainerPolicy,
        deadline: PublicationDeadline,
    ) -> Observed<VerifiedBinding> {
        let Ok(config) = config() else {
            return unknown();
        };
        // The engine inventory still crosses the normal bounded backend API.
        // Creator attribution is subsequently read from the REAL apply journal.
        match self.selected.adapter().inventory_until(
            &policy.repo_root,
            &policy.project_name,
            deadline.at(),
        ) {
            Observed::Verified(rows, _) => {
                let rows: Vec<_> = rows
                    .into_iter()
                    .filter(|row| row.id == config.container_id)
                    .collect();
                if rows.len() != 1 {
                    return unknown();
                }
                verified(VerifiedBinding {
                    container_id: config.container_id,
                    selected: self.selected.clone(),
                    workload: rows.into_iter().next(),
                })
            }
            _ => unknown(),
        }
    }
    fn identity(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &BackendSelection,
        target: &str,
        _: PublicationDeadline,
    ) -> Observed<WorkspaceContainerIdentity> {
        match config() {
            Ok(config) if target == config.container_id => verified(identity(&config)),
            _ => unknown(),
        }
    }
    fn image_platform(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: &str,
        _: PublicationDeadline,
    ) -> Observed<ObservedImagePlatform> {
        match config() {
            Ok(config) => verified(ObservedImagePlatform {
                os: "linux".into(),
                architecture: config.architecture,
                variant: String::new(),
            }),
            _ => unknown(),
        }
    }
    fn artifact(
        &mut self,
        _: &EffectiveContainerPolicy,
        _: LinuxWorkspaceTarget,
        _: PublicationDeadline,
    ) -> Observed<PathBuf> {
        match config() {
            Ok(config) => verified(config.artifact),
            _ => unknown(),
        }
    }
    fn guest<'a>(
        &'a mut self,
        _: &'a EffectiveContainerPolicy,
        _: &ContainerIdentity,
        _: PublicationDeadline,
    ) -> Result<Box<dyn GuestWorkspace + 'a>, EnvError> {
        Ok(Box::new(SharedGuest(Arc::clone(&self.guest))))
    }
}

struct SharedGuest(Shared);
macro_rules! delegate {
    ($name:ident($($arg:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&mut self, $($arg: $ty),*) -> Result<$result, RunnerError> {
            self.0.lock().map_err(|_| failure())?.guest.$name($($arg),*)
        }
    }
}
impl GuestReader for SharedGuest {
    delegate!(read_file(path: &str) -> Option<Vec<u8>>);
    delegate!(file_sha256(path: &str) -> Option<String>);
}
impl SharedGuest {
    fn write(
        &mut self,
        path: &str,
        bytes: &[u8],
        mode: u32,
        exclusive: bool,
    ) -> Result<(), RunnerError> {
        if config().is_ok_and(|c| c.fail_publication) && path.ends_with("/context.json") {
            return Err(RunnerError::task_invocation(
                "adversarial-secret-backend-diagnostic",
            ));
        }
        let mut state = self.0.lock().map_err(|_| failure())?;
        if exclusive {
            state.guest.write_no_replace(path, bytes, mode)?;
        } else {
            state.guest.write_file(path, bytes, mode)?;
        }
        // Private fixture representation of guest-only files. Never printed or
        // included in a startup acknowledgement; removed with the test fixture.
        state.written.insert(path.into(), bytes.to_vec());
        let bytes = serde_json::to_vec(&state.written).map_err(|_| failure())?;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&state.export)
            .map_err(|_| failure())?;
        file.write_all(&bytes).map_err(|_| failure())
    }
}
impl GuestWorkspace for SharedGuest {
    delegate!(read_link(path: &str) -> Option<String>);
    delegate!(ensure_dir(path: &str, mode: u32) -> ());
    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        self.write(path, bytes, mode, false)
    }
    fn write_no_replace(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError> {
        self.write(path, bytes, mode, true)
    }
    delegate!(install_no_replace(staging: &str, path: &str) -> ());
    delegate!(set_owner(path: &str, owner: &str) -> ());
    fn stage_host_file(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError> {
        if config().is_ok_and(|c| c.fail_binary_staging) {
            return Err(RunnerError::task_invocation(
                "adversarial-secret-backend-diagnostic",
            ));
        }
        self.0
            .lock()
            .map_err(|_| failure())?
            .guest
            .stage_host_file(host_path, guest_path)
    }
    delegate!(install_executable(staging: &str, path: &str) -> ());
    delegate!(replace_pointer(path: &str, target: &str) -> ());
    delegate!(replace_link(path: &str, target: &str) -> ());
    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError> {
        if config().is_ok_and(|c| c.fail_liveness) {
            return Err(RunnerError::task_invocation(
                "adversarial-secret-backend-diagnostic",
            ));
        }
        self.0
            .lock()
            .map_err(|_| failure())?
            .guest
            .probe_container_identity()
    }
}

pub(super) struct Probe {
    target: ContainerIdentity,
    guest: Shared,
}
impl Probe {
    pub(super) fn new(target: &ContainerIdentity) -> Result<Self, RunnerError> {
        Ok(Self {
            target: target.clone(),
            guest: shared()?,
        })
    }
}
impl GuestChannelProbe for Probe {
    fn resolve(
        &mut self,
        target: &WorkspaceContainerIdentity,
        alias: &str,
        _: Duration,
    ) -> Observed<ResolvedAlias> {
        if alias != "effigy-host" || target.container_id != self.target.container_id {
            return unknown();
        }
        verified(ResolvedAlias {
            target: target.clone(),
            address: "127.0.0.1".parse().expect("loopback"),
        })
    }
    fn dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        let Some(port) = endpoint
            .strip_prefix("effigy-host:")
            .and_then(|port| port.parse::<u16>().ok())
        else {
            return unknown();
        };
        let peer = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        // Real TLS-only handshake against the actual session-owned listener.
        match TlsChannelTransport::connect(peer, pin.clone(), budget) {
            Ok(transport) => {
                drop(transport);
                verified(GuestDialEvidence {
                    target: target.clone(),
                    peer,
                    pin: pin.clone(),
                })
            }
            Err(_) => unknown(),
        }
    }
}
impl WorkspaceGuestProbe for Probe {
    fn identity(
        &mut self,
        target: &WorkspaceContainerIdentity,
        _: &GuestObject,
        _: Duration,
    ) -> Observed<GuestSelfObservation> {
        let Ok(config) = config() else {
            return unknown();
        };
        let actual = identity(&config);
        if actual != *target {
            return unknown();
        }
        verified(GuestSelfObservation {
            container_id: actual.container_id,
            vm_boot_id: actual.vm_boot_id,
            platform: actual.platform,
            libc: actual.libc,
        })
    }
    fn context_dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        context: &GuestObject,
        budget: Duration,
    ) -> Observed<GuestDialEvidence> {
        let bytes = match self
            .guest
            .lock()
            .ok()
            .and_then(|mut state| state.guest.read_file(&context.path).ok().flatten())
        {
            Some(bytes) => bytes,
            None => return unknown(),
        };
        if Digest::of(&bytes) != context.digest {
            return unknown();
        }
        let document = match WorkspaceContextV2::parse(&bytes) {
            Ok(document) => document,
            Err(_) => return unknown(),
        };
        if document.artifact_digest != format!("sha256:{}", object.digest.hex()) {
            return unknown();
        }
        let Some(pin) = document
            .locator
            .pin
            .strip_prefix("sha256:")
            .and_then(Digest::parse_hex)
        else {
            return unknown();
        };
        self.dial(target, &document.locator.endpoint, &pin, budget)
    }
}

pub(super) fn activation_failure<E>(
    failure: &crate::runner::environment::workspace::integrated::IntegratedFailure<'_, E>,
) {
    use crate::runner::environment::workspace::integrated::IntegratedFailure;
    use crate::runner::environment::workspace::phase_a::PhaseAFailure;
    use crate::runner::environment::workspace::phase_b::PublicationFailure;
    let stage = match failure {
        IntegratedFailure::PhaseA(PhaseAFailure::BeforeAdmission(_)) => "before admission",
        IntegratedFailure::PhaseA(PhaseAFailure::Admission(_)) => "admission",
        IntegratedFailure::PhaseA(PhaseAFailure::Guarded { .. }) => "sealed apply",
        IntegratedFailure::Guarded { .. } => "bind/reacquire",
        IntegratedFailure::Parked { .. } => "integrations/resume",
        IntegratedFailure::Cleanup(_) => "guarded cleanup",
        IntegratedFailure::CleanupOnly { .. } => "cleanup integrations",
        IntegratedFailure::ParkedCleanup { .. } => "parked cleanup",
        IntegratedFailure::Publication {
            cause: PublicationFailure::Guarded { .. },
            ..
        } => "publication/selection",
        IntegratedFailure::Publication {
            cause: PublicationFailure::Finishing { .. },
            ..
        } => "selected finish",
    };
    diagnostic(stage);
}
