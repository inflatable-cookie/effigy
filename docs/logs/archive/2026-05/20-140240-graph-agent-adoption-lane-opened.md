# Graph Agent Adoption Lane Opened

Date: 2026-05-20
Card: [`1022`](../../../roadmaps/g07/batch-cards/1022-open-graph-agent-adoption-lane.md)
Strict lane: [`096`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

## Summary

Opened the graph agent-adoption follow-through lane.

The baseline confirms the original assessment: `effigy graph` is useful for
owner-shaped questions, but it is not yet reliable enough to become the default
agent navigation habit across repos. The next work must improve trust,
behavior-shaped query ranking, and edit-target/test-target packets without
hard-coding Effigy-only shortcuts.

## Vision Target Delta

Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`

Baseline:

- graph is present and useful for several implementation-owner queries
- agents still need to fall back to `rg` for phrasing-sensitive behavioral
  questions and exact proof
- freshness is technically available but not compact enough for first-glance
  trust

Target for this lane:

- graph becomes the natural first command for code-understanding questions
  across Effigy-adopting repos
- graph remains explicitly non-universal: exact-token proof still belongs to
  `rg`
- improvements are proven against Effigy, Underlay, decodelabs, and a small
  fixture path

## Baseline Status

Command:

```bash
effigy graph status --json
```

Observed summary:

- `ready=true`
- `stale_paths=61`
- `failed_paths=0`
- `files=3420`
- `symbols=33046`
- `edges=150099`

Interpretation:

- graph has a usable index
- the trust signal is still too noisy for agent flow because `ready=true` and a
  large stale-path count coexist in the status summary
- `1023` should make the trust decision compact while preserving detailed
  stale diagnostics

## Explore Baseline

Strong results:

- `where is catalog discovery implemented`
  - top owner: `crates/effigy-routing/src/discovery.rs`
- `where is task listing rendered`
  - top owner: `crates/effigy-tasks/src/listing.rs`
- `where is workspace linux artifact handoff implemented`
  - top owner: `src/runner/system_command/workspace_provisioning.rs`
- `where is graph explore ranking implemented`
  - top owner: `crates/effigy-codegraph/src/query/mod.rs`

Weak or mixed results:

- `where is the init wizard setup inventory built`
  - top owner: `crates/effigy-builtin/src/init/wizard.rs`
  - nearby, but the actual ownership split includes `init/inventory.rs`
- `where does effigy prompt to shut containers down on shell exit`
  - top owner: `crates/effigy-containers/src/session.rs`
  - wrong owner for the user-facing prompt behavior
- `prompt container shutdown on shell exit`
  - top owner: `src/runner/container_command/closeout.rs`
  - correct after rephrasing

Interpretation:

- owner/module language works well
- behavior-shaped language remains too sensitive to phrasing
- `1024` should improve generic behavior vocabulary and ranking reasons
  without adding Effigy-only path boosts

## Affected Baseline

Command attempted:

```bash
git diff --name-only | effigy graph affected --stdin --json
```

Observed behavior:

- current dirty worktree contained a broad mixed set of CLI, runner, release,
  distribution, docs, and roadmap files
- `graph affected` remained CPU-bound for more than a minute and was stopped

Interpretation:

- broad dirty-set impact analysis is currently too slow for agent adoption
- `1025` and `1026` should include impact-packet usefulness and benchmark
  coverage for changed-file inputs
- this is a baseline limitation, not a validation failure for `1022`

## Cross-Repo Targets

Required fixture-backed target:

- small synthetic fixture in the Effigy repo, so the benchmark runs on any
  machine without private local repos

Optional live targets present on this machine:

- Effigy: `/Users/tom/Dev/projects/effigy`
- Underlay: `/Users/tom/Dev/projects/underlay-reference`
- decodelabs app: `/Users/tom/Dev/legacy/sites/brains`
- decodelabs library: `/Users/tom/Dev/legacy/libraries/decodelabs/archetype`

Acceptance rule:

- optional live repos may skip when absent
- fixture-backed proof must remain runnable everywhere
- no ranking improvement is acceptable if it only works because of Effigy
  module names or paths

## Non-Goals Reconfirmed

- no MCP server
- no graph daemon
- no JavaScript runtime dependency
- no Effigy-only synonym table or path boost
- no private benchmark that only works on this laptop
- no claim that graph replaces `rg`

## Next

Move to `1023`: tighten graph freshness trust signals.
