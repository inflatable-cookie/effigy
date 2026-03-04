use super::super::{LoadedCatalog, ManifestJsPackageManager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::runner) enum DoctorSeverity {
    Info,
    Warning,
    Error,
}

impl DoctorSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    pub(in crate::runner) fn to_notice_level(self) -> crate::ui::NoticeLevel {
        match self {
            Self::Info => crate::ui::NoticeLevel::Info,
            Self::Warning => crate::ui::NoticeLevel::Warning,
            Self::Error => crate::ui::NoticeLevel::Error,
        }
    }

    pub(in crate::runner) fn rank(self) -> u8 {
        match self {
            Self::Error => 3,
            Self::Warning => 2,
            Self::Info => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorFinding {
    pub(in crate::runner) check_id: String,
    pub(in crate::runner) severity: DoctorSeverity,
    pub(in crate::runner) evidence: String,
    pub(in crate::runner) remediation: String,
    pub(in crate::runner) fixable: bool,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorSummary {
    pub(in crate::runner) checks: usize,
    pub(in crate::runner) pass: usize,
    pub(in crate::runner) warning: usize,
    pub(in crate::runner) error: usize,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorReport {
    pub(in crate::runner) summary: DoctorSummary,
    pub(in crate::runner) findings: Vec<DoctorFinding>,
    pub(in crate::runner) fixes: Vec<DoctorFixAction>,
    pub(in crate::runner) root_evidence: Vec<String>,
    pub(in crate::runner) root_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum DoctorFixStatus {
    Applied,
    Skipped,
}

impl DoctorFixStatus {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DoctorFixAction {
    pub(in crate::runner) fix_id: String,
    pub(in crate::runner) status: DoctorFixStatus,
    pub(in crate::runner) detail: String,
}

pub(in crate::runner) struct ManifestSnapshot {
    pub(in crate::runner) manifest_paths: Vec<std::path::PathBuf>,
    pub(in crate::runner) parsed_catalogs: Vec<LoadedCatalog>,
    pub(in crate::runner) preferred_js_pm: Option<ManifestJsPackageManager>,
    pub(in crate::runner) parse_ok_any: bool,
}
