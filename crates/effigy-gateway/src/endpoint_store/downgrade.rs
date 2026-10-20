//! Explicit, resumable handoff to the v0.14.1 files. Never a second writer.
use super::snapshot::Authority;
use super::*;
use effigy_kernel::fs::{DirFd, FileName};

impl EndpointStore {
    pub(crate) fn require_active(&self) -> Result<(), GatewayError> {
        match self.read() {
            Observed::Verified(snapshot, _) if snapshot.authority != Authority::Store => Err(error(
                "gateway endpoints were exported; run `effigy gateway setup` to re-import after stopping v0.14.1",
            )),
            Observed::Unknown(reason) => Err(error(format!("endpoint authority unknown: {reason:?}"))),
            _ => Ok(()),
        }
    }

    /// Called after the user daemon stops, before privileged platform removal.
    /// A durable Exporting state fences edits until all files are exported.
    /// Retry accepts only original input bytes or this frozen export's values.
    pub fn export_legacy(&self) -> Result<(), GatewayError> {
        if matches!(self.read(), Observed::Absent(_)) {
            self.import()?;
        }
        self.with_run(|run, op| {
            let mut snapshot = EndpointSnapshot::from_value(
                run.resource_state().map_err(error)?.ok_or_else(|| error("missing endpoint payload"))?,
            )?;
            let config = crate::GatewayConfig::standard(self.home.join(".effigy/gateway"));
            let status = operator::status(&config).map_err(error)?;
            if !matches!(status.status, "stopped" | "setup_required")
                || crate::identity::read_snapshot(&config.pid_file_path)?.is_some()
            {
                return Err(error("stop the verified gateway before exporting endpoints"));
            }
            if snapshot.authority == Authority::Legacy {
                return Ok(());
            }
            snapshot.stamp_legacy_routes()?;
            let mut managed = snapshot.routes()?.routes.into_values()
                .filter(|route| route.managed_listener.is_some())
                .map(|route| route.domain).collect::<Vec<_>>();
            managed.sort();
            if !managed.is_empty() {
                return Err(error(format!("v0.14.1 cannot enforce managed-listener ownership for: {}; stop these v0.15 consumers so their routes are withdrawn, then retry removal", managed.join(", "))));
            }
            let _locks = self.sources.lock(op.remaining().unwrap_or_default())?;
            let mut outputs = Vec::new();
            for (family, path) in self.sources.paths() {
                let desired = snapshot.document(family)?;
                let current = import::read_input(path)?;
                let equal = current.as_ref().is_some_and(|bytes| {
                    serde_json::from_slice::<serde_json::Value>(bytes).ok().as_ref() == Some(desired)
                });
                let original = current.as_ref().map(|bytes| crate::ports::sha256_hex(bytes));
                if !equal && original.as_ref() != snapshot.imported_sha256.get(family) {
                    return Err(error(format!("legacy input {} changed since import; preserving it; reconcile this exact file before retrying removal", path.display())));
                }
                let bytes = if equal { current.expect("equal requires bytes") } else {
                    serde_json::to_vec_pretty(desired).map_err(error)?
                };
                if bytes.len() > 8 << 20 {
                    return Err(error(format!("legacy export {} exceeds 8 MiB", path.display())));
                }
                outputs.push((path.to_owned(), bytes));
            }
            snapshot.authority = Authority::Exporting;
            snapshot.revision = run.fence().generation.get();
            publish_existing(run, &snapshot)?;
            for (path, bytes) in outputs {
                let dir = DirFd::open(path.parent().ok_or_else(|| error("export parent missing"))?).map_err(error)?;
                let name = FileName::new(path.file_name().and_then(|n| n.to_str()).ok_or_else(|| error("invalid export path"))?).map_err(error)?;
                run.replace_file(&dir, &name, &bytes, Default::default()).map_err(error)?;
                #[cfg(feature = "endpoint-store-faults")]
                effigy_kernel::fault_injection::point("gateway.export.after-file").map_err(error)?;
            }
            snapshot.authority = Authority::Legacy;
            publish_existing(run, &snapshot)
        })
    }

    /// Setup calls this only after authenticating and stopping the legacy
    /// daemon. Re-import current files, including edits made after downgrade.
    pub fn reimport_legacy(&self) -> Result<(), GatewayError> {
        self.with_run(|run, op| {
            let Some(state) = run.resource_state().map_err(error)? else {
                return Ok(());
            };
            let before = EndpointSnapshot::from_value(state)?;
            match before.authority {
                Authority::Store => return Ok(()),
                Authority::Exporting => {
                    return Err(error(
                        "gateway export is incomplete; retry `effigy gateway setup --remove` first",
                    ))
                }
                Authority::Legacy => {}
            }
            let config = crate::GatewayConfig::standard(self.home.join(".effigy/gateway"));
            if crate::identity::read_snapshot(&config.pid_file_path)?.is_some() {
                return Err(error(
                    "setup must stop the verified v0.14.1 daemon before re-import",
                ));
            }
            let _locks = self.sources.lock(op.remaining().unwrap_or_default())?;
            let mut snapshot = self.sources.read()?;
            snapshot.quarantined = before.quarantined.clone();
            let old = before.claims()?;
            let desired = snapshot.claims()?;
            snapshot.update_quarantine(&old, &desired)?;
            let wanted = desired
                .iter()
                .filter(|(name, _)| !old.contains_key(*name))
                .map(|(_, (resource, _))| resource.clone())
                .collect::<Vec<_>>();
            let token = if wanted.is_empty() {
                None
            } else {
                Some(Claims::reserve(run, &wanted, Duration::ZERO).map_err(error)?)
            };
            snapshot.revision = run.fence().generation.get();
            Claims::publish_retained(
                run,
                token.as_ref(),
                &desired.into_values().collect::<Vec<_>>(),
                &serde_json::to_value(snapshot).map_err(error)?,
                REUSE_QUARANTINE,
            )
            .map_err(error)
        })
    }
}
fn publish_existing(run: &mut Run<'_>, snapshot: &EndpointSnapshot) -> Result<(), GatewayError> {
    Claims::publish_retained(
        run,
        None,
        &snapshot.claims()?.into_values().collect::<Vec<_>>(),
        &serde_json::to_value(snapshot).map_err(error)?,
        REUSE_QUARANTINE,
    )
    .map_err(error)
}
