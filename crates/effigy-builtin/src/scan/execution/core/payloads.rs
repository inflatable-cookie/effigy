use serde_json::{json, Map, Value};

use super::api::ScanPayloadResult;
use effigy_scan::{
    AttentionMarkerScanResult, BoundaryViolationScanResult, CommentRatioScanResult,
    DeadCodeScanResult, DuplicateBlockScanResult, GeneratedAssetScanResult,
    GeneratedInSrcScanResult, GodFileScanResult, StaleSuppressionScanResult,
    ValidationGapScanResult,
};

impl ScanPayloadResult for GodFileScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert(
            "skipped_generated".into(),
            Value::from(self.skipped_generated),
        );
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for BoundaryViolationScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "configured_layers".into(),
            Value::from(self.configured_layers),
        );
        payload.insert("checked_edges".into(), Value::from(self.checked_edges));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for DeadCodeScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert("checked_files".into(), Value::from(self.checked_files));
        payload.insert("checked_symbols".into(), Value::from(self.checked_symbols));
        payload.insert(
            "skipped_allowlisted_paths".into(),
            Value::from(self.skipped_allowlisted_paths),
        );
        payload.insert(
            "skipped_allowlisted_symbols".into(),
            Value::from(self.skipped_allowlisted_symbols),
        );
        payload.insert(
            "skipped_non_implementation_files".into(),
            Value::from(self.skipped_non_implementation_files),
        );
        payload.insert(
            "skipped_unsupported_language_files".into(),
            Value::from(self.skipped_unsupported_language_files),
        );
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for ValidationGapScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert("mode".into(), Value::from(self.mode.clone()));
        payload.insert(
            "hotspot_threshold".into(),
            Value::from(self.hotspot_threshold),
        );
        payload.insert("affected_depth".into(), Value::from(self.affected_depth));
        payload.insert("changed_paths".into(), json!(&self.changed_paths));
        payload.insert("checked_files".into(), Value::from(self.checked_files));
        payload.insert(
            "skipped_allowlisted_paths".into(),
            Value::from(self.skipped_allowlisted_paths),
        );
        payload.insert(
            "skipped_non_implementation_files".into(),
            Value::from(self.skipped_non_implementation_files),
        );
        payload.insert(
            "skipped_unsupported_language_files".into(),
            Value::from(self.skipped_unsupported_language_files),
        );
        payload.insert("likely_test_files".into(), json!(&self.likely_test_files));
        payload.insert("likely_test_tasks".into(), json!(&self.likely_test_tasks));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for GeneratedAssetScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for GeneratedInSrcScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
            }),
        );
        payload.insert("source_roots".into(), json!(&self.source_roots));
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for DuplicateBlockScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
                "min_occurrences": self.thresholds.min_occurrences,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert(
            "candidate_blocks".into(),
            Value::from(self.candidate_blocks),
        );
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for CommentRatioScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "thresholds".into(),
            json!({
                "warn": self.thresholds.warn,
                "high": self.thresholds.high,
                "critical": self.thresholds.critical,
                "min_code_lines": self.thresholds.min_code_lines,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("candidate_files".into(), Value::from(self.candidate_files));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for AttentionMarkerScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "patterns".into(),
            json!({
                "warning": &self.patterns.warning,
                "high": &self.patterns.high,
                "critical": &self.patterns.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("matched_lines".into(), Value::from(self.matched_lines));
        payload.insert("findings".into(), json!(&self.findings));
    }
}

impl ScanPayloadResult for StaleSuppressionScanResult {
    fn root(&self) -> &str {
        &self.root
    }

    fn finding_count(&self) -> usize {
        self.findings.len()
    }

    fn insert_payload_fields(&self, payload: &mut Map<String, Value>) {
        payload.insert(
            "patterns".into(),
            json!({
                "warning": &self.patterns.warning,
                "high": &self.patterns.high,
                "critical": &self.patterns.critical,
            }),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert("matched_lines".into(), Value::from(self.matched_lines));
        payload.insert("findings".into(), json!(&self.findings));
    }
}
