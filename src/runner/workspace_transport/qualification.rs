//! Backend reachability facts are observations, never address guesses. This
//! layer runs after ordinary apply creates the exact target container/network.
use super::*;
use crate::runner::workspace_client::context::{
    WorkspaceChannelLocator, WorkspaceContainerIdentity,
};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::workspace_session::{ChannelBound, ServingProbe, SessionBinding, SessionLaunch};
use std::net::IpAddr;

/// Required backend families. Rootless and Apple Containers have no route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    Colima,
    DockerDesktop,
    LinuxDocker,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindRoute {
    Proxy { host_bind: IpAddr },
    Direct,
}
#[derive(Debug, Clone)]
pub(crate) struct ChannelAliasPlan {
    pub(crate) backend: Backend,
    pub(crate) profile: String,
    pub(crate) alias: String,
    pub(crate) render_value: String,
    pub(crate) route: BindRoute,
}
impl ChannelAliasPlan {
    /// `proxy_bind` and the Colima gateway must come from the selected backend's
    /// observations. Unknown proxy reachability cannot become wildcard/loopback.
    pub(crate) fn observed(
        backend: Backend,
        profile: &str,
        proxy_bind: Option<IpAddr>,
        colima_gateway: Option<IpAddr>,
    ) -> io::Result<Self> {
        if profile.trim().is_empty() {
            return Err(invalid("backend profile is unknown"));
        }
        let route = match backend {
            Backend::LinuxDocker => BindRoute::Direct,
            Backend::Colima | Backend::DockerDesktop => BindRoute::Proxy {
                host_bind: proxy_bind
                    .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
                    .ok_or_else(|| invalid("unobserved proxy bind address"))?,
            },
        };
        let render_value = match backend {
            Backend::Colima => colima_gateway
                .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
                .ok_or_else(|| invalid("unobserved Colima gateway"))?
                .to_string(),
            Backend::DockerDesktop | Backend::LinuxDocker => "host-gateway".into(),
        };
        Ok(Self {
            backend,
            profile: profile.to_owned(),
            alias: "effigy-host".into(),
            render_value,
            route,
        })
    }
}
#[derive(Debug, Clone)]
pub(crate) struct ResolvedAlias {
    pub(crate) target: WorkspaceContainerIdentity,
    pub(crate) address: IpAddr,
}
#[derive(Debug, Clone)]
pub(crate) struct ChannelBind {
    pub(crate) listen: SocketAddr,
    pub(crate) alias: String,
    pub(crate) expected_peer: IpAddr,
}
pub(crate) fn channel_bind(
    plan: &ChannelAliasPlan,
    target: &WorkspaceContainerIdentity,
    resolved: &ResolvedAlias,
) -> io::Result<ChannelBind> {
    if target != &resolved.target
        || resolved.address.is_unspecified()
        || resolved.address.is_multicast()
    {
        return Err(invalid("guest alias or exact target is unproven"));
    }
    let listen_ip = match plan.route {
        BindRoute::Direct => resolved.address,
        BindRoute::Proxy { host_bind } => host_bind,
    };
    Ok(ChannelBind {
        listen: SocketAddr::new(listen_ip, 0),
        alias: plan.alias.clone(),
        expected_peer: resolved.address,
    })
}
#[derive(Debug, Clone)]
pub(crate) struct GuestDialEvidence {
    pub(crate) target: WorkspaceContainerIdentity,
    pub(crate) peer: SocketAddr,
    pub(crate) pin: Digest,
}
/// The engine adapter executes context-free resolution/TLS-only dial in the
/// exact staged guest. Both methods must honor the remaining operation budget.
/// A dial sends no Hello, capability, credential or enrolment frame.
pub(crate) trait GuestChannelProbe {
    fn resolve(
        &mut self,
        target: &WorkspaceContainerIdentity,
        alias: &str,
        budget: Duration,
    ) -> Observed<ResolvedAlias>;
    fn dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<GuestDialEvidence>;
}

/// Non-secret preparation receipt. Only qualification plus the session's own
/// ChannelBound can construct it; a bare socket cannot publish a locator.
#[derive(Debug, Clone)]
pub(crate) struct QualifiedHostLocator {
    session: SessionBinding,
    target: WorkspaceContainerIdentity,
    channel: WorkspaceChannelLocator,
    bound: ChannelBound,
}

impl QualifiedHostLocator {
    pub(crate) fn session(&self) -> &SessionBinding {
        &self.session
    }
    pub(crate) fn target(&self) -> &WorkspaceContainerIdentity {
        &self.target
    }
    pub(crate) fn channel(&self) -> &WorkspaceChannelLocator {
        &self.channel
    }
    pub(crate) fn bound(&self) -> &ChannelBound {
        &self.bound
    }
}

pub(crate) struct GuestServingProbe<'a> {
    pub(crate) guest: &'a mut dyn GuestChannelProbe,
    pub(crate) target: &'a WorkspaceContainerIdentity,
    pub(crate) bind: &'a ChannelBind,
    pub(crate) qualified_pin: Option<Digest>,
}
impl std::fmt::Debug for GuestServingProbe<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuestServingProbe")
            .field("target", self.target)
            .field("bind", self.bind)
            .finish_non_exhaustive()
    }
}
impl ServingProbe for GuestServingProbe<'_> {
    fn pinned_handshake(
        &mut self,
        endpoint: SocketAddr,
        pin: &Digest,
        budget: Duration,
    ) -> Observed<()> {
        self.qualified_pin = None;
        if budget.is_zero() || endpoint.ip() != self.bind.listen.ip() || endpoint.port() == 0 {
            return Observed::Unknown(UnknownReason::Unparsable("invalid bound channel".into()));
        }
        let locator = format!("{}:{}", self.bind.alias, endpoint.port());
        let until = Instant::now().checked_add(budget);
        let reply = self.guest.dial(self.target, &locator, pin, budget);
        if until.is_none_or(|until| Instant::now() >= until) {
            return Observed::Unknown(UnknownReason::Unparsable(
                "guest TLS qualification deadline expired".into(),
            ));
        }
        match reply {
            Observed::Verified(reply, evidence)
                if reply.target == *self.target
                    && reply.peer == SocketAddr::new(self.bind.expected_peer, endpoint.port())
                    && reply.pin == *pin =>
            {
                self.qualified_pin = Some(pin.clone());
                Observed::Verified((), evidence)
            }
            Observed::Verified(..) => Observed::Unknown(UnknownReason::Unparsable(
                "guest TLS target, peer or pin mismatch".into(),
            )),
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }
}
impl GuestServingProbe<'_> {
    pub(crate) fn qualify(
        &mut self,
        launch: &mut SessionLaunch,
        run: &effigy_kernel::run::Run<'_>,
        budget: Duration,
    ) -> io::Result<QualifiedHostLocator> {
        launch
            .qualify(self, budget)
            .map_err(|_| invalid("guest TLS channel qualification refused"))?;
        let bound = launch
            .publish(run)
            .map_err(|_| invalid("session channel publication refused"))?;
        if self.qualified_pin.as_ref() != Some(bound.lease().pin()) {
            return Err(invalid("session channel pin changed"));
        }
        let endpoint = launch
            .endpoint()
            .ok_or_else(|| invalid("session listener stopped"))?;
        Ok(QualifiedHostLocator {
            session: bound.session().clone(),
            target: self.target.clone(),
            channel: WorkspaceChannelLocator {
                endpoint: Some(format!("{}:{}", self.bind.alias, endpoint.port())),
                server_identity: Some(format!("sha256:{}", bound.lease().pin().hex())),
            },
            bound,
        })
    }
}

/// Called by the owning activation only after apply created the exact target.
/// Hybrid returns no listener, key, probe or locator.
#[derive(Debug)]
pub(crate) struct ChannelPreparation<'a> {
    pub(crate) container_mode: bool,
    pub(crate) plan: &'a ChannelAliasPlan,
    pub(crate) target: &'a WorkspaceContainerIdentity,
    pub(crate) session: &'a effigy_kernel::workspace_session::SessionRef,
    pub(crate) supervisor: &'a str,
}
#[derive(Debug)]
pub(crate) struct PreparedHostChannel {
    pub(crate) launch: SessionLaunch,
    pub(crate) acceptor: super::listener::HostChannelAcceptor,
    pub(crate) locator: QualifiedHostLocator,
}
pub(crate) fn prepare_channel(
    preparation: ChannelPreparation<'_>,
    run: &mut effigy_kernel::run::Run<'_>,
    guest: &mut dyn GuestChannelProbe,
    budget: Duration,
) -> io::Result<Option<PreparedHostChannel>> {
    if !preparation.container_mode {
        return Ok(None);
    }
    let until = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| invalid("invalid channel preparation deadline"))?;
    let remaining = || {
        until
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| invalid("channel preparation deadline expired"))
    };
    let target = preparation.target;
    if target.container_id.len() != 64
        || !target.container_id.bytes().all(|b| b.is_ascii_hexdigit())
        || target.vm_boot_id.trim().is_empty()
        || target.platform.trim().is_empty()
        || target.libc.trim().is_empty()
    {
        return Err(invalid("channel target identity is incomplete"));
    }
    let resolved = match guest.resolve(target, &preparation.plan.alias, remaining()?) {
        Observed::Verified(resolved, _) => resolved,
        Observed::Absent(_) | Observed::Unknown(_) => {
            return Err(invalid("guest alias resolution unproven"))
        }
    };
    let bind = channel_bind(preparation.plan, target, &resolved)?;
    let (listener, key, acceptor) = super::listener::SessionTlsListener::bind(
        bind.listen,
        remaining()?.min(Duration::from_secs(5)),
    )?;
    let mut launch = SessionLaunch::launch(
        run,
        preparation.session,
        preparation.supervisor,
        key,
        Box::new(listener),
    )
    .map_err(|_| invalid("session listener custody refused"))?;
    let mut probe = GuestServingProbe {
        guest,
        target,
        bind: &bind,
        qualified_pin: None,
    };
    let qualified = match probe.qualify(
        &mut launch,
        run,
        match remaining() {
            Ok(budget) => budget,
            Err(error) => {
                launch.stop();
                return Err(error);
            }
        },
    ) {
        Ok(qualified) => qualified,
        Err(error) => {
            launch.stop();
            return Err(error);
        }
    };
    Ok(Some(PreparedHostChannel {
        launch,
        acceptor,
        locator: qualified,
    }))
}
