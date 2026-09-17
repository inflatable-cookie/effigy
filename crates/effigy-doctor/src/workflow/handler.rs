use std::path::Path;
use std::time::Instant;

use effigy_core::resolver::{resolve_target_root, ResolvedTarget};

use super::super::{finding_templates::WorkflowFinding, manifest};
use super::{phases, preparation, DoctorProgressReporter, DoctorRunConfig, DoctorRunOutput};
use crate::{
    manifest_snapshot::ManifestSnapshot, DoctorError, DoctorMode, DoctorRuntimePorts, DoctorScope,
    DoctorState,
};

pub(super) struct DefaultWorkflowPhaseHandler<'a> {
    progress: Option<&'a mut DoctorProgressReporter>,
    ports: &'a dyn DoctorRuntimePorts,
    invocation_cwd: std::path::PathBuf,
    config: DoctorRunConfig,
    started: Instant,
    scopes: Vec<DoctorScope>,
}

impl<'a> DefaultWorkflowPhaseHandler<'a> {
    pub(super) fn new(
        invocation_cwd: std::path::PathBuf,
        config: DoctorRunConfig,
        progress: Option<&'a mut DoctorProgressReporter>,
        ports: &'a dyn DoctorRuntimePorts,
    ) -> Self {
        Self {
            progress,
            ports,
            invocation_cwd,
            config,
            started: Instant::now(),
            scopes: Vec::new(),
        }
    }
}

impl phases::WorkflowPhaseHandler for DefaultWorkflowPhaseHandler<'_> {
    fn resolve_root(
        &mut self,
        cwd: std::path::PathBuf,
        repo_override: Option<std::path::PathBuf>,
    ) -> Result<ResolvedTarget, DoctorError> {
        resolve_target_root(cwd, repo_override).map_err(DoctorError::from)
    }

    fn emit_root_resolution_finding(&mut self, resolved: &ResolvedTarget, state: &mut DoctorState) {
        emit_root_resolution_finding(resolved, state);
    }

    fn prepare_manifest(
        &mut self,
        resolved_root: &Path,
        fix: bool,
        state: &mut DoctorState,
    ) -> Result<ManifestSnapshot, DoctorError> {
        preparation::prepare_manifest_snapshot_with(
            resolved_root,
            fix,
            state,
            collect_manifest_snapshot,
            |root, snapshot, state| {
                state
                    .fixes
                    .extend(manifest::apply_fixers(root, &snapshot.parsed_catalogs));
            },
        )
    }

    fn run_checks(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    ) -> Result<(), DoctorError> {
        let (scopes, selected_indices) = select_doctor_scopes(
            resolved_root,
            &self.invocation_cwd,
            &manifest.parsed_catalogs,
            self.config.mode,
            self.config.catalog.as_deref(),
            self.config.all_catalogs,
        )?;
        self.scopes = scopes;
        let deadline = self.deadline();
        let structural_complete = crate::checks::run_structural_checks(
            resolved_root,
            manifest,
            state,
            self.progress.as_deref_mut(),
            self.ports,
            deadline,
        );
        if !structural_complete || self.config.mode == DoctorMode::Fast {
            crate::checks::record_deep_checks_skipped(state);
            return Ok(());
        }

        for selected_index in selected_indices {
            let selected = &manifest.parsed_catalogs[selected_index];
            let pruned_roots = manifest
                .parsed_catalogs
                .iter()
                .enumerate()
                .filter(|(index, catalog)| {
                    *index != selected_index
                        && catalog.catalog_root.starts_with(&selected.catalog_root)
                })
                .map(|(_, catalog)| catalog.catalog_root.clone())
                .collect::<Vec<_>>();
            crate::deep_inventory::run_deep_inventory(
                crate::deep_inventory::DeepInventoryRequest {
                    workspace_root: resolved_root,
                    scope_root: &selected.catalog_root,
                    scope_alias: &selected.alias,
                    catalogs: &manifest.parsed_catalogs,
                    pruned_roots: &pruned_roots,
                    deadline,
                    refresh: self.config.refresh,
                },
                state,
            );
            if !state.is_complete() {
                state.record_skipped_check("health_task");
                break;
            }
            crate::checks::run_health_check(
                &selected.catalog_root,
                std::slice::from_ref(selected),
                state,
                self.progress.as_deref_mut(),
                self.ports,
                deadline,
            );
            if !state.is_complete() {
                break;
            }
        }
        Ok(())
    }

    fn finalize_fix_actions(&mut self, state: &mut DoctorState, fix: bool) {
        state.finalize_fix_actions(fix);
    }

    fn add_manifest_availability_findings(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    ) {
        add_manifest_availability_findings(resolved_root, manifest, state);
    }

    fn summarize_and_report(
        &mut self,
        mut state: DoctorState,
        resolved: ResolvedTarget,
    ) -> DoctorRunOutput {
        let diagnostics = if !state.is_complete() {
            crate::DoctorRuntimeDiagnostics::default()
        } else if self
            .deadline()
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            state.record_budget_exhausted("runtime_diagnostics", std::time::Duration::ZERO);
            crate::DoctorRuntimeDiagnostics::default()
        } else {
            self.ports
                .runtime_diagnostics(&resolved.resolved_root)
                .unwrap_or_else(|error| crate::DoctorRuntimeDiagnostics {
                    evidence: Vec::new(),
                    warnings: vec![format!(
                        "container runtime diagnostics unavailable: {error}"
                    )],
                    findings: Vec::new(),
                })
        };
        summarize_and_report_with_diagnostics(state, resolved, diagnostics).with_run_metadata(
            self.config.mode,
            std::mem::take(&mut self.scopes),
            self.config.budget,
            self.started.elapsed(),
        )
    }
}

impl DefaultWorkflowPhaseHandler<'_> {
    fn deadline(&self) -> Option<Instant> {
        self.config
            .budget
            .and_then(|budget| self.started.checked_add(budget))
    }
}

pub(super) fn emit_root_resolution_finding(resolved: &ResolvedTarget, state: &mut DoctorState) {
    WorkflowFinding::RootResolution {
        resolved_root: &resolved.resolved_root,
        resolution_mode: resolved.resolution_mode,
    }
    .emit(state);
}

pub(super) fn add_manifest_availability_findings(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
) {
    if manifest.manifest_paths.is_empty() {
        WorkflowFinding::MissingManifestFiles { resolved_root }.emit(state);
    } else if !manifest.parse_ok_any {
        WorkflowFinding::NoValidManifests.emit(state);
    }
}

pub(super) fn summarize_and_report_with_diagnostics(
    mut state: DoctorState,
    mut resolved: ResolvedTarget,
    diagnostics: crate::DoctorRuntimeDiagnostics,
) -> DoctorRunOutput {
    resolved.evidence.extend(diagnostics.evidence);
    resolved.warnings.extend(diagnostics.warnings);
    for finding in diagnostics.findings {
        state.add_finding(finding);
    }
    let summary = state.summarize();
    let error_count = summary.error;
    let report = state.into_report(
        resolved.resolved_root.display().to_string(),
        summary,
        resolved.evidence,
        resolved.warnings,
    );
    DoctorRunOutput {
        report,
        error_count,
    }
}

impl DoctorRunOutput {
    fn with_run_metadata(
        mut self,
        mode: DoctorMode,
        scopes: Vec<DoctorScope>,
        budget: Option<std::time::Duration>,
        elapsed: std::time::Duration,
    ) -> Self {
        self.report.mode = mode;
        self.report.scopes = scopes;
        self.report.budget_ms =
            budget.map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64);
        self.report.elapsed_ms = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
        self
    }
}

fn select_doctor_scopes(
    resolved_root: &Path,
    cwd: &Path,
    catalogs: &[effigy_manifest::LoadedCatalog],
    mode: DoctorMode,
    explicit_alias: Option<&str>,
    all_catalogs: bool,
) -> Result<(Vec<DoctorScope>, Vec<usize>), DoctorError> {
    let mut ordered = catalogs.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by(|(_, left), (_, right)| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.catalog_root.cmp(&right.catalog_root))
            .then_with(|| left.alias.cmp(&right.alias))
    });

    let selected = if mode == DoctorMode::Deep && all_catalogs {
        ordered
    } else if let Some(alias) = explicit_alias {
        let matches = ordered
            .into_iter()
            .filter(|(_, catalog)| catalog.alias == alias)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [catalog] => vec![*catalog],
            [] => {
                let mut aliases = catalogs
                    .iter()
                    .map(|catalog| catalog.alias.as_str())
                    .collect::<Vec<_>>();
                aliases.sort_unstable();
                return Err(DoctorError::task_invocation(format!(
                    "unknown doctor catalog `{alias}` (available: {})",
                    aliases.join(", ")
                )));
            }
            _ => {
                return Err(DoctorError::task_invocation(format!(
                    "doctor catalog alias `{alias}` is ambiguous"
                )))
            }
        }
    } else {
        let selected = ordered
            .iter()
            .copied()
            .filter(|(_, catalog)| cwd.starts_with(&catalog.catalog_root))
            .max_by_key(|(_, catalog)| catalog.depth)
            .or_else(|| {
                ordered
                    .iter()
                    .copied()
                    .find(|(_, catalog)| catalog.catalog_root == resolved_root)
            })
            .or_else(|| ordered.first().copied());
        selected.into_iter().collect()
    };

    let scopes = selected
        .iter()
        .map(|(_, catalog)| DoctorScope {
            alias: catalog.alias.clone(),
            root: catalog.catalog_root.display().to_string(),
        })
        .collect();
    let indices = selected.into_iter().map(|(index, _)| index).collect();
    Ok((scopes, indices))
}

fn collect_manifest_snapshot(
    resolved_root: &Path,
    state: &mut DoctorState,
) -> Result<ManifestSnapshot, DoctorError> {
    let (manifest_paths, parsed_catalogs, preferred_js_pm, parse_ok_any) =
        manifest::collect_manifest_findings(resolved_root, state)?;
    Ok(ManifestSnapshot {
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    })
}
