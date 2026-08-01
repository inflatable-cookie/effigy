# 006 - Compose Backend Compatibility

Status: Active
Last Updated: 2026-08-01

This contract defines the local compose-backend capability model that Effigy
expects for container-backed execution.

It exists to stop Docker Compose and Colima + `nerdctl compose` behavior from
being treated as accidental equals.

## Purpose

Effigy does not support "any backend that happens to parse compose YAML".

It supports a bounded local runtime contract. Some parts of that contract must
come directly from the backend. Other parts may be repaired by Effigy when a
supported backend is known to fall short.

This document names that boundary.

## Supported backend posture

Current supported posture:

- Docker Compose-compatible local runtime
- Colima + `nerdctl compose` compatibility path

The second path is supported because Effigy owns fallback behavior for known
gaps, not because `nerdctl compose` is assumed to be behavior-identical to
Docker Compose.

## Native backend candidate posture

Apple Containers 1.2 is a prototype candidate for a separate native backend
class. It is not currently supported, registered, auto-detected, or accepted as
a Compose implementation.

Prototype disposition: **watch-only**. The native primitive and lifecycle
proofs pass, but boot-time service discovery and manager/gateway integration do
not yet satisfy the supported-backend contract.

The candidate boundary is:

- backend id, if promoted: `apple-container`
- host: Apple silicon on macOS 26 or later
- input: Effigy-generated catalog stacks expressed as a typed effective stack
  plan
- excluded input: direct `compose_file` use and arbitrary Compose overrides
- fallback: select Docker Compose or Colima when excluded features are present

The prototype must not add Apple support by silently ignoring Compose fields.
Unsupported input fails before runtime mutation and names the supported backend
remedy.

## Capability classes

Effigy should classify backend-sensitive runtime behavior into three buckets:

- backend-required
- Effigy-repaired
- unsupported

### Compose-backend required

These must work from the backend/runtime itself for Effigy to function:

- compose file parsing and multi-file merge
- basic service bring-up and teardown
- `compose exec` against the selected service
- published-port reporting good enough for runtime inspection
- bind-mounted repo workspace visibility inside the execution target

If these fail, Effigy should stop rather than layering more repair logic on
top.

### Native-backend required

A native backend does not parse Compose. It must instead provide primitives
for Effigy to execute the same effective stack plan:

- OCI image pull and Dockerfile-compatible build
- deterministic create, start, stop, delete, and inspect operations
- exec, logs, copy, stats, and published-port inspection
- repo bind mounts and named data volumes
- project network creation and membership
- enough stable addressing for Effigy-owned service discovery

Effigy owns orchestration above those primitives: dependency order, readiness,
project naming, recovery, gateway registration, service mappings, and cleanup.
If the runtime primitives cannot support those repairs without machine-global
mutation, the backend remains unsupported.

### Effigy-repaired

These may be repaired by Effigy on supported backend paths when the backend
does not provide them directly:

- missing host bind-mount directory creation before `compose up`
- sibling-service bring-up after a partial or failed runtime start
- primary-service exec readiness after recreate or restart churn
- container-local TCP alias visibility inside Effigy-owned execution targets

These are legitimate product behaviors as long as:

- the repair is derived from the same effective model as the non-repaired path
- the repair runs through shared runtime-prep ownership
- the repaired behavior is explicitly documented and tested

### Unsupported

Effigy does not currently promise:

- arbitrary backend parity beyond the supported local paths
- alias visibility inside every compose service regardless of whether Effigy
  dispatches work there
- zero-repair semantics on the Colima + `nerdctl compose` path
- arbitrary Compose-file translation on a native backend
- Apple Containers support before its prototype gates pass

Those may widen later, but they are not current contract.

## Current capability matrix

### Host bind-mount preparation

Expected product guarantee:

- repo-owned bind mounts required for runtime bring-up exist before the user
  command dispatches

Backend status:

- Docker Compose commonly auto-creates missing host directories
- `nerdctl compose` may not

Effigy ownership:

- shared runtime-prep now creates repo-owned directory-style bind mounts before
  runtime prep continues

Target compatibility case:

- `bind_mount_host_dirs_are_prepared_before_exec_runtime`

### Sibling-service bring-up

Expected product guarantee:

- if the primary service is considered runnable, required sibling services are
  also brought online before container-backed exec or handoff

Backend status:

- partial `compose up` failure may leave sibling services in `Created` or
  otherwise unavailable state

Effigy ownership:

- shared runtime-prep performs an idempotent `compose up -d` before readiness
  and alias reconciliation

Target compatibility case:

- `runtime_prep_recovers_missing_sibling_services_before_dispatch`

### Primary-service exec readiness

Expected product guarantee:

- routed exec and handoff can use the resolved working directory immediately
  after runtime prep

Backend status:

- Colima + `nerdctl` may report a service running while `exec -w <dir>` still
  fails after recreate churn

Effigy ownership:

- shared runtime-prep probes real exec readiness and restarts the primary
  service once before failing

Target compatibility case:

- `runtime_prep_recovers_exec_readiness_after_recreate`

### Container-local TCP alias visibility

Expected product guarantee:

- Effigy-owned execution targets can resolve documented TCP backing-service
  aliases such as `mysql.<site>.legacy.test`

Backend status:

- compose-network alias materialization is not reliable enough on the
  supported Colima + `nerdctl compose` path

Effigy ownership:

- shared runtime-prep reconciles container-local TCP aliases inside the
  execution target from the same effective alias model used by host-visible
  service routing

Target compatibility case:

- `runtime_prep_reconciles_container_local_tcp_aliases`

## Validation direction

Compatibility coverage should prefer small targeted tests over one broad smoke
suite.

The first useful coverage set is:

- one proof that standard routed exec and workspace handoff both pass through
  the shared runtime-prep path
- one proof that bind-mount preparation happens before exec runtime dispatch
- one proof that exec-readiness recovery is attempted after recreate-style
  failure
- one proof that container-local alias reconciliation runs on the supported
  Colima-sensitive path

Before Apple Containers may enter the supported set, add live proofs for:

- service discovery across all containers in a representative Effigy stack
  without Apple system DNS as a required dependency
- dependency readiness, interrupted bring-up recovery, and idempotent restart
- gateway port discovery across runtime and host-network restarts
- named-volume preservation, reset, and destructive cleanup
- bind mounts, SSH-agent rotation, and Rosetta diagnostics
- cold/warm startup, idle memory, disk, and I/O against the same Docker and
  Colima stack
- early rejection of direct Compose files and unsupported override features

The `g08.018` prototype recorded these results:

| Proof area | State |
| --- | --- |
| generated-stack planning, lifecycle, readiness, exec, logs, bind mounts, ports, cleanup | pass |
| stale-state and full runtime restart recovery | pass |
| named-volume preserve/reset/destructive cleanup | pass |
| boot-time service aliases | blocked: post-start host reconciliation cannot protect a process that needs a peer during boot |
| gateway registration and VPN/host-network churn | incomplete |
| secret, SSH-agent, Rosetta, copy, stats, and streaming-log manager operations | incomplete; selected raw Apple primitives pass |
| four-service resource comparison | caution: about 240 MiB Apple guest memory versus 64 MiB Docker and 31 MiB Colima on the measured host |

Where live backend behavior is hard to reproduce in unit tests, coverage may
combine:

- unit tests for decision/ordering
- narrow live-repro tests for backend-sensitive behavior

## Drift triggers

Update this contract when Effigy changes:

- the supported local backend set
- which capability gaps are repaired by shared runtime prep
- the alias guarantee scope for Effigy-owned execution targets
- the expected compatibility cases used to prove backend-sensitive behavior
- the native-backend candidate scope or adoption gates

## Next Task

Keep the Apple candidate out of the supported backend registry. Translation
Memo 017 records the completed prototype and the conditions for reassessment.
