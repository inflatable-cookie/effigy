//! The rendered alias is an exact /etc/hosts entry in the created guest. Never
//! use DNS, a network gateway or an unbounded resolver as a credential probe.
use super::*;
use crate::runner::workspace_client::context::VerifiedWorkspaceContext;
use std::net::IpAddr;

pub(crate) trait ChannelOpener {
    fn open(&mut self, context: &VerifiedWorkspaceContext) -> io::Result<Box<dyn Transport>>;
}
#[derive(Debug)]
pub(crate) struct PinnedChannelOpener {
    pub(crate) timeout: Duration,
}
impl ChannelOpener for PinnedChannelOpener {
    fn open(&mut self, context: &VerifiedWorkspaceContext) -> io::Result<Box<dyn Transport>> {
        if self.timeout.is_zero() {
            return Err(invalid("workspace TLS budget is zero"));
        }
        let until = Instant::now()
            .checked_add(self.timeout)
            .ok_or_else(|| invalid("invalid workspace TLS deadline"))?;
        if context.protocol != 1 {
            return Err(invalid("unsupported workspace channel protocol"));
        }
        let endpoint = context
            .channel
            .endpoint
            .as_deref()
            .ok_or_else(|| invalid("workspace channel unavailable"))?;
        let pin = context
            .channel
            .server_identity
            .as_deref()
            .and_then(|text| text.strip_prefix("sha256:"))
            .and_then(Digest::parse_hex)
            .ok_or_else(|| invalid("workspace channel pin unknown"))?;
        // Validate the pin and locator before reading any resolution source.
        if pin == Digest([0; 32]) {
            return Err(invalid("workspace channel pin unknown"));
        }
        let port = endpoint
            .strip_prefix("effigy-host:")
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p != 0)
            .ok_or_else(|| invalid("workspace channel locator invalid"))?;
        let file = std::fs::File::open("/etc/hosts")?;
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err(invalid("workspace hosts file exceeds limit"));
        }
        let address = resolve_hosts(&bytes)?;
        Ok(Box::new(TlsChannelTransport::connect(
            SocketAddr::new(address, port),
            pin,
            until
                .checked_duration_since(Instant::now())
                .ok_or_else(|| invalid("workspace TLS deadline expired"))?,
        )?))
    }
}
/// Multiple distinct alias answers are ambiguous, even if one would connect.
pub(crate) fn resolve_hosts(bytes: &[u8]) -> io::Result<IpAddr> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| invalid("workspace hosts encoding invalid"))?;
    let mut found = None;
    for line in text.lines() {
        let mut fields = line.split('#').next().unwrap_or("").split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        if fields.any(|name| name == "effigy-host") {
            let address: IpAddr = address
                .parse()
                .map_err(|_| invalid("workspace host address invalid"))?;
            if address.is_unspecified()
                || address.is_multicast()
                || found.is_some_and(|previous| previous != address)
            {
                return Err(invalid("workspace host address ambiguous"));
            }
            found = Some(address);
        }
    }
    found.ok_or_else(|| invalid("workspace host address unknown"))
}
