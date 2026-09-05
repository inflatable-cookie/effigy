# Consumer Maturity Scoring Checkpoint

Status: deferred — operator direction 2026-09-05: not important right now;
`g09.004` release gate diagnosability runs first
Created: 2026-09-04
Owner: chatterbox
Decision: [`D-2026-05`](../vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md)
Artifacts: [`007`](../vision/007-vision-adoption-and-maturity-model-v1.md),
[`010`](../vision/010-vision-repository-maturity-scorecard-template-v1.md),
[`016`](../vision/016-cross-repo-rollout-comparison-scorecard-template-v1.md)
Evidence: [scorecard 2026-09-03](../vision/governance/2026-09-03-effigy-acowtancy-comparison-scorecard.md),
[log 03-014518](../logs/2026-09/03-014518-acowtancy-consumer-replay-1111.md)

## Issue

The first populated cross-repo scorecard could not assign Acowtancy any stage
(every cell `unknown`). Under artifact `007` rule 1 a stage claim needs every
prior-stage criterion, and Stage 1 requires full `doctor`, JSON-contract
coverage, and release-gate checks. A read-only consumer replay cannot supply
all three. The open checkpoint is whether to repair the model before adding
consumers, or expand the cohort now and accept more `unknown` rows.

## Known

- Full `effigy doctor` executes eligible repo-owned health tasks. There is no
  non-executing full-doctor flag (`doctor --help`, guide `056` §2). Only
  doctor explain is a routing probe. This gap is a **product or boundary**
  question, not a model wording question.
- JSON-contract evidence was **not collected**, not uncollectable:
  `--json` forms of `tasks`, doctor explain, and `test --plan` are read-only.
  Card `1111` simply scoped five text surfaces. A second window can close this
  gap with no model change.
- Acowtancy's orchestration root carries a workspace-container exception with
  no changelog/release expectation. `RELEASE` cannot be observed there by
  design. `007`/`010`/`016` have no "not applicable" disposition, so any
  non-releasing consumer is permanently unscoreable on aggregate.
- Theme 3's target envelope (backlog `g09-candidate-themes`) is contract parity
  and drift fed back to starters, not consumer stage scores. The scorecard is
  a governance by-product, not the theme's acceptance.
- Remaining cohort candidates named in `D-2026-05` Option C: Northstar and
  Bovine Accelerator. Neither has been frozen or inspected for release shape.

## Unknown

- Whether the operator wants consumers scored on the `007` scale at all, or
  only wants Effigy scored there with consumers tracked on a lighter adoption
  posture.
- Whether Northstar/Bovine Accelerator have release surfaces (would decide if
  the `RELEASE` N/A gap recurs on the next pilot).
- Whether a read-only doctor mode is worth a product lane, or whether "health
  unavailable under read-only boundary" is an acceptable permanent evidence
  class.

## Options (tentative, none operator-confirmed)

1. **Repair the model first (prior coordinator recommendation).** Amend
   `007`/`010`/`016` in a chatterbox docs promotion: allow per-dimension
   `n/a` with a stated reason, allow aggregate stage when all applicable
   dimensions meet criteria, and define read-only evidence equivalents
   (doctor explain + `--json` probes) for Stage 1. Then re-score Acowtancy
   from a second clean window that adds JSON evidence. No worker lane needed
   for the docs change; one bounded replay lane for the re-score.
2. **Expand the cohort now.** Freeze Northstar or Bovine Accelerator, run the
   same read-only matrix plus `--json` probes, feed drift to starters. Accept
   `unknown` stage rows until the model is repaired later. Serves Theme 3's
   actual envelope directly.
3. **Both, model repair first then one new pilot** with the repaired scoring
   and JSON collection in the same card. Longest single lane, but the second
   scorecard would be the first with two scored consumers.

Chatterbox lean: option 1 is cheap (vision docs only, promotable directly) and
unblocks scoring for every future pilot, so do it before the next replay. But
the decisive question is the first Unknown above.

## Next Task

Deferred. Reopen with the operator after `g09.004` ships. Operator then
decides which option (or reframes). On confirmation, chatterbox
promotes: vision artifact amendments and/or a new `g09.004` roadmap, strict
spec, ready card, and dispatch manifest; then prunes this note.
