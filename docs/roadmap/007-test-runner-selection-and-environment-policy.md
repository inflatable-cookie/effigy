# 007 - Test Runner Selection and Environment Policy

Status: Complete
Owner: Platform
Created: 2026-02-27
Depends on: 005

## 1) Problem

`effigy test` currently fans out to every detected suite. This is convenient for full runs, but ambiguous for named test invocations in mixed-suite repositories (for example Vitest + Rust). Effigy also needs a clearer strategy for environment flexibility without forcing heavy per-project configuration.

## 2) Goals

- [x] Make named test execution deterministic in mixed-suite repositories.
- [x] Support positional suite targeting with minimal flag usage.
- [x] Preserve zero-config auto-detection for common ecosystems.
- [x] Keep project-level configuration simple and optional.
- [x] Add package-manager awareness for JS/TS runner execution where needed.

## 3) Non-Goals

- [ ] No global/distributed runner pack system in phase 007.
- [ ] No requirement for users to configure detection in every project.
- [ ] No broad plugin framework for arbitrary external test adapters.

## 4) UX Contract

Primary command forms:
- `effigy test`
- `effigy test <suite> [runner args]`
- `effigy test <named-test-or-runner-args>`
- `effigy <catalog>/test ...`

Selection behavior:
- `effigy test` with no runner args may fan out to all detected suites.
- If runner args are provided and multiple suites are detected, Effigy errors unless suite is explicitly provided as the first positional argument.
- In single-suite contexts, `effigy test <named-test>` routes directly to the only detected suite.

Examples:
- `effigy test vitest user-service`
- `effigy test nextest user_service --nocapture`
- `effigy test user-service` (valid only when exactly one suite is runnable)

## 5) Execution Plan

### Phase 7.1 - Positional Suite Selection + Ambiguity Guard
- [x] Recognize positional suite tokens (`vitest`, `nextest`, `cargo-nextest`, `cargo-test`).
- [x] Strip suite token before passthrough argument forwarding.
- [x] Error on ambiguous named invocation when multiple suites are runnable and no suite token is provided.
- [x] Update help and examples to document positional suite targeting.
- [x] Add tests for ambiguous multi-suite errors and explicit suite selection.

### Phase 7.2 - Environment Awareness Baseline
- [x] Add lightweight package-manager awareness for JS/TS test invocation.
- [x] Ensure command composition remains deterministic and visible in `--plan`.
- [x] Add tests for package-manager specific invocation wiring.

### Phase 7.3 - Hardening + Adoption
- [x] Validate behavior in mixed repositories (Underlay/Acowtancy-style layouts).
- [x] Tighten error messaging for unavailable suites and remediation steps.
- [x] Publish migration notes for teams moving from implicit fanout to explicit suite targeting.

## 6) Acceptance Criteria

- [x] `effigy test <named-test>` errors in multi-suite contexts unless suite is explicitly targeted.
- [x] `effigy test <suite> <named-test>` executes only the selected suite.
- [x] Single-suite repositories retain simple `effigy test <named-test>` flow.
- [x] Help/docs clearly explain positional selection and ambiguity behavior.

## 7) Risks and Mitigations

- [ ] Risk: users rely on legacy implicit fanout for named invocations.
  - Mitigation: clear error message with direct command examples.
- [ ] Risk: suite token collisions with test names.
  - Mitigation: only treat first arg as suite when it matches a known suite id.
- [ ] Risk: environment complexity grows too quickly.
  - Mitigation: keep defaults hard-coded, add only focused project-level controls.

## 8) Deliverables

- [x] Runner updates for positional suite selection and ambiguity handling.
- [x] Updated test coverage for multi-suite targeting behavior.
- [x] Updated help and README guidance.
