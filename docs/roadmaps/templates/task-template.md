# <NNN> - <Task Title>

**Type: TEMPLATE** — Copy to `docs/roadmaps/gNN/NNN-<slug>.md` for each
executable Northstar task.

Status: draft
Owner: <owner>
Created: YYYY-MM-DD
Governing refs: <architecture files>, <contract files>
Depends on: <gNN.NNN or none>

## Outcome

State the exact bounded outcome for this task.

## Ready-State Rubric

- [ ] Objective is bounded enough to finish without fresh planning decisions.
- [ ] Governing refs point at current canonical surfaces.
- [ ] Scope, acceptance, validation, evidence, and stop conditions are explicit.
- [ ] The review oracle is present when acceptance is high-risk, universal,
  exact, or negative; otherwise it is explicitly not required.
- [ ] Continuation is explicit; the next task is ready if auto-start is enabled.
- [ ] No unresolved planning gap or operator intent checkpoint remains.

## Decisions

Record provisional design this task settles, or write `None`.

## Dispatch manifest

- **State:** <ready when the rubric holds; name parallel siblings or none>
- **Completion:** <observable done state plus required validation>
- **Owned mutable paths:** <exact paths this task may edit>
- **Reserved closeout surfaces:** <front doors or indexes owned elsewhere>
- **Worker:** <capability pool; frontier justification or none>
- **Excluded:** <explicit non-goals>
- **Escalation:** <owner of semantic or compatibility decisions>

## Work

1. <ordered step>
2. <ordered step>

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| <claim> | <smallest falsifying case> | <test/check/evidence> |

## Stop conditions

- Stop on planning gaps, contract contradictions, or failed evidence gates.
- Ask for operator intent if an unresolved planning branch or generation choice
  appears.

## Evidence

On completion, record outcome, validation run, PR link, reviewed exact head,
merge commit, and material limits or blockers.

## Next task

State the next ready task or promotion step unlocked by this task, or where
Chatterbox planning resumes.
