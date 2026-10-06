# 034 - Gateway Legacy Upgrade and Recovery (Proposed)

Status: proposed and unavailable until implementation. No `effigy gateway
recover` entrypoint, installer previous-binary preservation, or previous-binary
resolver exists today. Ships nothing. Current v0.14.0 behavior is fail-closed
and blocks a live numeric-only record: see
[020](020-container-infrastructure-design.md#gateway-process-identity) and
[guide 083](../../guides/083-v0.14.0-consumer-migration.md). Do not present this
flow as current behavior.

Owner: gateway lifecycle maintainers
Architecture: [020](020-container-infrastructure-design.md#gateway-process-identity)
Guidance: [083](../../guides/083-v0.14.0-consumer-migration.md)
Related: task 112 (legacy diagnostics), task 113 (elevated owner trust),
[Q-001](../questions.md#q-001--gateway-pid-identity),
[Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access)
Authority: no new standing helper, install, workflow, release or live-operation
authority is granted here.

## Purpose and boundary

Upgrading an active pre-identity gateway must not strand the user. Today a
numeric-only `gateway.pid` refuses every lifecycle action and the only recorded
transition is a manual procedure that assumes a previous binary that the
installer overwrites. This document defines one supported upgrade/recovery
design, its exact entrypoint, consent and privilege contract, its compatibility
limits, the private proofs that must land with it, and the patch runway. It is
a design for a bounded implementation task, not an implementation.

Hard boundaries:

- Current identity policy is unchanged: a numeric-only, malformed, unreadable
  or unknown record never authorizes a signal from the current CLI, never
  fabricates a sidecar, and is preserved byte-for-byte on refusal.
- No standing helper, install, privilege, host-run 010, workflow or release
  change. The only signal authority is an authenticated previous binary run by
  explicit operator consent.
- No silent config downgrade, no re-tag, no v0.14.0 asset mutation.

## Current truth versus proposal

| Concern | Current truth (v0.14.0, merged) | Proposal in this document |
| --- | --- | --- |
| Legacy numeric-only record | `get_verified_gateway_status*` returns `ProcessStateUnknown` before any probe; `up`, `down`, `status` refuse; records preserved | Same fail-closed refusal; adds a classified `legacy_record` diagnostic and one explicit recovery entrypoint |
| Stopping the old daemon | No current-CLI signal path; manual "previous binary" note only | `effigy gateway recover` resolves an authenticated previous binary, runs its `gateway down`, then independently confirms absence |
| Previous binary | Local install overwrites `.local-install/bin/effigy`; nothing preserved | Installer stages the replaced binary as an owner-only backup plus version file before activation |
| New daemon start | Blocked by the elevated-owner trust defect (task 113) | `recover` ends in a normal `up`; the complete path requires task 113 |
| Diagnostics | Generic `cannot determine gateway state (...) ; refusing to guess` | Task 112 owns a precise `legacy_record` payload and recovery pointer |
| Public channels | `releases/latest` = v0.13.1; Homebrew formula restored to v0.13.1; v0.14.0 body warns | Keep the done mitigation; add a fix-forward 0.14.1 patch runway |

## Failure

Reproduced by code reading, receipts in [Public channel assessment](#public-channel-assessment)
and [Private proofs](#private-proofs). Sequence for a gateway started by an
Effigy build that predates the identity sidecar:

1. The 0.13.x daemon wrote decimal `~/.effigy/gateway/gateway.pid` and
   `gateway.version` (for example `v0.13.1+local.677`). No `gateway.identity`
   exists. The daemon runs elevated and holds `:53`, `:80`, `:443`.
2. The user upgrades to 0.14.0. The local installer `mv`s the new binary over
   `.local-install/bin/effigy`; the previous binary and its version are gone.
   Homebrew and GitHub-releases installs replace the binary in place.
3. `effigy gateway status`, `up` and `down` call `read_snapshot`, get
   `record() == None`, and return `ProcessStateUnknown { pid }` before probing.
   The user sees "cannot determine gateway state ... ; refusing to guess". The
   old daemon keeps running. `up` cannot bind and cannot replace it.
4. The documented manual path ("stop the old daemon with the previous binary")
   is not reachable: the binary is overwritten, and running an older CLI for
   ordinary work fails on newer manifests. Receipt: a v0.13.1 CLI run against
   this repo's linked Longhorn `proof:artifacts` manifest fails to parse, so
   `effigy dev --plan` fails before execution. A global CLI downgrade is not a
   consumer launch solution.
5. Even after records clear, elevated `up` hits the distinct task-113 defect:
   `trusted_directory_owner` accepts only a root- or effigy-caller-owned
   directory, so a genuine ordinary-operator-owned `~/.effigy/gateway` is
   rejected as `gateway directory is unsafe` during elevation. The complete
   path needs task 113.

Two independent blockers: the legacy transition (this document) and the
elevated owner trust boundary (task 113, separate owner). This document covers
both only as an end-to-end sequence and names the dependency.

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
| Existing 0.14.0 installs | Running binary unchanged | Not protected by channel changes; need the patch or the recovery flow |

Homebrew digests were compared against the v0.13.1 asset metadata and read back
byte-equal. The rollback is reversible: applying the next formula commit
restores the channel. No tag, workflow or release-asset mutation is part of the
mitigation.

Minimal rollback/pin-or-patch proposal:

1. Pin (done): `latest` = v0.13.1, Homebrew = v0.13.1, v0.14.0 release body
   warning. Preserve the pin until 0.14.1 is verified.
2. Do not attempt to remove or re-tag v0.14.0 assets, and do not silently
   downgrade installed 0.14.0 clients.
3. Patch forward: ship `v0.14.1` (PATCH: `Fixed`/`Security`) containing task
   113, task 112, the recovery flow, and installer preservation. After the
   asset/install proof passes, re-pin `latest` and the Homebrew formula to
   0.14.1 in one formula commit.
4. Record the channel posture and the fix-forward rule in the release
   procedure; future regressions use the same reversible pin, never a re-tag.

## Proposed flow

One new lifecycle entrypoint, `effigy gateway recover`, plus installer and
diagnostic changes. Existing `status`, `up` and `down` stay fail-closed.

Classification (unchanged identity reads):

| Record state | Classification | Action |
| --- | --- | --- |
| PID + valid matching sidecar | Authenticated | Normal lifecycle |
| PID, no sidecar file at all | Legacy numeric-only | Refuse normally; `recover` may transition with consent |
| PID + malformed/unparseable sidecar, identity-only pair, unsafe mode, symlink, untrusted owner, unreadable | Unknown/untrusted | Preserve; refuse; diagnostics only, no `recover` |
| Absent | Not running | Normal start |

`recover` steps, in order:

1. Read and classify. Refuse on unknown/untrusted (no recovery). Confirm
   `gateway.pid` exists and no `gateway.identity` file is present; a present
   but malformed sidecar is unknown/untrusted, not legacy. Capture the exact
   decimal PID, `gateway.version`, record bytes, owner UID and digests before
   any action.
2. Require interactive-terminal consent, or `--yes` plus explicit
   `--accept-legacy-pid-risk`; print the fields in [Consent contract](#entrypoints-and-consent-contract).
   Non-interactive without the acknowledgement fails closed.
3. Acquire a recovery lock in the gateway directory (owner-only, same locking
   family as the record lock). Refuse if a concurrent `up`/`down`/`recover`
   holds it.
4. Resolve an authenticated previous binary in [resolver order](#previous-binary-resolution).
   Refuse if none is available (offline/no-backup case) with the offline
   alternatives from [Residual limits](#residual-limits-and-material-rulings).
5. Run that binary as `<previous> gateway down` with the operator's `HOME`, a
   neutral temporary cwd that contains no `effigy.toml`, and the inherited
   elevation prompt. Do not pass `--repo`. This is the only signal authority;
   the current CLI never signals a legacy PID.
6. If the previous binary exits non-zero, stop: preserve all records, report
   its output, do not start. If it exits zero, re-probe the captured PID with
   the production tri-state probe.
7. Accept only `ConfirmedAbsent`. On `Running` or `Unknown`, refuse, preserve
   any remaining records, and do not start. This is the "cleanup only after
   exact old gone" gate, and it is independent of the previous binary's own
   re-probe.
8. On confirmed absence, remove any remaining legacy records with
   compare-and-remove under the record lock (`remove_if_unchanged`). This is
   idempotent and touches only the captured generation. Preserve unrelated
   files. Never remove records on Running/Unknown/refusal.
9. Start via the normal `up` path, publishing `gateway.pid` + `gateway.identity`
   + `gateway.version`. The elevated-owner trust correction (task 113) is a
   hard prerequisite here.

Existing routes, TLS certs, loopback assignments, containers and volumes are
untouched. `routes.json` is never cleared or rewritten by recovery.

## Entrypoints and consent contract

Maintained surfaces after implementation:

| Surface | Behavior |
| --- | --- |
| `effigy gateway status [--json]` | Adds a classified `legacy_record`/`recovery` block (task 112). Still exit non-zero/refuse normally |
| `effigy gateway up`, `down` | Unchanged fail-closed refusal for legacy records; message points at `recover` |
| `effigy gateway recover [--json]` | The only new entrypoint. Interactive consent; `--yes` for non-interactive with the risk acknowledgement |
| `gateway_up_for_managed_task` and managed `dev` auto-start | Detect a legacy record, surface the `recover` pointer, refuse auto-start; no auto-recovery |
| Container activation auto-start | Same refusal; never a best-effort stop |

Consent prompt/JSON must show, before any action:

- recorded PID and `gateway.version`;
- gateway directory, record owner UID, and record digests;
- the previous binary path, provenance (`preserved-install` |
  `published-release` | `operator-provided`), version and digest;
- that the previous binary will signal a numeric-only record (the old-PID risk),
  and that no sidecar proves ownership;
- that records are preserved until the exact old process is confirmed gone.

`--json` emits `effigy.gateway.recover.v1` with `result`
(`recovered` | `refused` | `legacy_record`), `pid`, `version`,
`previous_binary { path, provenance, digest, version }`, `stop_confirmed`,
`records_removed`, and `warnings`. Refusals are structured, not prose-only.

## Previous binary resolution

Ordered, first match wins, all candidates verified against a recorded digest
before execution:

1. **Preserved install backup** — `<install-dir>/effigy.previous` plus
   `effigy.previous.version`, staged by the installer before replacement. Match
   the recorded `gateway.version`. Provenance `preserved-install`.
2. **Published release asset** — for a clean release `gateway.version` (for
   example `0.13.1`), download the platform asset for that tag into a private
   `mktemp -d` and verify its SHA-256 against a recorded digest. Trust anchor
   order: a `<asset>.sha256` sidecar when the release carried one (v0.14+); the
   Homebrew tap formula history line for that exact version (for example the
   v0.13.1 formula `27a79c277`, all four platform digests); an
   operator-supplied `--previous-sha256`. Provenance `published-release`.
3. **Operator-provided path** — `--previous-binary <path>` plus a required
   `--previous-sha256`. The binary must self-report the recorded version.
   Provenance `operator-provided`; the operator trusts the path, the digest is
   still enforced.

Local-build versions (`v0.13.1+local.<sha>`, `.dirty`) have no published asset.
With no preserved backup and no operator path, `recover` refuses. It never
falls back to a numeric-only signal from the current CLI.

## Privilege contract

- Classification, consent, resolution, download, digest check, lock and
  post-stop confirmation run unelevated.
- The previous binary keeps its own existing elevation path (osascript/sudo or
  equivalent). Declined, unavailable or non-interactive authentication there
  means the stop did not happen: `recover` reports unknown/refused, preserves
  records, and does not start.
- The new `up` keeps the existing elevation flow; the task-113 fix must make it
  accept the forwarded operator directory owner. No new helper, install,
  launchd/systemd unit, socket or standing privilege is introduced.
- No cross-UID live-identity read beyond the existing bounded read-only
  `__gateway-identity` prompt; that path is only for authenticated sidecar
  records and does not apply to legacy records.

## Error and edge matrix

| Case | Behavior |
| --- | --- |
| Elevation auth declined | No signal, records preserved, `refused` with the declined state |
| Previous binary exits non-zero / still running | No start, records preserved, previous binary output surfaced |
| Probe `Unknown` after stop | Refuse; preserve; no start; no record deletion |
| Probe `ConfirmedAbsent` | Compare-and-remove remaining records, then start |
| Crash before stop | Only the recovery lock and temp dir exist; records intact; rerunnable |
| Crash after stop, before start | Records may be gone or partial; next `up`/`recover` is idempotent; absent records + no daemon start cleanly |
| Crash after start | Normal authenticated lifecycle |
| Concurrent `up`/`down`/`recover` | Recovery lock serializes; loser refuses without side effects |
| Previous exe already replaced | Identical flow; resolution falls to published asset or operator path |
| Already-upgraded 0.14 user with legacy record | Identical flow; no version-state assumption |
| Network unavailable / offline | If no preserved or operator binary: refuse with offline alternatives; no signal |
| Shared routes (`routes.json` across checkouts) | Never cleared; new daemon loads the existing table under contract 033 trust; foreign live claims keep failing closed |
| Shared/elevated daemon owned by another operator UID | Refuse; ownership is not guessed; diagnostics name the untrusted owner |
| Malformed/untrusted/symlink/identity-only record | Preserve; no `recover`; diagnostics only |

## Compatibility

- Route table envelope (`_managed_by` marker, domain-keyed map) is unchanged
  between v0.13.1 and v0.14.0; recovery never rewrites it. A route table that
  fails contract 033 trust is left for the operator, not repaired.
- Record formats: decimal `gateway.pid` stays compatible; `gateway.identity` is
  additive; v0.13.1 ignores an unknown sidecar. The previous binary's `down`
  removes `gateway.pid` and `gateway.version` but leaves any sidecar, so the
  new CLI's confirmed-absence step owns final cleanup.
- CLI direction: forward upgrade is supported; a global downgrade is not a
  launch solution because older parsers reject newer manifests and older
  binaries reintroduce numeric-only signalling. Running the previous binary is
  scoped to its `gateway down` subcommand from a neutral cwd, never as the
  repo's working CLI.
- Kickoff mechanism rejected: emptying the global route table to trigger the
  daemon's 5-minute idle shutdown would disturb other checkouts' routes and
  does not prove ownership.

## Private proofs

Existing controls that already prove no foreign signal or record mutation on
refusal (cited, not modified):

- `crates/effigy-gateway/src/identity.rs::gateway_identity_legacy_and_malformed_bytes_remain_unmodified`
  — numeric-only and malformed bytes unchanged after a trusted read.
- `gateway_identity_interrupted_pair_is_unknown_and_preserved`,
  `gateway_identity_symlink_and_unsafe_mode_are_rejected_without_following`.
- `crates/effigy-gateway/src/server/tests.rs::server_probe_state_unknown_preserves_pid_and_version_records`,
  `server_probe_state_start_refuses_unknown_and_preserves_records`,
  `gateway_identity_matching_private_child_is_reported_running`,
  `gateway_identity_reused_live_pid_is_not_running_and_is_not_signalled`.
- `src/runner/gateway_command/daemon.rs::gateway_identity_mismatch_or_unknown_dispatches_no_signal`,
  `gateway_probe_state_unknown_before_stop_dispatches_no_signal`,
  `gateway_probe_state_unknown_after_term_refuses_success_without_kill`.
- `src/runner/gateway_command/tests.rs::gateway_identity_elevated_reader_decline_or_unavailable_is_unknown`,
  `probe_state_up_refuses_unknown_without_starting_a_replacement`.

New private controls the implementation task must land (recording/fake-binary,
fresh `mktemp -d`, no live daemon, no real signal):

1. **No-signal oracle**: record every signal dispatch behind the existing seam;
   assert zero dispatches for legacy classification, auth decline, non-zero
   previous binary, `Running` and `Unknown`.
2. **Previous-binary resolver**: a recording fake `gateway down` script proves
   resolution order (preserved-install beats published beats operator), digest
   rejection, and that the current CLI executes the previous binary exactly
   once with a neutral cwd and no `--repo`.
3. **Overwritten-binary case**: install fixture with no backup and a local-build
   version refuses with the offline message and zero signals; install fixture
   with a backup succeeds.
4. **New-manifest-unsupported-old-dev case**: a fake previous binary that
   rejects any directory containing `effigy.toml` proves the flow runs from a
   neutral cwd and never depends on the checkout manifest.
5. **Cleanup gate**: a fake previous binary that returns success without
   stopping a recorded live private child; assert `recover` sees `Running`,
   leaves records byte-identical and does not start.
6. **Installer preservation**: drive the local install path against a fake
   install dir and assert the previous binary + version are staged owner-only
   before the new binary is activated.

## Implementation plan

Minimal, bounded. Code unchanged by this assessment; the implementation task
owns these edits.

| Area | Owner | Change |
| --- | --- | --- |
| Diagnostics | task 112 | Classified `legacy_record` in `gateway status --json` and the refusal message |
| Elevated owner trust | task 113 | Fix `trusted_directory_owner` for the forwarded operator UID through the whole elevated chain |
| Recovery entrypoint | gateway runner | `effigy gateway recover`, consent, lock, resolver, stop, confirm, cleanup, then `up` |
| Previous binary resolution | gateway runner | Backup lookup, published-asset download + digest, operator path |
| Installer | `scripts/build-local-bin.rhai` | Stage `effigy.previous` + `.version` before activation |
| Selectors | `config/tasks.toml` | `test:gateway:legacy-recovery`, `check:gateway:legacy-recovery`, `test:install:previous-binary-preservation`; extend `qa:docs:gateway-identity` to the new knowledge |

Testing: private fixtures only, fresh `mktemp -d`, recording seams, no live
daemon, no install, no network in tests. Suggested selectors:

- `effigy test:gateway:identity` and `effigy check:gateway:identity` (existing).
- `effigy test:gateway:legacy-recovery` and `check:gateway:legacy-recovery` (new).
- `effigy test:gateway:probe-state`, `test:gateway:pid-domain` (existing refusal proofs).
- `effigy qa:docs:gateway-identity` and `effigy fmt:check`.

Rollback: the recovery entrypoint, resolver and installer backup are additive;
reverting the implementation commit restores current fail-closed behavior. Task
113's trust fix is the only non-additive change and reverts to the current
`gateway directory is unsafe` behavior. No data migration.

Patch runway: `v0.14.1` = task 113 + task 112 + this flow + installer
preservation. After tagged-source and install proof, re-pin `latest` and the
Homebrew formula to 0.14.1. Do not touch the v0.14.0 tag, assets or workflows.
Hold Q-001 release assurance until the patch passes its gates.

## Residual limits and material rulings

- The numeric-only record still confers no ownership proof. The stop relies on
  the authenticated previous binary signalling the recorded PID; the old-PID
  reuse risk is bounded by explicit consent and the confirmed-absence re-probe,
  not eliminated. This is the policy Tom's ruling already accepts for an
  explicit operator transition.
- Offline, already-replaced, local-build records with no operator-provided
  binary are not automatically recoverable under current policy. `recover`
  refuses with the alternatives: stage the exact published asset from a
  connected machine (with its digest), restore the preservation backup, or run
  the disclosed manual stop. It never signals from the current CLI.
- A materially different outcome (a previous-binary-free offline path, or
  auto-recovery inside `up`) is not achievable without weakening "no numeric
  record authorizes a current-CLI signal" or "no guessed process ownership".
  The smallest honest ruling needed is a narrowly scoped, explicit,
  interactive operator-consent transition that lets the current CLI inspect
  one live candidate by boot + precise start identity at recovery time. This
  document does not assume that ruling and does not design it as the default.
- The final identity-check-to-signal TOCTOU and the bounded macOS cross-UID
  reader limits recorded in [020](020-container-infrastructure-design.md#gateway-process-identity)
  remain unchanged.
- The complete end-to-end path depends on task 113; without it, `recover` can
  stop the old daemon but the new `up` still fails at the elevated owner trust
  boundary.

## Requirements mapping

| Brief requirement | Section |
| --- | --- |
| v0.13 active → upgrade → v0.14 start path | [Failure](#failure), [Proposed flow](#proposed-flow) |
| Already-upgraded recovery | [Error and edge matrix](#error-and-edge-matrix) |
| Exact entrypoints, privilege, consent | [Entrypoints](#entrypoints-and-consent-contract), [Privilege](#privilege-contract) |
| Realistic compatibility | [Compatibility](#compatibility) |
| Overwritten binary; new-manifest-unsupported old dev | [Failure](#failure), [Private proofs](#private-proofs) |
| No foreign signals / record deletion on unknown/refusal | [Private proofs](#private-proofs) |
| Existing selectors + minimal implementation/testing/rollback/patch | [Implementation plan](#implementation-plan) |
| Residual limits and material ruling | [Residual limits](#residual-limits-and-material-rulings) |
| Public channel assessment + minimal pin/rollback/patch | [Public channel assessment](#public-channel-assessment) |
| Source unchanged; architecture/guidance corrected | This document; [020](020-container-infrastructure-design.md#gateway-process-identity); [083](../../guides/083-v0.14.0-consumer-migration.md) |
