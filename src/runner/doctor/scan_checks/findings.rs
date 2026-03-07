use super::core::DoctorIntegratedScanFinding;
use super::*;

impl DoctorIntegratedScanFinding for GodFileFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GodFileSeverity::Warning => DoctorSeverity::Warning,
            GodFileSeverity::High | GodFileSeverity::Critical => DoctorSeverity::Error,
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} code lines ({} total) [{}] {}",
            self.code_lines,
            self.total_lines,
            self.severity.as_str(),
            self.path
        )
    }
}

impl DoctorIntegratedScanFinding for GeneratedAssetFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GeneratedAssetSeverity::Warning => DoctorSeverity::Warning,
            GeneratedAssetSeverity::High | GeneratedAssetSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} [{}] {} ({})",
            format_bytes(self.bytes),
            self.severity.as_str(),
            self.path,
            self.reason
        )
    }
}

impl DoctorIntegratedScanFinding for GeneratedInSrcFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GeneratedInSrcSeverity::Warning => DoctorSeverity::Warning,
            GeneratedInSrcSeverity::High | GeneratedInSrcSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} [{}] {} ({}/{})",
            format_bytes(self.size_bytes),
            self.severity.as_str(),
            self.path,
            self.category.as_str(),
            self.reason
        )
    }
}

impl DoctorIntegratedScanFinding for DuplicateBlockFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            DuplicateBlockSeverity::Warning => DoctorSeverity::Warning,
            DuplicateBlockSeverity::High | DuplicateBlockSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        let locations = self
            .locations
            .iter()
            .map(|location| {
                format!(
                    "{}:{}-{}",
                    location.path, location.start_line, location.end_line
                )
            })
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "{} lines [{}] {} occurrences ({})",
            self.block_lines,
            self.severity.as_str(),
            self.occurrences,
            locations
        )
    }
}

impl DoctorIntegratedScanFinding for CommentRatioFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            CommentRatioSeverity::Warning => DoctorSeverity::Warning,
            CommentRatioSeverity::High | CommentRatioSeverity::Critical => DoctorSeverity::Error,
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "ratio={} [{}] {} comment / {} code ({})",
            format_ratio(self.ratio),
            self.severity.as_str(),
            self.comment_lines,
            self.code_lines,
            self.path
        )
    }
}

impl DoctorIntegratedScanFinding for AttentionMarkerFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            AttentionMarkerSeverity::Warning => DoctorSeverity::Warning,
            AttentionMarkerSeverity::High | AttentionMarkerSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{}:{} [{}] {} [{}] {}",
            self.path,
            self.line,
            self.severity.as_str(),
            self.category.as_str(),
            self.marker,
            self.snippet
        )
    }
}

impl DoctorIntegratedScanFinding for StaleSuppressionFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            StaleSuppressionSeverity::Warning => DoctorSeverity::Warning,
            StaleSuppressionSeverity::High | StaleSuppressionSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{}:{} [{}] {} [{}] {}",
            self.path,
            self.line,
            self.severity.as_str(),
            self.category.as_str(),
            self.marker,
            self.snippet
        )
    }
}
