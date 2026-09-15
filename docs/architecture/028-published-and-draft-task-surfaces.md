# Published And Draft Task Surfaces

Status: active
Updated: 2026-09-15
Contract: [`046`](../contracts/046-published-and-draft-task-surface-contract.md)

## Purpose

Effigy task catalogs serve two different jobs. A small stable command surface
helps contributors, agents, completion, and automation understand a repository.
Short-lived proofs and temporary environments also need the same execution
machinery, but treating every experiment as a first-class selector makes the
catalog unreadable and leaves obsolete definitions behind.

Effigy separates those jobs without creating a second runner:

- `[tasks]` is the published repository interface;
- `[drafts]` is a provisional, explicitly selected workbench;
- both reach the same routing, execution, isolation, managed-session, argument,
  environment, and status machinery after selection.

Published and draft describe product lifecycle and discovery. They are not an
access-control or secrecy boundary.

## Published Surface

Existing `[tasks]` entries remain published by default. They retain flat
`effigy <selector>` and `<catalog>/<selector>` invocation, normal `effigy tasks`
listing, help and completion discovery, agent-facing inventory, composition,
and current JSON contracts.

A published task should be a maintained repository capability. Effigy does not
enforce an arbitrary count: visibility and explicit promotion create the
review boundary, while repository owners decide the appropriate public size.

Package-script migration continues to produce published tasks because a source
package script is already part of that repository's declared command surface.

## Draft Surface

Drafts use a separate manifest table and command namespace:

```toml
[drafts.provider-smoke]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Validate temporary provider integration"
run = "./scripts/provider-smoke {args}"
```

`created` and a non-empty `purpose` are required. `expires` is optional. Dates
use strict `YYYY-MM-DD` strings; an expiry cannot precede creation.

Drafts intentionally require the full table form so lifecycle metadata cannot
be omitted behind compact string or sequence syntax. The task body otherwise
supports the same runtime fields and profiles as a published task.

The operator surfaces are:

```text
effigy drafts [FILTER] [--json]
effigy draft <SELECTOR> [--json] [-- <ARGS>]
```

Normal task listing, help, completion, and agent inventory exclude draft
definitions. `effigy drafts` shows name, catalog, purpose, creation, optional
expiry, expiry state, and manifest provenance. Expiry is inventory evidence: it
does not delete or disable a draft. `doctor` reports expired declared drafts
with their exact source path and repair choice.

The explicit `draft` command selects only drafts and reuses catalog alias,
path-prefix, and cwd-nearest routing inside that surface. A draft and published
task may not share the same effective catalog/name pair; ambiguity fails during
manifest validation.

## Dependency Direction

Published tasks cannot depend on draft definitions. A stable public command
must not acquire a disposable dependency.

Drafts may call published tasks. Draft-to-draft composition must be explicit in
the task-step grammar rather than falling back from an unresolved published
reference. This keeps validation deterministic and makes removal impact
inspectable.

Removing a draft therefore cannot break the published graph. References from
other drafts fail with source-aware diagnostics before execution.

## Storage And Cleanup

Committed drafts remain part of the composed manifest and Git history. Effigy
does not recursively discover an implicit task directory, mutate includes, or
delete configuration automatically.

Repositories should place generated draft fragments under dated paths such as:

```text
config/drafts/2026-09-15-provider-smoke.toml
```

and include them through the existing explicit manifest composition rules.
Provenance in `effigy drafts` makes the definition and its include edge easy to
remove together. A later prune command may offer a preview, but automatic or
implicit deletion is outside this boundary.

Machine-local ignored drafts are also outside v1. They require a separate
decision about trust, discovery, portability, and precedence rather than an
implicit hidden overlay on committed repository behavior.

## Status And Machine Contracts

Draft execution keeps the normal execution pipeline and status safety, but its
identity includes the draft surface. Default published task listings and status
queries do not absorb draft definitions or draft-only historical rows.

`effigy drafts --json` and `effigy --json draft ...` have dedicated additive
schemas. Existing `effigy tasks` and ordinary task-run JSON remain byte- and
schema-compatible for repositories with no drafts.

## Failure Boundary

Fail before task execution when:

- draft lifecycle metadata is absent or malformed;
- creation/expiry ordering is invalid;
- a published and draft definition collide;
- a published task references a draft;
- a draft-to-draft reference is missing or ambiguous;
- the selected draft cannot be resolved in the requested catalog scope.

Expiry alone is not an execution failure and never grants deletion authority.

## Non-Goals

- task access control or secret storage;
- an arbitrary maximum published-task count;
- implicit filesystem discovery of draft fragments;
- automatic pruning;
- machine-local draft overlays;
- changing ordinary task routing precedence or execution semantics;
- converting existing tasks to drafts automatically.

## Drift Triggers

Revisit this architecture when adding local-only drafts, automatic generation
or pruning, draft sharing outside the repository, expiry enforcement, or any
new dependency direction between published and provisional surfaces.
