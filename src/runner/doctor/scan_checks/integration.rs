use super::core::{DoctorIntegratedScanOptions, DoctorIntegratedScanResult};
use super::*;

impl DoctorIntegratedScanOptions for GodFileScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for GeneratedAssetScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for GeneratedInSrcScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for DuplicateBlockScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for CommentRatioScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for AttentionMarkerScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for StaleSuppressionScanOptions {
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

impl DoctorIntegratedScanResult for GeneratedAssetScanResult {
    type Finding = GeneratedAssetFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for GeneratedInSrcScanResult {
    type Finding = GeneratedInSrcFinding;

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

impl DoctorIntegratedScanResult for CommentRatioScanResult {
    type Finding = CommentRatioFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for AttentionMarkerScanResult {
    type Finding = AttentionMarkerFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for StaleSuppressionScanResult {
    type Finding = StaleSuppressionFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}
