# Apple Containers Prototype Status

Status: Watch only
Applies to: Apple Containers 1.2.0, macOS 26, Apple silicon
Last validated: 2026-08-01

## Current Support State

Apple Containers is not an Effigy backend users can select. It is absent from
automatic detection and the default backend registry. Docker Compose and
Colima remain the supported macOS paths.

Effigy retains a typed generated-stack plan and an explicit native executor
prototype. They prove the architecture without creating a partial Compose
implementation or a premature public contract.

## What Passed

- OCI pull and Dockerfile build
- deterministic project networks, containers, and named volumes
- dependency ordering and Effigy-owned readiness probes
- routed exec and captured logs
- bind mounts and loopback-published ports
- project-local service names after startup without Apple system DNS
- named-volume preservation across container and Apple runtime restarts
- stale and interrupted project recovery
- destructive cleanup scoped to Effigy prototype resources
- raw Apple SSH-agent forwarding and Rosetta execution primitives

The live proof uses an app, Nginx, PostgreSQL, and Redis. It leaves no project
containers, networks, or volumes behind. Apple's own persistent `buildkit`
container remains runtime-managed.

## Why It Is Not Supported Yet

Apple 1.2 does not provide bare service-name discovery or static address
assignment on a project network. Effigy can inspect addresses and reconcile
`/etc/hosts` after startup, but that is too late for a service whose boot
process must resolve a dependency. This is a correctness gap, not polish.

The prototype also lacks complete manager integration for:

- gateway registration and VPN/host-network churn recovery
- runtime secret delivery
- SSH-agent and Rosetta policy in the effective stack plan
- copy, stats reports, streaming logs, and attached-session closeout
- project-scoped data export and import

Direct Compose files and arbitrary Compose overrides are intentionally outside
the native candidate scope.

## Local Network Permission

Published ports require Local Network access for `container-runtime-linux`:

1. Start a container with a published port once.
2. Open **System Settings → Privacy & Security → Local Network**.
3. Enable `container-runtime-linux`.
4. Restart Apple Containers if the setting changed.

A missing permission has a distinctive shape: the host proxy accepts the TCP
connection and resets it, while `container system logs` reports `No route to
host`. The container remains reachable directly on its vmnet address.

Permission may need renewal after an Apple Containers upgrade.

## Directional Comparison

One Apple-silicon host ran the same cached four-service fixture. Values are
directional, not portable benchmarks.

| Measure | Apple Containers | Docker Desktop | Colima containerd |
| --- | ---: | ---: | ---: |
| warm create/start | 6.13s | 3.29s | 1.76s |
| reported service memory | ~240 MiB | ~64 MiB | ~31 MiB |
| synced 64 MiB volume write | 0.10s | 0.09s | 0.36s |
| interrupted project recovery | 6.68s | not separately measured | not separately measured |
| full runtime restart recovery | 12.08s | not separately measured | not separately measured |

Apple's build path also keeps a builder VM configured for 2 CPUs and 2 GiB.
Container image stores and pre-existing caches were not normalized, so the
prototype does not claim a comparable project disk figure.

## Reassessment Gate

Reopen support planning only when one of these is true:

- Apple ships network-scoped service discovery or static addressing suitable
  for boot-time dependency aliases.
- Effigy has a bounded pre-start alias design that does not mutate machine
  global DNS and works for the supported catalog stack shapes.

A reassessment must then close the gateway, secret, SSH/Rosetta, data, and
attached-session gaps before adding a public selector.

## Evidence And Authority

- [Translation Memo 017](../research/translation-memos/017-apple-containers-runtime-backend.md)
- [Container backend compatibility contract](../contracts/006-compose-backend-compatibility.md)
- [Container manager contract](../contracts/012-container-manager-contract.md)
- [Prototype lifecycle evidence](../logs/2026-08/01-143249-apple-native-stack-lifecycle.md)
- [Parity decision evidence](../logs/2026-08/01-144647-apple-containers-parity-decision.md)

## Next Task

Keep Apple Containers watch-only. Reassess from the gate above rather than
adding a public backend selector around the current gaps.
