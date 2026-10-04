# 006 - Compose Backend Compatibility

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

## Capability classes

Effigy should classify backend-sensitive runtime behavior into three buckets:

- backend-required
- Effigy-repaired
- unsupported

### Backend-required

These must work from the backend/runtime itself for Effigy to function:

- compose file parsing and multi-file merge
- basic service bring-up and teardown
- `compose exec` against the selected service
- published-port reporting good enough for runtime inspection
- bind-mounted repo workspace visibility inside the execution target

If these fail, Effigy should stop rather than layering more repair logic on
top.

### Effigy-repaired

These may be repaired by Effigy on supported backend paths when the backend
does not provide them directly:

- missing host bind-mount directory creation before `compose up`
- sibling-service bring-up after a partial or failed runtime start
- owned `Exited`/`Created` containers after a Colima VM restart, started by
  name without recreating volumes or deleting systemd units
- primary-service exec readiness after recreate or restart churn
- container-local TCP alias visibility inside Effigy-owned execution targets
- generated Compose values that need a literal `$` for the container shell
  (`$code` written as `$$code` so Compose interpolation does not blank it)

These are legitimate product behaviors as long as:

- the repair is derived from the same effective model as the non-repaired path
- runtime repairs run through shared runtime-prep ownership
- the repaired behavior is explicitly documented and tested

### Unsupported

Effigy does not currently promise:

- arbitrary backend parity beyond the supported local paths
- alias visibility inside every compose service regardless of whether Effigy
  dispatches work there
- zero-repair semantics on the Colima + `nerdctl compose` path

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

### Owned service start after Colima restart

Expected product guarantee:

- after Compose up, owned services that are still `Exited` or `Created` are
  started or the failure names the live status and a start command
- persistent volumes stay; systemd health-check timer units are not deleted
- a stopped owned Colima/nerdctl container may recover its own stale
  transient `{full_id}.timer` / `{full_id}.service` collision once, after
  inspect proves the full hexadecimal ID and selected labels

Backend status:

- Colima + `nerdctl compose up` may return success while containers stay
  stopped after a VM restart
- `nerdctl start` of an owned container can succeed while logging
  `Unit <id>.timer was already loaded or has a fragment file`
- leftover transient health-check units named after the full container ID
  can block a later `systemd-run`

Effigy ownership:

- inspect owned Compose projects including stopped rows
- start only currently declared services whose project label matches an
  owned project; skip one-off `compose run` containers and undeclared
  orphans
- never rerun a one-time `compose run` command
- bound inspect and start so a hung nerdctl command still names live status
- treat a stale timer warning as a warning only after inspect shows the
  container running; start exit 0 is not readiness
- when the owned Colima container stays stopped with that collision,
  inspect the full ID, require stopped exact-owned labels, confirm the
  unit pair is transient in the selected profile, stop only
  `{full_id}.timer`, `reset-failed` `{full_id}.service`/`{full_id}.timer`,
  retry start once, and prove readiness by inspect
- refuse automatic repair for running, unknown, foreign, one-off,
  undeclared, persistent, mismatched, wrong-profile, or missing-authority
  cases; keep Docker start unchanged; never delete unit files
- if the container stays stopped or post-start inspect fails, keep the start
  backend text (including a stale-timer warning or refusal diagnostic) in
  the bounded failure with
  `colima nerdctl --profile <profile> -- start <container>`

Contract detail: `005-container-runtime-contract.md`.

Target compatibility cases:

- `recoverable_exited_service_is_started_despite_stale_timer`
- `stale_timer_collision_recovers_exact_units_then_retries_until_inspect_ready`
- `refused_persistent_unit_does_not_retry_start_and_keeps_diagnostics`
- `recoverable_transient_pair_stops_only_exact_timer_and_resets_pair`
- `persistent_exited_service_is_a_bounded_backend_failure`
- `successful_start_with_stale_timer_still_exited_is_a_bounded_failure`
- `declared_stopped_service_recovers_while_oneoff_and_orphan_are_left_alone`
- `inspect_timeout_after_successful_start_keeps_stale_timer_diagnosis`
- `start_timeout_reports_observed_exited_status`
- `docker_backend_does_not_recover_healthcheck_units`

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

### Generated Compose literal dollars

Expected product guarantee:

- generated Compose values keep container-shell `$` after Compose interpolation
  so health commands such as the Nginx wget `0`/`8` check still see `$code`

Backend status:

- Docker Compose and `nerdctl compose` interpolate `$VAR` in YAML values,
  warn when the variable is unset, and substitute an empty string

Effigy ownership:

- compose assembly escapes `$` in generated values (`$code` → `$$code`,
  already-escaped `$$` left alone) before writing the Compose file

Target compatibility case:

- `nginx_healthcheck_survives_compose_interpolation_and_treats_http_responses_as_ready`

## Validation direction

Compatibility coverage should prefer small targeted tests over one broad smoke
suite.

The first useful coverage set is:

- one proof that standard routed exec and workspace handoff both pass through
  the shared runtime-prep path
- one proof that bind-mount preparation happens before exec runtime dispatch
- one proof that exec-readiness recovery is attempted after recreate-style
  failure
- one proof that owned Exited services after a Colima restart are started or
  diagnosed without deleting systemd units
- one proof that container-local alias reconciliation runs on the supported
  Colima-sensitive path

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
- how generated Compose values escape `$` for backend interpolation
