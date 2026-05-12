use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerExecOperation {
    Captured(ContainerCapturedExecOperation),
    Shell(ContainerShellOperation),
}

impl ContainerExecOperation {
    pub fn captured(
        service: Option<String>,
        command: Vec<String>,
        stdin_file: Option<PathBuf>,
    ) -> Self {
        Self::Captured(ContainerCapturedExecOperation {
            service,
            command,
            stdin_file,
            cwd: None,
            env: BTreeMap::new(),
        })
    }

    pub fn shell(service: Option<String>, command: Option<String>, interactive: bool) -> Self {
        Self::Shell(ContainerShellOperation {
            service,
            command,
            interactive,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        ContainerSideEffectClass::InteractsWithRuntime
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        ContainerConfirmationPolicy::NoConfirmationRequired
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCapturedExecOperation {
    pub service: Option<String>,
    pub command: Vec<String>,
    pub stdin_file: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerShellOperation {
    pub service: Option<String>,
    pub command: Option<String>,
    pub interactive: bool,
}
