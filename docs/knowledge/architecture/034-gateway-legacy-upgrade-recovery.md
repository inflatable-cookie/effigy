# 034 - Gateway Legacy Upgrade and Recovery

Status: implemented on corrected main. `effigy gateway recover`, the bounded
read-only `__gateway-legacy-candidate` reader, and generation-bound
`__gateway-legacy-stop` are available. The published `v0.14.0` tag still cannot
complete the path: it rejects the operator-owned gateway directory during
elevation and reports `ProcessStateUnknown` for a record with no sidecar. Main
`4201e0b3` (tasks 113 and 112, PR228/PR229) corrected the directory check and
added `LegacyIdentityRequired`; this document's recovery protocol sits on that
base. Residual limits in [Residual limits](#residual-limits-and-material-rulings)
remain in force, including the disclosed last check-to-signal TOCTOU. See
[020](020-container-infrastructure-design.md#gateway-process-identity) and
[guide 083](../../guides/083-v0.14.0-consumer-migration.md).

Owner: gateway lifecycle maintainers
Architecture: [020](020-container-infrastructure-design.md#gateway-process-identity)
Guidance: [083](../../guides/083-v0.14.0-consumer-migration.md)
Related: task 112 (legacy diagnostics), task 113 (elevated owner trust),
[Q-001](../questions.md#q-001--gateway-pid-identity),
[Q-002](../questions.md#q-002--macos-gateway-cross-user-identity-access)
Authority: no new standing helper, install, workflow, release or live-operation
authority is granted here. Nothing below authorizes a blind numeric-only signal.

## Purpose and boundary

Upgrading an active pre-identity gateway must not strand the user. This document
defines confirmed-absence recover, the candidate-inspection and generation-bound
stop protocol that finishes the consumer path under the operator ruling, and its
exact authority boundary.

Hard boundaries:

- A numeric-only, malformed, unreadable or unknown record never authorizes a
  blind current-CLI signal, never fabricates a sidecar, and is preserved
  byte-for-byte on refusal.
- The current CLI does not delegate a numeric signal to a previous binary. A
  digest-verified previous binary proves provenance, not candidate-process
  ownership. See [Rejected: delegated previous-binary stop](#rejected-delegated-previous-binary-stop).
- A CLI flag cannot grant the authority the policy withholds. Consent is not
  ownership proof; it is permission to act on live candidate evidence the CLI
  has already proved.
- No new standing helper, install, privilege, host-run 010, workflow or release
  change. No silent config downgrade, no re-tag, no v0.14.0 asset mutation.

## Version and state scoping

Three different products are in scope. Do not mix them.

| State | Directory check on elevation | Numeric-only record | Recovery entrypoint |
| --- | --- | --- | --- |
| Published `v0.14.0` tag | Rejects the operator-owned directory (`gateway directory is unsafe`) | `ProcessStateUnknown { pid }` | None. Cannot complete a legacy upgrade |
| Main `4201e0b3` (tasks 113 + 112, PR228/PR229) | Accepts the authenticated operator-owned directory | `LegacyIdentityRequired { pid }` after a read-only probe; `ProcessStateUnknown` when the probe is unknown | None |
| Corrected main (this document) | Same as main | `LegacyIdentityRequired` plus a structured recovery payload | `effigy gateway recover` and the inspection/stop protocol |

The published `v0.14.0` binary is the one consumers have. Its start path stays
broken even after records clear, so no current CLI on that binary can finish the
recovery. A consumer completes the path only after installing a corrected
release that carries the directory correction and the new entrypoint.

## Current truth versus this implementation

| Concern | Published / pre-recovery main | This document (implemented) |
| --- | --- | --- |
| Legacy numeric-only record | Published `v0.14.0`: `ProcessStateUnknown`. Main `4201e0b3`: `LegacyIdentityRequired`; records preserved; no signal | Keep the fail-closed refusal; add a structured recovery payload |
| Stopping the old daemon | No CLI signal path; main `4201e0b3` guidance requires an operator-controlled stop with independently confirmed executable and owner | Confirmed-absence recover never signals. A live daemon uses bounded candidate inspection then a generation-bound stop after interactive digest adoption |
| Automated previous-binary stop | Not authorized by the 020 ruling; v0.13.1 `down` is not ownership-safe | Rejected; it cannot meet generation checks. See [Rejected](#rejected-delegated-previous-binary-stop) |
| New daemon start | Published `v0.14.0`: rejected as unsafe. Main `4201e0b3`: works for the authenticated operator context | `recover` ends in a normal start after releasing the transition lock |
| Public channels | `latest` = v0.13.1; Homebrew formula restored to v0.13.1; `v0.14.0` body warns | Keep the pin; ship the fix-forward patch |

## Failure

Receipts in [Public channel assessment](#public-channel-assessment) and
[Private proofs](#private-proofs). Sequence for a gateway started by an Effigy
build that predates the identity sidecar:

1. The 0.13.x daemon wrote decimal `~/.effigy/gateway/gateway.pid` and
   `gateway.version` (for example `v0.13.1+local.677`). No `gateway.identity`
   exists. The daemon runs elevated and holds the gateway endpoints.
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
   cannot read an elevated daemon's identity (Q-002 `CHECK_SAME_USER`; the
   current `__gateway-identity` reader binds a sidecar digest and returns
   Unknown when no sidecar exists), so this is a real gap. Running an older CLI
   for ordinary work also fails on newer manifests: a v0.13.1 CLI run against
   this repo's linked Longhorn `proof:artifacts` manifest fails to parse
   (`effigy dev --plan` fails before execution). A global downgrade is not a
   consumer launch solution.
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
and it is not the consumer flow.

The gap the outcome required closing: an ordinary user with an overwritten
binary and an elevated legacy daemon had no supported way to confirm and stop
it. The protocol below is the implemented consumer path.

## Public channel assessment

Read-only verification 2026-10-06. Mitigation is already applied; no authority
to change channels is claimed here.

| Channel | State | Exposure |
| --- | --- | --- |
| GitHub Releases `latest` | `v0.13.1` (`releases/latest` returns v0.13.1) | Pinned to last good; new default downloads get 0.13.1 |
| GitHub Release `v0.14.0` | Published tag with a gateway warning in its body | Explicit downloads still possible; must not be deleted or re-tagged |
| Homebrew tap `Formula/effigy.rb` | Restored to the exact v0.13.1 formula at `c7fa8c55597947c079def106c99f8dc2cdcd8305` (parent `c2045dc59` = the 0.14.0 bump; original v0.13.1 formula `27a79c277`) | `brew install` gets 0.13.1 again; existing 0.14.0 kegs are not silently downgraded |
| `cargo install` from tag / direct binary download | `v0.14.0` tag and assets remain | Unprotected; only the release-body warning applies |
| Local `bootstrap:local` install | `build-local-bin.rhai` stages owner-only `effigy.previous` + `.version` before `mv` over `.local-install/bin/effigy` | Previous binary is evidence/rollback only; recover never executes it |
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
   diagnostic from main, plus the recovery entrypoint and installer
   preservation. After tagged-source and install proof, re-pin `latest` and the
   Homebrew formula in one formula commit.
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
  ownership proof. Consent only authorizes action on evidence the CLI has
  already proved.

## v0.13.1 previous-binary behaviour

Compatibility record. Source read at tag `v0.13.1` (`d186388fc`):

- `run_gateway_down` calls `server::get_status(&config).ok()`. `get_status`
  reads the pid, and when `process_is_running(pid)` is false it calls
  `remove_pid_file` (unlink `gateway.pid` + `gateway.version`) and returns
  `NotRunning`. `process_is_running` runs `ps -p <pid> -o pid=` and returns
  `status.success()`; a `ps` launch failure or a nonzero/malformed result is
  `false`. So a failed probe is collapsed to not-running **and the records are
  deleted**.
- `run_gateway_down` then calls `remove_pid_file` unconditionally after the
  optional stop.
- `uninstall_resolver_if_needed` removes `/etc/resolver` entries unless
  `EFFIGY_GATEWAY_KEEP_RESOLVER=1` is set.
- `stop_gateway_process` → `terminate_gateway_process` sends
  `kill(Pid::from_raw(pid as i32), SIGTERM)`, then SIGKILL, with no ownership,
  start-identity or executable check.
- It has no interface to accept or re-check a candidate generation and no
  recovery lock.

Consequences: a delegated v0.13.1 stop cannot preserve records on an unknown
probe, cannot serialize with recovery, cannot bind a generation,
and can remove resolver state.

## Recovery flow

### Confirmed-absence recover

`effigy gateway recover` performs the parts the CLI can prove and refuses the
rest. The confirmed-absence path never signals. A still-running daemon uses
the inspection and generation-bound stop below.

1. Capture the record (see [Legacy record capture](#legacy-record-capture)).
   Refuse on unknown/untrusted.
2. Require an interactive terminal (or `--yes` for the absent-only path) and
   print the evidence: PID, version, gateway directory, owner UID, digests, and
   the exact operator checks required to confirm the live process is the
   recorded gateway.
3. Acquire the transition lock (see [Locking](#locking)). Refuse if a concurrent
   `up`/`down`/`recover` holds it.
4. Re-probe the captured PID with the production tri-state probe.
   - `ConfirmedAbsent`: continue.
   - `Running` or `Unknown`: refuse, print the operator checks and stop
     instructions, preserve every remaining record, do not start.
5. On `ConfirmedAbsent`, remove any remaining legacy records with
   compare-and-remove under the record lock. Idempotent; never remove on
   Running/Unknown/refusal.
6. Release the transition lock, then start through the normal elevated start
   (see [Locking](#locking)).

The confirmed-absence path is the recover flow when the recorded PID is already
gone. A still-running daemon uses the inspection and generation-bound stop
below.

### Legacy record capture

`LegacyRecordCapture` is the only input to inspection and stop. Fields:

- canonical gateway PID path digest (`target_digest`), derived from the fixed
  `~/.effigy/gateway/gateway.pid` path;
- validated decimal PID and positive signed-PID domain (`> 1`);
- exact `gateway.pid` bytes and digest, exact `gateway.version` bytes and
  digest, combined `record_digest`;
- gateway-directory owner UID (validated by the existing trusted-directory
  rule);
- `directory_owner_uid` and `operator_uid` kept as separate fields.

Rules: a missing sidecar with a readable PID is legacy; a present but malformed
sidecar, an identity-only pair, unsafe mode, symlink or untrusted owner is
unknown and preserved. A PID alone grants no authority. If records are absent
when `recover` starts, including when the gateway directory itself is missing,
there is no captured PID and no inspection: only the absent-record start
path applies (start, or surface a bind failure). A
previously captured PID whose records vanish mid-operation is treated as
changed; the operation refuses and re-runs. Partial records (one of
pid/version) are unknown, not inferred.

### Bounded candidate inspection

Implemented.

**Reader surface.** A hidden, bounded, read-only `__gateway-legacy-candidate`
command invoked through the existing administrator elevation (`/usr/bin/sudo` or
`/usr/bin/osascript`), mirroring `__gateway-identity`. It accepts only the
canonical `target_digest`, the `record_digest`, the operator UID, and the
gateway-directory owner UID. It accepts no arbitrary PID, path or signal
request. It verifies the elevation marker and operator UID, rechecks the
canonical path and trusted record pair, reads only that recorded target, and
returns a bounded response. It never signals, changes records, starts a daemon
or writes. Interactive terminal required; 15-second cap; nonzero exit, timeout,
malformed output or digest mismatch is Unknown.

**`LegacyCandidate` response fields** (bounded, no free-form text):

| Field | Meaning |
| --- | --- |
| `pid` | Derived from the trusted record, echoed for binding |
| `candidate_uid` | Kernel UID of the live process |
| `boot_identity` | Linux boot id / macOS `kern.bootsessionuuid` |
| `start_identity` | Linux `/proc/<pid>/stat` field 22 ticks; macOS `pbi_start_tvsec` + `pbi_start_tvusec` |
| `executable_path` | Canonical live executable path (`readlink /proc/<pid>/exe`; `proc_pidpath`), shown to the operator |
| `executable_path_digest` | Digest of that canonical path |
| `role` | Listening sockets the process owns on the expected gateway endpoints |
| `role_digest` | Digest over the endpoint set |

**Role predicate.** The candidate must own a listening socket on each expected
gateway endpoint:

- DNS UDP on the DNS bind address (canonical default `127.0.0.1:15353`). The
  DNS server binds UDP only (`crates/effigy-gateway/src/dns.rs`
  `UdpSocket::bind`); DNS TCP is not a role signal;
- HTTP TCP on the proxy bind address (canonical default `127.0.0.1:80`);
- HTTPS TCP on the TLS bind address only when TLS is configured (canonical
  default `127.0.0.1:443`).

A foreign Effigy task or worker running the same installed executable does not
own those listening sockets and fails the role predicate. The predicate proves
the live process currently plays the gateway role; it does not prove who spawned
it and does not reconstruct historical ownership. Endpoints come from the
canonical defaults, never ambient `EFFIGY_GATEWAY_*` values the recovering CLI
could have substituted. If the effective endpoint or config evidence is
unavailable or ambiguous — custom endpoints, unreadable socket tables, or a
partial role set — the reader returns Unknown and `recover` refuses. A gateway
configured on custom endpoints cannot be recovered through this path; there is
no confirmation or adoption exception for non-canonical binds.

**Supported platform API proof.**

| Platform | Role read | Authority |
| --- | --- | --- |
| Linux | `/proc/<pid>/fd/*` → `socket:[inode]`; `/proc/<pid>/net/tcp`, `tcp6`, `udp`, `udp6` → local address, TCP `LISTEN`, inode | Documented procfs interfaces; no SPI. Uses the process's own netns (`/proc/<pid>/net`) |
| macOS | `proc_pidinfo(pid, PROC_PIDLISTFDS, ...)` → `proc_fdinfo[]`; `proc_pidfdinfo(pid, fd, PROC_PIDFDSOCKETINFO, ...)` → `socket_fdinfo` local port/kind | Public `libproc`/`sys/proc_info.h` APIs. Locked `libc` 0.2.189 exposes `proc_pidinfo`, `proc_pidfdinfo`, `proc_fdinfo`, `PROC_PIDLISTFDS`, `PROX_FDTYPE_SOCKET`; the socket-info structs and `PROC_PIDFDSOCKETINFO` are declared in-tree from the public header. Cross-UID needs `PRIV_GLOBAL_PROC_INFO`, which the root reader has behind existing elevation |

If the fd/socket read is unavailable, refused, or the struct layout cannot be
verified against the SDK header, the reader returns Unknown and `recover`
refuses. No `netstat`/`lsof` parsing, no `kern.proc` SPI, no `ps` guessing.

**Executable and provenance limits.** The returned `executable_path` is the
canonical live path, shown in consent. After a binary replacement the path may
resolve to the replacement image; the live process may be running the old inode.
`gateway.version` is on-disk metadata, not a live version API; there is no live
version check and the target is never executed to guess one. The reader reports
what it can prove (role, owner, boot, precise start, live path) and refuses the
rest. Overlong paths (over `PROC_PIDPATHINFO_MAXSIZE`, 4096 bytes) or responses
over 8 KiB are refused.

**Allowed daemon owner policy.** `candidate_uid` must be the authenticated
operator UID (from `EFFIGY_GATEWAY_OPERATOR_UID`) or root (0) reached through
that authenticated operator context. The directory owner and the candidate UID
are separate facts: an operator-owned directory (for example UID 501) with a
root daemon (UID 0) is valid and expected. Any other `candidate_uid` is refused.
The check binds the actual probed UID; it never asserts candidate UID equals
directory owner.

**What adoption adds and cannot prove.** Explicit operator adoption confirms the
displayed candidate is the intended gateway and authorizes the bound stop. It
cannot prove historical spawn ownership, that the process is not a same-path
impostor, or that the live image equals the recorded version. The role predicate
plus owner/boot/start is the live evidence; adoption is permission, not proof.

### Consent and generation binding

1. `recover` invokes the reader for the captured record. Refuse on
   declined/unavailable/timeout/malformed, UID outside the allowed policy,
   failed role predicate, unreadable boot/start, or any ambiguity. No signal.
2. Show the candidate: `pid`, `candidate_uid`, `executable_path` (the actual
   path, not only a digest), boot/start, the role endpoints, and the candidate
   digest. The adoption prompt renders that live path with control characters
   escaped so the operator sees one unambiguous line; the candidate digest still
   hashes the canonical path bytes, not the display form. Require explicit
   interactive consent to adopt this exact generation. `--yes` is not sufficient
   for a stop. Decline → refuse and preserve.
3. Bind `candidate_digest` = digest over `target_digest`, `record_digest`, `pid`,
   `candidate_uid`, `boot_identity`, `start_identity`, `executable_path_digest`
   and `role_digest`. Adoption targets exactly this digest.

### Generation-bound stop surface

Implemented. This is a separate signal capability, not part of the reader.

`__gateway-legacy-stop` (hidden), invoked through the same elevation, accepts
only these fixed arguments:

- `--target-digest <64hex>`
- `--record-digest <64hex>`
- `--owner-uid <u32>` (authenticated operator UID)
- `--candidate-digest <64hex>` (adopted generation)
- `--phase <term|kill>` (enumerated)

No arbitrary PID, path, program, signal number or extra flag. The root handler:

1. requires `EFFIGY_GATEWAY_ESCALATED=1`, root euid, and
   `EFFIGY_GATEWAY_OPERATOR_UID == --owner-uid`;
2. recomputes the canonical target digest and compares;
3. reads the trusted legacy record and compares `record_digest`;
4. derives the PID from the trusted record (never from an argument);
5. re-inspects the live candidate (role, owner, boot, precise start, live path)
   and compares the full generation digest to `--candidate-digest`;
6. refuses on any substitution, mismatch, unknown or foreign case;
7. performs only the enumerated `SIGTERM` or `SIGKILL` on the derived PID;
8. never downloads or runs an old binary and never fabricates a sidecar.

Response schema `effigy.gateway.legacy-stop.v1`:
`{schema, target_digest, record_digest, candidate_digest, phase, result}` with
`result` in `sent | refused | unknown | absent`. Bounded, no free-form output.

Parent control flow:

- The parent requires an interactive terminal and passes the adopted digest. A
  forged or direct handler invocation without the elevation marker, operator
  context or matching digests returns `refused`/`unknown` and dispatches
  nothing.
- TERM: invoke `--phase term`; on `sent`, wait in the existing bounded
  escalation loop (40 × 50 ms) re-probing absence.
- KILL: only if still running, invoke `--phase kill` with the same candidate
  digest; the handler re-validates before the signal.
- Any refusal, unknown, timeout or mismatch stops the sequence with no further
  signal. The final re-check and the signal syscall remain separate; the
  residual TOCTOU is disclosed.

Neither hidden command writes records, `/etc/resolver` files, TLS certificates
or `loopback-ips.json`. `recover`'s only writes are the conditional
compare-and-remove of the legacy `gateway.pid`/`gateway.version` pair after
confirmed selected-generation absence, followed by the normal start. Routes,
resolver entries, TLS certificates, loopback assignments, containers and
volumes are preserved.

### Rejected: delegated previous-binary stop

Delegating to an unmodified v0.13.1 binary cannot meet generation checks: it
exposes no interface to accept a generation, performs its own
`get_status().ok()` (which deletes records on a failed probe), uninstalls
resolver state unless `EFFIGY_GATEWAY_KEEP_RESOLVER=1`, signals
`Pid::from_raw(pid as i32)` with no ownership/start check, and does not
participate in the transition lock. It is rejected, not laundered.

### Installer preservation

The local installer stages the replaced binary as an owner-only
`effigy.previous` + `effigy.previous.version` before activation, as evidence and
rollback. It is not signal authority and recover never executes it.

## Locking

Current participants: `GatewayRecordLock` (`gateway.pid.with_extension("lock")`)
is held only briefly inside `publish_current_gateway`, `remove_if_unchanged`,
and `remove_legacy_pair_if_unchanged` (`identity.rs`). The last of those
compares both PID and version bytes under the lock before deleting a legacy
pair. `run_gateway_up` and `run_gateway_down` do not hold it for the whole
command, so it cannot serialize commands. The daemon's publication takes it
briefly.

Implemented locking:

- Add an owner-only **transition lock** (`gateway.transition.lock`), held for the
  whole command by `up`, `down` and `recover`. `status` and unlocked `up`
  preflight are classification-only: they do not stop a mismatched generation,
  delete records, or stage elevated state. Privileged vacant first-start
  creates operator-owned files under the parent transition lock, then drops it
  before elevation. Stop and compare-and-remove run after the lock is held (or
  in the elevated child that acquires it). A vanished captured PID/version pair
  is a changed record: `remove_legacy_pair_if_unchanged` refuses rather than
  treating absence as successful cleanup. Transition-lock acquisition, elevated
  state preparation, and daemon spawn validate `.effigy` ancestor trust before
  creating a missing gateway parent, so a symlink `.effigy` cannot receive a
  gateway child in an untrusted location.
- Keep the **record lock** as today: brief, only inside publication and
  compare-and-remove.
- Acquisition order: transition lock (outer) → record lock (inner), never the
  reverse.
- `recover` holds the transition lock for capture, inspection, stop and cleanup.
  It then **releases** the lock and starts through the normal elevated start,
  which acquires the lock itself. Holding the lock across the elevated start
  would deadlock: the elevated `up` re-execs `gateway up` (`run_gateway_elevated`
  → `build_gateway_elevated_command`), which acquires the same lock in a child
  process. An unauthenticated "lock already held" bypass token is rejected.
- The release-then-start window is benign and explicit: the start re-reads the
  records and re-checks state under its own lock acquisition, so a concurrent
  `up`/`recover` yields `AlreadyRunning` or an idempotent start. No
  cross-elevation serialization and no half-transaction are claimed.

External old daemons and old CLIs do not honor the new transition lock. Every
decision therefore re-reads records and re-probes the live candidate rather than
trusting lock ownership; the design never invents serialization against a
process that does not participate.

## Missing record and crash treatment

| Situation at `recover` start | Behavior |
| --- | --- |
| Record present | Capture; probe; absent → clean + start; running/unknown → `--adopt-candidate` inspects then generation-bound stop, or refuse without it |
| Record absent initially | No captured PID and no inspection, including a missing gateway directory. Start only; if an old daemon still runs, the start fails to bind and the failure is surfaced. Never signal |
| Record vanishes mid-operation | Treat as changed; refuse, no signal; re-run from capture |
| Partial records (pid or version only, including malformed or symlink `gateway.version`) | Unknown; preserve; no inspection. Ordinary `status`/`up`/`down`/managed and container auto-start share this classifier and refuse before overwrite |
| Crash before inspection | Transition lock released on exit; records intact; rerunnable |
| Crash after stop, before cleanup | Records intact; next `recover` probes absent → clean + start |
| Crash after cleanup, before start | Absent records; next `recover`/`up` starts |
| Crash after start | Normal authenticated lifecycle |

## Entrypoints and consent contract

| Surface | Behavior |
| --- | --- |
| `effigy gateway status [--json]` | Corrected main: `LegacyIdentityRequired` plus a structured recovery payload. Never signals. Main `4201e0b3` has the diagnostic without recover |
| `effigy gateway up`, `down` | Unchanged fail-closed refusal for legacy records; message points at `recover` |
| `effigy gateway recover [--json]` | Confirmed-absence: verify + clean + start. Live daemon: `--adopt-candidate` inspects, requires interactive digest consent, generation-bound stop, clean, start. `--yes` cannot adopt. No blind numeric signal path |
| `__gateway-legacy-candidate` (hidden) | Bounded read-only elevated reader. No signal, no write, no start |
| `__gateway-legacy-stop` (hidden) | Bounded generation-bound elevated signal handler. Only TERM/KILL of the adopted generation |
| `gateway_up_for_managed_task` and managed `dev` auto-start | Detect a legacy record, surface the `recover` pointer, refuse auto-start; no auto-recovery |
| Container activation auto-start | Same refusal; never a best-effort stop |

Consent shows the candidate (including the live executable path rendered as
lossless printable-ASCII with non-ASCII/control/bidi/zero-width scalars
escaped; the candidate digest still binds the canonical path bytes), the exact
adopted digest, and the stop it authorizes. `--json` emits
`effigy.gateway.recover.v1` with `result` (`recovered` | `refused` |
`already_stopped`), `pid`, `version`, `probe`, `candidate`, `adopted`,
`records_removed`, `started`, and `warnings`. Refusals are that same document
on stdout (`ok: false`, reason in `warnings`); they are not nested inside a
task-failed envelope. `status --json` emits `effigy.gateway.status.v1` the
same way.

## Privilege contract

- Classification, consent, lock, probe, cleanup and start decisioning run
  unelevated.
- Confirmed-absence recover never signals. An operator-managed stop remains
  available only as host process management when the daemon is already gone; it
  is not the live-daemon recovery path.
- The reader and stop handler run as root behind the existing elevation prompt.
  The reader is strictly read-only. The stop handler signals only the adopted
  generation after re-validation, and only the enumerated phase.
- The start keeps the existing elevation flow; the task-113 correction in main
  makes it accept the authenticated forwarded operator directory owner.
- No new standing helper, install, launchd/systemd unit, socket or standing
  privilege. The Q-002 reader is not widened; the two hidden commands are
  separate, narrower capabilities authorized only within the operator ruling.

## Error and edge matrix

| Case | Behavior |
| --- | --- |
| Record present, PID `ConfirmedAbsent` | Compare-and-remove records, then start |
| Candidate reader declined/unavailable/unknown | Refuse; preserve; no signal |
| `candidate_uid` outside operator/root policy | Refuse; preserve; no signal |
| Role predicate fails (no gateway listening sockets) | Refuse; preserve; no signal |
| Custom, partial, or ambiguous endpoints | Refuse; no confirmation or adoption path |
| Same-path foreign Effigy worker | Fails the role predicate; refuse; no signal |
| Executable path unreadable/overlong, boot/start unreadable, ambiguity | Refuse; preserve; no signal |
| Candidate generation changes between checks | Refuse; no signal; re-inspect |
| Consent declined | Refuse; preserve; no signal |
| Forged/direct stop invocation (bad marker, UID, digest, phase) | Handler refuses; dispatches nothing |
| Records already absent | Start only; bind failure if the old daemon still lives |
| Records removed while the daemon still runs | Out of scope; no candidate to inspect; start may fail to bind |
| Independent v0.13.1 `down` already ran | May have deleted records/resolver; if `recover` had not captured the PID, the absent-record row applies; a live daemon causes a bind failure, never a signal |
| Concurrent `up`/`down`/`recover` | Transition lock serializes the CLI's own paths; start re-validates |
| Previous exe already replaced | No effect; neither flow needs it |
| Already-upgraded `v0.14.0` user with legacy record | Install the corrected release, then `recover` |
| Network unavailable / offline | No effect; neither flow downloads anything |
| Shared routes (`routes.json` across checkouts) | Never cleared; the new daemon loads the existing table under contract 033 trust |
| Malformed/untrusted/symlink/identity-only record | Preserve; no `recover`; diagnostics only |
| Reused PID (recorded PID is a foreign process) | Role/owner/start predicate fails; refuse; no signal |

## Compatibility

- Version scoping is authoritative: published `v0.14.0` cannot complete the
  path; main `4201e0b3` can start but has no recover entrypoint; this document's
  protocol is implemented on corrected main. A patch that ships that main to
  consumers is a separate publication decision; GitHub `latest` and Homebrew
  remain pinned to v0.13.1 until that patch is authorized.
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
- Already-upgraded macOS `v0.14.0` sidecars stored the raw `kern.boottime`
  timeval, whose microsecond field drifts within one boot. Corrected main
  compares such a record through its `sec` component plus the still-mandatory
  exact process start identity, so the existing sidecar keeps matching the live
  daemon it published. The record is never rewritten to the new
  `kern.bootsessionuuid` value and is never reclassified as absent. New records
  store the session identity and match exactly; unknown current evidence fails
  closed. This is task 117 of the gateway recovery wave; ordinary `status`,
  `up`, `down`, and managed auto-start share the comparison.

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
- Stable macOS boot identity and legacy record compatibility (task 117):
  `crates/effigy-process/src/identity.rs::boot_session_uuid_parser_rejects_missing_failed_and_malformed`,
  `boot_session_identity_reader_is_stable_and_ignores_boot_time`,
  `legacy_boot_time_identity_matches_across_microsecond_drift`,
  `boot_identity_uncached_is_stable_across_separate_processes`;
  `crates/effigy-gateway/src/identity.rs::gateway_identity_legacy_boot_time_record_matches_across_microsecond_drift`;
  `crates/effigy-gateway/src/server/tests.rs::gateway_identity_legacy_boot_time_sidecar_stays_running_across_drift`.
- Managed auto-start terminal transport (task 117):
  `src/runner/gateway_command/tests.rs::gateway_up_for_managed_task_preserves_terminal_stdin_and_diagnostics`
  re-execs the test binary under a real private PTY and under null stdin;
  `gateway_up_for_managed_task_startup_notice_is_state_accurate`. Selectors
  `test:gateway:boot-identity`, `check:gateway:boot-identity`,
  `test:gateway:managed-tty`.

New private controls the implementation task must land (recording/fake fixtures,
fresh `mktemp -d`, real owned children, no live gateway, no real elevated read,
no install, no network). These prove the designed behavior; they are not a
no-defects claim:

1. **No-signal oracle**: record every signal dispatch behind the existing seam;
   assert zero dispatches for legacy classification, running/unknown probes,
   declined inspection, refused candidate and the absent-continue path.
2. **Reader binding**: a recording fake reader proves it accepts only
   target/record digests + operator/directory UIDs, rejects arbitrary PID/path,
   caps output, and returns Unknown on decline/timeout/malformed/mismatch.
3. **Role predicate**: a real owned child that binds the production endpoint set
   (UDP DNS on 15353, TCP proxy on 80, TCP TLS on 443 when configured) passes; a
   **real foreign same-executable Effigy worker** that does not own them fails;
   a TCP DNS socket is not a substitute for the UDP signal. A wrong-UID
   candidate fails. A valid root-daemon case with an operator-owned directory
   (UID 501 dir, UID 0 candidate) passes.
4. **Live path/role ambiguity**: unreadable or overlong executable path, and a
   candidate whose role set is partial or ambiguous, refuse.
5. **Substitution**: changed record bytes, changed candidate generation, or a
   substituted digest refuse without signalling.
6. **Stop surface**: `__gateway-legacy-stop` refuses a forged invocation (no
   elevation marker, wrong operator UID, mismatched target/record/candidate
   digest, bad phase) with no signal; a valid adopted generation sends exactly
   one TERM; a generation change before KILL dispatches nothing; an Unknown
   after TERM refuses success without KILL.
7. **Locking**: `recover` holding the transition lock releases before the start
   and the elevated start acquires it without deadlock; a concurrent
   `up`/`down` loses without side effects; no collateral writes.
8. **Missing/changing/crash boundaries**: initially absent, vanished mid-run,
   partial records, crash-after-stop-before-cleanup and
   crash-after-cleanup-before-start are each handled as specified.
9. **Overwritten-binary/local-build**: a local-build version records evidence
   and continues without resolving or running a previous binary.
10. **Installer preservation**: the staged previous binary is owner-only and is
    never executed by `recover`.
11. **v0.13.1 negative controls**: a recording fake v0.13.1 `down` models record
    deletion on a failed probe, resolver uninstall without
    `EFFIGY_GATEWAY_KEEP_RESOLVER=1`, and lock non-participation. The supported
    flows never invoke it.

## Implementation plan

Landed on corrected main under the operator ruling. Residual limits below remain.

| Area | Owner | Change |
| --- | --- | --- |
| Diagnostics | main `4201e0b3` (task 112) plus this implementation | `LegacyIdentityRequired`, preserved records, structured recovery payload |
| Elevated owner trust | main `4201e0b3` (task 113) | Prerequisite for the start |
| Transition lock | gateway runner | `gateway.transition.lock`; hold in `up`/`down`/`recover`; inner record lock unchanged |
| Recover entrypoint | gateway runner | `effigy gateway recover`: consent, transition lock, probe, confirmed-absence gate, compare-and-remove, release + start |
| Candidate reader | gateway runner | `__gateway-legacy-candidate`, bounded read-only elevated role/owner/boot/start/path reader |
| Generation-bound stop | gateway runner | `__gateway-legacy-stop`, fixed-arg generation-bound TERM/KILL handler |
| Installer | `scripts/build-local-bin.rhai` (`preserve_previous_local_install` / `activate_local_install`) | Stage `effigy.previous` + `.version` before activation, evidence/rollback only |
| Selectors | `config/tasks.toml` | `test:gateway:legacy-recovery`, `check:gateway:legacy-recovery`, `test:install:previous-binary-preservation`; `qa:docs:gateway-identity` |

Testing: private fixtures only, fresh `mktemp -d`, recording seams, real owned
children, no live daemon, no install, no network. Maintained selectors:

- `effigy test:gateway:legacy-recovery` and `check:gateway:legacy-recovery`.
- `effigy test:install:previous-binary-preservation`.
- `effigy qa:docs:gateway-identity` and `effigy fmt:check`.
- Existing `test:gateway:identity`, `test:gateway:probe-state`, `test:gateway:pid-domain` remain for the sidecar/refusal proofs.

Rollback: the transition lock, recovery entrypoint, structured payload,
reader/stop and installer backup are additive; reverting the implementation
commit restores prior fail-closed behavior. No data migration.

Patch runway: a PATCH release carrying the directory/diagnostic corrections
from main plus recover, the generation-bound stop, and installer preservation.
Publication requires a separate operator ruling. After tagged-source and
install proof, re-pin `latest` and the Homebrew formula in one formula commit.
Do not touch the
`v0.14.0` tag, assets or workflows.

## Operator ruling

On 2026-10-07, Tom answered **"Go for it"** to decision
`6af855c8-bbc4-490a-a371-f3cf63a964bb`, approving implementation and independent
review of both bounded capabilities below. This grants no publication, tag,
workflow, global installation, live gateway, VM, chown, purge or Queue-unpause
authority. The two-part approval does **not** authorize a signal without the specified live evidence:

> 1. Authorize a bounded, read-only elevated `__gateway-legacy-candidate` reader
>    for the recorded legacy PID. It accepts only the canonical target digest,
>    the validated record digest and the operator/directory UIDs, and returns
>    the live executable path, kernel UID, boot identity, precise start
>    identity and process-owned gateway listening sockets. It never signals or
>    writes.
> 2. Authorize a separate generation-bound stop exception: a bounded elevated
>    `__gateway-legacy-stop` that accepts only the target digest, unchanged
>    record digest, operator UID, adopted candidate-generation digest and an
>    enumerated TERM/KILL phase; derives the PID from the trusted record;
>    re-validates role, owner, boot, precise start and live path against the
>    adopted generation; refuses every substitution or unknown; and signals only
>    the adopted generation.

The ruling authorizes the specified reader and generation-bound stop, including
private proofs. It does not authorize a blind numeric signal, general
previous-binary delegation, fabricated sidecar, arbitrary-target helper or
standing privilege. Live role evidence does not prove historical spawn ownership
or eliminate a same-path impostor; the last identity-check-to-signal race remains
explicitly disclosed. Both capabilities are implemented as specified. Residual limits below still apply.

## Residual limits and material rulings

- Offline, overwritten, local-build records are reachable through recover;
  the CLI needs no network and no previous binary.
- A still-running elevated daemon is inspected and stopped only through the
  bounded reader and generation-bound stop. Unelevated host process management
  is no longer the required consumer path.
- The numeric-only record still confers no ownership proof. The implemented
  reader and generation-bound stop supply live role/owner/boot/start evidence;
  confirmed-absence recover never signals and does not replace that evidence.
- The role predicate proves the live process currently plays the gateway role.
  It does not prove historical spawn ownership, and it cannot auto-prove a
  custom-endpoint gateway. Custom, partial, or ambiguous endpoints refuse with
  no confirmation or adoption path. After a binary replacement the live path may
  be the replacement image; `gateway.version` is metadata, not a live version.
- The final identity-check-to-signal TOCTOU is disclosed for both the existing
  sidecar path and the generation-bound stop; neither claims atomic targeting.
- A delegated previous-binary stop is rejected; its v0.13.1 hazards are recorded
  in [v0.13.1 previous-binary behaviour](#v0131-previous-binary-behaviour).
- The published `v0.14.0` binary cannot complete the path; a corrected release
  is required.

## Requirements mapping

| Brief requirement | Section |
| --- | --- |
| v0.13 active → upgrade → start path | [Failure](#failure), [Recovery flow](#recovery-flow) |
| Already-upgraded recovery | [Version and state scoping](#version-and-state-scoping), [Error and edge matrix](#error-and-edge-matrix) |
| Legacy record capture | [Legacy record capture](#legacy-record-capture) |
| Concrete candidate-inspection route | [Bounded candidate inspection](#bounded-candidate-inspection) |
| Daemon role evidence | [Bounded candidate inspection](#bounded-candidate-inspection) (role predicate, platform API proof) |
| Owner policy | [Bounded candidate inspection](#bounded-candidate-inspection) (allowed daemon owner policy) |
| Stop surface | [Generation-bound stop surface](#generation-bound-stop-surface) |
| Exact entrypoints, privilege, consent | [Entrypoints](#entrypoints-and-consent-contract), [Privilege](#privilege-contract) |
| Locking / non-reentrant start | [Locking](#locking) |
| Missing record / crash treatment | [Missing record and crash treatment](#missing-record-and-crash-treatment) |
| Private proof plan | [Private proofs](#private-proofs) |
| Selectors + implementation/testing/rollback/patch | [Implementation plan](#implementation-plan) |
| Residual limits and material ruling | [Operator ruling](#operator-ruling), [Residual limits](#residual-limits-and-material-rulings) |
| Implementation plus architecture/guidance | This document; [020](020-container-infrastructure-design.md#gateway-process-identity); [083](../../guides/083-v0.14.0-consumer-migration.md) |
