# Workspace-channel v1 fault and security proofs

Authority: [contract 052 §8.11](../../docs/knowledge/contracts/052-runtime-kernel-contract.md#811-workspace-client-interface-v1).

Run `effigy test:workspace-client:matrix` with installed Effigy. The narrower
`:protocol`, `:session` and `:provisioning` selectors isolate failure classes.
`:compile` checks the test targets; `:lint` checks the integration harness;
`:format -- --check` checks the changed Rust files without writing them.
Selectors use the repository nextest time and leak budgets, zero retries,
no-tests-as-failure and a 30-minute outer budget for Oxide admission.

| Failure class | Intended assertion |
| --- | --- |
| Missing/mismatched TLS pin | Refusal, zero bytes read, no host callback or reservation |
| Protocol, artifact or version mismatch | Stable refusal before host callbacks or reservation |
| Missing, invalid or revoked authentication | No resolution, admission, observation or secret echo |
| Context, environment, session, container or VM boot mismatch | Bound identity refusal before host effects |
| Malformed, duplicate-key, invalid UTF-8, oversized wire | Close/refuse before dispatch; oversized prefix reads only four bytes |
| Unknown method/field, traversal, argument/body limits | Invalid-request refusal before host resolution or reservation |
| Guest placement or out-of-grant target | Refusal, no guest fallback and no local admission |
| Observe retry | Two fresh observations, no run; host command JSON preserved |
| Same ID across reconnect or changed digest | One real admitted run, replayed identity or ID conflict |
| Lost owner or uncommitted reservation | Unknown, never readmission |
| Foreign stream/request, replayed or out-of-order terminal input | Invalid-request close/refusal before terminal dispatch |
| Terminal delivery and attachment loss | Input delivered once, latest resize retained, detach buffers nothing; released attachment cannot read or deliver |
| Output sequence and backpressure | Per-stream monotonic sequence; 16 frames / 512 KiB, source backpressure |
| Cancellation with no acknowledgements | Stop mailbox and response remain serviceable at the full data window |
| Reconnect cursor | Real retained logs resume each stream at its cursor; exact rotated gap precedes retained bytes; never silently restart output |
| EOF, acknowledged detach, secondary EOF | Owner stop only for EOF; detach and secondary loss leave owner running |
| Context identity locks and stale reads | Refuse before any guest or checkout read |
| Static guest reads | No state, vault, enrolment, capability or filesystem I/O |
| Hybrid Prepare | No guest operation or tree change |
| Container Prepare | Immutable binary/context, private enrolment, no secret in context JSON or effect trace |

The host fake scripts **every** resolver, entropy, admission and observation
call. An unexpected call fails immediately; unconsumed scripted calls fail
closeout. Admission uses `Run::begin_admitted`, not a second replay map. Sessions
use `SessionRef::admit`, and reconnect constructs a new dispatcher while keeping
the actual journal. The cleanup adapter rejects any unplanned effect. The terminal sink also scripts
every delivery and checks `Attachment::attached_observed` immediately before
recording it. `RetainedStreams` reads private captured-log segments through the
kernel reader; no fake replay store supplies its cursor or gap.

Every host fixture starts an unrelated process under a retained PID and keeps a
foreign-container marker. Assertions after owned-run settlement check both.
The harness stops and reaps only its own sentinel, recording its PID and exit
status. Guest Prepare separately preserves a foreign-container node. Fixtures
use fresh directories beneath Effigy's configured scratch; no real container,
engine, VM, install, gateway, vault or host configuration is accessed.

Context and provisioning cases are included by two existing in-crate test
modules. Their implementations remain private; no production visibility or
runtime entry point is added for the harness.

A reversible assertion probe must change a safety assertion, run its covering
selector to a nonzero result at that named assertion, then restore the exact
source bytes. A compile failure, empty test selection, unrelated failure, or
passing fake alone is not evidence of the probe or of source conformance.
Independent review must inspect the exact head and failure-to-assertion mapping.
These are source/fake-backend proofs; live transport/client integration and
backend qualification require their owning lane's proofs.
