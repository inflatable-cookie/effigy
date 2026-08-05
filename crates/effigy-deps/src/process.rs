use std::path::PathBuf;
use std::process::Command;

use crate::DepsError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait ReadOnlyProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdReadOnlyProcess;

impl ReadOnlyProcess for StdReadOnlyProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        let output = Command::new(&request.program)
            .args(&request.args)
            .current_dir(&request.cwd)
            .output()
            .map_err(|source| DepsError::ProcessSpawn {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                source,
            })?;
        let result = ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        if output.status.success() {
            Ok(result)
        } else {
            Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: result.status,
                stderr: result.stderr,
            })
        }
    }
}
