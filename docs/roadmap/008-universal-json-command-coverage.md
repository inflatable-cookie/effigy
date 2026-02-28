# 008 - Universal JSON Command Coverage

Status: In Progress
Owner: Platform
Created: 2026-02-28
Depends on: 001, 002, 003, 004, 005

## 1) Problem

Effigy initially supported JSON output for selected command paths (`tasks`, built-in `test`) but not as a universal contract across all interactions. In CI environments, this created inconsistent parsing logic and mixed human/machine output handling.

## 2) Goals

- [ ] Ensure all command interactions can emit JSON when `--json` is requested.
- [ ] Standardize command-level success/error envelopes for reliable CI parsing.
- [ ] Preserve non-zero exit code semantics while still returning machine-readable error payloads.
- [ ] Support built-in and catalog task execution paths with structured output where feasible.
- [ ] Keep existing command-specific JSON payloads available during migration.

## 3) Non-Goals

- [ ] No forced removal of existing command-specific JSON schemas in phase 008.
- [ ] No attempt to serialize full TUI screen state as JSON.
- [ ] No per-line streaming JSON protocol in phase 008 (batch payloads only).

## 4) UX Contract

Default expectation:
- `effigy --json <command>`

Behavior:
- JSON mode suppresses decorative CLI framing/header output.
- Success paths return structured JSON payloads.
- Failure paths return structured JSON payloads and non-zero exit code.
- `--json` applies consistently to:
  - `help` / command help
  - `tasks` / `catalogs`
  - `doctor`
  - built-in tasks (`test`, `config`, etc.)
  - catalog task execution (`effigy <task>`, `effigy <catalog>/<task>`)

## 5) Target JSON Model

Phase target (command envelope):

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
  "command": {
    "kind": "task",
    "name": "build"
  },
  "result": {},
  "error": null
}
```

Error envelope:

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": false,
  "command": {
    "kind": "task",
    "name": "build"
  },
  "result": null,
  "error": {
    "kind": "TaskCommandFailure",
    "message": "task command failed ...",
    "details": {}
  }
}
```

## 6) Execution Plan

### Phase 8.1 - Foundation (In Progress)
- [x] Create roadmap and migration strategy.
- [x] Add command-level JSON error envelope path in CLI entrypoint.
- [x] Ensure `--json` in task invocation is consumed by effigy runtime (not blindly forwarded to child commands).

### Phase 8.2 - Catalog Task JSON Runs
- [x] Add structured JSON payload for non-built-in task execution.
- [x] Capture child `stdout` and `stderr` into JSON payload buffers.
- [x] Preserve non-zero exit behavior while still emitting JSON output.

### Phase 8.3 - Built-in Coverage Expansion
- [x] Add JSON output for built-in `help`.
- [x] Add JSON output for built-in `config`.
- [x] Ensure `catalogs` alias preserves JSON consistency.

### Phase 8.4 - Envelope Unification
- [x] Introduce command envelope while preserving existing command-specific schemas in `result` (`--json` now returns `effigy.command.v1`).
- [x] Add temporary compatibility policy (`--json-raw` retains legacy command-specific top-level schemas).
- [x] Publish schema versioning and migration notes.

### Phase 8.5 - Contracts and CI Validation
- [x] Add JSON contract tests for success/failure across all command kinds.
- [x] Add negative-path tests for parser/runtime errors in JSON mode.
- [x] Update docs/contracts index and smoke scripts.

## 7) Acceptance Criteria

- [x] Every user-invokable interaction supports `--json` output.
- [x] All JSON failure responses are machine-readable and include error kind/message.
- [x] Non-zero command failures continue to exit non-zero in JSON mode.
- [x] No ANSI/control output appears in JSON mode responses.
- [x] CI can consume a single stable top-level shape for all command outcomes.

## 8) Risks and Mitigations

- [ ] Risk: breaking existing downstream parsers for current command-specific schemas.
  - Mitigation: phase rollout with compatibility mode and explicit schema docs.
- [ ] Risk: command-output capture changes behavior for long-running/interactive tasks.
  - Mitigation: only capture in JSON mode; keep text-mode runtime unchanged.
- [ ] Risk: duplicate JSON handling paths (runner vs CLI entrypoint).
  - Mitigation: centralize envelope/error rendering in one shared utility.

## 9) Deliverables

- [x] Command-level JSON envelope + error contract.
- [x] JSON support across built-in and catalog task execution paths.
- [x] Updated contract tests and documentation.
