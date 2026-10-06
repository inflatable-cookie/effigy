# Questions

Consumer application wiring of scoped worker hosts remains on Queue lead
`af3ab7c7-b631-4d4f-9da7-f2949ded82f3`.

## Q-001 — Gateway PID identity

Status: answered
Asked: 2026-10-05
Answered: 2026-10-05 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom's v0.14.0 publication block remains in force until the start-identity
sidecar and fail-closed legacy policy are merged and final release assurance
completes. He does not accept the PID-only risk and authorized dispatch of the
bounded correction now implemented in task 104. The owning architecture
records the implementation, residual TOCTOU, and remaining assurance boundary.

## Q-002 — macOS gateway cross-user identity access

Status: answered
Asked: 2026-10-05
Answered: 2026-10-06 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom approved the narrow, bounded read-only identity command through Effigy's
existing administrator elevation. It reads only the trusted recorded gateway
target and returns bounded identity/status data. Authentication may be required;
declined or non-interactive authentication yields unknown, and lifecycle commands
refuse to signal. The reader does not signal, change records, or start anything.

No new helper, install, standing privilege, live-operations authority or change
to host-run contract 010 is approved. The larger privilege model is not approved.
Q-001 still blocks v0.14.0 publication until the sidecar and fail-closed legacy
policy are implemented and verified. Decision: `634c5bd1-b3c9-4f18-88cc-4fd16d117b82`.
