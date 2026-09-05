---
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
lane: g09.006
card: 1115
spec: 122
base_commit: bbb6f31f53bf22787fb3966e7aa31e25e87ff91f
branch: worker/g09-006-cross-repository-source-routing-1115
workspace: pending
---

# Worker Handoff: Cross-Repository Source Routing

Implement card `1115` from the promoted Dispatch Manifest on clean `main` at
`bbb6f31f53bf22787fb3966e7aa31e25e87ff91f`. The serial prerequisite, card
`1114` / g09.007, is merged. This is the only approved active lane; do not
open siblings or begin g09.006 follow-on work.

## Authority

- Roadmap: `docs/roadmaps/g09/006-cross-repository-source-routing.md`
- Card: `docs/roadmaps/g09/batch-cards/1115-cross-repository-source-routing.md`
- Strict spec: `docs/specs/122-cross-repository-source-routing-strict-lane.md`
- Dispatch provenance: operator-confirmed Northstar/Chatterbox direction,
  promoted at `3431a840b2b2a5c890c44f35c19b84a06f283018`

Read the manifest, card, spec, governing docs-context contract, and relevant
guides before editing. The manifest and spec are the complete execution
boundary. If a stop condition fires, return to the orchestrator with facts;
do not widen the lane.

## Capability classification

This is a frontier-capable implementation handoff. It requires bounded
reasoning across a typed portfolio grammar, one-level repository enumeration,
sequential reuse of the existing `docs_context` entry point, grouped
cross-repository payload/status semantics, repository identity, fixtures,
benchmark freeze, and read-only K1–K5 replay. The material consequence is a
new consumer-facing provenance and repository-boundary surface. The strict
spec, existing entry point, no-parallelism rule, and frozen fixtures bound the
design space.

## Required scope

Implement `effigy docs context <QUERY> --sources <PATH>` with optional
`--only <HANDLE>`. Support a `[portfolio] directories = [...]` file whose
one-level directories join only when their own `effigy.toml` declares
`[docs_policy.sources] share = true`, with typed optional `front_doors` and
`skill_roots`. Run repositories sequentially through the existing
docs-context entry point. Group results per repository; never merge rankings.
Emit statuses `ok`, `empty`, `stale`, `timeout`, `not-shared`, `missing`,
`invalid`, and `disallowed` with next steps. Include current HEAD, indexed
HEAD, and `content_identity` per result. Use the distinct
`effigy.docs.context.sources.v1` payload. Add the two frozen fixture
repositories and portfolio benchmark. Opt Effigy in and emit the Northstar
starter block. Perform the manual K1–K5 read-only replay against
Northstar/Effigy/Underlay with an `rg` comparison table; record K5 as pending
if the canonical triage has not settled it.

## Hard boundaries

Do not add recursion/globs, merged ranking, global authority scaling, a shared
index/cache, parallel execution, single-repository ranking or budget changes,
a new environment variable, consumer-repository writes, release execution,
or `.github/workflows` edits. Preserve schema ids and existing local behavior.

## Ownership and closeout

Worker owns implementation, tests, fixtures, benchmark additions, starter/init
profile addition, and its dated evidence log only as allowed by the manifest.
Coordinator owns `CHANGELOG.md`, final card/roadmap/spec/front-door closeout,
and merge. Open a PR and report the exact reviewed head, validation, oracle
mapping, and any stop-condition facts. Do not merge your own PR.

## Validation gate

Run the spec-122 oracle, focused tests, frozen benchmark, read-only K1–K5
replay, `effigy qa`, fmt, clippy with `-D warnings`, and `git diff --check` as
appropriate. Keep the worktree clean at the reported head.

## Next Task

Open the reviewable PR for card `1115`; the orchestrator will launch a distinct
provider/model exact-head review and handle merge and closeout.
