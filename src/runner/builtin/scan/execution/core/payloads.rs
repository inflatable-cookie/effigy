use serde_json::{json, Map, Value};

use super::super::super::super::super::scan::model::{
    AttentionMarkerScanResult, CommentRatioScanResult, DuplicateBlockScanResult,
    GeneratedAssetScanResult, GeneratedInSrcScanResult, GodFileScanResult,
    StaleSuppressionScanResult,
};
use super::api::ScanPayloadResult;

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
