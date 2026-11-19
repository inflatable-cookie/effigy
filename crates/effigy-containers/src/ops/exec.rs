use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCapturedExecOperation {
    pub service: Option<String>,
    pub command: Vec<String>,
    pub stdin_file: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, OsString>,
}
