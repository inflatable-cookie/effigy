# Private administrative recovery — unarmed

The default [preparation gate](recovery.py) still refuses. The separate
[administrative callable](administrative_recovery.py) can attempt exactly one
numeric-PID SIGINT only inside a later authenticated owner continuation with a
source/input-bound operator GO explicitly accepting the remaining PID-reuse risk.
**No current bound execution or usable native allowance exists.** The historical
private administrative allowance is exhausted; see the accounting below.
No native question or runtime rehearsal is part of source preparation.

## Governance and honest mechanism

Race-free signaling was a condition of the earlier proposal, not an Effigy
product rule for separate administrative recovery. A scoped operator acceptance
of ordinary numeric-PID signaling changes no product/apply/custody guarantee.
The route therefore always reports `race_free: false`, including on success.
A fresh identity check and a registered exit watch reduce uncertainty but cannot
eliminate the check-to-signal interval or establish VZ/disk closure.

Apple's [kill implementation](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_sig.c)
resolves the positive PID during the syscall. The [event interface](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/sys/event.h)
provides exit observation, not a signal identity argument. The packet requests
NOTE_EXIT, not verified exit status. This source trace does not claim all Darwin
APIs unsafe; it states the limits of this ordinary administrative route.

## Actual future caller and trust boundary

`administrative_sigint(inputs, inputs_hash, go_index, index_hash, deadline)` is a
fixed-purpose library call, with no installed CLI/config/environment toggle. The
hashes must come from the authenticated Queue owner dispatch, using the same
scratch-artifact/GO-index trust scheme as the full-session launcher. The function
does not cryptographically authenticate a self-authored GO or manufacture owner
provenance. Calling it with self-chosen matching files is not authorization.

Before constructing its native observer it checks immutable source, input and
program hashes, the complete GO-index and GO artifact hashes, operator Tom with
board authority, native kind, exact target/profile and action/count, expiry,
explicit residual-risk acceptance and a **new** bounded allowance. The separate
effect is `workspace-private-531-admin-sigint-r1`; a full-session GO cannot cover
it. It rechecks the authority before consuming the action and before the final
identity check. Absent, expired, non-operator, changed or unbound authority refuses
before any native target observation or effect. No GO file is armed by this code.

The future input must include the exact expected host-agent command rendering.
The old diagnosis recorded the executable and start, not complete argv; that
input remains missing, not a claimed current native fact. The native observer
uses bounded fixed `ps -ww` fields under UTC/C locale, verifies PID/start/euid,
parent, canonical executable, exact command rendering and pinned private root,
owner receipt, executable and disk identities. ps rendering is not represented
as byte-delimited kernel argv. Unknown or drift refuses; no alternate PID is used.
These read-only target observations themselves require the future GO. No such
observer is invoked during this source turn.

After first admission it durably consumes a decision-specific O_EXCL/O_NOFOLLOW
one-shot claim under the verified 0700 control root, then registers the process
exit watch. Registration error consumes the attempt but sends no signal. It
revalidates bound authority and performs the final full identity/root recheck
immediately before one `os.kill(94093, SIGINT)`. A syscall error, timeout or
uncertainty never retries the action. There is no TERM/KILL/force-stop, alternate
PID, unlock/unlink/delete, state patch or fallback. Read-only observation clients
use the existing bounded packet command wrapper; this is not backend escalation.

The callable returns the two identity observations, decision/input binding,
attempt and syscall-return facts, watch result, UTC/monotonic timestamps and the
residual-risk label. Syscall return does not prove handler delivery; NOTE_EXIT
does not prove clean shutdown, VZ closure or healthy disks. All failures retain
resources and withhold resume. Future caller supervision must persist the original
receipt and honor the deadline including reporting; it must not relabel Unknown.

`resume_intent` accepts only a separately Queue-pinned independent closure
artifact bound to this source/input/decision, not a caller exit dictionary. It
requires independently verified agent/driver termination, handler settlement,
stopped profile, settled holders, unchanged disks and fresh recorded resource,
store/no-format/no-resize admission with agent-alive false. Its durable ledger
allows one intent. The returned fixed argv has **execute false**. This signal GO
is not a restart GO; any eventual normal resume actuator needs its own exact
operator scope. Normal Colima stop can no-op on Broken and Lima graceful stop
requires Running; force stop can kill/unlock/remove temporary files. No blind
stop/start protocol is hidden here.

## Local proofs and limits

The production transport is fixed; injectable transport is a source-test seam,
not an input/env switch. Tests use only newly owned harmless direct children,
recorded backend metadata and synthetic GO artifacts under fresh run scratch.
A real registered watch precedes exactly one SIGINT to an unreaped owned child.
Strong parent/sole-wait ownership prevents PID reuse for that child only; this
argument does not apply to retained non-child PID94093. The actual child PID and
fixture-only label remain in the receipt. All children are waited and owned
payloads removed. No real profile/process/disk/capacity inspection occurs.

`effigy test:qualification:workspace-private` covers authority refusal before
observation, late binding revocation, changed final identity, watcher failure,
one-shot consumption even on uncertain delivery, actual child watch/signal,
truthful risk/exit labels, independent closure and exact resource/store guards,
and one non-executing resume intent. Earlier recorded admission and watcher
boundaries remain. These are not backend/full-session or consumer receipts.

## Budget and holds

Conservative full Queue accounting leaves at most22.743 seconds of the original
1800-second preparation allowance. Historical726 seconds, unused400-cap time and
expired deadlines are not reusable. The historical a16a8464 v2 private-stop approval accepted the administrative
residual risk and granted one new600-second total. Its two runs charged
374.174 + 302.259 = 676.433 seconds through actual Queue cleanup. That allowance
is exhausted, including the unapproved automatic follow-up. It cannot fund a retry.
No signals, watches or claims occurred. The automatic follow-up made four metadata
observations and one waited ps read, then lost raw output after an assertion; no
admitted complete identity/rendering exists. Originals remain immutable. Owner
accounting is `531-native-accounting.lxn89f9l/accounting.json`, SHA256
`a2a02e2082a6e659e77462ff40d2a1337ffed8a0ab52796116dbbbea8b028f92`.
Any future observation requires a fresh owner-bound allowance; this source work
grants none.
Source work stays within the global1920-minute/50M single-extension ceiling.
Production Discard remains refused, aggregate UNARMED and merge held. Product
custody, native/full-session qualification, consumer and final-enable gates stay.


## Ready read-only preflight protocol — UNARMED

The fixed [preflight entry](preflight.py) is a separate library callable:
`preflight(inputs, inputs_hash, go_index, index_hash, deadline, evidence)`.
There is no installed CLI, environment toggle or command argument forwarding.
The four file/hash arguments come from the authenticated Queue owner dispatch,
using the packet's existing trust scheme. Self-authored matching GO files are
not authority. The [proposed request](preflight-request.example.json) is explicitly
unarmed and lists the missing bindings; it cannot be passed as an armed input.
Preparation and review finish before any future native window starts.

Inputs use schema `effigy.workspace-private-preflight.v1`: the fixed recovery
plan's `source_head`, `profile`, `budget_seconds`, `control`, `shared`, `root_disk`,
`data_disk`, `agent`, `executables`, plus `recorded` admission and `programs` hashes.
No additional keys or argv/environment overrides are accepted. Agent fields are
exactly PID, parent, euid (`uid`), UTC start, canonical executable and profile;
complete command rendering is the missing **output**, never guessed input.
Program hashes cover common.py, recovery.py, administrative_recovery.py and
preflight.py. The GO-index retains the existing `colima: {path, sha256}` format.
The GO must bind Tom/operator-board/native authority, exact source/input/target,
`workspace-private-531-preflight-r1`, `one-read-only-preflight`, count1,
`new-owner-bound-preflight`, and fresh start/expiry within the new allowance.
The old signal or full-session GO cannot cover this effect. Source/head/index
and every authority/deadline gate run before constructing a native observer.

The observable single-call budget is at most10 seconds in **both** UTC and
command-local monotonic time, with a ps client cap of2 seconds and16KiB per
stream. The owner must reserve reporting/cleanup time separately inside the
actual new approved total; a smaller call deadline is not a refund or extension.
The future caller supplies `recovery.Deadline(utc_expiry, monotonic_expiry)` and a
fresh owner-only0700 canonical scratch evidence directory outside Queue cleanup.
An existing/nonempty directory refuses; there is no observation retry.

After authentication the observer checks only the fixed retained roots, owner
receipt, executable pins/alias and disk **metadata**, retaining original metadata
before subsequent comparison. It runs one fixed `/bin/ps -ww -p 94093 -o
pid=,ppid=,uid=,lstart=,comm=,args=` under C/UTC. No process discovery, alternate
PID, Colima command, disk content or environment dump is available. Exact client
argv/PID and UTC/monotonic start/end/exit are retained; every child is waited.
If a client times out or exceeds the cap, only that recorded observation child
may be killed and waited; this settlement is explicitly recorded as
`client_cleanup_signal`, separate from `target_signals: 0`. There is no target
signal, exit-watch registration, SIGINT claim or administrative callable.

Raw bounded stdout/stderr bytes are fsynced to private0600 quarantine files and
hashed **before** decoding, parsing or identity assertions. Overflow retains only
the bounded prefix and marks truncation/failure, never a complete rendering.
Foreign or potentially credential-bearing bytes remain private; public receipt
fields contain no raw bytes, exception messages or command rendering. Only a
strict benign grammar admits a rendering: limactl/hostagent/fixed profile,
optional `--debug` and pidfile/socket/logfile flags beneath the exact control
root. Unknown flags or free arguments refuse; the grammar claims nothing about
what the retained process currently renders. Admitted rendering is also private.
It is **ps command rendering, not byte-delimited kernel argv**. Missing/failed
identity remains refused/unknown, never process absence, disk health or closure.
Explicit failure receipts survive comparison failures; no reconstruction occurs.

The packet selector includes synthetic authority/metadata controls and newly
owned harmless fixture executables for capture/parse/identity/overflow/exit/error
and deadline behavior. They never observe PID94093 or real backend resources.
The existing reviewed signal route is unchanged. A successful future preflight
would still require owner verification, actual cleanup accounting and a separate
fresh signal binding before any action. Production/full-session/consumer/final
release and merge holds remain unchanged.
