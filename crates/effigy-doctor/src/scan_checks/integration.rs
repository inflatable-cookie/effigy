use super::core::{DoctorIntegratedScanOptions, DoctorIntegratedScanResult};
use super::*;

impl DoctorIntegratedScanOptions for GodFileScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for DuplicateBlockScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanResult for GodFileScanResult {
    type Finding = GodFileFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for DuplicateBlockScanResult {
    type Finding = DuplicateBlockFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}
