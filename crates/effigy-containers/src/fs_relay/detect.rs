//! Pure activation policy for the relay.
use crate::{EffectiveFsEventsConfig, EffectiveFsEventsMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsEventsState {
    Active,
    Inactive,
    Noop,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsEventsDecision {
    pub state: FsEventsState,
    pub reason: &'static str,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    Dev,
    ContainerUp,
    Shell,
    Other,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    LinuxDockerEngine,
    OrbStack,
    Other,
}
/// Classify observed host and Docker endpoint facts without probing the environment.
pub fn classify_backend(
    linux_host: bool,
    docker_engine: bool,
    socket_or_context: Option<&str>,
    operating_system: Option<&str>,
) -> Backend {
    if linux_host && docker_engine {
        return Backend::LinuxDockerEngine;
    }
    if !linux_host
        && (socket_or_context.is_some_and(|s| s.to_ascii_lowercase().contains("orbstack"))
            || operating_system.is_some_and(|s| s.eq_ignore_ascii_case("orbstack")))
    {
        return Backend::OrbStack;
    }
    Backend::Other
}

#[derive(Debug, Clone, Copy)]
pub struct ActivationContext {
    pub inside_guest: bool,
    pub backend: Backend,
    pub invocation: Invocation,
    pub attached: bool,
    pub interactive_tty: bool,
    pub headless: bool,
    pub ci: bool,
    pub ephemeral: bool,
}

pub fn resolve_fs_events_activation(
    config: &EffectiveFsEventsConfig,
    ctx: &ActivationContext,
) -> FsEventsDecision {
    use FsEventsState::{Active, Inactive, Noop};
    let (state, reason) = if ctx.inside_guest {
        (Inactive, "inside_guest")
    } else if config.mode == EffectiveFsEventsMode::Off {
        (Inactive, "explicit_off")
    } else if ctx.backend == Backend::LinuxDockerEngine {
        (Noop, "native_kernel_inotify")
    } else if ctx.backend == Backend::OrbStack {
        (Noop, "orbstack_native_inotify")
    } else if config.mode == EffectiveFsEventsMode::On {
        (Active, "explicit_on")
    } else if ctx.headless || ctx.ci || ctx.ephemeral {
        (Inactive, "headless_or_ephemeral")
    } else {
        match ctx.invocation {
            Invocation::Dev if ctx.attached => (Active, "interactive_dev"),
            Invocation::ContainerUp if !ctx.attached => (Inactive, "detached_auto_off"),
            Invocation::ContainerUp if ctx.interactive_tty => (Active, "attached_container_up"),
            Invocation::Shell if ctx.interactive_tty => (Active, "interactive_shell"),
            _ => (Inactive, "headless_or_ephemeral"),
        }
    };
    FsEventsDecision { state, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_auto_matrix() {
        let mut c = EffectiveFsEventsConfig::default();
        let mut x = ActivationContext {
            inside_guest: false,
            backend: Backend::Other,
            invocation: Invocation::Dev,
            attached: true,
            interactive_tty: true,
            headless: false,
            ci: false,
            ephemeral: false,
        };
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "interactive_dev"
        );
        x.invocation = Invocation::ContainerUp;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "attached_container_up"
        );
        x.attached = false;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "detached_auto_off"
        );
        x.invocation = Invocation::Shell;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "interactive_shell"
        );
        x.ci = true;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "headless_or_ephemeral"
        );
        c.mode = EffectiveFsEventsMode::On;
        assert_eq!(resolve_fs_events_activation(&c, &x).reason, "explicit_on");
        x.backend = Backend::OrbStack;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "orbstack_native_inotify"
        );
        x.backend = Backend::LinuxDockerEngine;
        assert_eq!(
            resolve_fs_events_activation(&c, &x).reason,
            "native_kernel_inotify"
        );
        c.mode = EffectiveFsEventsMode::Off;
        assert_eq!(resolve_fs_events_activation(&c, &x).reason, "explicit_off");
        x.inside_guest = true;
        assert_eq!(resolve_fs_events_activation(&c, &x).reason, "inside_guest");
    }
    #[test]
    fn backend_observations_distinguish_native_from_vm() {
        assert_eq!(
            classify_backend(true, true, None, None),
            Backend::LinuxDockerEngine
        );
        assert_eq!(
            classify_backend(
                false,
                true,
                Some("unix:///Users/me/.orbstack/run/docker.sock"),
                None
            ),
            Backend::OrbStack
        );
        assert_eq!(
            classify_backend(false, true, None, Some("OrbStack")),
            Backend::OrbStack
        );
        assert_eq!(
            classify_backend(false, true, Some("colima"), Some("Docker Desktop")),
            Backend::Other
        );
    }
}
