# 033 - Trusted Host-Run Protocol Client

Contract: [Nucleus 010 Client protocol v1](../contracts/049-heavy-validation-admission-contract.md#scheduler-ownership-ruling)
Status: library primitives available; the root runner consumes them for opt-in
heavy routing only (see Integration below).

## Placement and boundary

`crates/effigy-host-run` is a leaf Rust client crate for the Queue/Nucleus
host-run socket. It has no scheduler, admission decisions, task selection,
process launching, or lease store. A following integration change may consume
its typed submit, attach, status, cancel and report APIs. The crate itself still
has no scheduling, admission decisions, task selection or process launching.

The client follows [contract 049](../contracts/049-heavy-validation-admission-contract.md)
and the signed [Nucleus 010 pin](https://github.com/inflatable-cookie/nucleus/blob/59cca903426635dd3581002a67058919e012eb45/docs/knowledge/contracts/010-host-run-scheduling.md#client-protocol-v1).
Contract [052](../contracts/052-owned-run-supervision-contract.md) remains
proposed and is not implemented here.

## Trust and discovery

Discovery opens the canonical `~/.local/state/host-run` root with
`O_DIRECTORY | O_NOFOLLOW`, verifies its owner and `0700` mode, then traverses
files and directories below it only with handle-relative `openat` and
`O_NOFOLLOW`. Authority and key files must be regular, owned by the current uid,
and `0600`; the socket must have the same owner and mode and be a socket.
Authority format, version, positive epoch, endpoint containment and socket
peer proof are checked before a request. Linux uses `SO_PEERCRED`; macOS uses
`LOCAL_PEERPID` plus `getpeereid`. Other platforms fail closed.

The protocol-specific peer identity is separate from the existing admission
record identity. Linux combines the current boot id with `/proc/<pid>/stat`
field 22, parsed after the last `)` in the command name. macOS reads the
kernel-reported process start seconds and emits UTC `YYYY-MM-DDTHH:MM:SSZ`.
Missing identities never match. No `ps` output or heuristic date conversion is
used.

## Client behavior

- Requests and responses use v1 newline-delimited JSON, with a 1 MiB frame
  ceiling. Attach chunks are bounded to 64 KiB.
- Submit carries its caller-generated `clientRequestId`. If its reply is
  ambiguous, the client queries status first and only resubmits that same ID
  after `unknown_run`. Conflicts stop without retry. A stale epoch causes
  authority rediscovery and status recovery.
- Attach tracks stdout and stderr byte offsets independently, trims duplicate
  retained prefixes, rejects gaps, reports `output_expired`, reconnects after
  interrupted streams, and validates the final settlement. Unlaunched
  cancellations and capacity timeouts have no result. A lost launched run
  remains result-unknown; the client never invents exit, signal or telemetry.
- Parent tokens use HMAC-SHA256 with current or immediately previous epoch
  keys, realpath root containment and expiry checks. Previous-epoch tokens also
  require status proof that the run is running at the current authority epoch.
  A present invalid token is a typed exit-77 error and cannot become an
  ordinary submit. An unreachable takeover lookup is exit 75.
- Report facts keep their original JSON bodies and client UUIDs in a private
  pending journal. A stable private lock plus fsync-and-rename updates serialize
  concurrent writers. Facts remain pending on transport failure, missing or
  conflicting stored-copy acknowledgements, and are replayed on the next client
  call until acknowledged. Container started/removed, nested and override
  facts are typed; none makes capacity decisions.

The client does not pass scheduler credentials or parent tokens in output or
container environments. It does not invoke a live endpoint during its tests or
remove the existing lease mechanism.

Interoperability with the reviewed Queue server (merge `7563a61`, PR186) fixed
two spellings the contract leaves open: `keyB64` is plain base64 (URL-safe and
unpadded spellings also decode, still exactly 32 bytes) and token `exp` is
integer milliseconds since the Unix epoch (RFC 3339 text also parses). The MAC
covers the payload either way. `HostRunRoot::open_journal` and
`journal_facts_offline` append facts to the pending journal without a reachable
scheduler, which is how an operator override is recorded durably.

## Integration

`src/runner/host_scheduler/` is the only consumer. It is inert unless
`EFFIGY_HOST_SCHEDULER=1` or a `HOST_RUN_TOKEN` is present; with neither, the
legacy lease path runs unchanged.

- `mod.rs` parses the opt-in and override, validates a present token, and picks
  one route per heavy invocation: legacy, nested (in place), override (recorded,
  direct) or submit. Refusals happen here, before any effect.
- `submit.rs` builds the request from the process's own argv, cwd and
  environment, streams attach output, turns the first interrupt into one cancel
  and maps the settlement to an exit status. A launched run exits with the
  child's real status through `RunnerError::HostRunSettled`; unlaunched
  outcomes are typed refusals.
- `facts.rs` reports `nested`, `override` and owned-container facts, journaling
  before any send.
- A nested or overridden run installs `OwnedChildrenScope`, a signal-forwarding
  scope with no lease and no admission-store access: it records only the process
  groups this process started and forwards termination to them.

Contract [049](../contracts/049-heavy-validation-admission-contract.md#opt-in-scheduler-backend)
owns the behavior. Run-scoped stop, logs and group `hard_timeout_ms` stay
unavailable; contract 052 is not implemented by this boundary.
