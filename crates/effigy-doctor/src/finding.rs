#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DoctorSeverity {
    Info,
    Warning,
    Error,
}

impl DoctorSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorFinding {
    pub check_id: String,
    pub severity: DoctorSeverity,
    pub evidence: String,
    pub remediation: String,
    pub fixable: bool,
}

pub trait FindingSink {
    fn add_check_error(&mut self, check_id: &str, evidence: String, remediation: String);
}
