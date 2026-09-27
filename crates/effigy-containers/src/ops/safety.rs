#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerSideEffectClass {
    ReadsRuntime,
    InteractsWithRuntime,
    WritesHostData,
    MutatesRuntimeData,
    RemovesCacheData,
    StartsRuntime,
    StopsRuntime,
    RecreatesRuntime,
    DestroysRuntimeData,
}

impl ContainerSideEffectClass {
    /// Host vault injection is only for bring-up. Read, exec, and teardown
    /// reuse whatever the running runtime already has.
    pub fn consumes_declared_container_secrets(self) -> bool {
        matches!(self, Self::StartsRuntime | Self::RecreatesRuntime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerConfirmationPolicy {
    NoConfirmationRequired,
    RequireConfirmation { reason: &'static str },
}
