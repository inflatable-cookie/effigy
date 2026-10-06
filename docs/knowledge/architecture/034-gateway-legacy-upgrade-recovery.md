# 034 - Gateway Legacy Upgrade and Recovery (Proposed)

Status: proposed and unavailable until implementation. No `effigy gateway
recover` entrypoint exists today. Published v0.14.0 cannot complete the path at
all: it rejects the operator-owned gateway directory during elevation and
reports `ProcessStateUnknown` for a record with no sidecar. Main `4201e0b3`
(tasks 113 and 112, PR228/PR229) corrects the directory check and adds the
distinct `LegacyIdentityRequired` migration error, but still has no recovery
entrypoint. Full automated recovery is not achievable under current ownership
policy; the exact ruling required is in
[Material ruling required](#material-ruling-required). See
[020](020-container-infrastructure-design.md#gateway-process-identity) and
[guide 083](../../guides/083-v0.14.0-consumer-migration.md).

Owner: gateway lifecycle maintainers
Architecture: [020](020-container-infrastructure-design.md#gateway-process-identity)
Guidance: [083](../../guides/083-v0.14.0-consumer-migration.md)
Related: task 112 (legacy diagnostics, merged), task 113 (elevated owner trust,
merged), [Q-001](../questions.md#q-001--gateway-pid-identity),
[Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access)
Authority: no new standing helper, install, workflow, release or live-operation
authority is granted here. Nothing below authorizes a numeric-only signal.

## Purpose and boundary

Upgrading an active pre-identity gateway must not strand the user. This document
defines the supported recovery that current ownership policy permits, states
exactly why full automation is not possible yet, and names the smallest honest
ruling needed. It is a design for a bounded implementation task, not an
implementation.

Hard boundaries:

- A numeric-only, malformed, unreadable or unknown record never authorizes a
  current-CLI signal, never fabricates a sidecar, and is preserved byte-for-byte
  on refusal. This design does not change that.
- The current CLI does not delegate a numeric signal to a previous binary. A
  digest-verified previous binary proves provenance, not candidate-process
  ownership. See [Policy boundary](#policy-boundary) and
  [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour).
- A CLI flag cannot grant the authority the policy withholds. Consent is not
  ownership proof.
- No new standing helper, install, privilege, host-run 010, workflow or release
  change. No silent config downgrade, no re-tag, no v0.14.0 asset mutation.

## Version and state scoping

Three different products are in scope. Do not mix them.

| State | Directory check on elevation | Numeric-only record | Recovery entrypoint |
| --- | --- | --- | --- |
| Published `v0.14.0` tag | Rejects the operator-owned directory (`gateway directory is unsafe`) | `ProcessStateUnknown { pid }` | None. Cannot complete a legacy upgrade |
| Main `4201e0b3` (tasks 113 + 112, PR228/PR229) | Accepts the authenticated operator-owned directory | `LegacyIdentityRequired { pid }` after a read-only probe; `ProcessStateUnknown` when the probe is unknown | None |
| Proposed (this document, target `v0.14.1`) | Same as main | `LegacyIdentityRequired` plus a structured recovery payload | `effigy gateway recover` (verify + clean + start; never signals) |

The published v0.14.0 binary is the one consumers have. Its start path stays
broken even after records clear, so no current CLI on that binary can finish the
recovery. A consumer completes the path only after installing the patched
`v0.14.1`, which carries the merged directory correction and the new entrypoint.

## Current truth versus proposal

| Concern | Current truth | Proposal in this document |
| --- | --- | --- |
| Legacy numeric-only record | Published v0.14.0: `ProcessStateUnknown`. Main `4201e0b3`: `LegacyIdentityRequired`; records preserved; no signal | Keep the fail-closed refusal; add a structured recovery payload |
| Stopping the old daemon | No CLI signal path; main's guidance requires an operator-controlled stop with independently confirmed executable and owner | `effigy gateway recover` verifies the recorded PID is gone, then cleans and starts. It never signals |
| Automated previous-binary stop | Not authorized by the 020 ruling; v0.13.1 `down` is not ownership-safe | Requires the material ruling in [Material ruling required](#material-ruling-required); explicitly not part of the supported flow |
| New daemon start | Published v0.14.0: rejected as unsafe. Main `4201e0b3`: works for the authenticated operator context | `recover` ends in a normal `up` on a v0.14.1 binary |
| Public channels | `latest` = v0.13.1; Homebrew formula restored to v0.13.1; v0.14.0 body warns | Keep the pin; ship the fix-forward 0.14.1 patch |

## Failure

Receipts in [Public channel assessment](#public-channel-assessment) and
[Private proofs](#private-proofs). Sequence for a gateway started by an Effigy
build that predates the identity sidecar:

1. The 0.13.x daemon wrote decimal `~/.effigy/gateway/gateway.pid` and
   `gateway.version` (for example `v0.13.1+local.677`). No `gateway.identity`
   exists. The daemon runs elevated and holds `:53`, `:80`, `:443`.
2. The user upgrades to v0.14.0. The local installer `mv`s the new binary over
   `.local-install/bin/effigy`; the previous binary and its version are gone.
   Homebrew and GitHub-releases installs replace the binary in place.
3. On published v0.14.0, `status`, `up` and `down` return
   `ProcessStateUnknown { pid }`; on main `4201e0b3` they return
   `LegacyIdentityRequired { pid }` after a read-only probe. Both preserve the
   files and refuse signal, cleanup or replacement. The old daemon keeps
   running. `up` cannot bind and cannot replace it.
4. The interim guidance requires an operator-controlled stop that independently
   confirms the daemon's executable and owner. A general user cannot do this,
   and the installer has already overwritten the previous binary. Running an
   older CLI for ordinary work fails on newer manifests: a v0.13.1 CLI run
   against this repo's linked Longhorn `proof:artifacts` manifest fails to parse
   (`effigy dev --plan` fails before execution). A global downgrade is not a
   consumer launch solution.
5. Published v0.14.0 additionally rejects the operator-owned gateway directory
   during elevation (`gateway directory is unsafe`), so even clearing the records
   does not let that binary start. Main `4201e0b3` corrects this: the elevated
   root caller accepts the forwarded non-root operator UID only when it matches
   ambient `SUDO_UID` (when present) and owns the forwarded `HOME` per passwd;
   unauthenticated root still accepts only root ownership.

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

The remaining blocker is the consumer transition: there is no supported
entrypoint that survives an overwritten binary, and no authorized way for the
CLI to stop a legacy daemon whose only record is numeric.

## Public channel assessment

Read-only verification 2026-10-06. Mitigation is already applied; no authority
to change channels is claimed here.

| Channel | State | Exposure |
| --- | --- | --- |
| GitHub Releases `latest` | `v0.13.1` (`releases/latest` returns v0.13.1) | Pinned to last good; new default downloads get 0.13.1 |
| GitHub Release `v0.14.0` | Immutable, published, not latest; body carries a gateway warning | Explicit downloads still possible; must not be deleted or re-tagged |
| Homebrew tap `Formula/effigy.rb` | Restored to the exact v0.13.1 formula at `c7fa8c55597947c079def106c99f8dc2cdcd8305` (parent `c2045dc59` = the 0.14.0 bump; original v0.13.1 formula `27a79c277`) | `brew install` gets 0.13.1 again; existing 0.14.0 kegs are not silently downgraded |
| `cargo install` from tag / direct binary download | v0.14.0 tag and assets remain | Unprotected; only the release-body warning applies |
| Local `bootstrap:local` install | `build-local-bin.rhai` `mv`s over `.local-install/bin/effigy`, writes `effigy.active-version` | No previous binary kept |
| Existing v0.14.0 installs | Running binary unchanged | Not protected by channel changes; need the patch |

Homebrew digests were compared against the v0.13.1 asset metadata and read back
byte-equal. The rollback is reversible. No tag, workflow or release-asset
mutation is part of the mitigation.

Minimal rollback/pin-or-patch proposal:

1. Pin (done): `latest` = v0.13.1, Homebrew = v0.13.1, v0.14.0 release body
   warning. Preserve the pin until v0.14.1 is verified.
2. Do not remove or re-tag v0.14.0 assets, and do not silently downgrade
   installed v0.14.0 clients.
3. Patch forward: ship `v0.14.1` containing the merged directory correction and
   diagnostic, plus the within-policy recovery entrypoint and installer
   preservation. After tagged-source and install proof, re-pin `latest` and the
   Homebrew formula to v0.14.1 in one formula commit.
4. Record the channel posture and the fix-forward rule in the release procedure;
   future regressions use the same reversible pin, never a re-tag.

## Policy boundary

The 020 ruling authorizes Tom's one-host transition only: an operator
independently confirmed the daemon's executable and owner before stopping it.
It is not a general exception that lets a numeric-only record authorize a
signal. Therefore:

- The current CLI never signals a numeric-only record.
- The current CLI does not invoke a previous binary to signal it either.
  Delegation by proxy is the same signal authority.
- A digest-verified previous binary proves the *binary's* provenance. It does
  not prove that the recorded PID is that binary. The 020 foreign-PID-reuse
  counterexample still applies: a `sleep` child given the same decimal PID is
  `Running` to the probe, and v0.13.1 `terminate_gateway_process` signals it.
- Consent (`--yes`, a risk acknowledgement, an interactive prompt) is not
  ownership proof and cannot supply the missing authority.

## v0.13.1 previous-binary behaviour

This is a compatibility record. It is why the delegated stop cannot make the
guarantees this design would need. Source read at tag `v0.13.1`:

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
- It has no recovery lock and does not join any lock this design proposes.

Consequences: a delegated v0.13.1 stop cannot preserve records on an unknown
probe, cannot serialize with the proposed recovery, and can remove resolver
state. Any fixture that asserts "byte-identical records after a successful
previous `down`" is wrong against production behaviour.

## Proposed flow

### Supported within current policy: operator-verified recovery

`effigy gateway recover` performs the parts the CLI can prove, and refuses the
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
3. Acquire a recovery lock in the gateway directory (owner-only, same locking
   family as the record lock). Refuse if a concurrent `up`/`down`/`recover`
   holds it. This serializes the CLI's own paths; it does not constrain a
   separately run previous binary.
4. Re-probe the captured PID with the production tri-state probe.
   - `ConfirmedAbsent`: continue.
   - `Running` or `Unknown`: refuse, print the operator checks and stop
     instructions, preserve every remaining record, do not start. If the
     operator has independently confirmed the recorded PID is not the gateway
     (a reused PID), the records are stale and may be removed by
     operator-controlled file management, not by the CLI.
5. On `ConfirmedAbsent`, remove any remaining legacy records with
   compare-and-remove under the record lock (`remove_if_unchanged`). This is
   idempotent and touches only the captured generation. Never remove on
   Running/Unknown/refusal.
6. Start via the normal `up` path, publishing `gateway.pid` + `gateway.identity`
   + `gateway.version`. This requires a `v0.14.1` binary (merged task 113).

The operator's stop step is host process management after independent
confirmation. The CLI contributes the freeze, the evidence, the confirmed-absence
gate, the safe cleanup and the start. Existing routes and TLS certs are not
touched by `recover`. If the operator used the v0.13.1 `down` instead, it may
already have deleted `gateway.pid`/`gateway.version` and removed `/etc/resolver`;
`recover` still verifies the captured PID is absent before starting, so a live
daemon is never replaced.

### Not supported: delegated previous-binary stop

The delegated variant (`recover` downloads a digest-verified v0.13.1 binary and
runs `gateway down`) is **not** the supported flow and is not implemented by
this design's default entrypoint. It requires the ruling in
[Material ruling required](#material-ruling-required). The document records the
v0.13.1 behaviour so the cost of that ruling is visible, not hidden.

### Installer preservation

The local installer should still stage the replaced binary as an owner-only
`effigy.previous` + `effigy.previous.version` before activation. Its purpose is
evidence and rollback, not signal authority. It does not make a delegated stop
safe.

## Entrypoints and consent contract

| Surface | Behavior |
| --- | --- |
| `effigy gateway status [--json]` | Main: `LegacyIdentityRequired` plus the proposal's structured recovery payload. Never signals |
| `effigy gateway up`, `down` | Unchanged fail-closed refusal for legacy records; message points at `recover` |
| `effigy gateway recover [--json]` | Verify + clean + start only. Interactive consent; `--yes` allows the absent-only path non-interactively. No signal flag exists |
| `gateway_up_for_managed_task` and managed `dev` auto-start | Detect a legacy record, surface the `recover` pointer, refuse auto-start; no auto-recovery |
| Container activation auto-start | Same refusal; never a best-effort stop |

Consent prompt/JSON shows, before any action: recorded PID and
`gateway.version`; gateway directory, owner UID, digests; the recorded PID's
probe result (`running` | `absent` | `unknown`); and the operator checks
required when it is running. `--json` emits `effigy.gateway.recover.v1` with
`result` (`recovered` | `refused` | `already_stopped`), `pid`, `version`,
`probe`, `records_removed`, `started`, and `warnings`. Refusals are structured,
not prose-only.

## Privilege contract

- Classification, consent, lock, probe, cleanup and start decisioning run
  unelevated.
- The operator's stop runs in the operator's host process-management context; it
  may require elevation independent of Effigy.
- The new `up` keeps the existing elevation flow; the merged task-113 correction
  makes it accept the authenticated forwarded operator directory owner. No new
  helper, install, launchd/systemd unit, socket or standing privilege.
- The bounded read-only `__gateway-identity` prompt stays bound to authenticated
  sidecar records. It does not apply to legacy records and is not widened here.

## Error and edge matrix

| Case | Behavior |
| --- | --- |
| Recorded PID `ConfirmedAbsent` | Compare-and-remove records, then start |
| Recorded PID `Running` | Refuse; preserve; print operator checks and stop instructions; no signal |
| Probe `Unknown` | Refuse; preserve; retry guidance; no signal |
| Operator stop auth declined (host) | Nothing stopped; `recover` reports running/unknown; no CLI signal |
| Operator used v0.13.1 `down` and it exited 0 | v0.13.1 may have deleted pid+version and `/etc/resolver` on a failed probe before confirmation. `recover` re-probes the captured PID; a live daemon still fails the `ConfirmedAbsent` gate and is never replaced |
| Records already removed, daemon gone | `recover`/`up` sees no record and starts cleanly |
| Records removed by the operator while the daemon still runs | Out of scope for the CLI: `recover` cannot probe a PID it cannot read, and `up` may fail to bind (the old daemon holds the ports). Do not remove live records; the supported flow removes them only after `ConfirmedAbsent` |
| Crash before probe | Only the recovery lock exists; records intact; rerunnable |
| Crash after cleanup, before start | Absent records + no daemon start cleanly; rerunnable |
| Concurrent `up`/`down`/`recover` | Recovery lock serializes; loser refuses without side effects |
| Previous exe already replaced | No effect; the supported flow does not need it |
| Already-upgraded v0.14.0 user with legacy record | Install v0.14.1, then `recover`; identical flow |
| Network unavailable / offline | No effect; the supported flow downloads nothing |
| Shared routes (`routes.json` across checkouts) | Never cleared; the new daemon loads the existing table under contract 033 trust; foreign live claims keep failing closed |
| Shared/elevated daemon owned by another UID | Refuse; ownership is not guessed |
| Malformed/untrusted/symlink/identity-only record | Preserve; no `recover`; diagnostics only |
| Reused PID (recorded PID is a foreign process) | `recover` refuses (Running). Only operator file management can clear stale records; the CLI never signals |

## Compatibility

- Version scoping is authoritative: published v0.14.0 cannot complete the path;
  main `4201e0b3` can start but has no recovery; the proposal adds recovery on a
  v0.14.1 binary. Owning guidance must mark each statement by version.
- Route table envelope (`_managed_by` marker, domain-keyed map) is unchanged
  between v0.13.1 and v0.14.0; recovery never rewrites it. A table that fails
  contract 033 trust is left for the operator.
- v0.13.1 `down` removes `gateway.pid` + `gateway.version` and `/etc/resolver`
  unless `EFFIGY_GATEWAY_KEEP_RESOLVER=1`; it has no ownership check and no
  lock. See [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour).
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
fresh `mktemp -d`, no live daemon, no real signal, no install, no network):

1. **No-signal oracle**: record every signal dispatch behind the existing seam;
   assert zero dispatches for legacy classification, running/unknown probes,
   refusal, and the absent-continue path.
2. **Confirmed-absence gate**: a recorded private live child with a legacy
   record; `recover` must refuse, leave records byte-identical and not start.
   After the child is reaped, `recover` must clean and start.
3. **Overwritten-binary case**: install fixture with no backup and a local-build
   version records evidence and continues after the operator stop; it never
   resolves or runs a previous binary.
4. **Neutral-cwd/manifest case**: a fixture that rejects any directory
   containing `effigy.toml` is never used by the supported flow; an operator
   manual-stop fixture proves the flow does not depend on the checkout manifest.
5. **Installer preservation**: drive the local install path against a fake
   install dir and assert the previous binary + version are staged owner-only
   before activation, and that the file is never executed by `recover`.
6. **v0.13.1 negative controls** (design-accurate, proving the delegated path is
   refused/unsafe): a recording fake v0.13.1 `down` models record deletion on a
   failed probe, resolver uninstall without `EFFIGY_GATEWAY_KEEP_RESOLVER=1`,
   and lock non-participation. The supported `recover` must never invoke it, and
   any future delegated path must fail these controls.

## Implementation plan

Minimal, bounded. Source unchanged by this assessment; the implementation task
owns these edits.

| Area | Owner | Change |
| --- | --- | --- |
| Diagnostics (merged) | task 112, main `4201e0b3` | `LegacyIdentityRequired`, preserved records; the proposal adds the structured payload |
| Elevated owner trust (merged) | task 113, main `4201e0b3` | Prerequisite for the start; no work in this task |
| Recovery entrypoint | gateway runner | `effigy gateway recover`: consent, lock, probe, confirmed-absence gate, compare-and-remove, then `up`. No signal |
| Installer | `scripts/build-local-bin.rhai` | Stage `effigy.previous` + `.version` before activation, as evidence/rollback only |
| Selectors | `config/tasks.toml` | `test:gateway:legacy-recovery`, `check:gateway:legacy-recovery`, `test:install:previous-binary-preservation`; extend `qa:docs:gateway-identity` to the new knowledge |

Testing: private fixtures only, fresh `mktemp -d`, recording seams, no live
daemon, no install, no network. Suggested selectors:

- `effigy test:gateway:identity` and `effigy check:gateway:identity` (existing).
- `effigy test:gateway:legacy-recovery` and `check:gateway:legacy-recovery` (new).
- `effigy test:gateway:probe-state`, `test:gateway:pid-domain` (existing refusal proofs).
- `effigy qa:docs:gateway-identity` and `effigy fmt:check`.

Rollback: the recovery entrypoint, structured payload and installer backup are
additive; reverting the implementation commit restores current fail-closed
behavior. No data migration.

Patch runway: `v0.14.1` = the merged directory/diagnostic corrections plus this
within-policy flow and installer preservation. After tagged-source and install
proof, re-pin `latest` and the Homebrew formula to v0.14.1. Do not touch the
v0.14.0 tag, assets or workflows. Hold Q-001 release assurance until the patch
passes its gates.

## Material ruling required

Full automated recovery is not achievable under current ownership policy. The
smallest honest ruling needed:

> May `effigy gateway recover` delegate a numeric PID signal to a
> digest-verified previous binary **without** pre-stop live executable and owner
> proof?

Current answer: **no**. A digest is provenance, not candidate-process proof, and
the 020 ruling authorizes only a one-host transition with independently
confirmed executable and owner. Until that ruling is granted, the supported
flow stays operator-verified as above.

A narrower alternative, if the product wants automation without delegating to a
previous binary, is a bounded, read-only live-identity capability for the
recorded legacy PID (executable path, owner UID, boot + precise start identity)
through existing elevation. That would supply candidate-process proof, but it is
a new read path and would still need a ruling before the CLI signals. This
document does not assume either ruling and does not design them as defaults.

## Residual limits and material rulings

- Offline, overwritten, local-build records are recoverable through the
  operator-verified flow; the CLI needs no network and no previous binary.
- The numeric-only record still confers no ownership proof. The operator's
  independent confirmation is the ownership evidence. If the operator cannot
  confirm and stop the daemon, the supported flow refuses rather than signalling.
- A delegated previous-binary stop remains unauthorized; its v0.13.1 hazards
  (record deletion on a failed probe, resolver uninstall, no lock, no ownership
  check) are recorded in [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour).
- The final identity-check-to-signal TOCTOU and the bounded macOS cross-UID
  reader limits recorded in [020](020-container-infrastructure-design.md#gateway-process-identity)
  are unchanged and do not apply to a legacy record.
- The published v0.14.0 binary cannot complete the path; the fix-forward
  v0.14.1 patch is required.

## Requirements mapping

| Brief requirement | Section |
| --- | --- |
| v0.13 active → upgrade → v0.14 start path | [Failure](#failure), [Proposed flow](#proposed-flow) |
| Already-upgraded recovery | [Version and state scoping](#version-and-state-scoping), [Error and edge matrix](#error-and-edge-matrix) |
| Exact entrypoints, privilege, consent | [Entrypoints](#entrypoints-and-consent-contract), [Privilege](#privilege-contract) |
| Realistic compatibility | [Compatibility](#compatibility), [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour) |
| Overwritten binary; old-CLI/new-manifest | [Failure](#failure), [Private proofs](#private-proofs) |
| No foreign signals / record deletion on unknown/refusal | [Private proofs](#private-proofs) |
| Existing selectors + minimal implementation/testing/rollback/patch | [Implementation plan](#implementation-plan) |
| Residual limits and material ruling | [Material ruling required](#material-ruling-required), [Residual limits](#residual-limits-and-material-rulings) |
| Public channel assessment + minimal pin/rollback/patch | [Public channel assessment](#public-channel-assessment) |
| Source unchanged; architecture/guidance corrected | This document; [020](020-container-infrastructure-design.md#gateway-process-identity); [083](../../guides/083-v0.14.0-consumer-migration.md) |
