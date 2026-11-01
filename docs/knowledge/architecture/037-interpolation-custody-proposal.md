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
an unpredictable attempt identifier and the kernel's generation-qualified
process/group custody record to the parent identity, original component, file
identity, and initial phase. The record contains no secret contents. Sync the
file and relevant directory entries, and persist the binding before releasing
the spawn barrier. A failed or uncertain publication refuses spawn and retains
the file; response loss is reconciled against the same attempt, never by
creating a second owner. The kernel's spawn receipt and binding must have an
ordered recovery protocol that covers a crash between each durable write: an
incomplete or ambiguous pair preserves residue and cannot establish closure.

Device and inode numbers alone can be reused after crash. Recovery may unlink
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
