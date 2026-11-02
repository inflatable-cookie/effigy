# Proposed interpolation custody and recovery design

This is a design for independent security review, not a description of shipped
behavior or authority to enable production Discard. It extends the current
no-clobber removal in [contract 006](../contracts/006-compose-backend-compatibility.md)
and must fit the kernel's custody and recovery rules in
[contract 052](../contracts/052-runtime-kernel-contract.md). The qualified
workspace apply remains refused as described in
[contract 053](../contracts/053-environment-reconciler-contract.md).

## Authority and identity

The kernel, not a Compose caller or a sweep, owns the transition from a
particular attempt and its group to verified closure. A non-cloneable custody
object contains the actual interpolation file descriptor and lock, pinned
parent directory descriptor and identity, file identity, original name,
quarantine name when allocated, and cleanup phase. It moves into the same
owner that controls the child/group before the spawn barrier opens. An Unknown
result or partial cleanup returns that same object to retained custody; caller
drop must not run the current automatic unlink. A dropped caller hands its
owner to a kernel-controlled custodian, not a detached thread whose lifetime is
inferred from the caller. If that handoff cannot be guaranteed, spawning must
refuse before the child can access the file.

A closure result must be bound to the exact recorded attempt and group, and be
issued by the kernel's verified settlement path. Leader exit, a free file lock,
EOF alone, a timeout, missing records, and an unverified group observation do
not authorize cleanup. A process or group identifier without generation and
custody evidence is not a closure proof. Closure and cleanup are separate:
cleanup failure retains the exact owner and its phase for retry.

## Durable binding and ordering

Before a child can access a path, create the file under the verified private
parent and hold its file lock. Under a cooperative directory-wide lock, publish
and sync a versioned binding in the kernel-owned durable attempt journal. Bind
an unpredictable invocation identifier, its one-to-one attempt identifier,
and the kernel's generation-qualified process/group custody record to the parent identity, original component, file
identity, and initial phase. The record contains no secret contents. Sync the
file and relevant directory entries, and persist the binding before releasing
the spawn barrier. A failed or uncertain publication refuses spawn and retains
the file; response loss is reconciled against the same attempt, never by
creating a second owner. The kernel's spawn receipt and binding must have an
ordered recovery protocol that covers a crash between each durable write: an
incomplete or ambiguous pair preserves residue and cannot establish closure.

Stat identity (device, inode and birthtime where available) is necessary but
never sufficient for cleanup; a generation-qualified group-closure receipt is
an independent authorizing gate. Device and inode numbers can be reused after
crash. Recovery may unlink
only if the kernel can reacquire a reviewed, generation-qualified identity
that proves the recorded object is still the same object, and can prove closure
of the bound group. A path, timestamp, file lock, or journal assertion alone is
not that proof. If a supported platform cannot provide durable object identity
or a reliable closure receipt after owner loss, recovery preserves the file and
reports Unknown. This is a deliberate fail-closed limit, not a claim that every
crash residue can be automatically reclaimed. Legacy and unbound residue keep
the existing notice and are never automatically removed.

All Effigy writers, cleanup attempts and sweeps acquire the same verified
directory lock before checking or changing entries or binding phases. Lock
ordering is directory lock, then binding/attempt lock; no path may take them in
reverse. A cooperating writer treats every occupied original or quarantine name
as taken. The lock protocol applies to creation, filling, rename, restoration,
unlink and sweep, including error paths. A noncooperating same-user process
that swaps an entry after a stat is outside the operator-approved threat scope;
detection of that race is not guaranteed.

## Cleanup state machine

Once the kernel verifies exact closure, persist cleanup intent with a chosen
quarantine name and expected identity before a no-replace rename. Sync the
journal and directory at each required durability boundary. Under the
directory lock, inspect the named entry without following links, rename without
replacement, inspect the moved object, then unlink only a positively matched
object. Persist and sync completion only after unlink and directory sync; a
lost response is reconciled from the durable phase and actual entries, never
reported as success merely because an entry is absent. An occupied quarantine
or original destination is preserved and reported Unknown; no random-name
overwrite fallback is permitted. If a foreign object moved, restore only with
no-replace semantics and retain both the object and phase on any uncertainty.
Never unlink a foreign object to complete a restore. A failed sync, missing
entry, conflicting binding, unsupported no-replace operation or uncertain
identity preserves evidence and produces Unknown.

A recovery sweep first obtains the directory lock, validates the trusted
journal and parent, then reconciles each recorded phase against the exact
attempt, group closure and object identity. It may retry only a transition
whose preconditions remain positively proven. It does not infer closure from
lock idleness or absence of a live owner. A crash between an operation and its
receipt is reconciled conservatively; where the durable evidence cannot
distinguish success from foreign removal or substitution, it remains Unknown.
The protocol must specify directory and journal sync semantics on each
supported platform before enabling cleanup there.

## Proposed kernel handoff and binding for review

The current checkout backup uses a pipe-only `MemoryGuard`; its Register frame
carries an effect receipt but cannot transfer an open file description. The
current `SpawnGate` sees a process receipt before `go`, and the process core is
installed before that callback. This live core installation is necessary for
retained ownership but does not transfer interpolation descriptors. Neither
interface by itself transfers the interpolation file descriptor, lock, parent
descriptor and cleanup phase into
custody. A new implementation must not interpret either acknowledgement as
proof of that handoff.

One candidate is a versioned Unix-domain control channel for the checkout
backup, established during verified helper startup, with `SCM_RIGHTS` transfer
of the exact locked file and verified directory descriptors. A registration
frame names the run ID, owner fence, process effect sequence, process receipt
(including nonce and generation-qualified leader and anchor), invocation ID,
file and directory identities, original name, and a unique request number.
The helper checks both descriptors, access modes, file type and ownership,
lock assumptions, identities, allowed single-component names and its existing
run/fence identity before acknowledging that exact request. Descriptor count,
truncation, extra ancillary messages, peer identity, malformed frames and old
helper versions all fail closed. No `go` may be sent unless the durable binding
and the exact helper's receipt are both verified, and the actual live owner has
moved into the corresponding process core. A failed or response-lost transfer
retains the same pending owner and descriptors; a retry reconciles the request
number and actual helper state, never sends a new file as a replacement. A
helper incapable of this protocol refuses before the child can execute.

The Queue planner's 2026-11-02 answer on Tom's behalf selects an extension of
the existing checkout backup for retry: it must retain the exact descriptors and gate across Unknown and
provide an authenticated retry channel. The current pipe-only backup cannot
do this: after its owner pipe closes it runs one settlement pass, then drops
its gates and exits even when that pass left residue. It now receives
the checkout lease and duplicates of the Env Run's own gate descriptions.
The descriptor handoff must retain both relevant gate custody and the exact file and directory descriptions
before acknowledging the handoff, and retain that same custody if the reply
is lost. An ordinary Env `finish` also discards the recoverable Run on error;
the retained path must keep or transfer that owner instead. This ruling does
not establish an authenticated retry protocol or enable cleanup.

The proposed versioned binding is a kernel-owned effect record, not a generic
adapter receipt: it associates one invocation ID and attempt ID with the Run
ID/fence and the exact process effect sequence, recorded group receipt and
anchor, the verified parent and file stat identities, original name, optional
quarantine name, and cleanup phase. File and directory stat matches are
necessary checks, not closure authority. The schema must reject unknown
versions, duplicate or conflicting bindings, missing identity fields, unsafe
components and any binding that does not refer to its own process effect. An
older helper must refuse the versioned record rather than ignore its binding.
No secret values, file bytes or file descriptors are serialized.

Persist and sync the binding and file before registering custody and opening
the spawn barrier. Cleanup starts only from the kernel's verified closure for
that exact process effect: acquire the directory lock, persist and sync intent
including the chosen quarantine name, and finish that transaction before
filesystem operations. Still holding the directory lock, perform no-replace
rename, check the moved inode and unlink only a match; sync the directory
before recording and syncing completion in a separate transaction. The Run and helper retain the exact live owner across an
Unknown or lost reply. After owner death the helper reconciles the durable
binding and actual entries under the same lock. If it cannot establish both
identity and closure, or if an unlink succeeded but its completion cannot be
proven, the result is Unknown and the evidence remains for later review. No
missing name, free file lock, absent process or reused PID is a completion
receipt. Unbound and legacy files remain notice-only.

This candidate requires independent review of descriptor-channel authentication,
helper-version compatibility, journal schema, sync ordering, owner-death and
lost-response transitions, and resource ownership before any production use.
It does not yet identify a macOS directory-entry durability guarantee or grant
permission to enable recovery there. The implementation must prove each state
transition and refuse any route where the exact owner cannot be transferred.

## Platform durability evidence still needed

Linux documents that syncing the file alone does not sync its directory entry:
[`fsync(2)`](https://man7.org/linux/man-pages/man2/fsync.2.html) requires an
explicit directory `fsync` for the entry. A Linux implementation must sync the
file and its containing directory in the binding and cleanup order above, and
must treat unsupported or failed sync as Unknown. It must also establish the
journal's durability ordering against those filesystem syncs.

On macOS, the [Apple `fsync(2)` manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html)
says ordinary `fsync` does not guarantee the write ordering needed across power
loss and identifies `F_FULLFSYNC` for stronger ordering. That statement alone
does not establish that a directory descriptor's sync makes an added, renamed
or removed entry durable. No such documented directory-entry guarantee has
been established for this design. Until that guarantee and its filesystem and
device limits are established, macOS crash recovery must preserve residue as
Unknown; a successful file sync or a process-crash test cannot supply the
missing power-loss guarantee. Tom ruled that macOS recovery stays fail-closed;
the lifecycle is to be implemented and proved where durability is established.
Neither platform is enabled by this proposal.

## Review and proof obligations

The independent security review must assess the kernel authority, durable
identity primitive, lock ordering, crash windows and every filesystem operation
before implementation changes custody or schema. In particular it must decide
whether the chosen platform identity mechanism survives inode reuse and whether
the journal and filesystem ordering can actually prove cleanup after a crash.
Implementation must retain fail-closed behavior for any unresolved case.

Acceptance requires a real caller-drop → file survives Unknown → verified
closure → cleanup test with the same owner retained on retry. Fault injection
must cover before and after rename, stat, unlink and restore, lost responses,
occupied destinations, competing cooperative writers, owner crash and sweep,
and exact baseline/no foreign deletion. Tests must cover both original and
quarantine phases, durable write and sync failures, stale/reused identities,
and missing or conflicting journal evidence. Native actions need their own GO;
production Discard remains refused until its final dependent task.
