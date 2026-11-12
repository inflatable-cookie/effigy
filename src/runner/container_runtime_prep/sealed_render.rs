//! Sealing one frozen render for a read-only preflight: a private, verified
//! tree whose bytes are the only ones the backend may hash, and the receipt the
//! approved publisher promotes under `Gate(Env)`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;
use crate::runner::workspace_command::provisioning::BoundedReadRefusal;

/// The verified private seal of one frozen render, for a read-only preflight.
///
/// The kernel owner (`Objects::seal_for_preflight`) stages the closure in a
/// private tree, `O_EXCL`/0600, verifies it with the kernel's own closure
/// verifier inside the deadline, and returns a receipt. Nothing canonical is
/// written, no gate is taken and no desired state changes; only
/// `Objects::publish_sealed_desired` under `Gate(Env)` can make those bytes
/// canonical.
pub(in crate::runner) struct SealedRender {
    // Read by the handoff accessors below; the one apply consumes the receipt.
    #[allow(dead_code)]
    /// The kernel's verified receipt. Retaining it keeps the private tree leased
    /// for as long as this value lives, and it is the only thing the approved
    /// publisher (`Objects::publish_sealed_desired`) will promote.
    receipt: effigy_kernel::objects::SealedPreflight,
    /// A clone of the policy whose Compose inputs name the sealed tree, so the
    /// backend hashes exactly the bytes the apply will publish.
    policy: EffectiveContainerPolicy,
    /// The sealed tree's root, from the kernel's documented staging layout
    /// `state/staging/preflight-<token>/<digest>`.
    root: PathBuf,
}

#[allow(dead_code)] // The consuming caller (#452) reads these in its gated apply.
impl SealedRender {
    /// The kernel receipt this preflight verified. The caller publishes it with
    /// `Objects::publish_sealed_desired` under `Gate(Env)`, on the one apply.
    pub(in crate::runner) fn receipt(&self) -> &effigy_kernel::objects::SealedPreflight {
        &self.receipt
    }

    /// The frozen closure the receipt describes.
    pub(in crate::runner) fn closure(&self) -> Option<&effigy_kernel::objects::Closure> {
        self.policy.render.as_ref().map(|render| &render.closure)
    }

    /// The policy whose Compose inputs name the sealed tree. The compatibility
    /// observation reads this, so it never stages a second seal.
    pub(in crate::runner) fn policy(&self) -> &EffectiveContainerPolicy {
        &self.policy
    }

    /// The sealed tree the receipt names.
    pub(in crate::runner) fn root(&self) -> &Path {
        &self.root
    }
}

/// Seal this policy's frozen render for the preflight, inside the publication's
/// one deadline and without any authority a Prepare has.
pub(in crate::runner) fn seal_render_for_preflight(
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<SealedRender, BoundedReadRefusal> {
    use effigy_kernel::ctx::{
        CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement,
    };

    let live = |phase: &str| {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                BoundedReadRefusal::Unavailable(format!(
                    "the render preflight deadline expired {phase}"
                ))
            })
    };
    live("before sealing the render")?;
    let Some(render) = policy.render.as_ref() else {
        return Err(BoundedReadRefusal::Unavailable(
            "the workspace render has no frozen closure to seal".into(),
        ));
    };
    let cx = OperationContext::new(
        Deadline::At(deadline),
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
    .map_err(|_| {
        BoundedReadRefusal::Unavailable(
            "the render preflight cannot take a Prepare capability".into(),
        )
    })?;
    // Opening the state root walks and opens directories, and the seal stages and
    // verifies files. Both are synchronous, so the pair runs on the owner's
    // bounded worker: the caller stops waiting at the deadline. An abandoned seal
    // only writes a private, leased staging tree that recovery reclaims; nothing
    // canonical can come of it, because only a returned receipt can be published.
    let closure = render.closure.clone();
    let token_deadline = deadline;
    let repo_root = policy.repo_root.clone();
    let (sealed, sealed_root) =
        match crate::runner::workspace_command::provisioning::bounded_publication_read(
            "workspace-render-seal",
            deadline,
            move || {
                let root = effigy_containers::environment::state_root(&cx, repo_root.as_path())
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                let sealed = effigy_kernel::objects::Objects::seal_for_preflight(
                    &cx,
                    &root,
                    &closure,
                    token_deadline,
                )
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
                // The receipt names no canonical object; its private tree follows
                // the documented layout `staging/preflight-<token>/<digest>`. A
                // wrong guess is fail-closed: Compose cannot read it, so the guard
                // refuses.
                let sealed_root = root
                    .state_directory()
                    .path()
                    .join("staging")
                    .join(format!("preflight-{}", sealed.token()))
                    .join(sealed.digest());
                Ok((sealed, sealed_root))
            },
        ) {
            Ok(pair) => pair,
            Err(crate::runner::workspace_command::provisioning::BoundedReadRefusal::TimedOut) => {
                return Err(BoundedReadRefusal::TimedOut)
            }
            Err(
                crate::runner::workspace_command::provisioning::BoundedReadRefusal::Unavailable(
                    reason,
                ),
            ) => return Err(BoundedReadRefusal::Unavailable(reason)),
        };
    let mut policy = policy.clone();
    let frozen = render.closure.files().clone();
    let compose_files = policy.compose_files.clone();
    let render_path = render.path.clone();
    let read_root = sealed_root.clone();
    // The trusted private tree still lives on a filesystem that can stall. A
    // returned seal does not make subsequent synchronous reads deadline-safe.
    // An abandoned verifier only reads; it cannot promote or mutate the tree.
    policy.compose_files =
        crate::runner::workspace_command::provisioning::bounded_publication_read(
            "workspace-compose-inputs",
            deadline,
            move || {
                verified_compose_inputs(&frozen, &compose_files, &render_path, &read_root)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            },
        )?;
    live("after sealing the render")?;
    Ok(SealedRender {
        receipt: sealed,
        policy,
        root: sealed_root,
    })
}

/// Verify that the frozen closure owns every Compose input the apply will pass,
/// then return those inputs pointed at the sealed private tree.
///
/// The 2026-11-01 Supervisor ruling freezes `checkout/.effigy/runtime/compose/
/// <name>.compose.yml` in the closure with the rest of the render, so a generated
/// input is not a special case: an input missing from the closure, an input
/// outside the rendered checkout, an absent sealed file and a sealed file whose
/// bytes differ all refuse by name. Nothing is hashed or applied from a path the
/// closure does not vouch for.
pub(super) fn verified_compose_inputs(
    frozen: &std::collections::BTreeMap<String, Vec<u8>>,
    compose_files: &[std::path::PathBuf],
    render_path: &Path,
    sealed_root: &Path,
) -> Result<Vec<std::path::PathBuf>, BoundedReadRefusal> {
    let mut selected = Vec::with_capacity(compose_files.len());
    for file in compose_files {
        let Ok(relative) = file.strip_prefix(render_path) else {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-outside-render-closure: `{}` is not under the rendered \
                 checkout, so the frozen closure cannot carry it",
                file.display()
            )));
        };
        // The admitted Compose inputs already carry the render's own `checkout/`
        // prefix, which is exactly how the render builder keys them in the
        // closure, so the render-relative path is the closure key and the sealed
        // path.
        let name = relative.to_string_lossy().replace('\\', "/");
        let Some(frozen_bytes) = frozen.get(name.as_str()) else {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-outside-render-closure: `{name}` is not part of the \
                 frozen closure, so the preflight cannot hash verified bytes (owner: the render \
                 must freeze every Compose input the apply reads)"
            )));
        };
        // The render is published by the reconcile that follows this preflight, so
        // its own path is not materialised yet: the bytes Compose will read come
        // from the sealed tree, and they must be the frozen ones verbatim.
        let sealed_path = sealed_root.join(&name);
        let sealed_bytes = std::fs::read(sealed_path.as_path()).map_err(|error| {
            BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-unreadable: `{name}` is not in the sealed tree: {error}"
            ))
        })?;
        if sealed_bytes != *frozen_bytes {
            return Err(BoundedReadRefusal::Unavailable(format!(
                "workspace-compose-input-closure-bytes-differ: `{name}` in the sealed tree is not \
                 the byte image the frozen closure holds"
            )));
        }
        selected.push(sealed_path);
    }
    Ok(selected)
}
