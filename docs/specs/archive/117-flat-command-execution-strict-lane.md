# 117 Flat Command Execution Strict Lane

Status: Archived (completed with card `1110` under `g09.002`)
Owner: Effigy orchestrator
Created: 2026-09-02
Closed: 2026-09-02
Roadmap: [`g09.002`](../../roadmaps/g09/002-flat-command-execution.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)
Completed card: [`1110`](../../roadmaps/g09/batch-cards/1110-remove-executable-command-namespaces.md)

## Outcome

Restore direct built-in invocation as Effigy's canonical execution grammar while
keeping the help taxonomy that made the large command inventory easier to scan.

## Fixed Decisions

- `effigy <command> ...` is the canonical built-in route.
- Remove executable `local`, `repo`, `deliver`, `extend`, and `admin` namespace
  aliases introduced by card `1109`.
- Keep the grouped general-help view and `effigy help <group>` discovery.
- Help-group entries display direct command spellings.
- Remove direct-command migration warnings, replacement notes, and the future
  `v1.0` direct-route removal gate.
- Preserve genuine command-owned subcommands such as `service pack update`,
  `docs context`, and `release gates`.
- Restore the pre-preview selector boundary for the five former namespace
  words. They are no longer reserved by built-in routing.
- Do not invent a new explicit built-in escape for repository tasks that shadow
  deferred built-ins. Existing direct-route precedence remains authoritative.

## Execution And Discovery Boundary

Help taxonomy is navigation, not execution grammar:

```text
effigy --help
effigy help repo
effigy graph explore "where is command parsing owned"
effigy docs context "which contract governs command routing"
effigy release gates
effigy skill tasks --path <SKILL>
```

`effigy repo graph`, `effigy deliver release`, and the other card-`1109`
namespace spellings are not built-in aliases after this lane. If a repository
owns a manifest task named `repo`, `deliver`, or another former namespace word,
normal selector routing owns that exact word and receives subsequent arguments.

## Dependency Runway

```text
operator rejects executable help taxonomy
  -> 1110 remove namespace aliases and restore flat discovery
  -> exact-head review and merge
  -> close g09.002; no v1 direct-route-removal gate remains
```

One worker owns the lane. Parser, dispatch, help, completion, warning-envelope,
docs, managed-skill, and closeout changes share one command-surface authority.
The worker is day-to-day non-frontier: the destination is settled and the
review oracle bounds the compatibility risk. Frontier review remains with the
orchestrator.

## Whole-Lane Review Oracle

Reject the rollback if any counterexample survives:

1. Any direct built-in changes command identity, arguments, side effects,
   stdout, JSON result/error payload, or exit apart from removal of the preview
   warning.
2. A former namespace word is still reserved instead of following normal
   manifest-selector routing.
3. A grouped executable alias still reaches a built-in in a repository that
   does not own the first word.
4. General help loses its job grouping or `effigy help <group>` stops working.
5. Help-group entries, completion, current guides, generated references, or
   managed skills teach namespace-prefixed execution.
6. Direct commands still emit `legacy-direct-command`, a migration note, or an
   empty warning field.
7. A genuine command-owned subcommand is flattened or otherwise changed.
8. Historical evidence is rewritten as though the preview never shipped.

Smallest counterexample set: one direct child from each former group; one
manifest task for each former namespace word; one unknown former grouped route;
general and focused help snapshots; completion candidates; text and JSON
success, usage-error, runtime-error, and `graph watch` stream cases; one
shadowing deferred built-in; and representative `service pack`, `docs context`,
and `release gates` commands.

## Validation And Evidence

Card `1110` maps every oracle row to named parser, routing, CLI, JSON, help,
completion, docs, and skill-parity proof. Run focused suites while iterating,
then `effigy qa`, formatting, clippy with warnings denied, `git diff --check`,
and `effigy doctor --json`. Record one dated evidence log and keep archived spec
`116`, roadmap `g09.001`, card `1109`, and its evidence historically accurate.

## Stop Conditions

Stop and return to planning if direct invocation needs a second implementation,
removing the aliases requires a new selector-precedence policy or built-in
escape, help grouping cannot remain independent of execution routing, JSON
warning removal changes an unrelated envelope contract, or the work expands
into consumer edits, workflow/release mutation, S3 extraction, or extension
transport.

## Next Task

This spec is archived with card `1110`. Direct invocation is canonical. No
`v1.0` direct-route-removal gate remains. Effigy release and S3 extraction stay
separately gated.
