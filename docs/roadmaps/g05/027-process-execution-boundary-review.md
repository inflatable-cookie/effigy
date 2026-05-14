# g05.027 - Process Execution Boundary Review

Status: Complete
Depends on: `g05.022`, `g05.026`

## Goal

Review direct subprocess execution across Effigy and define the minimum shared
boundary needed for predictable errors, redaction, cwd handling, and timeout
behavior.

This is a review-and-tighten lane, not a rewrite.

## Evidence

- direct `Command::new` usage remains in provider package git resolution,
  bundle source git resolution, release git helpers, distribution, gateway,
  artifacts, Rhai process helpers, and `effigy-process`
- `effigy-process` exists, but not every domain should necessarily route
  through one facade
- provider packages need process helpers with redacted output capture, while
  domain crates still need clear command-specific diagnostics

## Scope

- inventory direct `Command::new` and `ProcessCommand::new` call sites
- classify each call site as domain-owned, Rhai-host-owned, or shared utility
  candidate
- define shared primitives only where they remove duplicated failure handling,
  redaction, cwd/env handling, or timeout behavior
- document retained direct calls with rationale
- update tests only where shared primitives are introduced

## Out Of Scope

- no mega process facade
- no forced migration of every subprocess call
- no change to release protocol commands
- no behavior changes to external commands unless covered by tests
- no async runtime redesign

## Guardrails For A Cheaper Model

- keep domain-specific commands domain-specific
- centralize result shape, redaction, timeout, and error projection only when
  two or more callers need the exact same behavior
- do not hide command strings in abstractions that make diagnostics worse
- preserve stderr/stdout handling exactly unless there is a test for the change
- never use this lane to bypass release or distribution safety checks

## Suggested Implementation Steps

1. Run `rg -n "Command::new|ProcessCommand::new" src crates`.
2. Build a small table of call sites and ownership decisions.
3. Identify only repeated error/result/redaction patterns.
4. Add a tiny shared helper if the evidence supports it.
5. Migrate the lowest-risk duplicated callers first.
6. Document retained direct calls.
7. Run focused tests for every touched domain.

## Acceptance Criteria

- subprocess call sites are classified
- shared helper work, if any, is minimal and justified
- retained direct calls have clear domain rationale
- provider package and Rhai process behavior remains predictable
- no release/distribution safety behavior is weakened

## Classification Outcome

### Shared utility candidates

- bundle git source materialization in
  `crates/effigy-manifest/src/bundles/source.rs`
- deploy-provider git source materialization in
  `src/runner/deploy_command/provider_package.rs`

These two paths shared the same `git` spawn pattern, the same stderr-based
failure rendering, and the same low-level ownership boundary. They now use the
shared helper in `effigy-core::git_exec`.

### Retained domain-owned direct calls

- `crates/effigy-rhai/src/process_support.rs`:
  owns redaction, cwd/env/stdin option mapping, PTY fallback, and live
  stdout/stderr behavior for the Rhai host surface
- `crates/effigy-process/**`:
  owns managed child lifecycle, signal handling, PTY/session details, and
  supervisor diagnostics
- `crates/effigy-release/src/git.rs`:
  keeps release-specific git diagnostics and safety posture local until a
  broader release-surface migration is separately justified
- gateway, distribution, container exec, runtime signal, and doctor call sites:
  remain domain-owned because they bind directly to platform-specific tools,
  privilege elevation, transport choices, or interactive command behavior

### Explicit non-decision

- no mega process facade
- no forced migration of every `Command::new(...)`
- no widening of `effigy-process` into a universal subprocess crate

## Validation

Minimum focused validation depends on touched files. Always include:

```bash
cargo test -p effigy-process
cargo test -p effigy-rhai process
```

If release, distribution, provider package, or bundle source code changes, run
their focused tests too.

## Next Task

Use this as the closeout review for the reusable-core hardening suite. After it
lands, update `g05.020` with validation and residual-risk notes.
