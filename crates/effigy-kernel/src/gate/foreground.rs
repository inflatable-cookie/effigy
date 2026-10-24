//! Bounded, read-only foreground diagnostics. No body grants admission.
use super::*;
use crate::id::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForegroundDiag {
    #[serde(default)]
    pub root: bool,
    pub owner: ResourceId,
    pub task: TaskId,
    pub gate: ResourceId,
}

#[derive(Debug, Clone)]
pub struct ForegroundHolder {
    pub coordinates: ForegroundDiag,
    pub holder: HolderDiag,
    // An independent, read-only description of the observed inode. Waiting
    // for release must not mistake a missing or replaced pathname for closure.
    gate: std::sync::Arc<File>,
}

impl ForegroundHolder {
    /// Observe release of this exact gate inode, without acquiring custody.
    /// Missing or replaced checkout authority remains Unknown.
    pub fn released(&self, cx: &ObserveContext, root: &StateRoot) -> Observed<bool> {
        if cx.check("foreground custody release").is_err() {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "foreground custody release".into(),
            });
        }
        if let Err(error) = root.verify_binding() {
            return Observed::Unknown(crate::root::unknown(&error));
        }
        match fs::try_flock(&self.gate, libc::LOCK_SH) {
            Ok(free) => {
                if free {
                    fs::unlock(&self.gate);
                }
                Observed::Verified(
                    free,
                    Evidence::new("retained foreground gate inode".into(), None)
                        .expect("fixed source"),
                )
            }
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
}

impl GateGuard {
    pub(crate) fn set_foreground(
        &self,
        run: &RunId,
        owner: &ResourceId,
        task: &TaskId,
        root: bool,
    ) {
        for (id, file) in &self.gates {
            let mut value = current_body(Some(run.to_string()));
            value.foreground = Some(ForegroundDiag {
                root,
                owner: owner.clone(),
                task: task.clone(),
                gate: id.clone(),
            });
            if let Ok(bytes) = serde_json::to_vec(&value) {
                if bytes.len() as u64 <= DIAG_MAX {
                    let _ = fs::rewrite(&file.file, &bytes);
                }
            }
        }
    }
}
impl Gate {
    /// At most 64 scope directories and 8192 gate entries. Free gates and
    /// old diagnostic bodies never stand in for a current holder. A bound
    /// directory or gate observation failure is explicit uncertainty.
    pub fn foreground_holders(
        cx: &ObserveContext,
        root: &StateRoot,
    ) -> Observed<Vec<ForegroundHolder>> {
        let scan = || -> Result<_, String> {
            if root.is_machine() {
                return Ok(Vec::new());
            }
            root.verify_binding().map_err(|error| error.to_string())?;
            let Some(gates) = root
                .family(Family::Gates)
                .map_err(|error| error.to_string())?
            else {
                return Ok(Vec::new());
            };
            let mut rows = Vec::new();
            let mut remaining = 8192;
            for scope in gates.list(64).map_err(|error| error.to_string())? {
                cx.operation
                    .check("foreground gate inventory")
                    .map_err(|error| format!("{error}"))?;
                let scope = scope.to_str().ok_or("non-UTF8 gate scope")?;
                if scope.ends_with(".scope") {
                    continue;
                }
                let Some(dir) = gates
                    .child(scope, Trust::Private, false)
                    .map_err(|error| error.to_string())?
                else {
                    continue;
                };
                let names = dir.list(remaining).map_err(|error| error.to_string())?;
                remaining -= names.len();
                for name in names {
                    let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".gate"))
                    else {
                        continue;
                    };
                    cx.check("foreground gate inventory")
                        .map_err(|error| format!("{error}"))?;
                    let Some(file) = open_gate(&dir, &format!("{name}.gate"), false)
                        .map_err(|error| error.to_string())?
                    else {
                        continue;
                    };
                    if fs::try_flock(&file, libc::LOCK_SH).map_err(|error| error.to_string())? {
                        fs::unlock(&file);
                        continue;
                    }
                    let diag = match holder(&file) {
                        Observed::Verified(diag, _) => diag,
                        Observed::Unknown(reason) => {
                            return Err(format!("held gate diagnostics unavailable: {reason}"))
                        }
                        Observed::Absent(_) => return Err("held gate has no diagnostics".into()),
                    };
                    let Some(coordinates) = diag.foreground.clone() else {
                        continue;
                    };
                    if scope_name(coordinates.gate.scope()) != scope
                        || coordinates.gate.name().to_string() != name
                    {
                        return Err(
                            "foreground gate coordinates do not match its inode name".into()
                        );
                    }
                    if let Scope::Checkout(owner) = coordinates.owner.scope() {
                        if owner.kind() == crate::id::CheckoutKind::Filesystem
                            && root
                                .local_gate_identity(cx)
                                .map_err(|error| error.to_string())?
                                != Some(owner)
                        {
                            return Err(
                                "foreground holder belongs to a replaced checkout generation"
                                    .into(),
                            );
                        }
                    }
                    rows.push(ForegroundHolder {
                        coordinates,
                        holder: diag,
                        gate: std::sync::Arc::new(file),
                    });
                }
            }
            root.verify_binding().map_err(|error| error.to_string())?;
            Ok(rows)
        };
        match scan() {
            Ok(rows) => Observed::Verified(
                rows,
                Evidence::new("live foreground gate inventory".into(), None).expect("fixed source"),
            ),
            Err(reason) => Observed::Unknown(UnknownReason::Untrusted(reason)),
        }
    }
}
