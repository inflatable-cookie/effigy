use std::path::PathBuf;

use crate::DoctorError;

pub(crate) fn current_working_dir() -> Result<PathBuf, DoctorError> {
    std::env::current_dir().map_err(DoctorError::cwd_failure)
}
