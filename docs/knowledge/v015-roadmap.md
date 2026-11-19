# Effigy v0.15 roadmap

Current direction: Q-011–Q-017 in [questions.md](questions.md), the runtime
contracts, and the dated release direction in
[release.md](contracts/release.md#release-proof-obligations).
Queue owns live task status, lane order and proof receipts. This page states
durable release gates; it does not qualify a source head or authorize native
windows, installation, production enablement or publication.

## Qualification scope

Host actions run in the host CLI (Tom 2026-11-19, D1). A container-mode guest
runs its commands locally, on the container's own checkout and state, and holds
no host authority: there is no guest-to-host channel, grant, session, attach,
reconnect or guest PTY. The single refusal is a task the checkout binds to the
host, `run_in = "host"`, which names the host checkout. These retirements do not
waive guest security, native or consumer gates.

Qualification covers every bundle cohort on the exact frozen inputs: DecodeLabs
sites (including CBS critical), DecodeLabs libraries, Underlay (Acowtancy and
siblings), and other roots. It includes a fresh HOME and Tom's
machine. Owner-approved proofs must establish real outcomes and streaming,
positive publication of the guest binary and context, exact owner and custody,
pinned binary identity and credential secrecy, and the guest behavior on
Colima, Docker Desktop and Linux Docker Engine. Hybrid mode provisions no
guest. One small proof per backend establishes these positive claims
(effigy#1056): it brings up a real workspace with ordinary Effigy commands,
runs one representative task, tears the stack down and checks the engine
inventory returned to its starting state. The retired packets are an index of
reviewed commits, not an executable gate.

Ordinary-Compose synthetic proof is useful fixture evidence, not proof from an
actual Acow consumer. Worktree health is not data continuity: preserve the
existing data and prove genuine volume/row continuity in the owner-approved
consumer journey. A landed dependency or packet-source PR is not qualification.

## Required order

The selected order is recorded in Tom's 2026-11-02 v2 decision d01912ac
(relayed by Queue planner d070792d):

1. **Reviewed candidate.** Keep production refusal intact. Review the candidate
   and its evidence routes. This is not a production switch, native window,
   installation or consumer activation.
2. **Native per-backend qualification.** Prove the actual positive flows on
   Colima, Docker Desktop and Linux with separate owner-approved GOs for the
   required native and consumer windows. Each backend keeps one verdict file
   under the durable evidence root. Any candidate-changing fix resets affected
   evidence.
3. **Production enablement decision.** Only after proofs pass and review is
   complete may Tom give a distinct release-kind GO for production enablement.
   That GO is separate from the qualification GOs and grants no publication
   authority.

Tom's 2026-11-03 [source-split ruling](contracts/release.md#release-proof-obligations)
allows the reviewed implementation to merge after current-main integration
review with ordinary production Discard still refused. Separate owned tasks
carry the per-backend proofs, the Acow/CBS consumer continuity check and final
production enablement. All backend verdicts must name the same final candidate
inputs; every required backend and consumer gate must pass before Tom's
separate production-enable GO.

The per-backend routes and evidence rule are in
[`docs/qualification/README.md`](../qualification/README.md). Consumer
continuity is the owner-run
[checklist](../qualification/consumer-continuity-checklist.md): Tom checks
consumers by real use (2026-11-19, ruling 45). Neither route grants credit
until its own owner-approved execution records the result.

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
