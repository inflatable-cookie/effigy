# Apple Containers Parity Decision

Status: complete
Created: 2026-08-01
Roadmap: g08.018
Batch: 1053
Decision: watch-only

## Summary

- Ran the same app/web/Postgres/Redis fixture on Apple Containers 1.2, Docker
  Desktop 29.6.2, and Colima containerd.
- Proved Apple stale-resource recovery, full runtime restart recovery, named
  data preservation, SSH forwarding, and Rosetta primitives.
- Kept Apple out of the supported backend registry because boot-time service
  aliases and several manager/runtime-prep guarantees remain incomplete.

## Compatibility Matrix

| Gate | Result |
| --- | --- |
| typed stack plan and unsupported-field rejection | pass |
| build/pull, readiness, exec, logs, bind mounts, ports, cleanup | pass |
| post-start project-local service discovery | pass without global DNS |
| boot-time service discovery | blocked by missing native DNS/static addressing |
| stale container recovery | pass, 6.68s |
| full Apple runtime restart recovery | pass, 12.08s with named data preserved |
| named-volume preserve/reset/destructive cleanup | pass |
| project data export/import | not wired |
| gateway registration and VPN/host-network churn | not wired/proved |
| raw SSH-agent forwarding | pass |
| stack-plan SSH-agent policy | not wired |
| raw Rosetta `linux/amd64` execution | pass, `x86_64` reported |
| stack-plan platform/Rosetta diagnostics | not wired |
| runtime secret delivery | not wired through the native plan |
| direct Compose input | rejected before Apple side effects |

Local Network permission for `container-runtime-linux` is a required operator
prerequisite for published ports. Once registered and enabled, the same proxy
that reset connections returned HTTP 200.

## Resource Matrix

One Apple-silicon host ran the shared cached fixture. Image stores were not
cleared because they contain operator state, so initial image preparation and
project disk figures are not normalized.

| Measure | Apple Containers | Docker Desktop | Colima containerd |
| --- | ---: | ---: | ---: |
| first measured cached start | 7.05s | 13.57s with a Postgres pull | 10.96s with a Postgres pull |
| warm recreate/start | 6.13s | 3.29s | 1.76s |
| reported service memory | ~240 MiB | ~64 MiB | ~31 MiB |
| synced 64 MiB volume write | 0.10s | 0.09s | 0.36s |

Apple's build path also leaves its runtime-managed builder configured for 2
CPUs and 2 GiB. Apple service memory is roughly 4x Docker and 8x Colima in this
single directional snapshot, while volume write latency is competitive.

## Changes

- Added a reusable Compose parity fixture under
  `crates/effigy-containers/tests/fixtures/apple-parity/`.
- Expanded the ignored Apple live test with resource output, stale-container
  recovery, full runtime restart recovery, and data-preservation proof.
- Corrected prototype capability reporting so unwired attach, repair, copy,
  and streaming-log operations are not advertised.
- Added guide `077` with the watch-only state, permission diagnostic, measured
  tradeoffs, and reassessment gate.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: native candidate awaiting parity evidence -> bounded prototype
  retained with an explicit watch-only decision.
- Remaining gap: boot-time discovery plus gateway, secret, SSH/Rosetta policy,
  and project data integration. No public backend is promised.

## Validation Performed

- Apple four-service live lifecycle: passed in 44.73s, including stale and full
  runtime restart recovery
- raw Apple `--ssh` forwarding probe: passed
- raw Apple `--arch amd64 --rosetta` probe: passed
- Docker four-service lifecycle, discovery, port, bind, stats, I/O, warm
  recreate, cleanup: passed
- Colima containerd four-service lifecycle, discovery, port, bind, stats, I/O,
  warm recreate, cleanup: passed
- scoped cleanup verification: zero parity project resources remain on all
  three runtimes
- `cargo clippy -p effigy-catalog -p effigy-containers --all-targets -- -D warnings`:
  passed
- `effigy qa:architecture:runtime-container-drift`: passed
- `effigy qa:docs`: passed
- `effigy qa`: passed, 1,601 tests; docs and JSON contracts passed
- `cargo fmt --all -- --check`: passed
- `git diff --check`: passed

## Risks

- Post-start `/etc/hosts` repair is not a safe substitute for boot-time service
  discovery.
- The resource figures are one-host snapshots with different runtime memory
  accounting and pre-existing image caches.
- The Apple package and Local Network permission are machine-level prototype
  state; they are not Effigy installation requirements.

## Next Task

Keep Apple Containers watch-only. Reassess only when guide `077`'s boot-time
discovery gate is met.
