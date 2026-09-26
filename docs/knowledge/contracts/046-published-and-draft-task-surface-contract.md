# 046 Published And Draft Task Surface Contract

Owner: task manifest, discovery, routing, and execution maintainers
Architecture: [`028`](../architecture/028-published-and-draft-task-surfaces.md)

## Purpose

Keep the normal Effigy task inventory small and deliberate while allowing
temporary proofs and environments to use the established task runtime without
becoming published repository commands.

## Manifest Grammar

`[tasks]` retains its existing grammar and means published. Existing manifests
need no migration.

Drafts are keyed full tables:

```toml
[drafts.provider-smoke]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Validate temporary provider integration"
run = [{ task = "build" }, { run = "./scripts/provider-smoke {args}" }]
```

Each draft requires:

- `created`: a strict `YYYY-MM-DD` string;
- `purpose`: a non-empty human explanation;
- one valid task body under the existing runtime rules.

`expires` is an optional strict `YYYY-MM-DD` string and must not precede
`created`. Compact string, compact sequence, and inline shorthand definitions
are rejected under `[drafts]` because they cannot carry lifecycle metadata.

After metadata is separated, the remaining draft body uses the same typed task
definition, profiles, runtime binding, environment, secrets, managed mode,
locks, cache, readiness, and argument semantics as `[tasks]`.

Manifest composition treats `drafts.<name>` as a normal keyed path for
duplicate and explicit-override diagnostics. No directory is auto-discovered.

## Discovery And Commands

The published inventory remains:

```text
effigy tasks [FILTER] [--resolve <SELECTOR>] [--json]
```

It must exclude draft definitions and draft-only status rows from text, JSON,
help, completion, and agent-oriented output.

The draft inventory is:

```text
effigy drafts [FILTER] [--json]
```

Each draft row reports selector, catalog identity, purpose, created date,
optional expiry, `active` or `expired` lifecycle state, and the composed source
manifest. Its versioned JSON schema is additive and independent of
`effigy.tasks.v1`.

Draft execution is:

```text
effigy draft <SELECTOR> [--json] [-- <ARGS>]
```

It selects only `[drafts]`. It reuses explicit catalog alias, catalog path,
cwd-nearest, and shallowest-unambiguous routing within the draft set. Ordinary
flat task invocation never falls through to a draft.

`draft` and `drafts` join the work help group as direct built-ins. Existing
repository-selector precedence and diagnostics remain consistent with other
built-ins; help and completion expose the commands, never individual drafts.

## Identity And References

Task identity includes a surface discriminator: `published` or `draft`.

- The same effective catalog cannot declare the same name in both surfaces.
- A published task reference resolves only published tasks.
- A draft's ordinary `{ task = "..." }` step resolves only published tasks.
- Draft-to-draft composition uses an explicit `{ draft = "..." }` step and
  resolves only through the draft catalog rules.
- A published definition containing a draft step is invalid before execution.

Cycles and nested execution retain current fail-closed task-graph rules across
the combined explicitly referenced graph. Removing a draft cannot invalidate a
published task graph.

## Expiry And Doctor

Expiry is advisory lifecycle evidence, not runtime policy.

- `effigy drafts` marks an entry expired when the current local calendar date is
  after `expires`.
- `effigy doctor` reports each expired draft with selector, expiry, source
  manifest, and guidance to remove or deliberately extend it.
- Running an expired draft remains allowed and preserves normal task output and
  exit behavior.
- Effigy never deletes, edits, disables, or silently omits an expired draft.
- A missing `expires` value produces no age threshold or invented warning.

Tests must inject or otherwise control the evaluation date; wall-clock timing
must not make fixtures flaky.

## Execution And Status

After explicit draft selection, execution uses the canonical task request and
pipeline contracts. Drafts do not get a reduced runner or bypass isolation,
locking, secrets, managed sessions, interruption, or container policy.

Status records carry the surface discriminator. Default `effigy tasks status`
queries remain published-only and do not report deleted draft history as stale
published tasks. Draft inventory may report current draft execution status, but
a broader draft history/prune interface is not required in v1.

The draft run JSON result names the draft surface, resolved catalog, definition
source, and ordinary execution outcome without changing the existing published
task-run schema.

## Compatibility

- Every existing `[tasks]` manifest parses and behaves unchanged.
- `effigy tasks`, task help/completion, flat selectors, JSON, migration, task
  status, doctor without drafts, and execution remain compatible.
- `tasks migrate` continues to emit `[tasks]` and never silently reclassifies an
  existing package script as a draft.
- Existing included manifests gain no draft behavior unless they explicitly
  declare `[drafts]`.
- No repository task is rewritten, moved, or hidden automatically.

## Required Proof

1. An unchanged manifest produces identical published task text, JSON,
   completion candidates, resolution, and execution.
2. A draft is absent from every default published discovery surface and cannot
   run through flat selection.
3. `effigy drafts` reports exact lifecycle fields and composed origin in text
   and versioned JSON.
4. `effigy draft` resolves root, alias-prefixed, path-prefixed, and cwd-nearest
   drafts deterministically and passes arguments through the normal pipeline.
5. Host, container-bound, managed, locked, secret-aware, cached, and profiled
   draft fixtures use their existing execution guarantees rather than a second
   runtime.
6. Missing/malformed dates, empty purpose, reversed dates, compact definitions,
   and cross-surface name collisions fail before side effects.
7. Published-to-draft references fail validation; draft-to-published and
   explicit draft-to-draft references resolve; removal of a referenced draft
   gives source-aware failure.
8. An expired draft remains runnable, appears expired in injected-date
   inventory, and yields a useful doctor finding without file mutation.
9. Published status queries exclude draft-only records and deleted draft
   history; draft status identity cannot collide with published identity.
10. Explicitly included dated fragments report their physical source, while an
    unreferenced file under `config/drafts/` is not discovered.
11. `tasks migrate` and existing human/JSON contracts remain unchanged.

## Change Triggers

Revisit this contract for machine-local drafts, automatic creation or pruning,
implicit directory discovery, enforced expiry, access control, draft export,
or published-to-draft dependency support.
