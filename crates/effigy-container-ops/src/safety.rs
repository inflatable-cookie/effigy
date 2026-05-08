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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerConfirmationPolicy {
    NoConfirmationRequired,
    RequireConfirmation { reason: &'static str },
}
