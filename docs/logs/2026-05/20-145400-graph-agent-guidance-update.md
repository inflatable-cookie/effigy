# Graph Agent Guidance Update

Date: 2026-05-20
Roadmap: `g07.077`
Batch card: `1027`

## Scope

Updated active graph guidance so agents are more likely to use graph when it
fits, without treating graph as the front door for every Effigy task.

## What changed

- updated both Effigy skill copies:
  - `.agents/skills/effigy/SKILL.md`
  - `skills/effigy/SKILL.md`
- updated both graph-assist references:
  - `.agents/skills/effigy/references/graph-assist.md`
  - `skills/effigy/references/graph-assist.md`
- updated the first-contact adoption guide:
  - `docs/guides/047-agent-and-cross-repo-adoption.md`

## Guidance shape

- graph-first only for code-understanding questions
- explicit trust-state rule:
  - reindex on `missing-index`
  - reindex on `refresh-recommended`
  - reindex on `degraded`
- explicit portable query examples:
  - `where are redirect responses handled`
  - `where are config migrations validated before apply`
  - `where does shell exit cleanup prompt run`
- explicit `rg` fallback rule for:
  - exact token lookup
  - missing-symbol proof
  - final pre-edit call-site or string-literal confirmation
- explicit reminder that deploy, state, docs, containers, release, and direct
  task execution still start with their matching Effigy surfaces

## Validation

- `effigy docs check links ...`
- `effigy docs check index`

## Vision Target Delta

- primary tags: `OPERATE`, `CONTRACT`
- moved: broad "use graph first" guidance -> sharper trust, phrasing, and
  fallback rules that stay portable across repo shapes
- remains open: `1028` measured closeout and residual-limit summary
