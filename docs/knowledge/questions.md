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

Status: answered
Asked: 2026-10-18
Answered: 2026-10-18 by Tom

Effigy is the environment authority. It owns identity, the desired-state
record, the render, data claims and holders, routes, and who may change an
environment. Compose is the engine that applies the render on the Colima and
Docker backends and reports what is running.

- Direct `compose_file` mode is retired, provided the migration is clean (Tom:
  "If there's a clean migration to managed catalog fragments for the two
  projects, then let's do it"). For one release a `compose_file` container is
  read as a project-layer catalog fragment, with a deprecation warning, and
  `effigy container migrate` writes the fragment out. After that release the
  key is retired, with an error naming the command.
- The generated Compose stays internal: there is no mutating pass-through
  (Tom: "Probably stay internal"). A read-only `effigy container render <env>`
  shows the exact render, path and digest for the current generation.
- The backend seam is "reconcile this environment from its render", not "run
  Compose". Apple Containers, when adopted, brings its own answer to Compose
  (Tom: "if we do Apple Containers, then we need to come with a solution for
  Compose one way or another").

## Q-005 — Service catalog strategy

Status: answered
Asked: 2026-10-18
Answered: 2026-10-18 by Tom ("Yeah that sounds spot on")

The goal stands: projects, bundles and third parties can add or change
services, workspace images and configuration without an Effigy release. Pinned
source is enough; there is no central registry.

The mechanism is catalog sources, which are directories of service fragments
resolved in this order:

1. the project's `.effigy/catalog/`;
2. a `catalog/` directory shipped inside each bundle;
3. declared extra sources (`[catalog.sources.<name>]`, from git, OCI or a path,
   pinned per checkout and updated by explicit sync like bundles);
4. the compiled baseline.

A name can be qualified (`source/service`), and `service list` and `config`
show where each service came from. Fragments declare the capabilities and
schema version they need, so an older Effigy names the version it requires.

The installed catalog-pack runtime is retired. That covers the store,
install/update/rollback/reset, the OCI channel, the workspace pack handoff and
the support policy. The pack repository is archived and its fragments fold
back into the baseline. Architecture
[026](architecture/026-feature-placement-and-command-surface.md) §Service
Catalogs is superseded and is rewritten by the implementing change.

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
