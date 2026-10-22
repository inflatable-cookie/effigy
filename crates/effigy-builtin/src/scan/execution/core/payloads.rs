use serde_json::{json, Map, Value};

use super::api::ScanPayloadResult;
use effigy_scan::{DuplicateBlockScanResult, GodFileScanResult};

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
        payload.insert(
            "thresholds_source".into(),
            Value::from(self.thresholds_source.as_str()),
        );
        payload.insert("scanned_files".into(), Value::from(self.scanned_files));
        payload.insert(
            "skipped_generated".into(),
            Value::from(self.skipped_generated),
        );
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
