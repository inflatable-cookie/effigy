//! Bounded, read-only foreground diagnostics. No body grants admission.
use super::*;
use crate::id::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForegroundDiag {
    pub owner: ResourceId,
    pub task: TaskId,
    pub gate: ResourceId,
}

#[derive(Debug, Clone)]
pub struct ForegroundHolder {
    pub coordinates: ForegroundDiag,
    pub holder: HolderDiag,
}

impl GateGuard {
    pub(crate) fn set_foreground(&self, run: &RunId, owner: &ResourceId, task: &TaskId) {
        for (id, file) in &self.gates {
            let mut value = current_body(Some(run.to_string()));
            value.foreground = Some(ForegroundDiag {
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
                    .map_err(|error| format!("{error:?}"))?;
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
                    let state = probe_names(cx, root, scope, name);
                    let diag = match state {
                        Observed::Verified(GateState::Held(diag), _) => match *diag {
                            Observed::Verified(diag, _) => diag,
                            Observed::Unknown(reason) => {
                                return Err(format!(
                                    "held gate diagnostics unavailable: {reason:?}"
                                ))
                            }
                            Observed::Absent(_) => {
                                return Err("held gate has no diagnostics".into())
                            }
                        },
                        Observed::Unknown(reason) => {
                            return Err(format!("gate observation unavailable: {reason:?}"))
                        }
                        _ => continue,
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
                    rows.push(ForegroundHolder {
                        coordinates,
                        holder: diag,
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
