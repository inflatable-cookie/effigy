# Quality Targets

These are target envelopes for product decisions, not measured service-level
claims. A target becomes an SLO only after its source, baseline, owner, and
measurement window are defined. Report observed values with the measurement
method; do not turn an unmeasured target into a release claim.

| Area | Signal | Target envelope | Evidence source |
| --- | --- | --- | --- |
| Routing | same selector and context produce the same result and evidence | 99.9%+ consistency in controlled regression suites | routing fixtures and CLI parity tests |
| Routing | ambiguous selectors carry candidates and remediation | every ambiguity failure | routing/doctor error contract tests |
| Contracts | `--json` uses `effigy.command.v1` on supported paths | full contract-check conformance | JSON contract gate |
| Contracts | schema and docs drift | update in the same PR as a behavior change | contract index, examples, and CI |
| Operability | `tasks`, bounded `doctor`, and `test --plan` respond interactively | sub-second on a representative local baseline where applicable | timed baseline runs, with machine and repo recorded |
| Operability | failure output gives a usable action | at least 95% of sampled failures when a sampling method exists | sampled diagnostic review |
| Maintainability | oversized modules with mixed ownership | downward trend over comparable review windows | package map and focused module review |
| Refactor safety | public behavior after structural work | zero known contract regressions | targeted parity and contract tests |
| Release | gates run repeatably | at least 95% pass rate without bypass in comparable candidate windows | release gate evidence |
| Release | rollback can be executed | bounded window defined for the release channel and met in drills | release procedure and drill evidence |

The exact numerical targets came from the pre-cutover vision and have not all
been instrumented. Use them to choose proof and prioritize measurement, not to
assert current performance. Performance baselines need hardware, repository
size, cold/warm state, and command shape; otherwise comparisons mislead.

A reported metric should include name, observed value and window, target,
change since the previous comparable observation, source, and action if below
target. Avoid optimizing one signal by damaging another: faster routing that
loses evidence or fewer errors achieved by silent fallback is regression.
Change a threshold only with a reason and an account of the expected effect.
