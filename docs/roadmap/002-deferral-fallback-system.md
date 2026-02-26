# 002 - Deferral Fallback System

Status: In Progress
Owner: Platform
Created: 2026-02-26
Depends on: 001

## 1) Problem

Some repos still depend on a legacy task runner implementation. Effigy needs an opt-in fallback path so unresolved requests can be handed off without duplicating task names.

## 2) Goals

- [x] Add TOML config support for deferral command (`[defer].run`).
- [x] Defer unresolved task requests to configured command.
- [x] Forward original request + passthrough args to deferred command.
- [x] Add loop protection to prevent recursive re-entry.
- [ ] Document migration guidance for PHP fallback usage.

## 3) Non-Goals

- [ ] No remote/decentralized fallback registry.
- [ ] No deferral for built-in parse errors.
- [ ] No multi-step fallback chain in phase 002.

## 4) Execution Plan

### Phase 2.1 - Core deferral behavior
- [x] Extend manifest schema with `[defer].run`.
- [x] Attempt deferral when task resolution exhausts named tasks.
- [x] Support interpolation tokens: `{request}`, `{args}`, `{repo}`.
- [x] Preserve normal execution path when tasks match.

### Phase 2.2 - Safety and diagnostics
- [x] Add loop guard via `EFFIGY_DEFER_DEPTH`.
- [x] Add tests for unresolved deferral, prefixed deferral, token substitution, and loop guard.
- [x] Add verbose trace output for deferral path.

### Phase 2.3 - Adoption docs
- [ ] Add cookbook example for deferring to legacy PHP Effigy.
- [ ] Add deprecation guidance for eventually removing deferral.

## 5) Acceptance Criteria

- [x] Unresolved requests trigger configured deferral command.
- [x] Matched tasks do not defer.
- [x] Recursive deferral attempts are blocked.
- [ ] Migration cookbook documented in guides.

## 6) Risks and Mitigations

- [x] Risk: fallback recursion if deferred process re-invokes Effigy.
  - Mitigation: environment-based loop guard with explicit failure.
- [ ] Risk: hidden task ownership ambiguity.
  - Mitigation: keep task ambiguity errors non-deferrable.

## 7) Deliverables

- [x] Deferral schema and runtime behavior in runner.
- [x] Test coverage for key deferral paths.
- [ ] Cookbook docs for legacy PHP migration bridge.
