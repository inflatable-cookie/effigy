# Release Gates Satisfied by Hosted Exact-SHA Evidence

Status: open — Chatterbox recommendation, awaiting operator decision
Created: 2026-09-06
Owner: chatterbox
Source: Swallowtail Chatterbox request (2026-09-06); Swallowtail
`PAPERCUTS.md` "Effigy cannot skip release gates from hosted exact-SHA
evidence — 2026-09-06" and Swallowtail contract 036 "Hosted Gate Delegation"
Contracts: [`039`](../contracts/039-pre-release-ci-proof-contract.md)
Guide: [`051`](../guides/051-release-orchestration.md)
Related: [`20260905-092527`](./20260905-092527-release-gate-failure-diagnosability.md)
(keep-on-failure remainder)

## Issue

`[release.gates]` is one ordered table and every gate runs locally on every
gate-checked prepare. Swallowtail's heavy gates (`lint`, `lint:no-features`,
`test`, `floor`) already pass on a hosted exact-SHA run, so Swallowtail omits
them from the table and keeps them as a commented profile, losing the "one
complete table" property. They ask for a way to declare that named gates are
satisfied by hosted evidence, recorded in the prepare report.

## Known

- Effigy's own hosted proof is a configured shell gate (`ci` running
  `scripts/check-release-ci.sh`); the engine is provider-neutral and knows
  nothing about hosted runs. Any design must stay declarative and
  provider-neutral.
- Contract `039` draws a line the request must respect: hosted CI validates
  the **source commit**; local gates validate the **prepared mutations**
  (version, changelog, lockfile). A gate whose outcome can depend on those
  mutations (anything that reads `Cargo.lock` or the version) is not
  strictly proven by pre-mutation hosted evidence. Consumers must own that
  judgement per gate, and the report must say the gate was delegated, not
  passed.
- Card `1112` (merged) made gate outcomes and environment persistent under
  `.effigy/reports/release/gates/`; a delegated gate needs the same record.
- AGENTS rule: never bypass gates. A run-time `--skip-gate <name>` is an
  ad-hoc bypass and is the wrong shape; a committed declaration reviewed in
  the manifest is not.

## Chatterbox recommendation (not operator-confirmed)

Declarative, per gate, in the existing table:

```toml
[release.gates.ci]
command = "scripts/check-release-ci.sh"

[release.gates.test]
command = "cargo test"
satisfied-by = "ci"        # skip locally when gate `ci` passed in this run
```

- `satisfied-by` names an earlier gate in declaration order. When that gate
  passes, the dependent gate is recorded as `delegated` (not `pass`) with the
  satisfying gate's name and its captured stdout tail (which carries the
  hosted run id when the evidence script prints it). When the satisfying
  gate fails or is absent, the dependent gate runs locally as today.
- Report and JSON gain the `delegated` outcome and `satisfied_by` field;
  schema ids unchanged (additive). Text summary lists delegated gates
  explicitly so a release log never reads as if they ran.
- No new CLI flag. No `--hosted-run`; the evidence gate's own output is the
  record. No generic skip.
- `effigy release gates` (standalone) applies the same rule so local and
  prepare verdicts stay identical.

Effigy's own `config/release.toml` could adopt it for `test` under `ci`
once the contract `039` mutation-boundary caveat is documented in guide `051`.

## Unknown

- Whether the operator wants this now or after Swallowtail's next release
  shows the workaround cost; not urgent per the requester.
- Whether contract `039` should state that delegating a mutation-sensitive
  gate is a consumer decision, or forbid it for gates that read lockfiles.

## Next Task

Operator decides whether to queue a bounded `g09` lane (one card: grammar,
`delegated` outcome, report/JSON fields, guide `051` and contract `039`
wording, focused tests). On confirmation, Chatterbox promotes and tells the
Swallowtail Chatterbox the card id.
