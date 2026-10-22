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
