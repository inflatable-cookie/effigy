# Effigy v0.15 roadmap

Current direction: Q-011–Q-017 in [questions.md](questions.md), the runtime
contracts, and the dated release direction in
[release.md](contracts/release.md#v015-level-1-qualification-order-2026-11-02).
Queue owns live task status, lane order and proof receipts. This page states
durable release gates; it does not qualify a source head or authorize native
windows, installation, production enablement or publication.

## Qualification scope

Tom's 2026-11-01 Q-017 ruling narrows v0.15 workspace-client coverage to
noninteractive Level 1. Level 2 features, including guest attach, detach,
reconnect, terminal input and resize, shared-lock tasks, pause and the rest of
the former full command matrix, follow after v0.15. Unsupported requests must
refuse by name and direct the user to the independently invoked host CLI.
These deferrals do not waive positive Level 1 behavior, security, native or
consumer gates.

Qualification covers every bundle cohort on the exact frozen inputs: DecodeLabs
sites (including CBS critical), DecodeLabs libraries, Underlay (Acowtancy and
siblings), and other roots. It includes a fresh HOME and Tom's
machine. Owner-approved proofs must establish real outcomes and streaming,
positive full-session publication, exact owner and custody, pinned TLS and
credential secrecy, and the full-session behavior on Colima, Docker Desktop and
Linux Docker Engine. Hybrid mode provisions no guest. The existing refusal,
identity and baseline packets cannot establish these positive claims; they
need a qualification-capable candidate and packet.

Ordinary-Compose synthetic proof is useful fixture evidence, not proof from an
actual Acow consumer. Worktree health is not data continuity: preserve the
existing data and prove genuine volume/row continuity in the owner-approved
consumer journey. A landed dependency or packet-source PR is not qualification.

## Required order

The selected order is recorded in Tom's 2026-11-02 v2 decision d01912ac
(relayed by Queue planner d070792d):

1. **Reviewed test-only candidate and packet.** Keep production refusal intact.
   Review the candidate, test-only path and evidence packet. This is not a
   production switch, native window, installation or consumer activation.
2. **Native full-session qualification.** Prove the actual positive flows on
   Colima, Docker Desktop and Linux, with separate owner-approved GOs for the
   required native and consumer windows. Keep the exact inputs and outcomes in
   the packet. Any candidate-changing fix resets affected evidence.
3. **Production enablement decision.** Only after proofs pass and review is
   complete may Tom give a distinct release-kind GO for production enablement.
   That GO is separate from the qualification GOs and grants no publication
   authority.

The former proof-before-enablement cycle is superseded. No stage implies the
next stage's authority or completion.

## Other release gates

Keep full main milestone QA and final exact-candidate release certification.
Consumer journeys cover live installed success and failure paths, including
gateway reboot, crash and soak, relevant upgrades, interrupted operations,
manifest changes and deleted worktrees. Preserve a security disposition,
exact-candidate artifact/evidence certification, migration notes, and Tom's
final operator publication decision. Passing these gates does not publish
automatically.

The one-way effects journal around Compose apply remains deferred only while
the exact candidate's interrupted-`up` recovery proof passes (Q-011); failure
reopens that mechanism. The parked gateway private-fixture lifecycle and
listener-handoff chain reopen only for a demonstrated failure (Q-012). Preserve
the accepted threat and durability limits in contract 052.

Tom retired Convergence from the v0.15 consumer cohort on 2026-10-28. Its
checkout is not a migration or qualification target; historical references in
contracts and release records remain historical. The shipped MinIO catalog is
retired in favor of Silo (effigy#424), separately from that consumer decision.
