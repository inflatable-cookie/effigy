use std::path::{Path, PathBuf};

use crate::path_error_text::{
    failed_to_parse_toml_syntax_in_path, failed_to_read_path, strict_manifest_parse_failed_in_path,
};

use super::super::contracts::{check_id, remediation};
use super::super::report::DoctorState;

pub(in crate::runner::doctor) enum ManifestParseFinding {
    Read {
        manifest_path: PathBuf,
        error: String,
    },
    TomlSyntax {
        manifest_path: PathBuf,
        error: String,
    },
    StrictParse {
        manifest_path: PathBuf,
        error: String,
    },
}

impl ManifestParseFinding {
    pub(in crate::runner::doctor) fn read_failure(
        manifest_path: &Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::Read {
            manifest_path: manifest_path.to_path_buf(),
            error: error.to_string(),
        }
    }

    pub(in crate::runner::doctor) fn toml_syntax_failure(
        manifest_path: &Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::TomlSyntax {
            manifest_path: manifest_path.to_path_buf(),
            error: error.to_string(),
        }
    }

    pub(in crate::runner::doctor) fn strict_parse_failure(
        manifest_path: &Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::StrictParse {
            manifest_path: manifest_path.to_path_buf(),
            error: error.to_string(),
        }
    }

    pub(in crate::runner::doctor) fn emit(self, state: &mut DoctorState) {
        let (evidence, remediation) = match self {
            Self::Read {
                manifest_path,
                error,
            } => (
                failed_to_read_path(&manifest_path, error),
                remediation::MANIFEST_READ_FAILURE,
            ),
            Self::TomlSyntax {
                manifest_path,
                error,
            } => (
                failed_to_parse_toml_syntax_in_path(&manifest_path, error),
                remediation::MANIFEST_TOML_SYNTAX,
            ),
            Self::StrictParse {
                manifest_path,
                error,
            } => (
                strict_manifest_parse_failed_in_path(&manifest_path, error),
                remediation::MANIFEST_STRICT_PARSE,
            ),
        };

        state.add_check_error(check_id::MANIFEST_PARSE, evidence, remediation);
    }
}
