# 034 - Gateway Legacy Upgrade and Recovery (Proposed)

Status: proposed and unavailable until implementation. No `effigy gateway
recover` entrypoint, legacy-candidate reader or generation-bound stop exists
today. The published `v0.14.0` tag cannot complete the path at all: it rejects
the operator-owned gateway directory during elevation and reports
`ProcessStateUnknown` for a record with no sidecar. Main `4201e0b3` (tasks 113
and 112, PR228/PR229) corrects the directory check and adds the distinct
`LegacyIdentityRequired` migration error, but still has no recovery entrypoint.
The candidate-inspection and generation-bound stop below are an unapproved
protocol that requires the material ruling in
[Material ruling required](#material-ruling-required). See
[020](020-container-infrastructure-design.md#gateway-process-identity) and
[guide 083](../../guides/083-v0.14.0-consumer-migration.md).

Owner: gateway lifecycle maintainers
Architecture: [020](020-container-infrastructure-design.md#gateway-process-identity)
Guidance: [083](../../guides/083-v0.14.0-consumer-migration.md)
Related: task 112 (legacy diagnostics), task 113 (elevated owner trust),
[Q-001](../questions.md#q-001--gateway-pid-identity),
[Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access)
Authority: no new standing helper, install, workflow, release or live-operation
authority is granted here. Nothing below authorizes a numeric-only signal.

## Purpose and boundary

Upgrading an active pre-identity gateway must not strand the user. This document
defines three things: the fallback that current ownership policy already
permits, a complete contingent candidate-inspection and generation-bound stop
protocol that would finish the consumer path if the operator grants the ruling,
and the exact ruling text. It is a design for a bounded implementation task, not
an implementation.

Hard boundaries:

- A numeric-only, malformed, unreadable or unknown record never authorizes a
  blind current-CLI signal, never fabricates a sidecar, and is preserved
  byte-for-byte on refusal.
- The current CLI does not delegate a numeric signal to a previous binary. A
  digest-verified previous binary proves provenance, not candidate-process
  ownership. See [Rejected: delegated previous-binary stop](#rejected-delegated-previous-binary-stop).
- A CLI flag cannot grant the authority the policy withholds. Consent is not
  ownership proof; it is permission to act on ownership evidence the CLI
  already proved.
- No new standing helper, install, privilege, host-run 010, workflow or release
  change. No silent config downgrade, no re-tag, no v0.14.0 asset mutation.

## Version and state scoping

Three different products are in scope. Do not mix them.

| State | Directory check on elevation | Numeric-only record | Recovery entrypoint |
| --- | --- | --- | --- |
| Published `v0.14.0` tag | Rejects the operator-owned directory (`gateway directory is unsafe`) | `ProcessStateUnknown { pid }` | None. Cannot complete a legacy upgrade |
| Main `4201e0b3` (tasks 113 + 112, PR228/PR229) | Accepts the authenticated operator-owned directory | `LegacyIdentityRequired { pid }` after a read-only probe; `ProcessStateUnknown` when the probe is unknown | None |
| Proposed (this document, target `v0.14.1`) | Same as main | `LegacyIdentityRequired` plus a structured recovery payload | `effigy gateway recover` and the contingent inspection/stop protocol |

The published `v0.14.0` binary is the one consumers have. Its start path stays
broken even after records clear, so no current CLI on that binary can finish the
recovery. A consumer completes the path only after installing a corrected
release that carries the directory correction from main and the new entrypoint.

## Current truth versus proposal

| Concern | Current truth | Proposal in this document |
| --- | --- | --- |
| Legacy numeric-only record | Published `v0.14.0`: `ProcessStateUnknown`. Main `4201e0b3`: `LegacyIdentityRequired`; records preserved; no signal | Keep the fail-closed refusal; add a structured recovery payload |
| Stopping the old daemon | No CLI signal path; main's guidance requires an operator-controlled stop with independently confirmed executable and owner | Fallback: operator stop + verify + clean + start. Contingent: bounded candidate inspection then a generation-bound stop |
| Automated previous-binary stop | Not authorized by the 020 ruling; v0.13.1 `down` is not ownership-safe | Rejected; it cannot meet generation checks. See [Rejected](#rejected-delegated-previous-binary-stop) |
| New daemon start | Published `v0.14.0`: rejected as unsafe. Main `4201e0b3`: works for the authenticated operator context | `recover` ends in a normal non-reentrant start under the transition lock |
| Public channels | `latest` = v0.13.1; Homebrew formula restored to v0.13.1; `v0.14.0` body warns | Keep the pin; ship the fix-forward patch |

## Failure

Receipts in [Public channel assessment](#public-channel-assessment) and
[Private proofs](#private-proofs). Sequence for a gateway started by an Effigy
build that predates the identity sidecar:

1. The 0.13.x daemon wrote decimal `~/.effigy/gateway/gateway.pid` and
   `gateway.version` (for example `v0.13.1+local.677`). No `gateway.identity`
   exists. The daemon runs elevated and holds `:53`, `:80`, `:443`.
2. The user upgrades to `v0.14.0`. The local installer `mv`s the new binary over
   `.local-install/bin/effigy`; the previous binary and its version are gone.
   Homebrew and GitHub-releases installs replace the binary in place.
3. On published `v0.14.0`, `status`, `up` and `down` return
   `ProcessStateUnknown { pid }`; on main `4201e0b3` they return
   `LegacyIdentityRequired { pid }` after a read-only probe. Both preserve the
   files and refuse signal, cleanup or replacement. The old daemon keeps
   running. `up` cannot bind and cannot replace it.
4. The interim guidance requires an operator-controlled stop that independently
   confirms the daemon's executable and owner. An ordinary unelevated user
   cannot read an elevated daemon's executable/owner (Q-002
   `CHECK_SAME_USER`; the current `__gateway-identity` reader binds a sidecar
   digest and returns Unknown when no sidecar exists), so this is a real gap,
   not a solved path. Running an older CLI for ordinary work also fails on newer
   manifests: a v0.13.1 CLI run against this repo's linked Longhorn
   `proof:artifacts` manifest fails to parse (`effigy dev --plan` fails before
   execution). A global downgrade is not a consumer launch solution.
5. Published `v0.14.0` additionally rejects the operator-owned gateway directory
   during elevation (`gateway directory is unsafe`), so even clearing the
   records does not let that binary start. Main `4201e0b3` corrects this: the
   elevated root caller accepts the forwarded non-root operator UID only when it
   matches ambient `SUDO_UID` (when present) and owns the forwarded `HOME` per
   passwd; unauthenticated root still accepts only root ownership.

Host transition evidence (2026-10-06, planner, explicit authority, no new live
operations here): maintained `bootstrap:local` exit 0; the exact legacy PID
72111 stopped through a verified private v0.13.1 binary with
`EFFIGY_GATEWAY_KEEP_RESOLVER=1`, exit 0 and confirmed absent; corrected
`gateway up` exit 0, PID 11887, root, stable installed binary; ordinary TTY
`gateway status` exit 0, trusted; the cross-UID reader worked; PID + sidecar
owned by UID 501, mode `0600`; `routes.json` and `loopback-ips.json`
byte-unchanged. This was a *one-host transition with independently confirmed
executable and owner*. It is not a general exception for numeric-only signals
and it is not the consumer flow. The resolver was kept only because the operator
passed `EFFIGY_GATEWAY_KEEP_RESOLVER=1`; v0.13.1 `down` otherwise removes it.

The gap the outcome requires closing: an ordinary user with an overwritten
binary and an elevated legacy daemon has no supported way to confirm and stop
it. The contingent protocol below is the design that closes it; it is not
implemented and not authorized yet.

## Public channel assessment

Read-only verification 2026-10-06. Mitigation is already applied; no authority
to change channels is claimed here.

| Channel | State | Exposure |
| --- | --- | --- |
| GitHub Releases `latest` | `v0.13.1` (`releases/latest` returns v0.13.1) | Pinned to last good; new default downloads get 0.13.1 |
| GitHub Release `v0.14.0` | Published tag with a gateway warning in its body | Explicit downloads still possible; must not be deleted or re-tagged |
| Homebrew tap `Formula/effigy.rb` | Restored to the exact v0.13.1 formula at `c7fa8c55597947c079def106c99f8dc2cdcd8305` (parent `c2045dc59` = the 0.14.0 bump; original v0.13.1 formula `27a79c277`) | `brew install` gets 0.13.1 again; existing 0.14.0 kegs are not silently downgraded |
| `cargo install` from tag / direct binary download | `v0.14.0` tag and assets remain | Unprotected; only the release-body warning applies |
| Local `bootstrap:local` install | `build-local-bin.rhai` `mv`s over `.local-install/bin/effigy`, writes `effigy.active-version` | No previous binary kept |
| Existing `v0.14.0` installs | Running binary unchanged | Not protected by channel changes; need the patch |

Homebrew digests were compared against the v0.13.1 asset metadata and read back
byte-equal. The rollback is reversible. No tag, workflow or release-asset
mutation is part of the mitigation.

Minimal rollback/pin-or-patch proposal:

1. Pin (in place): `latest` = v0.13.1, Homebrew = v0.13.1, `v0.14.0` release
   body warning. Preserve the pin until the corrected release is verified.
2. Do not remove or re-tag `v0.14.0` assets, and do not silently downgrade
   installed `v0.14.0` clients.
3. Patch forward: ship a PATCH containing the directory correction and
   diagnostic, plus the recovery entrypoint and installer preservation. After
   tagged-source and install proof, re-pin `latest` and the Homebrew formula in
   one formula commit.
4. Record the channel posture and the fix-forward rule in the release procedure;
   future regressions use the same reversible pin, never a re-tag.

## Policy boundary

The 020 ruling authorizes Tom's one-host transition only: an operator
independently confirmed the daemon's executable and owner before stopping it.
It is not a general exception that lets a numeric-only record authorize a
signal. Therefore:

- The current CLI never signals a numeric-only record blindly.
- The current CLI does not invoke a previous binary to signal it. Delegation by
  proxy is the same signal authority.
- A digest-verified previous binary proves the *binary's* provenance. It does
  not prove that the recorded PID is that binary. The 020 foreign-PID-reuse
  counterexample still applies: a `sleep` child given the same decimal PID is
  `Running` to the probe, and v0.13.1 `terminate_gateway_process` signals it.
- Consent (`--yes`, a risk acknowledgement, an interactive prompt) is not
  ownership proof. Consent only authorizes action on ownership evidence the CLI
  has already proved.

## v0.13.1 previous-binary behaviour

This is a compatibility record. Source read at tag `v0.13.1` (`d186388fc`):

- `run_gateway_down` calls `server::get_status(&config).ok()`. `get_status`
  reads the pid, and when `process_is_running(pid)` is false it calls
  `remove_pid_file` (unlink `gateway.pid` + `gateway.version`) and returns
  `NotRunning`. `process_is_running` runs `ps -p <pid> -o pid=` and returns
  `status.success()`; a `ps` launch failure or a nonzero/malformed result is
  `false`. So a failed probe is collapsed to not-running **and the records are
  deleted**.
- `run_gateway_down` then calls `remove_pid_file` unconditionally after the
  optional stop. It removes records even when `get_status` returned `None`.
- `uninstall_resolver_if_needed` removes `/etc/resolver` entries unless
  `EFFIGY_GATEWAY_KEEP_RESOLVER=1` is set. The authorized host transition set
  that variable; the design cannot assume it.
- `stop_gateway_process` → `terminate_gateway_process` sends
  `kill(Pid::from_raw(pid as i32), SIGTERM)`, then SIGKILL, with no ownership,
  start-identity or executable check.
- It has no interface to accept or re-check a candidate generation and no
  recovery lock.

Consequences: a delegated v0.13.1 stop cannot preserve records on an unknown
probe, cannot serialize with the proposed recovery, cannot bind a generation,
and can remove resolver state. Any fixture that asserts "byte-identical records
after a successful previous `down`" is wrong against production behaviour.

## Proposed flow

### Fallback supported within current policy: operator-verified recovery

`effigy gateway recover` performs the parts the CLI can prove and refuses the
rest. It never signals, and the operator stops the daemon outside the CLI.

1. Read and classify. Refuse on unknown/untrusted (no recovery). Only a PID
   record with no `gateway.identity` file is legacy; a present but malformed
   sidecar is unknown/untrusted. Capture the decimal PID, `gateway.version`,
   record bytes, owner UID and digests.
2. Require an interactive terminal (or `--yes` for the absent-only path) and
   print the evidence: PID, version, gateway directory, owner UID, digests, and
   the exact operator checks required to confirm the live process is the
   recorded gateway (executable path/command, owner UID, start time, listening
   `:53`/`:80`/`:443`).
3. Acquire the transition lock (see [Locking](#locking)). Refuse if a concurrent
   `up`/`down`/`recover` holds it.
4. Re-probe the captured PID with the production tri-state probe.
   - `ConfirmedAbsent`: continue.
   - `Running` or `Unknown`: refuse, print the operator checks and stop
     instructions, preserve every remaining record, do not start.
5. On `ConfirmedAbsent`, remove any remaining legacy records with
   compare-and-remove under the record lock (`remove_if_unchanged`). This is
   idempotent and touches only the captured generation. Never remove on
   Running/Unknown/refusal.
6. Start through the internal non-reentrant start while still holding the
   transition lock (see [Locking](#locking)).

The fallback is implementable now, but it does not close the outcome for an
ordinary user: step 4's `Running` case still requires host process management an
unelevated user may not be able to perform. The contingent protocol below is the
route that closes it.

### Contingent: bounded candidate inspection

This is the concrete proposal the outcome requires. It is unapproved and
unimplemented.

**Reader surface.** A hidden, bounded, read-only `__gateway-legacy-candidate`
command, invoked through the existing administrator elevation (`/usr/bin/sudo`
or `/usr/bin/osascript`) exactly like `__gateway-identity`. It accepts only:

- a digest of the canonical fixed gateway PID path,
- a digest of the validated legacy record bytes (PID + `gateway.version`),
- the operator UID and the gateway-directory owner UID.

It accepts no arbitrary PID, no path, no signal request. It verifies the
elevation marker and operator UID, rechecks the canonical path and the trusted
record pair, reads only that recorded target, and returns at most 1 KiB of
bounded identity. It never signals, changes records, starts a daemon or writes.
The caller requires an interactive terminal, caps the reader wait at 15 seconds,
and rejects nonzero exit, timeout, malformed output, or either digest mismatch as
Unknown.

**Returned candidate identity.** `pid`, `boot_identity`, precise start identity
(Linux `/proc/<pid>/stat` field 22 ticks; macOS `pbi_start_tvsec` +
`pbi_start_tvusec`), `owner_uid`, the executable path digest and basename,
`comm`, and a bounded `is_effigy` verdict. No free-form output.

**Supported platform API proof.**

| Platform | Read | Authority |
| --- | --- | --- |
| Linux | `readlink /proc/<pid>/exe`; `/proc/<pid>/stat` field 22; `/proc/<pid>/status` Uid; `/proc/sys/kernel/random/boot_id` | Documented kernel interfaces; no SPI |
| macOS | `proc_pidinfo(PROC_PIDTBSDINFO)` → `pbi_start_tvsec`/`pbi_start_tvusec`, `pbi_uid`, `pbi_comm`; `proc_pidpath(pid, ...)` → executable path | Public XNU/libproc headers; cross-UID needs `PRIV_GLOBAL_PROC_INFO`, which the root reader has behind existing elevation |

If `proc_pidpath` is unavailable or refused, the reader returns Unknown and the
recover path refuses. It does not fall back to `kern.proc` SPI, `ps -o lstart=`
(locale-dependent), or a guessed owner.

**Candidate generation.** `{pid, boot_identity, start_identity, owner_uid,
exe_digest, exe_basename, recorded_version}`. It is bound by a digest over the
canonical path digest, record digest, and candidate identity.

### Contingent: consent, adoption and generation-bound stop

1. `recover` invokes the reader for the legacy record. Refuse on
   declined/unavailable/timeout/malformed, UID mismatch, non-Effigy executable,
   version mismatch, unreadable boot/start, or any ambiguity. No signal.
2. Show the candidate generation and require explicit interactive consent to
   adopt it. `--yes` is not sufficient for a stop; only the absent-only fallback
   accepts `--yes`. Decline → refuse and preserve.
3. After adoption, re-read the candidate identity immediately before each
   signal (local read first, bounded elevated reader if denied) and send TERM
   only if the full generation still matches. Re-check again before KILL. A
   mismatch means the bound generation is gone: no signal.
4. After the stop, confirm `ConfirmedAbsent`; then compare-and-remove records
   under the record lock and start through the internal non-reentrant start.
5. The final identity check and the signal syscall remain separate; the residual
   TOCTOU is disclosed, not hidden.

This is a **generation-bound stop exception**, not a numeric-only signal: the
CLI signals only a process whose live executable, owner, boot identity and
precise start identity were read and bound moments before, with explicit
consent. It still requires a material ruling because it is a new signal
authority outside the persisted-sidecar policy.

### Rejected: delegated previous-binary stop

Delegating the stop to an unmodified v0.13.1 binary cannot meet generation
checks: it exposes no interface to accept a generation, performs its own
`get_status().ok()` (which deletes records on a failed probe), uninstalls
resolver state unless `EFFIGY_GATEWAY_KEEP_RESOLVER=1`, signals
`Pid::from_raw(pid as i32)` with no ownership/start check, and does not
participate in the transition lock. There is no way to bind it to the inspected
candidate. It is rejected rather than accepted with a numeric-signal risk.

### Installer preservation

The local installer should still stage the replaced binary as an owner-only
`effigy.previous` + `effigy.previous.version` before activation. Its purpose is
evidence and rollback, not signal authority. It does not make a delegated stop
safe and the contingent protocol does not need it.

## Locking

The current `GatewayRecordLock` (`gateway.pid.with_extension("lock")`) is held
only briefly inside `publish_current_gateway` and `remove_if_unchanged`
(`identity.rs`). `run_gateway_up` and `run_gateway_down` do not hold it for the
command, so it cannot serialize commands. A `recover` that held it and then
called the public `up` would re-acquire the same exclusive flock on a second fd
and deadlock; dropping it before `up` would drop the claimed serialization.

Proposed ordering, implemented in the same change:

- Add an owner-only **transition lock** (`gateway.transition.lock`) held by
  `up`, `down` and `recover` for the whole command.
- Keep the **record lock** as today: brief, only inside `publish_current_gateway`
  and `remove_if_unchanged`.
- Ordering is transition lock (outer) → record lock (inner), never the reverse.
- `recover` performs cleanup and start under the transition lock through an
  internal **non-reentrant start** that assumes the lock is held. Public `up`
  acquires the transition lock then calls the same internal function. `recover`
  never calls public `up`, so there is no second acquisition and no deadlock.

Until the transition lock is added, no cross-command serialization may be
claimed. The contingent protocol is designed against the post-change ordering.

## Missing record and crash treatment

| Situation at `recover` start | Behavior |
| --- | --- |
| Record present | Capture PID/version/bytes/owner/digests; probe; absent → clean + start; running/unknown → inspect (contingent) or refuse (fallback) |
| Record already absent | No candidate to inspect or probe. Start only; if the old daemon still runs, the start fails to bind and the failure is surfaced. Never signal |
| Crash before inspection | Transition lock released on process exit; records intact; rerunnable |
| Crash after stop, before cleanup | Records intact; next `recover` probes absent → clean + start |
| Crash after cleanup, before start | Absent records; next `recover`/`up` starts |
| Crash after start | Normal authenticated lifecycle |

## Entrypoints and consent contract

| Surface | Behavior |
| --- | --- |
| `effigy gateway status [--json]` | Main: `LegacyIdentityRequired` plus the proposal's structured recovery payload. Never signals |
| `effigy gateway up`, `down` | Unchanged fail-closed refusal for legacy records; message points at `recover` |
| `effigy gateway recover [--json]` | Fallback: verify + clean + start. Contingent `--adopt-candidate`: inspect, consent, generation-bound stop, clean, start. No blind numeric signal path exists |
| `__gateway-legacy-candidate` (hidden) | Bounded read-only elevated reader. No signal, no write, no start |
| `gateway_up_for_managed_task` and managed `dev` auto-start | Detect a legacy record, surface the `recover` pointer, refuse auto-start; no auto-recovery |
| Container activation auto-start | Same refusal; never a best-effort stop |

Consent prompt/JSON shows, before any action: recorded PID and
`gateway.version`; gateway directory, owner UID, digests; the candidate
generation when inspected (boot, precise start, executable digest/basename,
`is_effigy`); and the exact stop that adoption authorizes. `--json` emits
`effigy.gateway.recover.v1` with `result` (`recovered` | `refused` |
`already_stopped`), `pid`, `version`, `probe`, `candidate`, `adopted`,
`records_removed`, `started`, and `warnings`. Refusals are structured.

## Privilege contract

- Classification, consent, lock, probe, cleanup and start decisioning run
  unelevated.
- The fallback operator stop runs in the operator's host process-management
  context; it may require elevation independent of Effigy.
- The contingent reader runs read-only as root behind the existing elevation
  prompt. The generation-bound stop may also need elevation to signal an
  elevated daemon; it re-checks the bound generation before each signal.
- The start keeps the existing elevation flow; the task-113 correction in main
  makes it accept the authenticated forwarded operator directory owner.
- No new standing helper, install, launchd/systemd unit, socket or standing
  privilege. The Q-002 reader is not widened; the legacy-candidate reader is a
  separate, narrower, read-only command that still requires the ruling.

## Error and edge matrix

| Case | Behavior |
| --- | --- |
| Record present, PID `ConfirmedAbsent` | Compare-and-remove records, then start |
| Record present, PID `Running`, candidate inspection declined/unavailable | Fallback refuses; contingent refuses; preserve; no signal |
| Candidate UID != directory owner | Refuse; preserve; no signal |
| Candidate executable not an Effigy gateway / version mismatch | Refuse; preserve; no signal |
| Candidate boot/start unreadable or ambiguous | Refuse; preserve; no signal |
| Candidate generation changes between checks | Refuse; no signal; re-inspect |
| Consent declined | Refuse; preserve; no signal |
| Records already removed, daemon gone | `recover`/`up` sees no record and starts cleanly |
| Records removed while the daemon still runs | Out of scope for the CLI: no candidate to inspect; `up` may fail to bind. Do not remove live records |
| Independent v0.13.1 `down` already ran | It may already have deleted pid+version and `/etc/resolver`; if `recover` had not yet captured the PID, the absent-record row applies. A live daemon still causes a bind failure, never a signal |
| Concurrent `up`/`down`/`recover` | Transition lock serializes; loser refuses without side effects |
| Previous exe already replaced | No effect; neither the fallback nor the contingent protocol needs it |
| Already-upgraded `v0.14.0` user with legacy record | Install the corrected release, then `recover`; identical flow |
| Network unavailable / offline | No effect; neither flow downloads anything |
| Shared routes (`routes.json` across checkouts) | Never cleared; the new daemon loads the existing table under contract 033 trust; foreign live claims keep failing closed |
| Shared/elevated daemon owned by another UID | Refuse; ownership is not guessed |
| Malformed/untrusted/symlink/identity-only record | Preserve; no `recover`; diagnostics only |
| Reused PID (recorded PID is a foreign process) | Candidate inspection fails `is_effigy`/owner/version; refuse; no signal |

## Compatibility

- Version scoping is authoritative: published `v0.14.0` cannot complete the
  path; main `4201e0b3` can start but has no recovery; the proposal adds
  recovery on a corrected release.
- Route table envelope (`_managed_by` marker, domain-keyed map) is unchanged
  between v0.13.1 and v0.14.0; recovery never rewrites it. A table that fails
  contract 033 trust is left for the operator.
- v0.13.1 `down` removes `gateway.pid` + `gateway.version` and `/etc/resolver`
  unless `EFFIGY_GATEWAY_KEEP_RESOLVER=1`; it has no ownership check, no
  generation binding and no lock.
- CLI direction: forward upgrade is supported; a global downgrade is not a
  launch solution because older parsers reject newer manifests and older
  binaries reintroduce numeric-only signalling.
- Kickoff mechanism rejected: emptying the global route table to trigger the
  daemon's 5-minute idle shutdown would disturb other checkouts' routes and does
  not prove ownership.

## Private proofs

Existing controls that prove the current CLI does no foreign signal or record
mutation on refusal (cited, not modified):

- `crates/effigy-gateway/src/identity.rs::gateway_identity_legacy_and_malformed_bytes_remain_unmodified`
  — numeric-only and malformed bytes unchanged after a trusted read.
- `gateway_identity_interrupted_pair_is_unknown_and_preserved`,
  `gateway_identity_symlink_and_unsafe_mode_are_rejected_without_following`.
- `crates/effigy-gateway/src/server/tests.rs::server_probe_state_status_unknown_preserves_pid_and_version_records`,
  `server_probe_state_start_refuses_unknown_and_preserves_records`,
  `gateway_identity_legacy_record_requires_migration_and_unknown_probe_stays_distinct`,
  `gateway_identity_matching_private_child_is_reported_running`,
  `gateway_identity_reused_live_pid_is_not_running_and_is_not_signalled`.
- `src/runner/gateway_command/daemon.rs::gateway_identity_mismatch_or_unknown_dispatches_no_signal`,
  `gateway_probe_state_unknown_before_stop_dispatches_no_signal`,
  `gateway_probe_state_unknown_after_term_refuses_success_without_kill`.
- `src/runner/gateway_command/tests.rs::gateway_identity_legacy_active_record_refused_by_status_up_down_and_managed_start`,
  `gateway_identity_elevated_reader_decline_or_unavailable_is_unknown`,
  `probe_state_up_refuses_unknown_without_starting_a_replacement`.

New private controls the implementation task must land (recording/fake fixtures,
fresh `mktemp -d`, real owned children, no live gateway, no real elevated read,
no install, no network):

1. **No-signal oracle**: record every signal dispatch behind the existing seam;
   assert zero dispatches for legacy classification, running/unknown probes,
   declined inspection, refused candidate, and the absent-continue path.
2. **Candidate reader**: a recording fake reader proves it accepts only the path
   digest + record digest + operator UID, rejects arbitrary PID/path, caps
   output, and returns Unknown on decline/timeout/malformed/mismatch.
3. **Candidate binding**: a real owned child as the candidate; assert adoption
   requires the full generation and that a boot/start/uid/exe mismatch refuses.
4. **Generation-bound stop**: a real owned child whose identity matches is
   signalled exactly once with TERM and re-checked before KILL; a child that
   changes generation (exit/reuse) receives no signal.
5. **Fallback gate**: with a recorded live child, fallback `recover` refuses,
   leaves records byte-identical and does not start; after the child is reaped,
   it cleans and starts.
6. **Lock ordering**: `recover` holding the transition lock calls the internal
   start without a second acquisition and completes; a concurrent `up`/`down`
   loses without side effects; no flock deadlock.
7. **Missing-record/crash**: absent-record start, crash-after-stop-before-cleanup,
   and crash-after-cleanup-before-start are each rerunnable.
8. **Overwritten-binary/local-build**: a local-build version records evidence and
   continues without resolving or running a previous binary.
9. **Installer preservation**: the staged previous binary is owner-only and is
   never executed by `recover`.
10. **v0.13.1 negative controls**: a recording fake v0.13.1 `down` models record
    deletion on a failed probe, resolver uninstall without
    `EFFIGY_GATEWAY_KEEP_RESOLVER=1`, and lock non-participation. The supported
    flows never invoke it, and the delegated design stays rejected.

## Implementation plan

Minimal, bounded. Source unchanged by this assessment; the implementation task
owns these edits, and the contingent protocol is blocked on the material ruling.

| Area | Owner | Change |
| --- | --- | --- |
| Diagnostics | main `4201e0b3` (task 112) | `LegacyIdentityRequired`, preserved records; the proposal adds the structured payload |
| Elevated owner trust | main `4201e0b3` (task 113) | Prerequisite for the start; no work in this task |
| Transition lock | gateway runner | Add `gateway.transition.lock`; hold in `up`/`down`/`recover`; inner record lock unchanged |
| Fallback entrypoint | gateway runner | `effigy gateway recover`: consent, transition lock, probe, confirmed-absence gate, compare-and-remove, internal start. No signal |
| Contingent reader | gateway runner | `__gateway-legacy-candidate`, bounded read-only elevated; no signal/write/start |
| Contingent stop | gateway runner | `recover --adopt-candidate`: consent, generation-bound TERM/KILL with re-check |
| Installer | `scripts/build-local-bin.rhai` | Stage `effigy.previous` + `.version` before activation, as evidence/rollback only |
| Selectors | `config/tasks.toml` | `test:gateway:legacy-recovery`, `check:gateway:legacy-recovery`, `test:install:previous-binary-preservation`; extend `qa:docs:gateway-identity` |

Testing: private fixtures only, fresh `mktemp -d`, recording seams, real owned
children, no live daemon, no install, no network. Suggested selectors:

- `effigy test:gateway:identity` and `effigy check:gateway:identity` (existing).
- `effigy test:gateway:legacy-recovery` and `check:gateway:legacy-recovery` (new).
- `effigy test:gateway:probe-state`, `test:gateway:pid-domain` (existing refusal proofs).
- `effigy qa:docs:gateway-identity` and `effigy fmt:check`.

Rollback: the transition lock, recovery entrypoint, structured payload,
contingent reader/stop and installer backup are additive; reverting the
implementation commit restores current fail-closed behavior. No data migration.

Patch runway: a PATCH release carrying the directory/diagnostic corrections
plus the fallback flow and installer preservation; the contingent
protocol waits for the ruling. After tagged-source and install proof, re-pin
`latest` and the Homebrew formula in one formula commit. Do not touch the
`v0.14.0` tag, assets or workflows.

## Material ruling required

The smallest honest ruling needed has two parts. It does **not** ask to signal
without ownership proof:

> 1. Authorize a bounded, read-only elevated `__gateway-legacy-candidate` reader
>    for the recorded legacy PID, accepting only the canonical path digest, the
>    validated record digest and the operator UID, returning bounded
>    executable/owner/boot/precise-start evidence, never signalling or writing.
> 2. Authorize a generation-bound stop exception: after explicit interactive
>    consent, `recover` may TERM/KILL only the exact inspected generation,
>    re-checking boot, precise start, owner and executable immediately before
>    each signal, and refusing on any mismatch, unknown or foreign case.

Until that ruling is granted, the contingent protocol is unimplemented and only
the fallback exists. The ruling does not authorize a blind numeric signal, a
general previous-binary delegation, or a fabricated sidecar. If the operator
declines, the honest outcome is the fallback plus the documented manual
procedure, and the consumer gap remains.

## Residual limits and material rulings

- Offline, overwritten, local-build records are reachable through the fallback;
  the CLI needs no network and no previous binary.
- The fallback's `Running` case still needs operator host-process management; an
  unelevated user may not be able to read an elevated daemon's identity. This is
  the gap the contingent ruling closes.
- The numeric-only record still confers no ownership proof. The contingent
  protocol supplies live candidate evidence; the fallback relies on the
  operator's independent confirmation.
- A delegated previous-binary stop is rejected; its v0.13.1 hazards (record
  deletion on a failed probe, resolver uninstall, no lock, no generation
  binding) are recorded in [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour).
- The final identity-check-to-signal TOCTOU is disclosed for both the existing
  sidecar path and the contingent generation-bound stop; neither claims atomic
  process targeting.
- The published `v0.14.0` binary cannot complete the path; a corrected release
  is required.

## Requirements mapping

| Brief requirement | Section |
| --- | --- |
| v0.13 active → upgrade → start path | [Failure](#failure), [Proposed flow](#proposed-flow) |
| Already-upgraded recovery | [Version and state scoping](#version-and-state-scoping), [Error and edge matrix](#error-and-edge-matrix) |
| Concrete candidate-inspection route | [Contingent: bounded candidate inspection](#contingent-bounded-candidate-inspection), [Consent and generation-bound stop](#contingent-consent-adoption-and-generation-bound-stop) |
| Exact entrypoints, privilege, consent | [Entrypoints](#entrypoints-and-consent-contract), [Privilege](#privilege-contract) |
| Realistic compatibility | [Compatibility](#compatibility), [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour) |
| Overwritten binary; old-CLI/new-manifest | [Failure](#failure), [Private proofs](#private-proofs) |
| No foreign signals / record deletion on unknown/refusal | [Private proofs](#private-proofs) |
| Lock ordering / non-reentrant start | [Locking](#locking) |
| Missing record / crash treatment | [Missing record and crash treatment](#missing-record-and-crash-treatment) |
| Selectors + implementation/testing/rollback/patch | [Implementation plan](#implementation-plan) |
| Residual limits and material ruling | [Material ruling required](#material-ruling-required), [Residual limits](#residual-limits-and-material-rulings) |
| Public channel assessment + pin/rollback/patch | [Public channel assessment](#public-channel-assessment) |
| Source unchanged; architecture/guidance corrected | This document; [020](020-container-infrastructure-design.md#gateway-process-identity); [083](../../guides/083-v0.14.0-consumer-migration.md) |
