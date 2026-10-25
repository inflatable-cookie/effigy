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
- exclusive owned disposable rust/cargo/target named volumes prepared for
  the resolved workspace uid/gid, with actual non-root write/lock
  verification before children launch; named volumes shared by two or more
  compose or managed services are not mutated

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
- recursive chown of host bind sources, shared/external volumes, or
  arbitrary checkout trees to make rust `target`/cargo writable

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
- re-observe each candidate container with its own `inspect` immediately
  before the start, and start it only when that observation proves a
  stopped `created`, `exited` or `dead` container; a running or in-flight
  container is left to the readiness wait
- never rerun a one-time `compose run` command
- bound inspect and start so a hung nerdctl command still names live status
- treat a timer collision as a warning once inspect shows the container
  running; start exit 0 is not readiness
- when the owned Colima container stays stopped with that collision,
  inspect the full ID, require stopped exact-owned labels, confirm the
  unit pair is transient in the selected profile, stop only
  `{full_id}.timer`, `reset-failed` `{full_id}.service`/`{full_id}.timer`,
  retry start once, and prove readiness by inspect
- operation exit 0 is not unload proof: after stop/reset, re-probe the exact
  pair with a bounded wait until both units are unloaded; a late unload
  recovers once, a never-unloaded pair fails bounded with its load/fragment
  state, and a persistent/unverified unit appearing mid-flight refuses
- a start that reports the collision while the container ends up running
  hands the service to the normal readiness and health wait with the exact
  inspect command in the recorded warning, never a stale-backend-state
  failure; never repeat a unit mutation on a now-running container
- refuse automatic repair for running, unknown, foreign, one-off,
  undeclared, persistent, mismatched, wrong-profile, or missing-authority
  cases; keep Docker start unchanged; never delete unit files
- if the container stays stopped or post-start inspect fails, keep the start
  backend text (including a stale-timer warning or refusal diagnostic) in
  the bounded failure with
  `colima nerdctl --profile <profile> -- start <container>`
- a recurring collision refuses service-only recreation with the missing
  preservation interfaces named in contract 005; inspect mount references
  alone do not authorize replacement or mutable-volume retirement

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
- `recurring_health_collision_refuses_recreation_through_production_adapters`
  (private fake transport and file markers; no real engine or live exec proof)
- `unload_proof_stop_reset_exit_zero_but_still_loaded_fails_bounded`
- `unload_proof_late_unload_within_bounded_probes_recovers_once`
- `unload_proof_persistent_fragment_mid_flight_refuses`
- `unload_proof_post_stop_probe_failure_fails_closed`
- `pre_start_observation_skips_a_container_compose_already_started`
- `pre_start_observation_skips_containers_the_backend_is_acting_on`
- `pre_start_observation_still_recovers_a_genuinely_stopped_sibling`
- `failed_pre_start_observation_refuses_the_start`
- `reboot_timer_collision_with_a_running_container_hands_off_to_readiness`
  (private fake transport; no real engine or readiness proof)
- `docker_backend_does_not_recover_healthcheck_units`

### Primary-service exec readiness

Expected product guarantee:

- routed exec and handoff can use the resolved working directory immediately
  after runtime prep

Backend status:

- Colima + `nerdctl` may report a service running while `exec -w <dir>` still
  fails after recreate churn

Effigy ownership:

- shared runtime-prep verifies full container ID, project, declared service,
  non-oneoff/running status, canonical checkout and exact scope before probing
- readiness returns an internal inspected binding. Mount probes and ordinary
  shell/task exec retain that binding, reverify the expected full ID and owner,
  and use native Docker or nerdctl exec against the full ID. Builtin suite
  rendering obtains and reverifies its own readiness binding before rendering
  native ID-targeted exec. No service-name cache authorizes these execs.
- a missing/foreign binding or replacement refuses before exec or recovery;
  a known owned primary may be restarted once by its verified ID. Legacy
  dependent-service recovery retains its declared dependency selection.
- argv, workdir, user, environment, capture/TTY and stdin-file behavior survive
  native transport. Unsupported exec options refuse rather than being dropped.
- inspections and remote exec are separate observations, not an atomic remote
  transaction. A replacement after final inspect cannot redirect ID-targeted
  exec into the successor; the old ID can fail. In-place label/state changes
  after the final observation and remote cancellation are not guaranteed.
- the existing deadline-aware diagnosis/permission transport retains its
  observation sequencing and budgets. Doctor does not gain a workload-readiness
  prerequisite or restart authority from this change.

Target compatibility case:

- `runtime_prep_recovers_exec_readiness_after_recreate`
- `direct_builder_activation_tests` exercises direct and generated no-port,
  no-route activation through policy admission, readiness, registration and
  native exec, including replacement and sibling-state preservation.

Direct Compose labels come from the supported ownership override, never from a
hardcoded checkout in the builder YAML. The maintained
`workspace:linux:artifact` task uses the builder-only
`container::workspace_artifact_build(args)` operation. An already running owned
builder can build current bound source even after the source changed. This is
execution-only recovery: it claims no Compose activation, starts nothing and
stops nothing.

The operation serializes callers with the existing lock machinery, lists the
selected project including stopped containers, requires exactly one builder,
and inspects its full ID, project/service, explicit checkout and exact scope.
It requires one writable bind of this canonical checkout at `/workspace`, no
shadowing mounts below it, and no published ports. It reverifies that identity
and bind at the native ID-targeted exec boundary under the original observation
deadline. Unknown, foreign, missing, stopped, malformed or changing identities
refuse without adoption, restart, teardown, marker removal or publication. A
retained admission with no container refuses; a first activation with neither
container nor admission uses the ordinary container manager. Existing runtime
drift checks remain in force. No VM recovery or broad cleanup is added.

The builder remains detached after success or failure, preserving its cache
volumes and completed artifacts for retry. Its original image may be unable to
compile newer source; a failed build then remains a failure and grants no
artifact identity. Both ordinary and bounded handoff continue through the same
maintained task and retain their receipt, target, digest and freshness checks.
There is no public-release fallback for an unpublished local identity.

The maintained Linux builder uses the stable private Compose project base
`effigy-artifact-builder-v1`, with the ordinary checkout/worktree scope suffix.
This is a one-time namespace migration from `effigy-linux-release`, not a new
namespace per source version, upgrade or retry. The first build activates a
new labelled builder through the existing manager; subsequent builds reuse its
strictly verified immutable ID, including after a changed source input. Artifact paths and the task interface are unchanged.

Legacy `effigy-linux-release` containers and their admission records, cache
volumes and other retained data are left untouched, whether absent, labelled,
or ambiguous. A pre-label nerdctl container lacks the checkout/scope and oneoff
labels; a named `compose run` can reproduce its native name and metadata, and
the old admission does not record its native ID. That evidence cannot prove a
non-oneoff association. Effigy therefore never adopts, executes in, relabels,
stops or recreates that old builder during automatic artifact preparation.
No user reset or cleanup is required. Later operator cleanup is separate and
optional. The new project uses separate named caches, so the first build may
need image/dependency downloads and additional disk space; old caches remain.

A collision in the new project is not an empty project: missing, foreign,
mixed or contradictory ownership evidence still refuses before activation or
exec. No generic ownership check or ordinary application drift guard is
relaxed. Private transport fixtures exercise the shipped declaration, the
native legacy shape and named-oneoff counterexample, both automatic handoffs,
repeat upgrades and failures; they do not attest live consumer recovery.

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

### VM Compose interpolation

Expected product guarantee:

- a rendered generation's `${EFFIGY_RENDER_ROOT}` paths interpolate on the
  Colima backend, for every Compose command Effigy runs

Backend status:

- `colima nerdctl` runs Compose inside the VM, so the host process environment
  (including `EFFIGY_RENDER_ROOT` and any value the plan forwards for
  `secrets.delivery = "compose-env"`) never reaches Compose interpolation
- `nerdctl compose` accepts exactly one `--env-file` and resolves it against
  paths the VM can read; host paths under the user's home are mounted there

Effigy ownership:

- the shared invocation transform in `policy::render` (reached by attached,
  detached, `config`, `ps`, `logs`, `restart` and shared-service invocations)
  writes the generation root and the plan's forwarded values to
  `<effigy home>/compose-env/<pid>-<content>.env` (0600 inside a 0700
  directory) and inserts `--env-file` after the Compose subcommand
- values are double quoted with `\`, `"`, newlines and `$` escaped, so Compose
  reads each value literally instead of interpolating it again; a
  non-identifier name, a non-UTF-8 value or a carriage return refuses before
  spawning
- the file is outside the immutable closure, so the generation digest never
  depends on the state-home path; an activation that this failure interrupted
  is reconciled by a plain `effigy container up`
- Compose's default `.env` lookup is the first Compose file's directory, which
  inside a generation is the sealed object directory and never holds one, so
  naming this file drops no interpolation the invocation had
- a file whose writer is gone is reclaimed by the next invocation; retained
  files are bounded and older than a day is reclaimed even while its writer
  lives

Target compatibility cases:

- `a_vm_hosted_invocation_names_a_file_the_vm_can_read`
- `a_render_generation_matches_across_state_home_paths`
- one real-Colima fixture whose Compose stack mounts a generated closure file
  through the render root

## Validation direction

Compatibility coverage should prefer small targeted tests over one broad smoke
suite.

The first useful coverage set is:

- one proof that standard routed exec and workspace handoff both pass through
  the shared runtime-prep path
- one proof that bind-mount preparation happens before exec runtime dispatch
- one proof that workspace rust ownership prep classifies named volumes as
  owned-disposable and bind rust paths as verify-only, then observes
  non-root write/lock rather than mount-root owner heuristics
- one proof that doctor ownership inspects share the remaining doctor budget
  and report timeout as unavailable rather than clean
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
- how a VM-hosted backend receives its Compose interpolation environment

## Detached subprocess evidence boundary

Compose detach alone does not isolate stdout/stderr inheritance. The launch
adapter sends a detached backend's stdout and stderr to one regular file in the
checkout's retained runtime record (`.effigy/runtime/logs/<project>.compose.log`)
instead of an Effigy-owned pipe or a null sink: a regular file has no reader
lifetime and stays writable after the CLI exits. After the backend exits the
record keeps only its last 64 KiB behind a visible truncation marker, and a
failure names the file and prints its last 4 KiB. That tail is operator-facing
error text; detached `--json` failure details keep their fixed structural
vocabulary and never carry raw backend bytes. A detached `up` whose record
cannot be created refuses before spawning rather than reverting to a
throwaway sink. Finite Unix inventory capture uses nonblocking reads with
bounded retention and no reader threads. Success requires client exit and both
EOFs; overflow or unverified settlement refuses. Cancellation owns only the
client process group and positively reaps its direct child. Remote engine
cancellation and historical SIGPIPE sender/pipe provenance remain unverified.
The private exit-141 controls (`test:containers:exit141`, effigy#300) add what
this checkout does own: a detached launch gives the client one regular file on
stdout and stderr, so a writer that outlives the client never loses a reader;
a cancelled capture signals only the client's group, and a writer that
inherited the capture pipe dies of SIGPIPE only when Effigy gives up its own
read end. Those controls do not identify the original nerdctl reader, so the
papercut stays open for that observation. Non-Unix bounded pipe capture
requires a missing nonblocking adapter and returns an explicit unsupported
error.

The private CLI matrix runs three overlapping scoped stacks through production
adapters, maintains persistent exec sentinel connections, tests warm
reconciliation and full-ID/port replacement, and exercises noninteractive
retirement ownership refusals. Native engine semantics remain separate from
these private command-transport controls.
