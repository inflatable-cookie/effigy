# g09 Candidate Themes (Backlog)

Status: Exploratory
Owner: Platform
Created: 2026-08-17
Selected: 2026-08-29 — **Theme 2 active as a narrow g08 extension**
(`g08.035`, strict spec `108`)
Source: Northstar Atlas refresh ([`020-strategic-runway-atlas-v1.md`](../../vision/020-strategic-runway-atlas-v1.md))

These are strategic theme options for the next planning era. Theme 2 is now
scheduled as a narrow `g08` extension; themes 3–5 remain unscheduled.

Do not treat this file as an execution queue. Promote exactly one theme (or a
deliberate `g08` extension) into a numbered roadmap, spec, and ready cards.

## Theme 1 — Vision governance operationalization

**Status:** complete (`g08.032`, cards `1082`–`1084`, archived spec `105`)

**Primary tags:** `MAINT`, `RELEASE`, `OPERATE`

**Target envelope:** artifact status register, decision index, and first
governance review cycle run on live Effigy data within two planning iterations.

**Promotion signals:**

- `019` maturity baseline referenced by populated governance artifacts
- release/log entries include vision target deltas on schedule
- `effigy qa:docs:vision` stays green after register/index additions

**Dependencies:** none beyond operator intent.

**Non-goals:** automating scorecards before manual review discipline exists.

## Theme 2 — Agent-native maintainer experience

**Status:** completed (`g08.035`, cards `1088`–`1090`, archived strict spec `108`)

**Primary tags:** `OPERATE`, `MAINT`, `ROUTE`

**Target envelope:** measurable reduction in agent code-understanding thrash via
graph explore, scan intelligence, and papercuts triage on this repo plus one
external consumer replay.

**Promotion signals:**

- `effigy doctor` graph freshness warnings drive documented recovery paths
- papercuts open queue trends down or converts to bounded fixes
- [`076-code-graph-and-agent-workflows.md`](../../guides/076-code-graph-and-agent-workflows.md)
  guidance matches measured benchmark outcomes

**Dependencies:** graph index freshness discipline; papercuts capture habit.

**Non-goals:** MCP, daemon, LLM-generated graph summaries.

## Theme 3 — Consumer adoption cohort replay

**Primary tags:** `RELEASE`, `OPERATE`, `CONTRACT`

**Target envelope:** at least one non-fixture consumer repo passes
`qa:northstar` end-to-end with archived evidence log and contract drift notes
fed back into starter templates.

**Promotion signals:**

- starter `AGENTS.md` / `effigy.toml` / docs_policy parity with guide `056`
- cohort log linked from [`g01/029`](../g01/029-northstar-effigy-consumer-adoption-kit.md)
  or successor milestone

**Dependencies:** released `qa:northstar` bundle (already shipped).

**Non-goals:** forcing workspace-container shape onto single-repo pilots.

## Theme 4 — Breaking command-surface and compaction preview

**Primary tags:** `MAINT`, `ROUTE`, `RELEASE`

**Target envelope:** operator-approved breaking plan with migration guide,
JSON contract coverage, and explicit semver gate — no silent CLI moves.

**Promotion signals:**

- superseded [`breaking-command-surface-and-container-compaction.md`](./breaking-command-surface-and-container-compaction.md)
  decisions refreshed against current CLI reality
- consumer impact inventory attached before any compile

**Dependencies:** explicit operator appetite for a breaking release.

**Non-goals:** reopening the completed `v0.5.0` backlog verbatim; mixing with
unrelated feature work.

## Theme 5 — Release candidate hardening (`v0.12+`)

**Primary tags:** `RELEASE`, `CONTRACT`

**Target envelope:** green exact-SHA hosted CI proof, gate pass, and documented
release orchestration for the next semantic version without workflow edits
unless approved.

**Promotion signals:**

- contract `039` evidence for candidate SHA
- `effigy release gates` + changelog extract path exercised in log

**Dependencies:** operator release instruction; clean tree on candidate commit.

**Non-goals:** tag mutation on failed releases.

## Promotion rule

Pick one theme (or justify a narrow `g08` extension instead). Planning compiles:

1. one strict spec
2. one roadmap milestone in the active generation
3. ready batch cards with acceptance, validation, and stop conditions

Archive stale specs `097`, `099`, and `100` during the same planning sweep if
they remain in the active tree.

## Next Task

Execute ready card `1093` in the separate feature-boundary follow-through.
Themes 3–5 remain unscheduled.
