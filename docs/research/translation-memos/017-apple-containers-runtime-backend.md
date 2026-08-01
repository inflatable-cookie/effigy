# Translation Memo 017: Apple Containers Runtime Backend

Status: Complete
Disposition: Watch only after prototype
Memo: 017
Owner: Platform research
Last updated: 2026-08-01
Related contracts: `005-container-runtime-contract.md`,
`006-compose-backend-compatibility.md`, `012-container-manager-contract.md`

## 1) Effigy problem statement

Effigy currently supports Docker Compose and Colima with `nerdctl compose` on
macOS. Both satisfy a Compose-shaped product contract and, on macOS, normally
run the stack inside one shared Linux VM.

Apple Containers 1.2 is now a credible third runtime candidate. It runs each
container in its own lightweight virtual machine, uses OCI images, supports
Dockerfile builds, and exposes native lifecycle, exec, network, volume, port,
log, inspect, and copy commands.

The question is not whether Effigy can invoke the `container` CLI. It can. The
question is whether Effigy can preserve its service-stack contract without
pretending that Apple Containers implements Compose.

## 2) External evidence summary

Apple Containers crossed 1.0 in June 2026 and released 1.2.0 on 2026-07-29.
The tagged 1.2 surface provides:

- one lightweight virtual machine per Linux container
- OCI image pull, push, tag, inspect, and build support
- Dockerfile and Containerfile builds through BuildKit
- container create, run, start, stop, delete, exec, logs, inspect, stats, and
  copy operations
- bind mounts, named volumes, tmpfs, shared memory, resource limits, user and
  working-directory selection, capabilities, and published ports
- custom networks and isolated networks on macOS 26
- structured output on relevant inventory and inspection commands
- Apple-silicon execution, including Rosetta support for some `linux/amd64`
  workloads
- SSH-agent forwarding that follows host socket rotation

The same evidence also exposes a material orchestration gap:

- the 1.2 command reference has no Compose surface
- first-class container-to-container DNS discovery and network-scoped aliases
  remain open upstream
- healthcheck support remains open upstream
- Compose support remains an open request, not a committed compatibility
  contract
- the host-service DNS workaround is global system state and has documented
  Private Relay and restart limitations
- each service VM has its own resource floor, so a multi-service stack may use
  materially more memory than a shared-VM runtime

Apple Containers is therefore a capable container runtime, not a complete
local service orchestrator.

## 3) Recommendation

Adopt Apple Containers as a **prototype candidate for an Effigy-native
backend**, with the future id `apple-container`.

Do not present it as supported yet. Do not model it as another Compose backend.
Docker Compose and Colima remain the supported paths until the prototype passes
the gates in this memo.

The durable architecture is:

1. Promote Effigy's effective container configuration into a typed,
   backend-neutral stack plan.
2. Render that plan to Compose for Docker and Colima.
3. Execute that plan as native `container` operations for Apple Containers.
4. Keep backend selection, lifecycle, reports, and attached-session closeout
   behind `ContainerManager`.

The stack plan must carry at least:

- project and service identity
- image or build definition
- command, environment, user, and working directory
- mounts and named data volumes
- published ports and project network membership
- dependency order and readiness probes
- execution target and gateway metadata
- lifecycle and cleanup policy

Initial Apple scope should cover Effigy-generated catalog stacks only. Direct
`compose_file` and arbitrary Compose overrides stay on Docker or Colima. A
Compose-YAML-to-CLI translator would inherit ambiguous Compose behavior and
become a second, partial Compose implementation inside Effigy.

### Native orchestration ownership

For the prototype, Effigy must own:

- one deterministic project network and project-scoped container names
- image build or pull before container creation
- idempotent create, start, stop, delete, and recovery behavior
- dependency ordering and explicit readiness polling
- service discovery for every participating Effigy-owned container
- live port inspection for gateway registration
- attached logs and interrupt-aware closeout
- volume preservation and explicit destructive cleanup

Service discovery is the hardest current gap. The prototype should test static
project-network addresses plus generated host mappings inside all participating
containers. Apple system DNS should not be the default workaround because it
changes global host state. If reliable service aliases cannot be provided
without global mutation, the backend is not supportable for multi-service
stacks yet.

## 4) Tradeoffs Effigy would accept

| Benefit | Accepted cost or constraint |
| --- | --- |
| per-container VM isolation and failure containment | possible higher aggregate memory use for multi-service stacks |
| no shared Colima/Docker Linux VM lifecycle or disk | macOS 26 and Apple silicon only |
| OCI images and Dockerfile interoperability | no Docker socket/API compatibility for external tools |
| native networking, volumes, ports, exec, logs, and inspect | Effigy owns orchestration that Compose currently supplies |
| SSH forwarding follows rotated host sockets | upstream DNS, alias, healthcheck, and restart-policy gaps remain |
| per-container resource and kernel control | more backend-specific recovery and diagnostics work |

This is an optional macOS backend. It must not weaken cross-platform behavior
or force Apple-only concepts into the manifest.

## 5) What must be true before adoption

- [x] Apple Containers has a stable post-1.0 tagged release
- [x] Core OCI build, lifecycle, exec, network, volume, and port primitives
      exist
- [x] Effigy's manager facade can own a new backend id
- [x] A backend-neutral stack plan replaces Compose invocation as the manager's
      semantic boundary
- [ ] A typical Effigy web, database, and cache stack gets reliable
      service-name discovery without global DNS mutation
- [x] Dependency readiness and failed-start recovery are deterministic
- [ ] Gateway routing survives runtime restart, VPN use, and host-network churn
- [ ] Named-volume preserve, reset, export, and destructive cleanup semantics
      match the current product contract
- [ ] Apple-silicon and Rosetta image compatibility produce clear diagnostics
- [ ] Resource measurements are acceptable for representative four-to-six
      service projects
- [x] Unsupported `compose_file` and override features fail early with a clear
      backend-selection remedy

## 6) Required prototype or validation work

Build one bounded spike outside the default backend path:

1. Add no public manifest grammar and no automatic backend detection.
2. Materialize one catalog-derived stack plan containing app, web, database,
   and cache services.
3. Build or pull images, create the project network, create all containers,
   inject project service mappings, and start in dependency order.
4. Prove routed exec, workspace mounts, database connectivity, published-port
   inspection, gateway registration, logs, stop/start, and teardown.
5. Repeat after interrupted bring-up, runtime service restart, and stale local
   state.
6. Measure cold start, warm start, idle memory, disk use, and I/O against the
   same Docker and Colima stack.
7. Record VPN, Private Relay, SSH-agent rotation, named-volume, and Rosetta
   results.

The prototype passes only if it preserves the Effigy contract without a
machine-global DNS dependency. Passing does not imply arbitrary Compose
compatibility.

## 7) Promotion target

- [x] `concept contract work` — Candidate boundary added to container contracts
- [x] `roadmap execution planning` — Open only after operator approval of the
      prototype lane
- [x] `watch only` — Use if service discovery cannot meet the gate
- [ ] `reject` — Use if resource or recovery results make the backend
      impractical

## 8) Rejected alternatives

### Treat Apple Containers as a Compose executable

Rejected. There is no upstream Compose contract, and Effigy would hide rather
than resolve semantic gaps.

### Translate arbitrary Compose YAML directly to `container` commands

Rejected as the durable design. It creates an incomplete Compose engine and
makes backend behavior depend on unsupported YAML features. A narrow parser may
be useful inside a disposable spike, but it must not become the product seam.

### Replace Docker and Colima immediately

Rejected. Apple Containers is platform-limited and does not serve tools that
require the Docker API. It should widen choice, not remove working paths.

### Use Apple system DNS as the normal service-discovery layer

Rejected as the default. It mutates global host state and carries documented
Private Relay and restart caveats.

## 9) Prototype outcome

The 2026-08-01 prototype proved that the native architecture is workable, but
not yet supportable as an Effigy backend.

| Gate | Result | Evidence |
| --- | --- | --- |
| typed generated-stack plan | pass | Compose and native planning share `EffectiveStackPlan`; unsupported fields fail early |
| representative lifecycle | pass | app/web/Postgres/Redis build, readiness, exec, logs, ports, mounts, restart, recovery, and cleanup pass |
| service discovery | partial/blocker | project-local host mappings work after startup; Apple 1.2 supplies no bare-name DNS or static IP assignment, so peer aliases cannot be guaranteed before a service process boots |
| runtime recovery | pass | stale container recovery takes 6.68s; full Apple runtime restart recovery takes 12.08s with data preserved |
| gateway and network churn | fail/incomplete | the prototype is not wired into gateway registration; VPN and host-network churn remain unproved |
| named data | partial | preserve, restart, reset, and destructive cleanup pass; export/data-pipeline integration is not wired |
| SSH agent | primitive pass | raw Apple `--ssh` forwarding works; the stack plan and manager do not model it yet |
| Rosetta | primitive pass | raw `linux/amd64` Alpine with `--rosetta` reports `x86_64`; selection and diagnostics are not modeled |
| secret delivery | fail/incomplete | native exec and bind primitives exist, but the effective plan does not carry Effigy's runtime secret delivery contract |
| resource cost | caution | four Apple service VMs reported about 240 MiB aggregate guest memory versus about 64 MiB on Docker and 31 MiB on Colima; Apple's persistent builder is configured for another 2 GiB |
| supported-input boundary | pass | direct Compose stays Docker/Colima-only and Apple remains absent from detection and the default registry |

Warm cached startup was 6.13s on Apple, 3.29s on Docker, and 1.76s on Colima.
A 64 MiB synced volume write measured 0.10s, 0.09s, and 0.36s respectively.
These are one-host directional measurements, not general benchmarks.

Decision: keep Apple Containers watch-only. Preserve the typed plan and native
adapter prototype, but do not expose a public selector until boot-time service
discovery, gateway/runtime-prep integration, secret/SSH/Rosetta policy, and
project-scoped data operations have complete contracts and proofs.

## 10) Sources

| Source | Confidence | Notes |
| --- | --- | --- |
| [Apple Containers 1.2.0 release](https://github.com/apple/container/releases/tag/1.2.0) | high | release date and tagged change set |
| [Apple Containers 1.2 README](https://github.com/apple/container/blob/1.2.0/README.md) | high | platform and per-container VM model |
| [Apple Containers 1.2 command reference](https://github.com/apple/container/blob/1.2.0/docs/command-reference.md) | high | supported CLI capability surface |
| [Apple Containers 1.2 how-to](https://github.com/apple/container/blob/1.2.0/docs/how-to.md) | high | networking, DNS, SSH, port, and resource caveats |
| [Apple Containerization project](https://github.com/apple/containerization) | high | underlying VM and isolation architecture |
| [Compose support request #1846](https://github.com/apple/container/issues/1846) | medium | open upstream feature request |
| [Container DNS discovery #1809](https://github.com/apple/container/issues/1809) | medium | open service-discovery gap |
| [Network-scoped aliases #1839](https://github.com/apple/container/issues/1839) | medium | open alias gap |
| [Healthcheck support #1918](https://github.com/apple/container/issues/1918) | medium | open readiness gap |
| [Postgres resource report #1698](https://github.com/apple/container/issues/1698) | low | user-reported benchmark; must be reproduced locally |

## Next Task

Keep the public backend set unchanged. Reassess after Apple ships first-class
network-scoped discovery/static addressing, or after an Effigy design proves
boot-time aliases and the remaining manager integrations without global state.
