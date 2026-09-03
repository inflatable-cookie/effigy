# Acowtancy Consumer Adoption Replay 1111 Closeout

Status: complete
Created: 2026-09-03
Roadmap: g09.003
Card: [`1111`](../../roadmaps/g09/batch-cards/1111-acowtancy-consumer-adoption-replay.md)
Spec: [`118`](../../specs/118-acowtancy-consumer-adoption-replay-strict-lane.md)
Decision: [`D-2026-05`](../../vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md)
Scorecard: [`2026-09-03-effigy-acowtancy-comparison-scorecard`](../../vision/governance/2026-09-03-effigy-acowtancy-comparison-scorecard.md)

## Summary

- Clean replay executed at the frozen consumer revision with a non-executing
  five-surface matrix; every surface exited 0 and the consumer tree stayed
  byte-for-byte unchanged.
- The stopped first run is preserved below as disclosed discovery evidence and
  is excluded from scorecard scoring. It proved full `effigy doctor` executes
  eligible repo-owned health tasks and is not read-only on a real consumer.
- Integrated health is marked **unavailable** under this replay's read-only
  boundary: not passing, not failing.
- The bounded guide `056` correction required by card `1111` is the only Effigy
  change. No starter surface changed, so no recurrence proof is required.

## Frozen Identities

- Effigy planning base: `adbad9924282a2b515bf34463559bc580e689e5f`
- Effigy `main` at replay: `e44da9fd59e4696d4c7868d6c7e528201eb41e24`
  (`HEAD == origin/main`; worker branch fast-forwarded to the same head)
- Binary identity: `effigy v0.12.1+local.e44da9f`,
  sha256 `5579d6848d21ae07aaff4633d024d8e51fa479439669801175327a87c901bbb5`.
  The release build is byte-identical to the previous `b909de4` build because
  `b909de4..e44da9f` touches docs only; the `+local.<sha>` suffix resolves git
  context at runtime.
- Acowtancy repository: `git@github.com:acowtancy/market.git`, primary checkout
  `/Users/tom/Dev/projects/acowtancy`
- Frozen consumer SHA: `91228893cbc2c6440b115b5aa1ee2fe34064f35b`

## Stopped Runs (disclosed discovery evidence, excluded from scorecard)

1. **Freeze repair.** The first dispatch stopped before consumer commands:
   Acowtancy had advanced from the discovery SHA `e42b64b1…`. The orchestrator
   re-froze clean pushed `main` at `6bcf6c70…` (commit `b909de4`).
2. **Matrix stop at `6bcf6c70…`.** The first matrix began clean, but Acowtancy
   planning writes (new cards `194`–`197`, spec/triage edits) overlapped the
   batch window (file mtimes inside and after the run; the consumer's own
   workers were dispatched by `91228893` itself). Per card stop conditions the
   lane stopped. None of that run's output is scorecard evidence.
3. **Full-doctor discovery at `6bcf6c70…`.** Exit 1, `ok:13 warn:4 err:4`.
   - `health.task.execute` (error): full doctor executed Acowtancy's `health`
     bundle; `farmyard/health` ran `cargo check --workspace --all-features` on
     the host (~7 minutes of compilation into gitignored `target/`) and failed
     with exit 1 in committed farmyard code (`crates/migration` compile issue).
     Ownership: **consumer-owned** — no Rust source was dirty during the run.
     The bundle aborts at first failure, so five later health tasks did not run.
   - Scan errors (`attention-markers` 5, `generated-in-src` 1, `god-files`
     209): consumer-owned code/threshold state; Effigy-native scan engines
     behaved as designed.
   - `container.workspace-ownership` warning and a running colima profile:
     environmental/consumer state; doctor observed, did not start it.
   - Doctor also installed a linux Effigy binary into the already-running
     workspace container (`[info] installing linux effigy into workspace
     container`) — a runtime-surface write outside the git tree.
   - Effigy-owned finding: guide `056` presented full doctor as a default
     health surface without stating that it executes repo-owned health tasks,
     so a read-only replay assumption failed on a real consumer. Repaired in
     guide `056` as required by card `1111`.
4. **Integrated health: unavailable** under the read-only pilot boundary. It is
   scored nowhere as passing or failing; the clean matrix substitutes doctor
   explain as the non-executing routing probe.

## Clean Command Matrix (frozen `91228893…`, binary `e44da9f…`)

| # | Command | Exit | Outcome | Ownership |
| --- | --- | --- | --- | --- |
| 1 | `effigy tasks --repo <acowtancy>` | 0 | 9 catalogs: root `acowtancy`, `docs` authority, children `cream`/`dairy`/`farmyard`/`cattle-grid`/`froyo`, sibling member mounts `poodle`/`underlay`; full selector inventory rendered | n/a (green) |
| 2 | `effigy doctor --repo <acowtancy> docs/qa:docs` | 0 | Explain trace: `resolved-root` = Acowtancy root, `selected-catalog: docs` via `explicit_prefix`, depth 1, `selection-status: ok`; non-executing | n/a (green) |
| 3 | `effigy test --plan --repo <acowtancy>` | 0 | Plan-only 7-member fanout (cream, dairy, farmyard, cattle-grid, froyo, poodle, underlay); farmyard `configured` suites (`managed`, `openapi-contract`), others auto-detected `vitest`/`cargo-nextest`; no execution | n/a (green) |
| 4 | `effigy docs/qa:docs --repo <acowtancy>` | 0 | 5/5 checks passed: links, index(vision), forbidden (`--repo .`) twice, next-action | n/a (green) |
| 5 | `effigy docs/qa:northstar --repo <acowtancy>` | 0 | 6/6 heading checks passed | n/a (green) |

Observed CLI behavior (recorded, not a contract defect): `effigy doctor
<task> --repo <path>` parses `--repo` as a task argument and fails root
resolution from outside the repo; placing `--repo` before the task selector is
the working form, and the error message names the fix.

## Pre/Post Consumer State

- Pre-matrix: HEAD `91228893cbc2c6440b115b5aa1ee2fe34064f35b`,
  `git status --porcelain` empty.
- Post-matrix: HEAD `91228893cbc2c6440b115b5aa1ee2fe34064f35b`,
  `git status --porcelain` empty.
- Writes attributable to replay commands: none. The five clean surfaces are
  non-executing (the batch completed in under three seconds with no
  compilation), and the post-state check confirms the tree unchanged.

## Selector and Root Ownership

Doctor explain proves ownership before attribution: `docs/qa:docs` resolves to
the `docs` child catalog (`docs/effigy.toml`, the documentation authority) via
explicit task prefix against the workspace root. Root-level invocation of
child-catalog refs matches Acowtancy's own AGENTS guidance, so no result in
this replay is attributed to the wrong catalog.

## Retained Child-Catalog/Container-Registry Workaround

- Location: Acowtancy `AGENTS.md` ("Effigy-First Execution" / workspace notes)
  teaches validating the docs spine through the workspace root with
  `docs/qa:docs` and `docs/qa:northstar`, mirrored by the root
  `qa`/`health`/`validate` bundles that invoke child-catalog refs from the
  root — the workspace-root re-entry pattern retained by Acowtancy card `162`
  pending revalidation of Effigy's child-catalog suite registry fix (log
  [`01-173500`](./01-173500-child-catalog-suite-registry-1100.md)).
- State: **retained**. No Acowtancy-owned downstream revalidation of that fix
  exists in the tree. This lane did not remove or edit it.

## Scorecard Rationale

The scorecard uses the `007` stage scale (0–4) per dimension for both
repositories in the same window. Acowtancy scores rest only on the clean
matrix; `RELEASE`, risk/exception counts, and movement are **unknown** for
Acowtancy because the replay did not exercise them. Integrated health is
unavailable, not a score. One pilot supports no universal claim; the
scorecard states this explicitly.

## Changed Effigy Surfaces

- `docs/guides/056-northstar-effigy-consumer-repo-contract.md` §2: full doctor
  is documented as executing eligible repo-owned health tasks and not
  guaranteed read-only; `effigy doctor <task>` (doctor explain) is documented
  as the non-executing routing probe. Job-based doctor guidance elsewhere in
  the guide is unchanged.
- No starter or machine-owned surface changed, so no focused recurrence proof
  is required by the card.

## Review Oracle Mapping

| Row | Counterexample | Proof |
| --- | --- | --- |
| 1 | Identity missing, mutable, or off the frozen boundary | Frozen Identities section; pre/post state both `91228893…` with empty porcelain; binary sha256 recorded |
| 2 | Acowtancy changed or runtime/stateful work started | Post-state empty porcelain; clean matrix is non-executing; full-doctor discovery (which did touch container/gitignored state) is disclosed and excluded from scoring |
| 3 | Result attributed without selector/root evidence | Doctor explain trace in the matrix table; selector/root ownership section |
| 4 | Failure without owner/next action, or mislabeled consumer policy | Stopped-runs section classifies every discovered failure; clean matrix has no failures; health unavailability is explicit |
| 5 | Score without evidence, unknown scored, universal claim | Scorecard links every score to this log or a named surface; Acowtancy `RELEASE`/risks/movement are unknown; non-universality stated |
| 6 | Effigy edit beyond the smallest generic repair | Only the guide `056` §2 doctor bullets changed; directly required by the proved full-doctor behavior |
| 7 | Retained workaround edited or pronounced obsolete | Retained-workaround section: unchanged, revalidation still owed by Acowtancy |
| 8 | Matrix omits a required surface or reruns full doctor | Matrix table contains tasks, doctor explain, test plan, docs QA, Northstar QA; full doctor was not rerun after the boundary was known |

## Validation Performed

- Clean consumer matrix (table above), run without full doctor, containers,
  secrets, installs, state mutation, or managed sessions.
- `effigy qa:docs` in Effigy: passed.
- `git diff --check` in Effigy: clean.
- `effigy doctor --json` in Effigy: exit 0, machine-readable report rendered
  (`ok:20 warn:1 err:0`; the warning is the pre-existing local `god-files`
  threshold finding, unrelated to this docs-only lane).
- Focused Effigy tests: not required — no machine-owned starter or product
  surface changed.

## Risks

- The consumer moved twice during this lane; a later re-freeze may be needed
  again if Acowtancy lands more work before review.
- The consumer-owned `farmyard/health` failure at `6bcf6c70…` is evidence for
  Acowtancy's owner, not this lane.
- One pilot: no portfolio or compatibility claim follows.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Movement: consumer contract unproven -> first populated cross-repo scorecard
  with a clean frozen replay and a bounded guide repair
- Remaining gap: cohort expansion decision, Acowtancy-owned health/workaround
  revalidation, and release-dimension evidence for consumers

## Next Task

Return the exact-head PR to the Effigy orchestrator; do not merge. After
review, decide cohort expansion versus a second bounded repair.
