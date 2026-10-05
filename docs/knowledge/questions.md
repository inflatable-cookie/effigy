# Questions

Consumer application wiring of scoped worker hosts remains on Queue lead
`af3ab7c7-b631-4d4f-9da7-f2949ded82f3`.

## Q-001 — Gateway PID identity

Status: answered
Asked: 2026-10-05
Answered: 2026-10-05 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom blocks v0.14.0 publication until the start-identity sidecar and fail-closed
legacy policy are implemented. He does not accept the current PID-only risk
and authorized dispatch of the bounded correction. The owning architecture
records its scope and remaining proof requirements.

## Q-002 — macOS gateway cross-user identity access

Status: open
Asked: 2026-10-05
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

The precise macOS start-identity reader cannot inspect the existing root-owned
gateway from the ordinary operator uid through the supported API. Q-001's
publication block remains; this is a separate implementation boundary.

Proposed smallest correction: authorize a narrowly scoped, read-only identity
operation through the existing gateway administrator elevation path. It must
read only the trusted recorded gateway target, return bounded identity/status
data, and never signal, change records, or start a daemon. Reading a root
daemon may require administrator authentication; denied, unavailable or
non-interactive elevation remains unknown and fails closed. No installed
helper or host-run protocol change is proposed. Changing the daemon privilege
model is the larger alternative. Neither option is authorized yet.
