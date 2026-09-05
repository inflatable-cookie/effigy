# 020 Strategic Runway Atlas v1

Status: Draft
Owner: Platform + Maintainers
Purpose: long-horizon strategic runway for Effigy after `g08` closeout, shaped by
Northstar Atlas on 2026-08-17.

## 1. Destination and horizon

Effigy is the deterministic orchestration spine for monorepos and agent-driven
work. `g08` closed the graph-aware scan, security/posture, dependency-linking,
explicit-catalog-membership, unified test orchestration, pre-release CI proof,
and committed Bun pinning lanes.

The material horizon now is not "ship the next feature card." It is choosing
which strategic era `g09` should open after operator intent lands, while
operationalizing the vision governance layer that templates `001`–`019`
promised but have not yet populated with live evidence.

Out of scope for this atlas pass: opening `g09`, release execution, workflow
edits, or pretending every future card is already known.

## 2. Strategic direction

Direction (outcomes and constraints):

1. **Deterministic spine** — routing, contracts, and release gates stay the
   product moat; convenience must not erode explainability.
2. **Agent-native operations** — graph, scan, papercuts, and docs-policy
   surfaces should reduce agent thrash without inventing hidden automation.
3. **Portfolio realism** — cross-repo linking, pinning, bootstrap, and consumer
   adoption must work on real sibling-repo graphs, not fixture-only stories.
4. **Governed evolution** — vision artifacts, decision records, and exception
   tracking become routine evidence, not template shelfware.
5. **Sustainable internals** — crate boundaries, god-file pressure, and command
   surface clarity remain first-class; new capability pays down complexity tax.

Non-goals for the next era:

- MCP server or graph daemon as canonical product surfaces
- speculative plugin marketplace before contracted extension points exist
- generation rollover while `g08` closeout debris still lives in active specs
- breaking command-surface moves without an explicit operator-owned breaking
  release decision

## 3. Current shape

Canonical evidence:

- `g08` is complete through `g08.031`; no ready strict card remains.
- Vision index `001`–`019` is broad; maturity baseline `019` still points at
  unpopulated governance registers.
- Consumer Northstar contract (`056`) and starter `qa:northstar` bundle are
  released; cohort proof lives mostly in archived logs.
- Agent instruction surfaces were stale versus updated Northstar repo contract
  (`CLAUDE.md` bridge, papercuts loop, `scripts/README.md`).

Material contradictions resolved in this refresh:

- root `CLAUDE.md` was symlinked to `AGENTS.md` instead of bridging with
  `@AGENTS.md`
- `AGENTS.md` lacked the Northstar papercuts loop and worker-mode boundary
- `scripts/README.md` and `.agents.local.env.example` were missing

Accepted uncertainty (operator-owned):

- whether the next lane is release hardening, governance operationalization,
  agent-adoption proof, or a breaking tidy-up tranche
- when `g09` rollover is justified versus extending `g08` backlog themes
- how aggressively to pursue research Phase 3 (remote execution, plugins,
  telemetry) before Phase 2 DX gaps close

## 4. Horizon model

### Horizon A — Stabilize and choose (now → next ready card)

Outcome: trustworthy planning surfaces and one operator-selected lane compiled
into strict specs/cards.

Depends on:

- refreshed instruction surfaces and docs spine (this refresh)
- operator intent for the next bounded owner (release, governance, adoption,
  or breaking cleanup)

Unlocks: a single honest `g08` extension or a disciplined `g09` open.

Excludes: parallel "small polish" lanes across unrelated seams.

Rollover trigger: operator confirms lane + acceptance; planning compiles cards.

### Horizon B — Governed product operations (next 1–2 generations)

Outcome: vision governance runs on live data — artifact status register,
decision index, scheduled reviews, exception burden visible in logs.

Depends on:

- Horizon A lane selection
- templates `009`, `015`, `017`, `018` populated and referenced from roadmaps

Unlocks: promotion decisions grounded in measured tag movement, not ad hoc
roadmap appetite.

Excludes: full maturity scorecard automation before two manual review cycles.

### Horizon C — Agent-first maintainer experience (mid runway)

Outcome: agents reliably prefer Effigy graph/scan/docs surfaces over raw
tooling for code understanding, hygiene, and cross-repo work.

Depends on:

- graph freshness trust (`doctor`/`graph status` adoption)
- scan intelligence tuned for real repos
- papercuts triage loop feeding bounded product fixes
- consumer cohort replays with `qa:northstar`

Unlocks: lower agent cost per maintenance task; credible external adoption
story.

Excludes: LLM summaries as canonical graph data; MCP as required path.

### Horizon D — Platform and ecosystem scale (long runway)

Outcome: Effigy is the default orchestration layer across a portfolio of repos
with predictable distribution, extension, and optional remote execution.

Depends on:

- Horizon B–C discipline
- research promotion on remote execution, CI depth, plugins, telemetry
- distribution channel maturity beyond current Homebrew/curl/cargo paths
- breaking command-surface compaction only when operator approves semver pain

Unlocks: Underlay/decodelabs bundle consumers treat Effigy as infrastructure,
not a side tool.

Excludes: premature v1.0 compatibility shims; ecosystem promises without
contracted extension surfaces.

## 5. Strategic bets and dependencies

| Bet | Trade-off | Depends on | Irreversible if wrong |
| --- | --- | --- | --- |
| Operationalize governance before new feature sprawl | slower visible feature velocity | Horizon A intent | low — mostly docs/process |
| Double down on agent-native surfaces | maintenance cost of ranking/benchmark proof | graph+scan freshness | medium — API shape churn |
| Portfolio linking/pinning as differentiator | Bun/Cargo edge-case tax | consumer proof discipline | medium — manifest semantics |
| Defer breaking command compaction | root CLI stays noisier longer | operator breaking-release appetite | high once moved |
| Research Phase 3 later | remote/plugin demand unserved short-term | Phase 2 DX closure | low until promoted |

The sequencing constraint was satisfied on 2026-09-02: the operator selected
the breaking-cleanup preview, all `g08` roadmaps were complete, stale strict
specs were archived, and `g09.001` opened with additive migration only. Live
use rejected its executable namespaces, so `g09.002` restored direct execution.
The second governance cycle then selected Theme 3 on 2026-09-03 as `g09.003`,
with Acowtancy as the frozen first consumer replay.

## 6. Coarse runway (milestone transitions, not a task queue)

1. **Refresh closeout** — instruction surfaces and docs spine aligned with
   updated Northstar contract (complete in this pass).
2. **Operator intent checkpoint** — pick one owner: governance registers,
   release candidate, agent-adoption cohort, or breaking cleanup preview.
3. **Compile next strict lane** — one spec, one roadmap milestone, ready cards
   with acceptance + validation + stop conditions.
4. **Governance operationalization** — first populated status register +
   decision index + governance review cycle.
5. **Agent adoption proof** — cross-repo benchmark + papercuts-driven fixes
   with published guidance updates.
6. **Generation rollover review** — only when `g08` is visibly closed in
   front doors and active specs tree is clean; then open `g09` with one theme.

## 7. Promotion map

| Outcome | Destination |
| --- | --- |
| Strategic horizons and bets | this document (`docs/vision/`) |
| Candidate `g09` themes awaiting operator pick | [`docs/roadmaps/backlog/g09-candidate-themes.md`](../roadmaps/backlog/g09-candidate-themes.md) |
| Time-ordered execution | next strict spec + `g08` extension or `g09` README after intent |
| Governance live data | future populated artifacts under `docs/vision/` + logs |
| Instruction-surface contract | root `AGENTS.md`, `CLAUDE.md`, `scripts/README.md` |

## Next Task

Execute `g09.004` release gate diagnosability (card `1112`) first; the Theme 3
cohort checkpoint is deferred by operator direction (2026-09-05). Direct
invocation remains canonical; Effigy release remains separately gated.
