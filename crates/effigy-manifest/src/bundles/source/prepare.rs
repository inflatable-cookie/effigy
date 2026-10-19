//! The single source updater. Every acquisition runs under a Prepare owner;
//! one checkout lock selects a complete generation after all sources validate.
use super::*;
use effigy_kernel::{
    ctx::{CleanupContext, EffectContext},
    custody::{Custody, EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal},
    gate::Wait,
    id::{
        CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, ResourceId,
        Route, RunId, Scope, Surface, TaskId,
    },
    observe::UnknownReason,
    outcome::{CleanupOutcome, Invocation, WorkloadOutcome},
    root::StateRoot,
    run::{BeginOpts, Effect, EffectSpec, Fence, Reconciled, Run, Settlement, SettlementAdapter},
    supervise::{Executable, LifecyclePlan},
};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::{
    ffi::OsStrExt,
    fs::{OpenOptionsExt, PermissionsExt},
};

const BUDGET: Duration = Duration::from_secs(120);
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_TREE: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 65_536;

#[derive(Default)]
struct SnapshotUsage {
    bytes: u64,
    entries: usize,
}

#[derive(Debug)]
pub struct SourcePreparation {
    pub home: PathBuf,
    pub executable: PathBuf,
    pub cancel: CancelToken,
    pub next: Invocation,
    pub budget: Duration,
}

/// Only custody effects need recovery calls. Published source generations are
/// retained; an unknown future engine effect is never interpreted as absent.
#[derive(Debug)]
pub struct SourceSettlement;
impl SettlementAdapter for SourceSettlement {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source effect".into(),
        ))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source release".into(),
        ))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent(
            "unrecognized source reconciliation".into(),
        ))
    }
}

/// Explicit sync updates selected sources, after importing any v0.14.1 bytes.
pub fn sync_bundle_source(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, true)
}
/// Bootstrap/first dev only acquire missing sources. Existing pins never move.
pub fn prepare_bundle_sources(
    manifest: &Path,
    preparation: &SourcePreparation,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    prepare(manifest, preparation, false)
}
fn prepare(
    manifest: &Path,
    preparation: &SourcePreparation,
    update: bool,
) -> Result<Option<BundleSyncReport>, ManifestError> {
    static TTL_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if std::env::var_os("EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS").is_some()
        && !TTL_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        eprintln!("[bundle] EFFIGY_GIT_BUNDLE_REMOTE_CHECK_TTL_SECS is retired; only `effigy bundle sync` updates source pins");
    }
    let manifest = std::fs::canonicalize(manifest).map_err(|err| io_error(manifest, err))?;
    let (bundle, catalogs) = crate::composition::load_manifest_source_declarations(&manifest)?;
    let mut selections = BTreeMap::new();
    if let Some(bundle) = &bundle {
        selections.insert(
            "bundle".to_owned(),
            resolve_bundle_selection(&manifest, bundle)?,
        );
    }
    for (name, spec) in catalogs {
        if name.trim().is_empty() || name.contains('/') || name.contains(':') {
            return Err(error(
                &manifest,
                "catalog source names must be non-empty handles without '/' or ':'",
            ));
        }
        selections.insert(
            format!("catalog:{name}"),
            BundleSelection::from_spec(&manifest, &spec)?,
        );
    }
    if selections.is_empty() {
        return Ok(None);
    }
    // No gate, token or file creation for an already prepared first-dev path.
    if !update
        && read_lock(&manifest)?.is_some_and(|lock| {
            selections
                .keys()
                .all(|name| lock.sources.contains_key(name))
        })
    {
        return Ok(None);
    }
    if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
        if !update
            && selections
                .iter()
                .all(|(name, selection)| resolve_source(&manifest, name, selection).is_ok())
        {
            return Ok(None);
        }
        return Err(error(&manifest, "source acquisition requires host authority; run `effigy bundle sync` on the host before entering the workspace"));
    }
    let deadline = Instant::now()
        .checked_add(preparation.budget)
        .ok_or_else(|| error(&manifest, "source preparation deadline is out of range"))?;
    let op = OperationContext::new(
        Deadline::At(deadline),
        preparation.cancel.clone(),
        Limits {
            capture_tail: 1024 * 1024,
            log_rotate: 1024 * 1024,
            log_keep: 2,
            line_max: 64 * 1024,
            queue_bytes: 1024 * 1024,
            probe_concurrency: 1,
        },
        EffectClass::Prepare,
        Placement::Host,
    );
    let cx = op
        .effects()
        .map_err(|err| error(&manifest, format!("{err:?}")))?;
    let root = StateRoot::effects(&cx, &preparation.home)
        .map_err(|err| error(&manifest, err.to_string()))?;
    let checkout = CheckoutId::new(CheckoutKind::Machine, [0; 16]);
    let owner = ResourceId::store(Scope::Machine, "bundles")
        .map_err(|err| error(&manifest, err.to_string()))?;
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout, "", Surface::Published, "bundle:sync", None)
            .map_err(|err| error(&manifest, err.to_string()))?,
        owner,
        route: Route::Builtin("bundle sync".into()),
        argv: Vec::new(),
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Prepare,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .map_err(|err| error(&manifest, err.to_string()))?;
    let mut random = [0; 10];
    getrandom::fill(&mut random).map_err(|err| error(&manifest, err.to_string()))?;
    let run_id = RunId::new(0, random).map_err(|err| error(&manifest, err.to_string()))?;
    let remote = selections
        .values()
        .any(|selection| !matches!(selection, BundleSelection::Path { .. }));
    let custody = if remote {
        Some(LifecyclePlan {
            executable: Executable::capture(&preparation.executable)
                .map_err(|err| io_error(&preparation.executable, err))?,
            placement: effigy_kernel::supervise::Placement::Inline,
        })
    } else {
        None
    };
    let cleanup = op
        .cleanup(Duration::from_secs(15))
        .map_err(|err| error(&manifest, format!("{err:?}")))?;
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: run_id,
            wait: Wait::Until(deadline),
            acknowledge: Vec::new(),
            next: preparation.next.clone(),
            custody,
        },
        &cleanup,
        &mut SourceSettlement,
    )
    .map_err(|err| error(&manifest, err.to_string()))?;
    let result = acquire_and_publish(
        &manifest,
        &selections,
        update,
        &cx,
        &mut run,
        &preparation.home,
        root.path(),
    );
    // Settlement gets its own fresh budget after acquisition, independent of
    // both the workload deadline and begin's predecessor-recovery budget.
    let cleanup = op
        .cleanup(Duration::from_secs(15))
        .map_err(|err| error(&manifest, format!("{err:?}")))?;
    let completion = if result.is_ok() {
        run.finish(WorkloadOutcome::Succeeded)
    } else {
        match op.check("source preparation") {
            Ok(()) => run.finish(WorkloadOutcome::Failed { code: 1 }),
            Err(effigy_kernel::ctx::Interrupt::TimedOut { phase, .. }) => {
                run.interrupt(&cleanup, WorkloadOutcome::TimedOut { phase })
            }
            Err(effigy_kernel::ctx::Interrupt::Cancelled { cause }) => run.interrupt(
                &cleanup,
                WorkloadOutcome::Cancelled {
                    cause,
                    signal: None,
                },
            ),
        }
    };
    let settled = run
        .settle(&cleanup, &mut SourceSettlement)
        .map_err(|err| error(&manifest, err.to_string()))?;
    if settled.cleanup() != CleanupOutcome::Complete {
        return Err(error(&manifest, "source preparation cleanup is incomplete; retry `effigy bundle sync` to settle the prior run"));
    }
    if let Err(record_error) = completion {
        return Err(error(
            &manifest,
            format!(
                "{}; recording source outcome failed: {record_error}",
                result
                    .as_ref()
                    .err()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "source acquisition completed".into())
            ),
        ));
    }
    result.map(Some)
}
fn source_dir(
    parent: &DirFd,
    name: &str,
    cx: &EffectContext,
    manifest: &Path,
) -> Result<DirFd, ManifestError> {
    let name = FileName::new(name).map_err(|err| error(manifest, err.to_string()))?;
    parent
        .prepare_child(cx, &name)
        .map_err(|err| error(manifest, err.to_string()))
}
fn retained_intent(
    run: &mut Run<'_>,
    kind: &str,
    path: &Path,
    manifest: &Path,
) -> Result<effigy_kernel::run::EffectTicket, ManifestError> {
    run.intend(EffectSpec {
        target: run.fence().clone(),
        kind: kind.into(),
        payload: serde_json::json!({"path":path.to_string_lossy()}),
        settlement: Settlement::Retain,
        required: false,
    })
    .map_err(|err| error(manifest, err.to_string()))
}
fn publish_lock(
    manifest: &Path,
    lock: &SourceLock,
    directory: &DirFd,
    run: &mut Run<'_>,
) -> Result<(), ManifestError> {
    let bytes = serde_json::to_vec_pretty(lock).map_err(|err| error(manifest, err.to_string()))?;
    if bytes.len() as u64 > LOCK_MAX {
        return Err(error(manifest, "source lock exceeds size limit"));
    }
    let ticket = retained_intent(
        run,
        "sources:publish",
        &directory.path().join(LOCK_FILE),
        manifest,
    )?;
    let name = FileName::new(LOCK_FILE).map_err(|err| error(manifest, err.to_string()))?;
    run.replace_record(
        directory,
        &name,
        &bytes,
        effigy_kernel::record::ReplaceOpts {
            mode: 0o644,
            keep_previous: false,
        },
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    run.applied(&ticket, serde_json::json!({"sha256":sha256_hex(&bytes)}))
        .map_err(|err| error(manifest, err.to_string()))
}
fn acquire_and_publish(
    manifest: &Path,
    selections: &BTreeMap<String, BundleSelection>,
    update: bool,
    cx: &EffectContext,
    run: &mut Run<'_>,
    home: &Path,
    store: &Path,
) -> Result<BundleSyncReport, ManifestError> {
    let old = read_lock(manifest)?.unwrap_or(SourceLock {
        version: 1,
        sources: BTreeMap::new(),
    });
    let ticket = retained_intent(run, "sources:generation", parent(manifest), manifest)?;
    let project = DirFd::open(parent(manifest)).map_err(|err| error(manifest, err.to_string()))?;
    let directory = source_dir(&project, ".effigy", cx, manifest)?;
    let cache = source_dir(&directory, "cache", cx, manifest)?;
    let objects = source_dir(&cache, "sources", cx, manifest)?;
    let generation = source_dir(&objects, &run.id().to_string(), cx, manifest)?;
    run.applied(
        &ticket,
        serde_json::json!({"path":generation.path().to_string_lossy()}),
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    let mut imported = old.sources.clone();
    // Preserve the exact pre-upgrade bytes even when the first explicit sync
    // subsequently fails. No legacy cache is changed or removed.
    for (name, selection) in selections {
        if imported.contains_key(name) {
            continue;
        }
        if let Some(legacy) = legacy_root(manifest, selection).filter(|root| root.is_dir()) {
            let object = source_dir(
                &generation,
                &format!("import-{}", sha256_hex(name.as_bytes())),
                cx,
                manifest,
            )?;
            let version = snapshot(&legacy, &object, cx, manifest)?;
            imported.insert(
                name.clone(),
                SourcePin {
                    declaration: selection.spec(),
                    object: relative_object(manifest, object.path())?,
                    version,
                    revision: legacy_version(&legacy, selection),
                },
            );
        }
    }
    if imported.len() != old.sources.len() {
        publish_lock(
            manifest,
            &SourceLock {
                version: 1,
                sources: imported.clone(),
            },
            &directory,
            run,
        )?;
    }
    let mut pins = imported.clone();
    let mut reports = BTreeMap::new();
    let mut changed = false;
    for (name, selection) in selections {
        if !update {
            if let Some(pin) = pins.get(name) {
                reports.insert(name.clone(), pin_report(manifest, selection, pin));
                continue;
            }
        }
        let object = source_dir(&generation, &sha256_hex(name.as_bytes()), cx, manifest)?;
        let (version, revision) = match selection {
            BundleSelection::Path { path } => (snapshot(path, &object, cx, manifest)?, None),
            BundleSelection::Git { url, reference } => {
                let checkout = source_dir(
                    &generation,
                    &format!("git-{}", sha256_hex(name.as_bytes())),
                    cx,
                    manifest,
                )?;
                command(
                    manifest,
                    run,
                    cx,
                    home,
                    parent(manifest),
                    vec![
                        "git".into(),
                        "clone".into(),
                        "--no-checkout".into(),
                        "--".into(),
                        url.into(),
                        checkout.path().as_os_str().into(),
                    ],
                )?;
                command(
                    manifest,
                    run,
                    cx,
                    home,
                    checkout.path(),
                    vec![
                        "git".into(),
                        "fetch".into(),
                        "--depth".into(),
                        "1".into(),
                        "--".into(),
                        "origin".into(),
                        reference.as_deref().unwrap_or("main").into(),
                    ],
                )?;
                command(
                    manifest,
                    run,
                    cx,
                    home,
                    checkout.path(),
                    vec![
                        "git".into(),
                        "-c".into(),
                        "core.hooksPath=/dev/null".into(),
                        "checkout".into(),
                        "--detach".into(),
                        "FETCH_HEAD".into(),
                    ],
                )?;
                (
                    snapshot(checkout.path(), &object, cx, manifest)?,
                    legacy_version(checkout.path(), selection),
                )
            }
            BundleSelection::Oci { url } => {
                let reference = url.trim_start_matches("oci://");
                let raw = command(
                    manifest,
                    run,
                    cx,
                    home,
                    parent(manifest),
                    vec![
                        "oras".into(),
                        "manifest".into(),
                        "fetch".into(),
                        "--descriptor".into(),
                        reference.into(),
                    ],
                )?;
                let descriptor: serde_json::Value = serde_json::from_slice(&raw)
                    .map_err(|err| error(manifest, format!("invalid OCI descriptor: {err}")))?;
                let digest = descriptor
                    .get("digest")
                    .and_then(|value| value.as_str())
                    .filter(|digest| {
                        digest.starts_with("sha256:")
                            && digest.len() == 71
                            && digest[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    .ok_or_else(|| error(manifest, "OCI source has no valid sha256 digest"))?;
                let repository = reference.split('@').next().unwrap_or(reference);
                let repository = match repository.rfind(':') {
                    Some(colon) if repository.rfind('/').is_some_and(|slash| colon > slash) => {
                        &repository[..colon]
                    }
                    _ => repository,
                };
                let pull = source_dir(
                    &generation,
                    &format!("oci-{}", sha256_hex(name.as_bytes())),
                    cx,
                    manifest,
                )?;
                command(
                    manifest,
                    run,
                    cx,
                    home,
                    parent(manifest),
                    vec![
                        "oras".into(),
                        "pull".into(),
                        format!("{repository}@{digest}").into(),
                        "--output".into(),
                        pull.path().as_os_str().into(),
                    ],
                )?;
                (
                    snapshot(pull.path(), &object, cx, manifest)?,
                    Some(digest.into()),
                )
            }
        };
        if name == "bundle" {
            crate::composition::validate_source_snapshot(manifest, object.path())?;
        }
        changed |= pins
            .get(name)
            .is_none_or(|pin| pin.version != version || pin.declaration != selection.spec());
        let pin = SourcePin {
            declaration: selection.spec(),
            object: relative_object(manifest, object.path())?,
            version,
            revision,
        };
        reports.insert(name.clone(), pin_report(manifest, selection, &pin));
        pins.insert(name.clone(), pin);
    }
    // Git exclusions are explicit Prepare effects, never a manifest read effect.
    let exclude = retained_intent(run, "sources:git-exclude", parent(manifest), manifest)?;
    effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root(parent(manifest))
        .map_err(|err| io_error(manifest, err))?;
    effigy_core::runtime_dir::ensure_local_overlay_ignored_in_git_root(parent(manifest))
        .map_err(|err| io_error(manifest, err))?;
    run.applied(&exclude, serde_json::Value::Null)
        .map_err(|err| error(manifest, err.to_string()))?;
    // Other checkouts can report a known newer source without remote probes.
    // This bounded hint is never used to select or acquire source bytes.
    let store_dir = DirFd::open(store).map_err(|err| error(manifest, err.to_string()))?;
    let known_name =
        FileName::new("sources-known.json").map_err(|err| error(manifest, err.to_string()))?;
    let observe = cx
        .operation()
        .observe()
        .map_err(|err| error(manifest, format!("{err:?}")))?;
    let mut known =
        effigy_kernel::record::read_cache::<KnownVersions>(&observe, &store_dir, &known_name)
            .unwrap_or_default();
    for (name, selection) in selections {
        if let Some(pin) = pins.get(name) {
            known.0.insert(
                source_key(selection).map_err(|err| error(manifest, err.to_string()))?,
                pin.version.clone(),
            );
        }
    }
    while known.0.len() > 4096 {
        known.0.pop_first();
    }
    let known_bytes = serde_json::to_vec(&known).map_err(|err| error(manifest, err.to_string()))?;
    if known_bytes.len() as u64 > LOCK_MAX {
        return Err(error(manifest, "source hint inventory exceeds size limit"));
    }
    let known_ticket = retained_intent(
        run,
        "sources:known-versions",
        &store.join("sources-known.json"),
        manifest,
    )?;
    run.replace_record(
        &store_dir,
        &known_name,
        &known_bytes,
        effigy_kernel::record::ReplaceOpts::default(),
    )
    .map_err(|err| error(manifest, err.to_string()))?;
    run.applied(&known_ticket, serde_json::Value::Null)
        .map_err(|err| error(manifest, err.to_string()))?;
    if update {
        pins.retain(|name, _| selections.contains_key(name));
    }
    // One pointer publication commits bundle and catalog updates together.
    publish_lock(
        manifest,
        &SourceLock {
            version: 1,
            sources: pins,
        },
        &directory,
        run,
    )?;
    let primary = reports
        .get("bundle")
        .or_else(|| reports.values().next())
        .ok_or_else(|| error(manifest, "no source reports"))?;
    Ok(BundleSyncReport {
        source_type: primary.source_type,
        source_path: primary.source_path.clone(),
        local_path: Some(primary.local_path.clone()),
        version_hint: primary.version_hint.clone(),
        changed,
        applicable: true,
        sources: reports,
    })
}
fn relative_object(manifest: &Path, root: &Path) -> Result<PathBuf, ManifestError> {
    root.strip_prefix(parent(manifest).join(".effigy"))
        .map(Path::to_path_buf)
        .map_err(|err| error(manifest, err.to_string()))
}
fn pin_report(
    manifest: &Path,
    selection: &BundleSelection,
    pin: &SourcePin,
) -> BundleSourceInspectReport {
    BundleSourceInspectReport {
        source_type: selection.kind(),
        source_path: selection.label(),
        local_path: parent(manifest).join(".effigy").join(&pin.object),
        version_hint: pin.revision.clone().or_else(|| Some(pin.version.clone())),
        stale: pin.declaration != selection.spec(),
    }
}
fn command(
    manifest: &Path,
    run: &mut Run<'_>,
    cx: &EffectContext,
    home: &Path,
    cwd: &Path,
    mut argv: Vec<OsString>,
) -> Result<Vec<u8>, ManifestError> {
    let program = argv
        .first_mut()
        .ok_or_else(|| error(manifest, "source command has no executable"))?;
    if !Path::new(program).is_absolute() {
        let executable = std::env::var_os("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join(&*program))
                    .find(|candidate| {
                        candidate.is_file()
                            && std::fs::metadata(candidate)
                                .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
                    })
            })
            .ok_or_else(|| {
                error(
                    manifest,
                    format!(
                        "source acquisition requires `{}` on PATH",
                        program.to_string_lossy()
                    ),
                )
            })?;
        *program = std::fs::canonicalize(&executable)
            .map_err(|err| io_error(&executable, err))?
            .into_os_string();
    }

    let mut env = std::env::vars_os()
        .filter(|(name, _)| {
            !matches!(
                name.to_str(),
                Some(
                    "GIT_DIR"
                        | "GIT_WORK_TREE"
                        | "GIT_COMMON_DIR"
                        | "GIT_INDEX_FILE"
                        | "GIT_OBJECT_DIRECTORY"
                        | "GIT_ALTERNATE_OBJECT_DIRECTORIES"
                )
            )
        })
        .map(|(name, value)| {
            let upper = name.to_string_lossy().to_ascii_uppercase();
            let secret = ["TOKEN", "PASSWORD", "SECRET", "PRIVATE_KEY"]
                .iter()
                .any(|word| upper.contains(word));
            let value = if secret && !value.is_empty() {
                SecretValue::new(value.as_bytes().to_vec()).map(EnvValue::Secret)
            } else {
                Ok(EnvValue::Plain(value))
            };
            value.map(|value| (name, value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|err| io_error(manifest, err))?;
    env.insert("HOME".into(), EnvValue::Plain(home.as_os_str().into()));
    env.insert("GIT_TERMINAL_PROMPT".into(), EnvValue::Plain("0".into()));
    let mut child = Custody::spawn_source(
        run,
        ProcessSpec {
            argv,
            cwd: cwd.into(),
            env,
            grace: Duration::from_secs(1),
            lifetime: Lifetime::Finite(BUDGET),
            terminal: Terminal::None,
            label: "source acquisition".into(),
        },
        cx,
    )
    .map_err(|err| error(manifest, format!("source acquisition refused: {err:?}")))?;
    match child.wait(cx.operation()) {
        Observed::Verified(exit, _) => {
            let bytes = child.take_output();
            child
                .complete(run, exit)
                .map_err(|err| error(manifest, err.to_string()))?;
            if exit.code != 0 {
                return Err(error(
                    manifest,
                    format!(
                        "source acquisition exited {}: {}",
                        exit.code,
                        String::from_utf8_lossy(&bytes)
                    ),
                ));
            }
            Ok(bytes)
        }
        other => Err(error(
            manifest,
            format!("source acquisition unavailable: {other:?}"),
        )),
    }
}
fn snapshot(
    source: &Path,
    destination: &DirFd,
    cx: &EffectContext,
    manifest: &Path,
) -> Result<String, ManifestError> {
    let mut hash = Sha256::new();
    let mut usage = SnapshotUsage::default();
    copy_tree(
        source,
        destination,
        Path::new(""),
        0,
        &mut usage,
        &mut hash,
        cx,
        manifest,
    )?;
    Ok(format!(
        "sha256:{}",
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}
#[allow(clippy::too_many_arguments)]
fn copy_tree(
    source: &Path,
    destination: &DirFd,
    relative: &Path,
    depth: usize,
    usage: &mut SnapshotUsage,
    hash: &mut Sha256,
    cx: &EffectContext,
    manifest: &Path,
) -> Result<(), ManifestError> {
    if depth > 32 {
        return Err(error(manifest, "source tree exceeds depth limit"));
    }
    cx.operation()
        .check("snapshot source")
        .map_err(|err| error(manifest, format!("{err:?}")))?;
    let mut entries = std::fs::read_dir(source)
        .map_err(|err| io_error(source, err))?
        .take(MAX_ENTRIES + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| io_error(source, err))?;
    if entries.len() > MAX_ENTRIES {
        return Err(error(manifest, "source directory exceeds entry limit"));
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        cx.operation()
            .check("snapshot entry")
            .map_err(|err| error(manifest, format!("{err:?}")))?;
        usage.entries += 1;
        if usage.entries > MAX_ENTRIES {
            return Err(error(manifest, "source tree exceeds entry limit"));
        }
        let name = entry.file_name();
        if depth == 0
            && (name == ".git"
                || name == ".effigy"
                || name.to_string_lossy().starts_with(".effigy-bundle-source."))
        {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|err| io_error(&path, err))?;
        let relative = relative.join(&name);
        let name = name
            .to_str()
            .ok_or_else(|| error(manifest, "source filename is not UTF-8"))?;
        if metadata.is_dir() {
            hash.update([0]);
            hash.update((relative.as_os_str().as_bytes().len() as u64).to_be_bytes());
            hash.update(relative.as_os_str().as_bytes());
            let child = source_dir(destination, name, cx, manifest)?;
            copy_tree(
                &path,
                &child,
                &relative,
                depth + 1,
                usage,
                hash,
                cx,
                manifest,
            )?;
        } else if metadata.is_file() {
            if metadata.len() > MAX_FILE {
                return Err(error(manifest, "source file exceeds size limit"));
            }
            usage.bytes = usage.bytes.saturating_add(metadata.len());
            if usage.bytes > MAX_TREE {
                return Err(error(manifest, "source tree exceeds size limit"));
            }
            let mode = metadata.permissions().mode() & 0o777 & !0o022;
            hash.update([1]);
            hash.update((relative.as_os_str().as_bytes().len() as u64).to_be_bytes());
            hash.update(relative.as_os_str().as_bytes());
            hash.update(mode.to_be_bytes());
            let mut bytes = Vec::new();
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
                .map_err(|err| io_error(&path, err))?;
            file.take(MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(|err| io_error(&path, err))?;
            if bytes.len() as u64 != metadata.len() {
                return Err(error(manifest, "source file changed during snapshot"));
            }
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(&bytes);
            let target = destination.path().join(name);
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode)
                .open(&target)
                .map_err(|err| io_error(&target, err))?;
            output
                .write_all(&bytes)
                .and_then(|()| output.sync_all())
                .map_err(|err| io_error(&target, err))?;
        } else {
            return Err(error(
                manifest,
                format!(
                    "source contains a symlink or special file: {}",
                    path.display()
                ),
            ));
        }
    }
    std::fs::File::open(destination.path())
        .and_then(|directory| directory.sync_all())
        .map_err(|err| io_error(destination.path(), err))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> (PathBuf, SourcePreparation) {
        let bundle = root.join("bundle");
        std::fs::create_dir(&bundle).unwrap();
        std::fs::write(bundle.join("bundle.toml"), "[bundle]\nname = \"fixture\"\n").unwrap();
        std::fs::write(
            bundle.join("effigy.toml"),
            "[tasks.hello]\nrun = \"echo old\"\n",
        )
        .unwrap();
        let manifest = root.join("effigy.toml");
        std::fs::write(&manifest, "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n[catalog.sources.extra]\ntype = \"path\"\ndir = \"bundle\"\n").unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        (
            manifest,
            SourcePreparation {
                home,
                executable: std::env::current_exe().unwrap(),
                cancel: CancelToken::default(),
                budget: Duration::from_secs(10),
                next: Invocation::validated(vec!["recover".into()], |args| args == ["recover"])
                    .unwrap(),
            },
        )
    }
    #[test]
    fn path_and_catalog_pins_move_only_on_sync_and_failure_keeps_prior_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        prepare_bundle_sources(&manifest, &prep).unwrap();
        let original = std::fs::read(tmp.path().join(".effigy").join(LOCK_FILE)).unwrap();
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = \"echo new\"\n",
        )
        .unwrap();
        prepare_bundle_sources(&manifest, &prep).unwrap();
        assert_eq!(
            original,
            std::fs::read(tmp.path().join(".effigy").join(LOCK_FILE)).unwrap()
        );
        let loaded = crate::load_task_manifest_with_inspection(&manifest).unwrap();
        assert!(loaded.effective_manifest.contains("echo old"));
        let report = sync_bundle_source(&manifest, &prep).unwrap().unwrap();
        assert!(report.changed);
        assert_eq!(report.sources.len(), 2);
        assert!(crate::load_task_manifest_with_inspection(&manifest)
            .unwrap()
            .effective_manifest
            .contains("echo new"));
        let previous = std::fs::read(tmp.path().join(".effigy").join(LOCK_FILE)).unwrap();
        std::fs::write(
            tmp.path().join("bundle/effigy.toml"),
            "[tasks.hello]\nrun = 4\n",
        )
        .unwrap();
        assert!(sync_bundle_source(&manifest, &prep).is_err());
        assert_eq!(
            previous,
            std::fs::read(tmp.path().join(".effigy").join(LOCK_FILE)).unwrap()
        );
        assert!(crate::load_task_manifest_with_inspection(&manifest)
            .unwrap()
            .effective_manifest
            .contains("echo new"));
    }
    #[test]
    fn unsafe_cache_symlink_cannot_write_foreign_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, prep) = fixture(tmp.path());
        let foreign = tmp.path().join("foreign");
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(foreign.join("sentinel"), "owned by someone else").unwrap();
        std::fs::create_dir(tmp.path().join(".effigy")).unwrap();
        std::os::unix::fs::symlink(&foreign, tmp.path().join(".effigy/cache")).unwrap();
        assert!(sync_bundle_source(&manifest, &prep).is_err());
        assert_eq!(
            std::fs::read_to_string(foreign.join("sentinel")).unwrap(),
            "owned by someone else"
        );
        assert_eq!(std::fs::read_dir(&foreign).unwrap().count(), 1);
    }
}
