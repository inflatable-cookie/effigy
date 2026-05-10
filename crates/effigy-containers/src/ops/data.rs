use std::path::PathBuf;

use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerDataOperation {
    List,
    Export(ContainerDataTransferOperation),
    Import(ContainerDataTransferOperation),
    PullProduction(ContainerPromptedOperation),
    Seed(ContainerPromptedOperation),
    Dump(ContainerDumpOperation),
}

impl ContainerDataOperation {
    pub fn list() -> Self {
        Self::List
    }

    pub fn export(volume: impl Into<String>, path: PathBuf) -> Self {
        Self::Export(ContainerDataTransferOperation {
            volume: volume.into(),
            path,
        })
    }

    pub fn import(volume: impl Into<String>, path: PathBuf) -> Self {
        Self::Import(ContainerDataTransferOperation {
            volume: volume.into(),
            path,
        })
    }

    pub fn pull_production(assume_yes: bool) -> Self {
        Self::PullProduction(ContainerPromptedOperation { assume_yes })
    }

    pub fn seed(assume_yes: bool) -> Self {
        Self::Seed(ContainerPromptedOperation { assume_yes })
    }

    pub fn dump(push: bool) -> Self {
        Self::Dump(ContainerDumpOperation { push })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::List => ContainerSideEffectClass::ReadsRuntime,
            Self::Export(_) | Self::Dump(_) => ContainerSideEffectClass::WritesHostData,
            Self::Import(_) | Self::PullProduction(_) | Self::Seed(_) => {
                ContainerSideEffectClass::MutatesRuntimeData
            }
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Import(_) => ContainerConfirmationPolicy::RequireConfirmation {
                reason: "operation mutates runtime data",
            },
            Self::PullProduction(operation) | Self::Seed(operation) if !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "operation mutates runtime data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDataTransferOperation {
    pub volume: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerPromptedOperation {
    pub assume_yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerDumpOperation {
    pub push: bool,
}
