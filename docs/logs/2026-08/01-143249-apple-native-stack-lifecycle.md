# Apple Native Stack Lifecycle

Status: complete
Created: 2026-08-01
Roadmap: g08.018
Batch: 1052

## Summary

- Added an explicit Apple Containers native adapter without registering it as
  a supported or automatically detected backend.
- Proved a four-service app/web/Postgres/Redis stack through build, pull,
  readiness, service discovery, bind mount, host port, exec, logs, restart,
  named-volume persistence, and cleanup.
- Kept Docker and Colima manager behavior and tests green.

## Runtime Evidence

- Runtime: Apple Containers 1.2.0 on macOS 26.5.2, Apple silicon
- Fixture: one built Alpine app plus `nginx:alpine`, `postgres:17-alpine`, and
  `redis:7-alpine`
- Cold image preparation exposed Apple's persistent `buildkit` VM at 2 CPUs and
  2 GiB; cached four-service lifecycle completed in 19.73 seconds including a
  full stop/recreate/restart cycle
- Apple custom-network DNS does not resolve bare service names. The adapter
  uses project-local `/etc/hosts` reconciliation inside known project
  containers and does not mutate machine-global DNS.
- Published ports require macOS Local Network access for
  `container-runtime-linux`. Without it, the proxy accepts TCP and resets with
  runtime log error `No route to host`; after registration and approval the
  same probe returns HTTP 200.
- Apple named volumes are ext4-backed and contain `lost+found`. The Postgres
  catalog now sets `PGDATA=/var/lib/postgresql/data/pgdata` below the mount root.
- Cleanup left zero Effigy prototype containers, custom networks, or volumes.
  Apple's runtime-managed `buildkit` container remains running by design.

## Changes

- Added `AppleContainerBackend`, `AppleStackLifecyclePlan`, and
  `AppleStackExecutor` for native command planning and execution.
- Added deterministic project resource naming, dependency ordering, readiness
  polling, IP inspection, scoped service aliases, exec/log routing, and
  idempotent cleanup.
- Added an ignored live integration test with cleanup on success or failure.
- Added a targeted diagnostic when a published port remains unreachable due to
  missing Local Network permission.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: semantic plan only -> bounded native Apple lifecycle proven on a
  representative stack.
- Remaining gap: gateway, VPN churn, SSH-agent, secret, Rosetta, comparative
  resource/I/O evidence, and the final support decision remain in `1053`.

## Validation Performed

- `cargo test -p effigy-containers --test apple_live apple_native_four_service_lifecycle -- --ignored --exact --nocapture`:
  passed, 1 live test in 19.73 seconds
- `cargo test -p effigy-containers`: passed, 220 unit tests; live test ignored by default
- `cargo clippy -p effigy-containers --all-targets -- -D warnings`: passed
- `effigy qa:architecture:runtime-container-drift`: passed
- `cargo fmt --all -- --check`: passed
- `git diff --check`: passed

## Risks

- Service aliases are installed after services start. A service whose own boot
  command requires a peer alias remains unsupported until aliases can be
  supplied before process launch or Apple ships first-class discovery.
- Local Network permission is an operator prerequisite and may need renewal
  after an Apple Containers upgrade.
- The persistent builder VM is real idle overhead and must be included in the
  Batch `1053` comparison.

## Next Task

Execute ready card `1053`: complete the compatibility/resource matrix and make
the support decision.
