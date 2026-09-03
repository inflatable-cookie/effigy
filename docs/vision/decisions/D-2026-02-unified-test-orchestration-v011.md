# Vision Decision Record — D-2026-02

Context
- Date: 2026-08-10
- Owner: Platform Lead
- Scope: Effigy test orchestration for v0.11
- Tags: CONTRACT, OPERATE

Decision
- Summary: `[test]` becomes the sole test authority; `tasks.test` is forbidden; `--plan` is a no-execution boundary.
- Principle(s): One canonical machine interface (`008`); operator inspectability before execution (`001`).
- Chosen Option: Unified built-in `effigy test` with manifest `[test]` suites and hardened plan mode.

Alternatives Considered
- Option A: Keep `tasks.test` as a repo alias — rejected because it competes with the built-in and blurs plan semantics.
- Option B: Defer polyglot suite declaration — rejected because implicit detection hides lifecycle boundaries.

Impact
- Positive: one test entrypoint; predictable `--plan`; JSON contract `038` owns behavior.
- Risk: repos with custom `tasks.test` need migration.
- Compatibility Effect: medium — manifest migration required for non-trivial repos.

Controls
- Mitigation: contract `038`; archived spec `102`; closeout log `11-144402`.
- Reversal Condition: polyglot repos require undeclared `tasks.test` or plan mode executes suites.
- Exit Plan: N/A — monitor through monthly governance reviews.

Traceability
- Related Exception: none
- Related Risk: VR-02
- Related Artifacts: [`g08.029`](../../roadmaps/g08/029-unified-test-orchestration-v011.md), [`11-144402-unified-test-orchestration-v011-closeout.md`](../../logs/2026-08/11-144402-unified-test-orchestration-v011-closeout.md)

Review checkpoint: completed 2026-09-03; decision remains Stabilized.
