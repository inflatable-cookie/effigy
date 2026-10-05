# Questions

Consumer application wiring of scoped worker hosts remains on Queue lead
`af3ab7c7-b631-4d4f-9da7-f2949ded82f3`.

## Q-001 — Gateway PID identity for v0.14.0

Status: open
Asked: 2026-10-05
Owner: [Gateway process identity](architecture/020-container-infrastructure-design.md#gateway-process-identity)

v0.14.0 recommendation: do not publish until a persisted start-identity sidecar
and fail-closed legacy/unknown policy land, unless Tom explicitly accepts the
residual. 099/101 closed numeric domain and unknown-probe collapse. They did
not prove the live PID is the gateway. A leftover `gateway.pid` after crash or
reboot can make `status` / `up` / `down` / elevation SIGTERM/SIGKILL a live
unrelated process, including after elevation.

This question is the live ruling. The architecture owner states current
behavior, the bounded proposal, and that disclosure is not acceptance.
