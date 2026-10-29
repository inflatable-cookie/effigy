use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_manifest::LoadedCatalog;
use effigy_scan::{
    doctor_duplicate_block_options, doctor_god_file_options, run_doctor_scan_inventory,
    DoctorScanInventoryOptions, DoctorScanInventoryResult,
};

use crate::contracts::{check_id, remediation};
use crate::progress::{
    DoctorProgressReporter, ScanLabel, ScanOutcome, DUPLICATE_BLOCKS, GOD_FILES,
};
use crate::scan_checks::core::{DoctorIntegratedScanFinding, DoctorIntegratedScanResult};
use crate::DoctorState;

const SCAN_CHECK_NAMES: [(&str, &str); 2] = [
    ("god_files", check_id::SCAN_GOD_FILES),
    ("duplicate_blocks", check_id::SCAN_DUPLICATE_BLOCKS),
];

/// The step each scan check shows, in `SCAN_CHECK_NAMES` order.
const SCAN_STEP_LABELS: [ScanLabel; 2] = [GOD_FILES, DUPLICATE_BLOCKS];

pub(super) struct DeepInventoryRequest<'a> {
    pub(super) workspace_root: &'a Path,
    pub(super) scope_root: &'a Path,
    pub(super) scope_alias: &'a str,
    pub(super) catalogs: &'a [LoadedCatalog],
    pub(super) pruned_roots: &'a [PathBuf],
    pub(super) deadline: Option<Instant>,
    pub(super) refresh: bool,
}

pub(super) fn run_deep_inventory(
    request: DeepInventoryRequest<'_>,
    state: &mut DoctorState,
    mut progress: Option<&mut DoctorProgressReporter>,
) {
    let Some(options) = load_options(request.scope_root, request.catalogs, state) else {
        for (name, _) in SCAN_CHECK_NAMES {
            state.record_skipped_check(name);
        }
        return;
    };
    let enabled = enabled_checks(&options);
    start_scan_steps(progress.as_deref_mut(), &enabled);
    let findings_before = state.findings.len();
    let started = Instant::now();
    let result = run_doctor_scan_inventory(
        request.workspace_root,
        request.scope_alias,
        request.scope_root,
        request.pruned_roots,
        &options,
        request.refresh,
        request.deadline,
    );
    let elapsed = started.elapsed();
    let result = match result {
        Ok(result) => result,
        Err(error) if error.to_string().contains("budget exhausted") => {
            state.record_budget_exhausted("scan_inventory", elapsed);
            for (name, _) in SCAN_CHECK_NAMES {
                state.record_skipped_check(name);
            }
            finish_scan_steps(
                progress.as_deref_mut(),
                [
                    ScanOutcome::Stopped("stopped at the time budget"),
                    ScanOutcome::Stopped("stopped at the time budget"),
                ],
            );
            return;
        }
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_GOD_FILES,
                format!("shared doctor scan inventory failed: {error}"),
                "Fix the reported scan error, then rerun `effigy doctor --deep`.",
            );
            for (name, _) in SCAN_CHECK_NAMES {
                state.record_skipped_check(name);
            }
            finish_scan_steps(
                progress.as_deref_mut(),
                [
                    outcome_for(state, findings_before, check_id::SCAN_GOD_FILES),
                    ScanOutcome::Stopped("skipped, the shared scan failed"),
                ],
            );
            return;
        }
    };
    state.record_cache_summary(
        result.cache_hits,
        result.cache_misses,
        result.invalid_cache_entries,
    );
    let fully_cached = result.cache_hits > 0 && result.cache_misses == 0;
    for warning in &result.warnings {
        state.add_check_warning(
            check_id::SCAN_GOD_FILES,
            warning,
            "The disposable doctor cache was ignored; rerun `effigy doctor --deep --refresh` if the warning persists.",
        );
    }
    debug_assert_eq!(result.physical_walks, 1, "deep inventory must use one walk");
    apply_results(result, state);
    finish_scan_steps(
        progress,
        [
            outcome_for(state, findings_before, check_id::SCAN_GOD_FILES),
            outcome_for(state, findings_before, check_id::SCAN_DUPLICATE_BLOCKS),
        ],
    );
    for (index, (name, _)) in SCAN_CHECK_NAMES.iter().enumerate() {
        if enabled[index] {
            let duration = if index == 0 { elapsed } else { Duration::ZERO };
            if fully_cached {
                state.record_cached_check(name, duration);
            } else {
                state.record_completed_check(name, duration);
            }
        } else {
            state.record_skipped_check(name);
        }
    }
}

fn start_scan_steps(progress: Option<&mut DoctorProgressReporter>, enabled: &[bool; 2]) {
    let Some(progress) = progress else {
        return;
    };
    let total = enabled.iter().filter(|on| **on).count() as u64;
    let mut done = 0u64;
    for (index, label) in SCAN_STEP_LABELS.into_iter().enumerate() {
        if enabled[index] {
            done += 1;
            progress.start_scan_with(
                label,
                Some((done, total)),
                Some("shared scan inventory".to_owned()),
            );
        }
    }
}

/// End each scan check's step. A check that never started has no step, so its
/// outcome is ignored.
fn finish_scan_steps(progress: Option<&mut DoctorProgressReporter>, outcomes: [ScanOutcome; 2]) {
    let Some(progress) = progress else {
        return;
    };
    for (label, outcome) in SCAN_STEP_LABELS.into_iter().zip(outcomes.iter()) {
        progress.finish_scan(label, outcome);
    }
}

/// The outcome of one scan check: the findings it raised since `from`.
fn outcome_for(state: &DoctorState, from: usize, check: &str) -> ScanOutcome {
    ScanOutcome::from_findings(
        state.findings[from..]
            .iter()
            .filter(|finding| finding.check_id == check),
    )
}

fn load_options(
    scope_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) -> Option<DoctorScanInventoryOptions> {
    Some(DoctorScanInventoryOptions {
        god_files: load(
            doctor_god_file_options(scope_root, catalogs),
            check_id::SCAN_GOD_FILES,
            "god-files",
            state,
        )?,
        duplicate_blocks: load(
            doctor_duplicate_block_options(scope_root, catalogs),
            check_id::SCAN_DUPLICATE_BLOCKS,
            "duplicate-blocks",
            state,
        )?,
    })
}

fn load<T>(
    result: Result<T, effigy_scan::ScanError>,
    check: &str,
    label: &str,
    state: &mut DoctorState,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            state.add_check_error(
                check,
                format!("{label} configuration is invalid: {error}"),
                "Fix manifest parse/schema errors first, then re-run `effigy doctor --deep`.",
            );
            None
        }
    }
}

fn enabled_checks(options: &DoctorScanInventoryOptions) -> [bool; 2] {
    [
        options.god_files.doctor_enabled,
        options.duplicate_blocks.doctor_enabled,
    ]
}

fn apply_results(result: DoctorScanInventoryResult, state: &mut DoctorState) {
    if let Some(result) = result.god_files {
        apply(
            result,
            check_id::SCAN_GOD_FILES,
            remediation::SPLIT_GOD_FILES,
            state,
        );
    }
    if let Some(result) = result.duplicate_blocks {
        apply(
            result,
            check_id::SCAN_DUPLICATE_BLOCKS,
            remediation::REDUCE_DUPLICATE_BLOCKS,
            state,
        );
    }
}

fn apply<TResult>(result: TResult, check_id: &str, remediation: &str, state: &mut DoctorState)
where
    TResult: DoctorIntegratedScanResult,
{
    for finding in result.into_findings() {
        state.add_check_finding(
            check_id,
            finding.doctor_severity(),
            finding.doctor_evidence(),
            remediation,
            false,
        );
    }
}
