# 1110 - Remove Executable Command Namespaces

Roadmap: [`../002-flat-command-execution.md`](../002-flat-command-execution.md)
Spec: [`../../../specs/archive/117-flat-command-execution-strict-lane.md`](../../../specs/archive/117-flat-command-execution-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: CLI command routing, discovery, completion, migration-diagnostic removal,
and current guidance
Created: 2026-09-02
Closed: 2026-09-02 — rollback PR
[#87](https://github.com/inflatable-cookie/effigy/pull/87) submitted for
exact-head review; evidence log
[`02-224606-flat-command-execution-1110.md`](../../../logs/2026-09/02-224606-flat-command-execution-1110.md)

## Purpose

Remove the executable namespace preview while preserving its useful help
organization and restoring flat direct invocation everywhere operators and
agents are taught to work.

## Acceptance

- remove built-in parsing and dispatch for exact space-separated `local`,
  `repo`, `deliver`, `extend`, and `admin` prefixes
- return those first words to ordinary selector routing; prove a manifest task
  with each name receives its following arguments
- restore direct built-ins as primary help rows, detail usage, completion
  candidates, current guides/examples, generated references, configuration
  examples, and both authoritative managed-skill copies
- preserve general help grouping and every `effigy help <group>` focused view
- remove `legacy-direct-command` warnings, migration-note rendering, grouped
  warning state, and the optional warning output only where it was introduced
  solely for the namespace preview
- prove text, JSON, usage-error, runtime-error, and streaming direct routes keep
  their pre-warning behavior
- preserve repository task shadowing, slash selectors, leading global flags,
  direct detailed help, and genuine command-owned subcommands
- update `[Unreleased]`, card/roadmap/spec/front doors, and one dated evidence
  log; keep g09.001/1109/spec-116/evidence historical bodies intact

## Validation

- typed inventory proving every built-in formerly placed under a namespace is
  directly reachable and no grouped mapping remains
- parser/routing fixtures for all five former words as manifest tasks, an
  unknown unowned former grouped route, a shadowing deferred built-in, slash
  selectors, and leading `--repo`/`--json`
- text and JSON success, usage-error, and runtime-error recurrence tests with no
  warning or migration metadata
- `graph watch --json` event-stream proof with no migration stderr
- general/group/detail help and completion recurrence tests
- current docs, generated-reference, configuration, changelog, and managed-skill
  source/install parity checks
- `effigy qa`, `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `git diff --check`, and
  `effigy doctor --json`

## Evidence

Write one log under `docs/logs/2026-09/` mapping each acceptance and review
oracle row to named proof. Record the exact PR head, deleted alias/warning
surfaces, restored selector fixtures, help-group preservation, validation, and
any remaining shadowing limitation. Do not rewrite historical preview records.

## Review Oracle

Reject the PR if:

1. any direct built-in changes inner text/JSON, side effects, command identity,
   arguments, or exit apart from warning removal;
2. any former namespace word remains reserved or fails to execute a same-named
   manifest task with its arguments;
3. an unowned grouped spelling still reaches the built-in child;
4. general help is no longer grouped or a focused `help <group>` view regresses;
5. help, completion, current docs, generated references, or managed skills still
   teach namespace-prefixed execution;
6. any direct route emits a migration warning/note or unrelated JSON output
   retains preview-only warning state;
7. repository shadowing, slash selectors, leading global flags, or a genuine
   subcommand changes;
8. archived preview evidence is rewritten rather than superseded by current
   authority.

## Stop Conditions

Stop if the rollback needs a second command implementation, a new built-in
escape or selector-precedence rule, a breaking unrelated JSON change, command
subcommand flattening, consumer edits, workflow/release mutation, S3 work, or
extension-transport design.

## Next Task

This card is complete on PR
[#87](https://github.com/inflatable-cookie/effigy/pull/87) awaiting
orchestrator review and merge. Direct invocation is canonical. No `v1.0`
direct-route-removal gate remains. Effigy release and S3 extraction stay
separately gated.
