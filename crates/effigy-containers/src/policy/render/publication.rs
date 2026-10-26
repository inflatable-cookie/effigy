use super::*;
use effigy_kernel::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, Limits, OperationContext, Placement,
};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
    Route, RunId, Surface, TaskId,
};
use effigy_kernel::objects::Objects;
use effigy_kernel::observe::{AbsenceProof, Observed, UnknownReason};
use effigy_kernel::outcome::{CleanupOutcome, Invocation};
use effigy_kernel::run::{BeginOpts, Effect, Fence, Reconciled, Run, SettlementAdapter};
use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) fn state_home() -> Result<PathBuf, ContainerPolicyError> {
    let home = crate::effigy_home().ok_or_else(|| error("HOME is required"))?;
    // Test overrides may name a private directory directly. Never escape it.
    Ok(if home.file_name() == Some(OsStr::new(".effigy")) {
        home.parent()
            .ok_or_else(|| error("invalid Effigy home"))?
            .into()
    } else {
        home
    })
}

pub(super) fn object_directory(repo_root: &Path) -> Result<PathBuf, ContainerPolicyError> {
    // Render objects are checkout state: a nested catalog directory publishes
    // into its checkout's journal, never a journal of its own (contract 052
    // §7.1).
    let checkout = effigy_core::worktree_scope::checkout_root(repo_root);
    let root = effigy_kernel::fs::DirFd::open(&checkout).map_err(error)?;
    Ok(root.path().join(".effigy/state/objects"))
}

/// Refuse an unresolved legacy cutover before minting checkout identity or
/// allocating render inputs. This is Prepare, never an Observe side effect.
pub(super) fn prepare_identity(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        return Err(error(
            "host authority is required for checkout runtime state",
        ));
    }
    let cx = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Prepare,
        Placement::Host,
    )
    .effects()
    .map_err(|_| error("Prepare capability refused"))?;
    let root = crate::environment::state_root(&cx, &policy.repo_root).map_err(error)?;
    let token =
        effigy_core::worktree_scope::prepare_checkout_token(&policy.repo_root).map_err(error)?;
    effigy_kernel::gate::Gate::prepare_checkout_fence(&cx, &root, &token, Wait::FailFast)
        .map_err(error)?;
    Ok(())
}

pub fn prepare(policy: &EffectiveContainerPolicy) -> Result<(), ContainerPolicyError> {
    let Some(render) = &policy.render else {
        return Ok(());
    };
    prepare_identity(policy)?;
    // This is the Prepare stage of a Mutate command. Gate(Env) deliberately
    // requires that command class; Observe render never enters this path.
    // The backend is recorded under the same owner gate, before any engine
    // effect (K3); a v0.14 runtime file is imported here, never rewritten.
    let backend = crate::backend::metadata_to_record(policy);
    with_owner(policy, EffectClass::Mutate, |run| {
        Objects::publish_desired(run, &render.closure)?;
        match &backend {
            Some(backend) => run.set_backend(backend),
            None => Ok(()),
        }
    })?;
    // A shared service belongs to no checkout, so its render is published
    // outside this generation, at one machine-wide content-addressed directory
    // every checkout that shares it computes (effigy#377).
    for shared in &render.shared {
        publish_shared_render(shared)?;
    }
    Ok(())
}

/// Write a shared render's files where its digest names them. Every file is
/// content-addressed, so an identical file already there is left alone and a
/// concurrent writer of the same bytes writes the same bytes (through a
/// temporary file in the same directory, renamed into place).
fn publish_shared_render(shared: &crate::SharedRender) -> Result<(), ContainerPolicyError> {
    for (relative, bytes) in &shared.files {
        let path = shared.directory.join(relative);
        if std::fs::read(&path).is_ok_and(|existing| existing == *bytes) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ContainerPolicyError::Read {
                path: parent.into(),
                error,
            })?;
        }
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&temporary, bytes).map_err(|error| ContainerPolicyError::Read {
            path: temporary.clone(),
            error,
        })?;
        std::fs::rename(&temporary, &path).map_err(|error| ContainerPolicyError::Read {
            path: path.clone(),
            error,
        })?;
    }
    Ok(())
}

/// The environment's recorded backend. Observe only: an unprepared checkout
/// or missing store is `Absent`, an unreadable one `Unknown`.
pub(crate) fn recorded_backend(
    repo_root: &Path,
    name: &str,
) -> Observed<Option<effigy_kernel::store::BackendMetadata>> {
    let Ok(Some(_)) = effigy_core::worktree_scope::checkout_token(repo_root) else {
        return Observed::Absent(AbsenceProof::NoFile {
            name: "checkout token".into(),
        });
    };
    let Ok(checkout) = checkout_for_root(repo_root) else {
        return Observed::Unknown(UnknownReason::Inconsistent("checkout identity".into()));
    };
    let Ok(id) = ResourceId::environment(checkout, name) else {
        return Observed::Unknown(UnknownReason::Inconsistent("environment identity".into()));
    };
    let context = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(10)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    );
    let Ok(cx) = context.observe() else {
        return Observed::Unknown(UnknownReason::Inconsistent("observe capability".into()));
    };
    let root = match crate::environment::observe_root(&cx, repo_root) {
        Observed::Verified(root, _) => root,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let reader = match effigy_kernel::store::Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    match reader.resource(&id) {
        Observed::Verified(record, evidence) => Observed::Verified(record.backend, evidence),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

pub fn reset(policy: &EffectiveContainerPolicy) -> Result<(), ContainerPolicyError> {
    if policy.render.is_none() {
        return Ok(());
    }
    prepare_identity(policy)?;
    with_owner(policy, EffectClass::Mutate, Objects::reset)
}

// This owner journals only object publication. Kernel staging settlement is
// internal; an unexpected external effect must remain unknown, never clean.
struct ObjectEffects;
impl SettlementAdapter for ObjectEffects {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected render effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected render effect".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unexpected render effect".into(),
        ))
    }
}

fn with_owner(
    policy: &EffectiveContainerPolicy,
    class: EffectClass,
    effect: impl FnOnce(&mut Run<'_>) -> Result<(), effigy_kernel::run::RunError>,
) -> Result<(), ContainerPolicyError> {
    let checkout = checkout(policy)?;
    let context = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        class,
        Placement::Host,
    );
    let cx = context
        .effects()
        .map_err(|_| error("Prepare capability refused"))?;
    let root = crate::environment::state_root(&cx, &policy.repo_root).map_err(error)?;
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout, "", Surface::Published, "container-render", None)
            .map_err(error)?,
        route: Route::Task,
        argv: vec![],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: ResourceId::environment(checkout, &policy.name).map_err(error)?,
        gates: vec![],
        attachments: vec![],
        class,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .map_err(error)?;
    let mut entropy = [0; 10];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut entropy))
        .map_err(error)?;
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_millis();
    let id = RunId::new(u64::try_from(milliseconds).map_err(error)?, entropy).map_err(error)?;
    let cleanup = context
        .cleanup(Duration::from_secs(30))
        .map_err(|_| error("cleanup budget overflow"))?;
    let mut adapter = ObjectEffects;
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id,
            wait: Wait::Until(Instant::now() + Duration::from_secs(30)),
            acknowledge: vec![],
            // Fixed, known CLI grammar; no external argument is accepted here.
            next: Invocation::validated(
                vec![
                    "effigy".into(),
                    "container".into(),
                    "up".into(),
                    "--detach".into(),
                ],
                |_| true,
            )
            .map_err(|_| error("invalid recovery command"))?,
            custody: None,
        },
        &cleanup,
        &mut adapter,
    )
    .map_err(error)?;
    let result = effect(&mut run).map_err(error);
    let outcome = run.settle(&cleanup, &mut adapter).map_err(error)?;
    if outcome.cleanup() != CleanupOutcome::Complete {
        return Err(error("render publication has unresolved cleanup"));
    }
    result
}

pub(crate) fn checkout(
    policy: &EffectiveContainerPolicy,
) -> Result<CheckoutId, ContainerPolicyError> {
    checkout_for_root(&policy.repo_root)
}

/// The checkout identity Prepare minted for `repo_root`; never mints one.
pub(crate) fn checkout_for_root(repo_root: &Path) -> Result<CheckoutId, ContainerPolicyError> {
    let token = effigy_core::worktree_scope::checkout_token(repo_root)
        .map_err(error)?
        .ok_or_else(|| error("no checkout runtime state; Prepare must establish identity first"))?;
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16).map_err(error)?;
    }
    let kind = match effigy_core::worktree_scope::scope_kind(repo_root).map_err(error)? {
        Some(effigy_core::worktree_scope::ScopeKind::LinkedWorktree) => {
            CheckoutKind::LinkedWorktree
        }
        Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone) => {
            CheckoutKind::EphemeralClone
        }
        None => CheckoutKind::Primary,
    };
    Ok(CheckoutId::new(kind, bytes))
}

pub fn verify(policy: &EffectiveContainerPolicy) -> Result<(), ContainerPolicyError> {
    let render = policy
        .render
        .as_ref()
        .ok_or_else(|| error("missing render generation"))?;
    let context = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Observe,
        Placement::Host,
    );
    let cx = context
        .observe()
        .map_err(|_| error("Observe capability refused"))?;
    let root_observation = crate::environment::observe_root(&cx, &policy.repo_root);
    let root = root_observation
        .verified()
        .ok_or_else(|| error("object store is absent or unavailable"))?;
    let id = ResourceId::environment(checkout(policy)?, &policy.name).map_err(error)?;
    let object_observation =
        Objects::resolve(&cx, root, &id, effigy_kernel::objects::Role::Desired);
    let object = object_observation
        .verified()
        .ok_or_else(|| error("desired generation is absent or unavailable"))?;
    if object.closure.digest() != render.closure.digest() || object.path != render.path {
        return Err(error("desired environment generation changed"));
    }
    Ok(())
}
