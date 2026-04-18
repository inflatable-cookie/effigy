# 014 - Rust-Native Gateway

Generation: `g02`

Status: In Progress (crate shipped; command integration and route registration still open)
Owner: Platform
Created: 2026-04-16
Depends on: 006

## Vision Alignment

Running projects should be discoverable via local domains.
`http://clientname.test` should work in the browser while the container is
running and stop working when it's not. The gateway is the shared
infrastructure that makes this possible without per-project DNS or proxy
configuration.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy ships a Rust-native background process for DNS resolution and reverse
  proxying.
- `*.test` domains resolve to `127.0.0.1` on the host.
- Reverse proxy routes by hostname to the correct project port.
- Optional HTTPS via mkcert certificates.
- Projects register/deregister routes on container up/down.
- Non-container projects can register routes via task lifecycle.
- macOS resolver integration via `/etc/resolver/test`.

## Vision Target Delta

- Move from `localhost:PORT for every project` toward `named .test domains
  with automatic DNS and proxying`.

## 1) Problem

With multiple projects running simultaneously:

- port numbers are arbitrary and hard to remember
- URLs don't match production patterns
- no HTTPS for testing TLS-dependent features
- no automatic cleanup when projects stop

## 2) Goals

- [ ] Implement DNS resolver for `*.test` using `hickory-dns`.
- [ ] Implement reverse proxy using `hyper` / `tower` with hostname-based
      routing.
- [ ] Implement TLS termination using mkcert-generated certificates.
- [ ] Define `effigy gateway up/down/status` command surface.
- [ ] Define `dns.domain` manifest field for per-project domains.
- [ ] Implement file-based route table at `~/.effigy/gateway/routes.json`.
- [ ] Implement route registration on `container up` / deregistration on
      `container down`.
- [ ] Implement macOS `/etc/resolver/test` setup and teardown.
- [ ] Implement `effigy gateway setup-tls` for one-time mkcert CA
      installation.
- [ ] Support non-container project routes via task lifecycle.
- [ ] Prove with multiple projects running simultaneously.

## 3) Non-Goals

- [ ] No load balancing or health-aware routing — this is local dev, not
      production infrastructure.
- [ ] No WebSocket upgrade in the first proof — add if needed.
- [ ] No automatic port discovery — routes declare explicit ports.
- [ ] No Linux or Windows resolver integration in the first proof (macOS
      first).

## 4) Contract Direction

### 4.1 Gateway Runtime

The gateway is a host-native background process, not a container. It starts as
a forked subprocess of effigy and writes its PID to
`~/.effigy/gateway/gateway.pid`.

```bash
effigy gateway up       # starts background process
effigy gateway down     # stops it (SIGTERM)
effigy gateway status   # shows PID, uptime, registered routes
```

### 4.2 DNS Resolver

Listens on `127.0.0.1:15353` (or configurable port). Responds to A queries for
`*.test` with `127.0.0.1`. Forwards all other queries upstream.

macOS integration: write `/etc/resolver/test` pointing to the resolver port.
This is a standard macOS convention — the OS resolver checks this directory for
per-TLD overrides.

### 4.3 Reverse Proxy

Listens on `127.0.0.1:80` (HTTP) and optionally `127.0.0.1:443` (HTTPS).
Routes based on `Host` header to the target port from the route table.

### 4.4 Route Table

```json
{
  "routes": [
    {
      "domain": "clientname.test",
      "target": "localhost:8080",
      "source": "container",
      "project": "/path/to/project",
      "tls": false,
      "registered": "2026-04-16T10:00:00Z"
    }
  ]
}
```

The gateway watches this file for changes (inotify/kqueue). Container
lifecycle events and task lifecycle events update the file atomically.

### 4.5 TLS

When `tls = true` in the DNS config:

1. `effigy gateway setup-tls` installs the mkcert CA (one-time).
2. On route registration, effigy generates a certificate for the domain via
   `mkcert`.
3. Certificates stored in `~/.effigy/gateway/certs/`.
4. The gateway serves them on port 443.

`.test` without TLS works out of the box. `.dev` requires TLS due to HSTS
preload.

### 4.6 Non-Container Routes

```toml
[tasks.dev]
gateway_route = { domain = "myrust.test", port = 3000 }
run = "cargo run"
```

Route registers when the task starts, deregisters when it stops. The gateway
is port-agnostic — it doesn't know or care what's behind the port.

## 5) Implementation Approach

### 5.1 New Crate

`crates/effigy-gateway` — isolated library crate.

Dependencies:

- `hickory-dns` — DNS resolver
- `hyper` / `tower` — HTTP reverse proxy
- `rustls` — TLS termination
- `notify` or `kqueue` — file watching for route table

Public API surface:

- gateway lifecycle (start, stop, status)
- route table management (add, remove, list)
- DNS resolver configuration
- TLS certificate management

### 5.2 Integration Boundary

The crate is a pure library. CLI integration (`effigy gateway` commands) and
container lifecycle hooks happen after `g02.010` modularization completes.

### 5.3 Testing Strategy

- Unit tests for DNS query handling.
- Unit tests for proxy routing logic.
- Integration test that starts the gateway, registers a route, makes an HTTP
  request through the proxy, verifies routing.
- macOS resolver test (manual, requires sudo).

## 6) Milestone Relationship

The gateway is independently valuable and has no dependency on `g02.011`–
`g02.013` for its core functionality. It can be developed in parallel.

However, `g02.013` (dev front door) integrates with the gateway for auto-start.
And `g02.011`/`g02.012` determine the container lifecycle hooks that update the
route table.

## Next Task

`g02.014` is now the active integration lane. Execute `266` first: make the
host-native `effigy gateway up/down/status` surface real, then wire DNS
contract and route registration in the next batch.
