use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_manifest::LoadedCatalog;
use effigy_scan::{
    doctor_attention_marker_options, doctor_comment_ratio_options, doctor_duplicate_block_options,
    doctor_generated_asset_options, doctor_generated_in_src_options, doctor_god_file_options,
    doctor_stale_suppression_options, run_doctor_scan_inventory, DoctorScanInventoryOptions,
    DoctorScanInventoryResult,
};

use crate::contracts::{check_id, remediation};
use crate::scan_checks::core::{DoctorIntegratedScanFinding, DoctorIntegratedScanResult};
use crate::DoctorState;

const SCAN_CHECK_NAMES: [(&str, &str); 7] = [
    ("god_files", check_id::SCAN_GOD_FILES),
    ("duplicate_blocks", check_id::SCAN_DUPLICATE_BLOCKS),
    ("comment_ratio", check_id::SCAN_COMMENT_RATIO),
    ("generated_assets", check_id::SCAN_GENERATED_ASSETS),
    ("generated_in_src", check_id::SCAN_GENERATED_IN_SRC),
    ("attention_markers", check_id::SCAN_ATTENTION_MARKERS),
    ("stale_suppressions", check_id::SCAN_STALE_SUPPRESSIONS),
];

pub(super) struct DeepInventoryRequest<'a> {
    pub(super) workspace_root: &'a Path,
    pub(super) scope_root: &'a Path,
    pub(super) scope_alias: &'a str,
    pub(super) catalogs: &'a [LoadedCatalog],
    pub(super) pruned_roots: &'a [PathBuf],
    pub(super) deadline: Option<Instant>,
    pub(super) refresh: bool,
}

pub(super) fn run_deep_inventory(request: DeepInventoryRequest<'_>, state: &mut DoctorState) {
    let Some(options) = load_options(request.scope_root, request.catalogs, state) else {
        for (name, _) in SCAN_CHECK_NAMES {
            state.record_skipped_check(name);
        }
        return;
    };
    let enabled = enabled_checks(&options);
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
        comment_ratio: load(
            doctor_comment_ratio_options(scope_root, catalogs),
            check_id::SCAN_COMMENT_RATIO,
            "comment-ratio",
            state,
        )?,
        generated_assets: load(
            doctor_generated_asset_options(scope_root, catalogs),
            check_id::SCAN_GENERATED_ASSETS,
            "generated-assets",
            state,
        )?,
        generated_in_src: load(
            doctor_generated_in_src_options(scope_root, catalogs),
            check_id::SCAN_GENERATED_IN_SRC,
            "generated-in-src",
            state,
        )?,
        attention_markers: load(
            doctor_attention_marker_options(scope_root, catalogs),
            check_id::SCAN_ATTENTION_MARKERS,
            "attention-markers",
            state,
        )?,
        stale_suppressions: load(
            doctor_stale_suppression_options(scope_root, catalogs),
            check_id::SCAN_STALE_SUPPRESSIONS,
            "stale-suppressions",
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

fn enabled_checks(options: &DoctorScanInventoryOptions) -> [bool; 7] {
    [
        options.god_files.doctor_enabled,
        options.duplicate_blocks.doctor_enabled,
        options.comment_ratio.doctor_enabled,
        options.generated_assets.doctor_enabled,
        options.generated_in_src.doctor_enabled,
        options.attention_markers.doctor_enabled,
        options.stale_suppressions.doctor_enabled,
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
    if let Some(result) = result.comment_ratio {
        apply(
            result,
            check_id::SCAN_COMMENT_RATIO,
            remediation::REDUCE_COMMENT_RATIO,
            state,
        );
    }
    if let Some(result) = result.generated_assets {
        apply(
            result,
            check_id::SCAN_GENERATED_ASSETS,
            remediation::REMOVE_OR_IGNORE_GENERATED_ASSETS,
            state,
        );
    }
    if let Some(result) = result.generated_in_src {
        apply(
            result,
            check_id::SCAN_GENERATED_IN_SRC,
            remediation::REMOVE_GENERATED_FROM_SOURCE_TREES,
            state,
        );
    }
    if let Some(result) = result.attention_markers {
        apply(
            result,
            check_id::SCAN_ATTENTION_MARKERS,
            remediation::RESOLVE_ATTENTION_MARKERS,
            state,
        );
    }
    if let Some(result) = result.stale_suppressions {
        apply(
            result,
            check_id::SCAN_STALE_SUPPRESSIONS,
            remediation::REMOVE_STALE_SUPPRESSIONS,
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
