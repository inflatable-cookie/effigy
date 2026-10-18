# Questions

Consumer application wiring of scoped worker hosts remains on Queue lead
`af3ab7c7-b631-4d4f-9da7-f2949ded82f3`.

## Q-001 — Gateway PID identity

Status: answered
Asked: 2026-10-05
Answered: 2026-10-05 by Tom
Owner: [Gateway process identity ruling](architecture/020-container-infrastructure-design.md#gateway-process-identity-ruling)

Tom's v0.14.0 publication block remains in force until final release assurance
completes. He does not accept the PID-only risk and authorized dispatch of the
bounded correction now implemented in task 104 and merged in PR 220. The owning
architecture records the implementation, residual TOCTOU, and remaining
assurance boundary; this does not claim publication readiness.

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
Q-001's publication block remains until exact-candidate release assurance
completes; task 104's sidecar and fail-closed legacy policy are merged, with the
documented TOCTOU and cross-UID proof limitation still visible. Decision:
`634c5bd1-b3c9-4f18-88cc-4fd16d117b82`.

## Q-003 — Production deployment ownership

Status: open
Asked: 2026-10-18

Should production deployment be part of Effigy, or a separate tool? Tom would
like Effigy to be a cohesive end-to-end development tool, but says this needs
thinking through properly first. The current `deploy` surface reports success
for obligations it never executes; see the October 2026 deep audit, analysis
D8.

## Q-004 — Relationship to Compose

Status: open
Asked: 2026-10-18

Effigy is probably not a Compose replacement, but it has to understand
Compose. Which parts of environment lifecycle Effigy owns, and which it
delegates to Compose, needs its own discussion with Tom.

## Q-005 — Service catalog strategy

Status: open
Asked: 2026-10-18

The catalog-pack split was meant to let container services be added without
new Effigy releases. Tom agrees that the split isn't helping much (2026-10-18).
The goal still stands; the open question is what mechanism achieves it.

## Q-006 — Heavy tasks

Status: open
Asked: 2026-10-18

Agreed so far (Tom, 2026-10-18):

- A task is heavy only when it is marked `admission = "heavy"` and the host has
  opted in.
- The `qa`/`ci`/`ci:fresh` name rule goes.
- Host-wide scheduling is not in Effigy's scope.

Open: what remains of heavy-task handling, if anything, and what shape it
takes.
