# External Skill Task Runner Closeout

Status: complete
Created: 2026-08-31
Roadmap: g08.037
Card: 1092
Spec: 110 (archived)

## Summary

Effigy now runs one explicitly selected installed-skill task catalog against an
independently resolved consumer repository:

```text
effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]
effigy skill run --path <SKILL_DIR|EFFIGY_TOML> <SELECTOR> [--repo <CONSUMER>] [--json] [-- <ARGS>]
```

The source owns task definitions, includes, Rhai/script assets, nested source
selectors, and `{skill}`. The consumer owns process CWD, `{repo}`/`{project}`,
env files, cache inputs/outputs, runtime metadata, and nested built-in targets.
V1 accepts standard host tasks and rejects member catalogs, escaping
composition or Rhai assets, container/runtime binding, managed/TUI/concurrent
shapes, and manifest-backed secrets before side effects.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- Movement: installed task code and its consumer target were coupled through
  CWD/`--repo` workarounds -> one typed command carries distinct source,
  target, invocation, and execution identities with auditable text/JSON proof.
- Remaining gap: no skill discovery/install/trust registry or container-bound
  skill execution. Those remain explicit non-goals. Documentation-context card
  `1089` is restored as the single ready task.

## Source And Target Matrix

| Path class | Resolution | Proof |
| --- | --- | --- |
| source root / manifest | canonical explicit `--path` only | task inventory and run JSON fixtures |
| manifest includes / bundle roots | source-relative and bounded by canonical source | isolated routing rejection fixtures |
| Rhai/script assets | source-relative and canonically contained | nested/absolute/symlink escape fixtures and installed Northstar smoke |
| `{skill}` | canonical source-root placeholder | identity fixture |
| target root | nearest consumer from invocation CWD or explicit `--repo` | two-consumer override fixture |
| invocation CWD | retained as evidence | explicit second-consumer fixture |
| execution CWD / `{repo}` / `{project}` | target-relative | identity fixture |
| env files / cache inputs / outputs / metadata | target-relative | disposable consumer fixture; source stayed untouched |
| nested skill selectors | isolated source lookup plus preserved target | nested-task fixture |
| nested built-ins | typed target dispatch | execution request/context coverage |

## Review Oracle

1. Source and consumer selector collision: `collision` ran the source selector;
   the consumer selector never entered the candidate set.
2. `{repo}` and `{skill}`: the identity fixture reported distinct canonical
   consumer and source roots.
3. Consumer default container: the consumer declared container defaulting;
   the explicit host skill task ran on the host without consumer inheritance.
4. External member: the member-bearing source failed during isolated catalog
   load; the consumer marker remained absent.
5. Nested skill task: `nested` retained source catalog, consumer target, and
   consumer execution CWD. Rhai reported the same split.
6. Second `--repo` consumer: target and execution CWD used consumer two while
   invocation evidence retained consumer one.

Additional no-side-effect failures cover unreadable/missing sources, missing
selectors, escaping includes, direct and nested container-bound tasks, and an
unresolved consumer target. Review repair added relative, absolute, and symlink
Rhai escape rejection plus managed/TUI/concurrent rejection without source or
target runtime-state leakage. Recursive validation runs across reachable source
task references before execution.

## Public And Agent Surfaces

- Added parser, scoped/general help, dispatch, text output, and command-envelope
  JSON for `skill tasks` and `skill run`.
- Added `effigy.skill.tasks.v1` and `effigy.skill.run.v1` to the JSON schema
  index, fixture expansion, examples, and selection checks.
- Updated quick start, troubleshooting, command matrix, architecture/package
  map, changelog, and both synchronized Effigy agent skills.
- Text and JSON both expose canonical source evidence and target resolution
  evidence.

## Installed Northstar Smoke

The local source
`/Users/tom/Dev/projects/northstar/skills/northstar/effigy.toml` was present.
Read-only `northstar/check:agent-instructions` ran through `skill run` against
the Effigy worker checkout:

- source: `/Users/tom/Dev/projects/northstar/skills/northstar`
- target: `/Users/tom/Dev/worktrees/effigy-skill-runner-1092`
- invocation CWD: `/Users/tom/Dev/worktrees/effigy-skill-runner-1092`
- execution CWD: `/Users/tom/Dev/worktrees/effigy-skill-runner-1092`
- exit/task status: `0` / `ok`
- Northstar and consumer files changed: none

## Validation Performed

- focused `effigy-cli`, `effigy-context`, `effigy-manifest`, `effigy-routing`,
  `effigy-execution`, `effigy-managed`, `effigy-rhai`, and `effigy-contracts`
  test runs: passed
- skill CLI-output fixtures: 15 passed
- complete CLI-output integration binary: 259 passed, 1 ignored
- indexed JSON fast checks: both skill schemas and all selected contracts passed
- first full QA attempt: exposed three ordinary container/handoff Rhai/context
  regressions; the explicit source split was narrowed and all three focused
  reruns passed
- initial implementation `effigy qa`: 3433 passed, 1 skipped; docs links, JSON examples,
  indexes, workflow paths, vision indexes/next actions, and JSON contracts passed
- review-repair `effigy qa`: passed after the three new no-side-effect CLI
  regressions; the installed Northstar read-only smoke also remained green
- `cargo fmt --all -- --check`: passed
- `cargo clippy --all-targets -- -D warnings`: passed
- `git diff --check`: passed

## Closeout

- Card `1092`, roadmap `g08.037`, and strict lane `110` are complete.
- Strict spec `110` is archived; contract `042` and architecture `025` remain
  durable authority.
- Roadmap `g08.035`, strict spec `108`, and card `1089` are active/ready again.
- The overlapping open PAPERCUTS entry about vendored skill portfolio status
  remains open and deferred; this card did not widen into that work.

## Next Task

Execute ready card
[`1089`](../../roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md).
