use crate::{ContainerConfirmationPolicy, ContainerSideEffectClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerReadOperation {
    Status(ContainerStatusOperation),
    Logs(ContainerLogsOperation),
    Stats(ContainerStatsOperation),
}

impl ContainerReadOperation {
    pub fn status(all: bool) -> Self {
        Self::Status(ContainerStatusOperation { all })
    }

    pub fn logs(service: Option<String>, follow: bool) -> Self {
        Self::Logs(ContainerLogsOperation { service, follow })
    }

    pub fn stats(all: bool) -> Self {
        Self::Stats(ContainerStatsOperation { all })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        ContainerSideEffectClass::ReadsRuntime
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        ContainerConfirmationPolicy::NoConfirmationRequired
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerStatusOperation {
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerLogsOperation {
    pub service: Option<String>,
    pub follow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerStatsOperation {
    pub all: bool,
}
