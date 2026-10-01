# 033 - Trusted Host-Run Protocol Client

Contract: [Nucleus 010 Client protocol v1](../contracts/049-heavy-validation-admission-contract.md#scheduler-ownership-ruling)
Status: library primitives available; execution integration is deferred.

## Placement and boundary

`crates/effigy-host-run` is a leaf Rust client crate for the Queue/Nucleus
host-run socket. It has no scheduler, admission decisions, task selection,
process launching, or lease store. A following integration change may consume
its typed submit, attach, status, cancel and report APIs after the conforming
Queue server is qualified. The current Effigy admission and selector execution
paths do not call this crate.

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
container environments. It does not invoke a live endpoint during its tests,
change selector execution, switch admission, or remove the existing lease
mechanism.
