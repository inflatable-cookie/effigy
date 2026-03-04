#[path = "report/state.rs"]
mod state;
#[path = "report/types.rs"]
mod types;

pub(in crate::runner) use state::DoctorState;
pub(in crate::runner) use types::{
    DoctorFinding, DoctorFixAction, DoctorFixStatus, DoctorReport, DoctorSeverity, ManifestSnapshot,
};
