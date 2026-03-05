# 001 Effigy Runner Blueprint v1

Status: Draft
Owner: Platform + Runtime
Purpose: define Effigy's architectural ideals, measurable target envelopes, and operating constraints for the next growth phase.

## 1. Product Ideals

1. Deterministic routing and execution.
- The same selector and context should resolve the same way every time.
- Ambiguity should fail loudly with actionable evidence.

2. Automation-grade contracts.
- `effigy --json` is the canonical machine interface.
- Envelope and payload schemas are versioned and validated continuously.

3. Operator-first ergonomics.
- Text, JSON, and TUI experiences should express the same decision model.
- Common workflows (`tasks`, `doctor`, `test`, `watch`) should be fast to inspect and safe to run.

4. Sustainable maintainability.
- Parsing, planning, execution, and rendering responsibilities stay modular.
- Refactors preserve command contracts while reducing coupling and branch complexity.

5. Evidence-driven delivery.
- Release gates and report artifacts prove behavior, not just implementation intent.

## 2. North-Star Targets (v1 Envelopes)

These are target envelopes, not guarantees. They should be tightened with measured baselines in CI and release reports.

### Resolution and Explainability Targets

| Area | Target |
| --- | --- |
| Selector determinism | Identical selector + cwd + repo override produces identical selection outcome and evidence |
| Ambiguity handling | Ambiguous unprefixed selectors fail with candidate set and remediation hint |
| Explain parity | `doctor` explain text and JSON carry equivalent reasoning fields for selection and deferral |
| Built-in precedence clarity | Built-in task resolution is explicit in output evidence and contract payloads |

### Operator Throughput Targets

| Area | Target |
| --- | --- |
| Discovery responsiveness | `effigy tasks` remains interactive for common monorepo scales (target: sub-second on typical dev machines) |
| Planning responsiveness | `effigy test --plan` returns predictable suite-selection output without executing suites |
| Failure diagnosability | Routing, lock, and health failures include direct next-action hints |
| Migration friction | `init` and `migrate` cover common onboarding paths without manual manifest bootstrapping |

### Contract and Release Reliability Targets

| Area | Target |
| --- | --- |
| JSON coverage | All supported command paths remain under `effigy.command.v1` envelope coverage |
| Schema governance | Contract index and examples stay synchronized with runtime output |
| Gate repeatability | QA/release gates pass consistently in local and CI contexts using documented commands |
| Report quality | Validation and release reports include reproducible command evidence and explicit outcomes |

### Maintainability Targets

| Area | Target |
| --- | --- |
| Module boundaries | Built-in command flows isolate parse/request logic from execution and rendering |
| Refactor safety | Behavior-preserving refactors are backed by contract and targeted behavioral tests |
| Docs drift control | Guides/roadmaps/reports retain stable terminology and command shapes |
| Complexity burn-down | New feature work avoids reintroducing oversized multi-responsibility modules |

## 3. Architecture Shape

## 3.1 Runtime Layers

1. CLI and command surface.
- Argument parsing, command dispatch, output-mode handling.

2. Resolution and catalog model.
- Root resolution, catalog discovery, selector precedence, task targeting.

3. Runner and execution orchestration.
- Built-in command dispatch, manifest task execution, deferral, lock policy, cache hooks.

4. Diagnostic and governance services.
- Doctor workflows, explain reasoning, contract shaping, release/report support.

5. Presentation surfaces.
- Text renderer, JSON envelope/payload emitters, multiprocess TUI runtime.

## 3.2 Boundary Rules

1. Parsing and request normalization should be separable from runtime execution.
2. Built-in command request contracts should stay explicit and test-covered.
3. JSON contract shaping should remain centralized and versioned.
4. Routing and deferral evidence should be generated from shared resolution facts, not duplicated branch text.
5. Docs examples should be treated as compatibility fixtures for operator expectations.

## 4. Quality Strategy

1. Keep contract tests and CLI envelope tests as non-optional gate layers.
2. Preserve deterministic failure messaging for routing and policy errors.
3. Continue modularization by extracting cohesive submodules from orchestration hotspots.
4. Treat docs QA scripts and report indexes as runtime-adjacent quality infrastructure.

## 5. Differentiators

1. One runner surface that spans catalog tasks and operational built-ins.
2. Deterministic routing with explicit evidence instead of hidden fallback behavior.
3. JSON-first automation compatibility without sacrificing human-readable workflows.
4. Integrated onboarding (`init`/`migrate`) and diagnostics (`doctor`/`explain`) in the same toolchain.

## 6. Initial Realignment Directions

1. Add vision tags and target-envelope sections to roadmap and high-traffic guides.
2. Standardize report families with an explicit "Vision Target Delta" section.
3. Link release and distribution checklists to measurable target movement.
4. Add an index-level policy for keeping contracts/examples/guides synchronized.

## 7. Exit Criteria for Blueprint v1 Acceptance

1. Vision tags and target envelopes are applied to core roadmap and guide artifacts.
2. Compliance evidence is produced with explicit pass/fail outcomes.
3. At least one release/readiness report includes vision-target deltas.
4. Exceptions to blueprint constraints are documented with rationale.

## Next Task

Define quantitative success metrics for each blueprint target envelope and record them in a dedicated vision metrics document.
