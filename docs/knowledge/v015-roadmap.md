# Effigy v0.15 roadmap

Current direction: Q-011–Q-017 in [questions.md](questions.md). Queue owns live
task status, lane order and proof receipts; this document states the remaining
release gates. It does not qualify a new source head or authorize an install,
runtime change or release. No interim v0.15 release is planned.

## Criteria reconciled on 2026-10-27

| Earlier criterion | Current criterion and authority |
| --- | --- |
| Supervisor: 108-cell managed strict-fake end-to-end matrix | Q-013 trims end-to-end managed tests to one cell per front end and stop kind; the full cross product remains in the kernel model test (contract 052 §12.3). Fault injection at journal boundaries and real consumer stop/settle proof still apply. |
| Task engine: resolver shadow diff against all 128 consumer roots | The legacy comparison layer was deleted by Audit2 L (#373/PR #519) under Q-012 §3. Prove the canonical resolver, `--plan`/`--resolve`, and affected consumer roots against current expectations. Keep the INT/TERM/HUP/KILL custody matrix. No shadow comparison to deleted behavior. |
| Migration: import locks, status, markers, leases, routes, loopback and ports | Q-011 removes duplicate authorities; Q-012 §3 deletes the v0.14.1 environment importer and PID-only refusal layer. Keep only migrations for surviving state and manifest/bundle shapes, including the live v0.14.1 gateway upgrade. The local MariaDB volume-and-row proof passed in #373; shared-service continuity remains a separate pre-release gate. |
| Workspace client and operator feedback listed as later | Q-017 says both are v0.15 release gates. Later is lane order, not a waiver. The full Acow/CBS container-mode matrix, three backend channels and every-command-plane feedback must pass on the candidate. |

The one-way effects journal around Compose apply remains deferred only while
the exact candidate's interrupted-`up` recovery proof passes (Q-011). A failure
reopens that mechanism. The parked gateway private-fixture lifecycle and listener
handoff chain reopen only for a demonstrated failure (Q-012).

Tom retired Convergence from the v0.15 consumer cohort on 2026-10-28. Its
checkout is not a migration or qualification target; historical references in
contracts and release records remain historical. The shipped MinIO catalog is
retired in favor of Silo (effigy#424), separately from that consumer decision.

## Sequence and gates

All source work uses approved Queue briefs, narrow selectors, independent review
and exact-head receipts. Each candidate-changing fix resets affected candidate
proofs. Acow coordination is required before its shared runtime, gateway,
Colima profile or install is touched. Tom's explicit release decision is last.

| Phase | Entry; owner | Exit and what it blocks | Parallel work / coordination |
| --- | --- | --- | --- |
| 0. Source, criteria and feedback | Q-011 active; Effigy Chatterbox and Queue task owners | Complete the REMOVE/SIMPLIFY audit and current tasks: #374 selector/QA proof, #394 fresh linked-worktree first-up scope fix, #392 shared-name golden vector, #375 docs truth, #388 anchor lifecycle, #389 local signing, #390 listener/restart fixes, #391 focused-test lock. #374's same-environment linked proof waits for #394; the separate-environment fixture is not credit for that consumer case. Keep Q-011's live consumer proof for each merge. #375 must make guide 085's v0.14.1 upgrade steps match #373's local data proof and disclose the old fresh-shared-MariaDB init defect. Resolve genuine residual supervisor, task-engine, environment, gateway, workspace-client and migration criteria above. Audit every command plane now using a main-built binary and private roots; brief, fix and review spinner/error gaps before candidate freeze. | Independent source areas may run in parallel; serialize runner/manifest and shared runtime areas. #374 is blocked on the first-up scope defect and its live proof needs a fresh exact-head window after #394. No whole suites in worker tasks. |
| 1. Main milestone QA | Phase 0 source changes reviewed; Effigy planner via Queue `project.qa.run` | Full main QA, including the Q-013 fault-injection group, passes. Failures become bounded tasks with targeted review. Re-run the affected QA slice and maintain a final exact-commit gate. This blocks candidate freeze. | QA is a separate owned run, not a worker whole suite; no conflicting runtime work. |
| 2. Private candidate dry run | Phase 0 and main QA green; Effigy planner with consumer owners | Freeze source and binary identity, then use scratch/private gateways and non-installed candidate proof on Acow hybrid and stack, re-up, interrupted `up`, manifest edit and deleted worktree. Flush source defects before final candidate qualification. A failure unfreezes the candidate, returns to Phase 0 and repeats affected QA/proof. An earlier exploratory install needs its own Acow scratch-compatibility proof and clearance, but does not count as this candidate dry run. | Requires Acow's proof window, but no install or machine gateway replacement. Can overlap independent proof-environment setup. |
| 3. Proof-environment preparation | Tom chose primary Linux gate; Effigy qualification owner with environment owners | Prepare Colima and Docker Desktop on this Mac, plus Tom's Parallels Ubuntu 26.04 arm64 VM (`Ubuntu 26.04 ARM64`, still being set up) as the primary Linux Docker Engine/systemd gate. Its package and intended `clean-base` snapshot live on Vault: check that drive is attached before each window, measure throughput before timing-sensitive work, shut the VM down before removing Vault, and do not run unattended without drive confirmation. Build the pinned candidate commit natively inside the VM with plain Cargo and record the aarch64 binary checksum before Linux proof; Oxide is not installed in that guest, and the release workflow is not a proof build route. Once Tom confirms Docker Engine, SSH, Rust and the `clean-base` snapshot, an exploratory current-main Linux pass may find adapter gaps; it is not candidate qualification. Bazzite SSH is optional native x86_64 second-distro evidence; Rosetta x86_64 in the arm64 guest counts only as a smoke test. Record backend, OS, snapshot and binary identities; do not reuse invalid existing Parallels VMs. | Preparation may run alongside Phases 0–2. Tom is setting up the VM; runtime/host changes stay coordinated. No consumer runtime or QA overlap. |
| 4. Coordinated local installs | Tom approved iterative exploratory installs on 2026-10-28 and superseded the earlier Acow timing/permission conditions. Each exact main SHA needs a pinned binary, private-root smoke, Acow scratch-checkout compatibility check, timed 593b201 rollback rehearsal, fresh QA fence/runtime read and exact protected-state inventory. Final candidate qualification still requires green private dry run and exact QA evidence; Effigy Chatterbox owns the install, with courtesy notice to Acow | Capture before/after identities on every iteration. Use `effigy install:local` with the stable signing identity, restart and verify the machine gateway, then run Acow smoke. On the first iteration, compare Acow's shared MariaDB container ID, render hash, named volume, and health before and after `container stack up --detach` using the #377 proof method; replace only if the render changed, and preserve it otherwise. Set `[scratch] root` and its one writable Colima mount only under the separately approved configuration step, with fail-loud missing-drive behavior and undo. Inventory stale Acow containers by exact ID and retire only those explicitly authorized. Intermediate installs are exploratory, not release qualification. | Do not park Acow-related Queue projects without Tom's separate ruling. Send Acow a courtesy notice; do not wait for permission or a fixed receipt interval. Reserve measured rollback time. No QA or Acow runtime overlap during each short install, and no release/tag/publish without Tom. |
| 5. Installed consumer and failure journeys | Environments ready, exact candidate installed; consumer owners with Effigy planner | Acow hybrid and stack, Inventors, CBS/DecodeLabs site and library, Underlay/reference and representative other roots pass. Include re-up, interrupted `up`, manifest edit, deleted worktree, crash/restart/reboot, and v0.14.1 upgrade. Full Acow/CBS container-mode workspace-client matrix and host channel on Colima, Docker Desktop and Linux; hybrid provisions no guest. Confirm full feedback on live slow/failing paths. Preserve shared data and prove genuine volume continuity. Failure gets a cause and bounded task, then affected proof replay. | Independent cohorts may overlap only where runtime resources do not. Colima and Docker Desktop proofs use separate windows with the other backend stopped. Acow/Underlay live proof needs their owners' window and Tom's direct go where already stipulated. |
| 6. Gateway workflow observation | Installed exact candidate with #382 attribution, after deliberate reboot/crash journeys; Effigy planner and Acow | Run a few hours of workflow checks across the three Underlay bundles and consumers on the installed candidate, with the machine gateway up and unrestarted. Record gateway identity and stop attribution throughout. An unexplained stop blocks release and gets a diagnosis task scoped to evidence. | May overlap Phase 5 journeys that do not intentionally restart the gateway; coordinate Acow ports and runtime. |
| 7. Final exact-commit gates and certified artifact | Phases 1–6 passed and source frozen; Effigy release owner | Recheck full QA/release gates on the certified commit after any candidate-changing fix; contract 054 gates certify the exact artifact and evidence. No prepare, tag or publish follows automatically. | Release CI/artifact work follows its explicit workflow and human authority. |
| 8. Release decision | Certified identity and migration/consumer notes ready; Tom | Tom decides whether and when to publish v0.15. | Underlay minimum version rises only after publication. |

Q-005 retires the installed catalog-pack runtime; Tom archived the pack
repository on 2026-10-28. Immutable pack 1.2.0
remains resolvable for older Effigy consumers. The MinIO cut landed only in
Effigy's baseline catalog (#424), with no new pack release in this set.

## Open papercut disposition

Queue papercuts are the live register. This mapping covers the 35 open Effigy
records on 2026-10-27; a phase assignment is not a claim of closure. Phase 0
items must be implemented or explicitly resolved before candidate freeze.

| Phase | Papercut IDs and disposition |
| --- | --- |
| 0 — #375 docs truth | `b73d8bd4`, `5c177f0c`, `a1c06784`, `0adc3137`, `29c4fa43`, `1c98cf00`, `ac831cc7`, `0c0c0fc1`, `18d418ed`, `5a25fe44`, `8b61acc0`, `2bd6f460`: reconcile text/selectors with main and Q-011–Q-013. |
| 0 — #388 custody | `001c1993`: anchors exit on TERM or settled run, including direct invocations; verify no run-owned survivor. |
| 0 — #390 runtime fixes | `c912d627`, `1c1d66b5`, `55474d1c`: retry failed route publication and bound restart rollback/readiness to its target. |
| 0 — #389 signing | `c5ed861f`: stable local-install signing identity. |
| 0 — #374 selector proof | `c37e7b6a`, `86c6461b`, `53825dfc`, `303d3843`, `27ab6e2f`: selector coverage, prune oracle, timeout and private fixture findings; route exact gaps into #374 or a reviewed follow-up. |
| 0 — residual source fixes | `45a0c1b8`, `01febc1e`, `b02c7882`, `0bb99e35`, `8eb10faa`, `ff56bf12`, `2ecdcaff`, `8064c5b9`: environment, source-pin and gateway reliability; assign bounded tasks by hot area before candidate freeze where reproducible. |
| 4 — install / Colima | `62e6b400`: one scratch mount in install window, then move fixtures off temporary `~/.cache` staging before closure. |
| 0 — feedback audit | `26b19047`: verify lint crate set, rare source-directory spinner and JSON `next_action` before freeze; confirm on the installed candidate. |
| 5 — consumer data | `b3189cc7`: qualify MariaDB exec-probe reaping and supported init behavior during live data journeys. |
| 1 and 7 — QA | `b997bada`, `46dc6003`: investigate docs admission/hosted helper EOF and artifact prerequisite in full QA context; fix or evidence a scoped disposition. |

None of the 35 is silently marked “after v0.15.” An evidenced, harmless
post-release deferral requires an explicit Queue disposition and must not waive
Q-011, Q-012, Q-017 or the consumer/release gates.
`8eb10faa` (name-store retry outside the streaming path) and `8064c5b9`
(detached exit 141 under host saturation) are candidates for an explicit
evidence-based deferral if qualification cannot reproduce them; until such a
ruling, they remain Phase 0 findings.
