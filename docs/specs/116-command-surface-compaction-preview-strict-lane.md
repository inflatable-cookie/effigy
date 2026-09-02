# 116 Command-Surface Compaction Preview Strict Lane

Status: Active
Owner: Effigy orchestrator
Created: 2026-09-02
Roadmap: [`g09.001`](../roadmaps/g09/001-command-surface-compaction-preview.md)
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)

## Outcome

Make five operator-job namespaces executable without duplicating command
implementations, breaking current automation, or silently consuming selector
names. The preview establishes the canonical post-`v1.0` shape while retained
direct routes provide an explicit migration window.

## Fixed Decisions

- Keep `<task>`, `<catalog>/<task>`, `tasks`, `test`, `watch`, `doctor`, and
  `init` direct.
- Keep `help`, `--help`, `--version`, leading `--json`, and `--repo` direct.
- Make `local`, `repo`, `deliver`, `extend`, and `admin` executable namespaces.
- Retain displaced direct built-ins until `v1.0` with a visible migration
  diagnostic.
- Make grouped forms primary in general help, group help, completions, current
  guides, and the managed Effigy skill.
- Keep legacy detailed help until removal wherever current deferral does not
  give the name to a manifest selector, but do not keep legacy commands in
  primary inventories.
- End this lane after the additive preview. Direct-route removal remains blocked
  on the explicit `v1.0` gate and refreshed consumer evidence.

## Canonical Route Map

| Namespace | Child commands |
| --- | --- |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts`, `papercuts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap`, `demo` |
| `extend` | `skill`, `rhai` |
| `admin` | `config`, `deps`, `secrets`, `defer`, `uninstall`, `version` |

`config completion` moves with its owning command, so its canonical route is
`effigy admin config completion`. `help` remains direct. The canonical detailed
help form is `<namespace> <child> --help`; `effigy help <child>` remains as the
legacy detail route during the preview where current deferral permits it. No new multi-token
`effigy help <namespace> <child>` grammar is required.

## Routing And Precedence

- An exact first argument matching one of the five namespaces enters grouped
  built-in routing. No manifest task owns that bare word after this preview.
- A slash selector such as `admin/test` remains a catalog/task selector. Group
  matching never splits or consumes slash selectors.
- A namespace without a child renders the same group inventory as
  `effigy help <namespace>`.
- A recognized child delegates to the existing typed command value after the
  namespace token is removed. Its arguments, side effects, exits, text output,
  command identity, result payload, and error details retain one owner.
- An unknown or missing-required child argument fails as grouped-command usage;
  it never falls through to manifest task execution.
- Grouped routes are the explicit built-in escape for a child name shadowed by
  repository task precedence. The retained direct spelling keeps its existing
  deferral behavior and receives no deprecation warning when it executes a
  manifest task.
- Leading global `--json` and `--repo <PATH>` retain current normalization and
  target selection. Child-specific trailing flags retain the child command's
  current parser behavior.

The 2026-09-02 inventory found no bare task collision across 30 top-level
Effigy repositories. Compli-me has a catalog alias `admin`; exact regression
proof must preserve `admin/<task>` while reserving space-separated `admin`.

## Migration Diagnostic

A displaced direct built-in emits one migration warning once routing has proved
that the built-in, rather than a manifest selector, owns the invocation.

Human mode writes one line to stderr. It does not alter stdout or exit status.
JSON mode keeps stdout as one valid `effigy.command.v1` document and adds a
top-level `warnings` array only when nonempty. Each warning has exactly:

```json
{
  "code": "legacy-direct-command",
  "message": "direct command `graph` is deprecated; use `effigy repo graph`",
  "replacement": "effigy repo graph",
  "removal": "v1.0"
}
```

The additive optional field does not alter `command`, `result`, or `error`.
Grouped routes and unrelated direct routes do not gain an empty `warnings`
field. Success, usage-error, and runtime-error envelopes use the same warning
shape when the displaced direct built-in was selected.

Detailed legacy help carries the same replacement and removal facts. Help-root
invocations such as `effigy help graph` do not themselves emit a deprecation
warning because `help` remains a direct command.

## Discovery And Completion

- General and group help show canonical grouped spellings only.
- `<namespace>` renders its group inventory; `<namespace> <child> --help`
  renders the existing typed child panel with canonical usage examples.
- `effigy help <child>` and `<child> --help` remain available with one migration
  note until `v1.0` wherever existing manifest deferral permits them.
- Completion candidates suggest grouped routes and their descendants. Retained
  legacy routes remain executable but are not primary suggestions.
- Current guides, examples, generated references, and the authoritative Effigy
  skill use grouped spellings. Historical logs, archived specs, and closed
  roadmaps keep historically accurate direct spellings.

## Dependency Runway

```text
1109 additive grouped routes + migration surface
  -> exact-head review and merge
  -> preview closeout; v1.0 removal remains a future gated lane
```

One worker owns card `1109`. Parser, help, completion, warning-envelope, docs,
and skill edits share one command-surface authority and are not safe parallel
write lanes. The worker is day-to-day: the choices are settled and the review
oracle bounds the material compatibility risk. Frontier review remains with the
orchestrator.

## Whole-Lane Review Oracle

Reject the preview if any of these counterexamples survives:

1. A grouped route reaches a second implementation or changes the child
   command's result/error payload, side effects, or exit.
2. `admin/<task>` is consumed as the `admin` namespace.
3. A shadowing manifest task is bypassed or warned through a retained direct
   route.
4. The explicit grouped route cannot reach a built-in whose direct name is
   shadowed.
5. A legacy warning contaminates JSON stdout, changes the inner payload, or is
   emitted for grouped, daily-spine, task, or slash-selector routes.
6. An unknown grouped child falls through to task execution.
7. Primary help or completion still teaches displaced direct spellings.
8. Any displaced direct command is removed before the `v1.0` gate.

## Validation And Evidence

Card `1109` maps every oracle row to a named parser, routing, CLI, JSON, help,
completion, or consumer fixture. Run focused suites while iterating, then
`effigy qa`, formatting, clippy with warnings denied, `git diff --check`, and
`effigy doctor --json` on the final tree. Record one dated evidence log with
the exact reviewed head and consumer-impact reconciliation.

## Stop Conditions

Stop and return to planning if grouped routing requires a second command
implementation, an existing bare task collision appears, slash selectors cannot
remain unambiguous, the warning cannot remain additive in
`effigy.command.v1`, or implementation needs direct-route removal, workflow
edits, a release, S3 work, or extension-transport decisions.

## Next Task

Execute ready card [`1109`](../roadmaps/g09/batch-cards/1109-add-executable-command-namespaces.md).
