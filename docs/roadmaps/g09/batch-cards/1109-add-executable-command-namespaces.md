# 1109 - Add Executable Command Namespaces

Roadmap: [`../001-command-surface-compaction-preview.md`](../001-command-surface-compaction-preview.md)
Spec: [`../../../specs/archive/116-command-surface-compaction-preview-strict-lane.md`](../../../specs/archive/116-command-surface-compaction-preview-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: CLI command routing, discovery, completion, migration diagnostics, and current guidance
Created: 2026-09-02
Closed: 2026-09-02 — additive preview PR
[#85](https://github.com/inflatable-cookie/effigy/pull/85) submitted for
exact-head review; evidence log
[`02-205536-command-surface-preview-1109.md`](../../../logs/2026-09/02-205536-command-surface-preview-1109.md)

## Purpose

Ship the additive grouped-command preview as one coherent public-surface change.

## Acceptance

- implement the exact namespace/child map in spec `116` through the existing
  typed command values and renderers
- reserve only exact space-separated namespace words; preserve slash selectors,
  including a fixture equivalent to Compli-me's `admin/<task>` alias
- keep the daily spine and global flags direct and unchanged
- preserve direct selector deferral while making grouped routes an explicit
  built-in escape
- emit the specified stderr or optional top-level JSON warning only after a
  displaced direct built-in is selected
- preserve child command identity, stdout, result/error payload, side effects,
  and exit apart from the migration warning
- make canonical grouped spellings primary in general/group help, completion
  candidates, current command/cookbook/agent guidance, generated references,
  and both authoritative managed-skill copies
- retain legacy detailed help with replacement and `v1.0` facts
- add an `[Unreleased]` changelog entry and one dated evidence log

## Validation

- parser tables for every namespace/child pair, missing child, unknown child,
  leading `--repo`, leading `--json`, and child help
- routing fixtures for bare-task reservation, direct deferral, grouped escape,
  catalog slash aliases, and unknown-child no-execution
- text and JSON success, usage-error, and runtime-error parity tests
- warning absence tests for grouped, daily-spine, manifest-task, and slash
  selector routes
- general/group/detail help and completion candidate recurrence tests
- current docs, generated-reference, skill-source/install parity, and JSON
  contract checks
- `effigy qa`, `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `git diff --check`, and
  `effigy doctor --json`

## Evidence

Write one log under `docs/logs/2026-09/` mapping every acceptance and review
oracle row to named proof. Record the exact PR head, changed public routes,
warning schema, collision fixture, validation results, and remaining `v1.0`
gate. Do not rewrite historical direct-command evidence.

## Review Oracle

Reject the PR if:

1. any grouped pair reaches different command logic or changes inner text/JSON,
   side effects, or exit;
2. `admin/<task>` or another slash selector enters grouped parsing;
3. a direct shadowing task is bypassed or warned;
4. a grouped route cannot bypass child-name shadowing intentionally;
5. JSON stdout is not one valid envelope, the warning field appears empty on
   unrelated routes, or success/error warning facts diverge;
6. an unknown grouped child executes a task;
7. help/completion has two primary spellings or omits the retained legacy
   detail migration note;
8. a displaced direct route, `watch`, global flag, or daily-spine route is
   removed or changed.

Smallest counterexample set: one child per namespace; all children in a typed
mapping test; a repo with a shadowing `docs` task; a repo with an `admin` catalog
alias and `admin/test`; one success, usage failure, and runtime failure in JSON;
one unknown child backed by a same-named manifest task; and completion/help
snapshots containing both canonical-primary and legacy-detail facts.

## Stop Conditions

Stop if implementation exposes an unrecorded selector collision, needs a second
command implementation or payload schema, cannot preserve direct deferral, or
expands into direct-route removal, consumer-repository edits, workflow/release
mutation, S3 extraction, or extension transport.

## Next Task

This card is complete and merged (PR `85` at `a50a2fc`). The next checkpoint
is the future `v1.0` consumer-evidence gate: direct-route removal requires a
refreshed consumer inventory and explicit release authority, and no removal
card exists yet. Effigy release authority stays separate.
