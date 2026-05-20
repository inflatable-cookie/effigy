# g07.074 - Behavioral Query Ranking And Vocabulary

Status: Complete
Depends on: `g07.073`

## Goal

Improve `graph explore` for human behavioral questions, especially when the
query language differs from code symbols.

The solution must be generic. Effigy can provide seed examples, but the
implementation should help any repo where users ask behavior-shaped questions
like "why does this shut down?", "where is the prompt?", "how does upload
validation work?", or "what handles login redirect?"

## Problem

The audit showed a real miss:

- Query: `where does effigy prompt to shut containers down on shell exit`
- First result: wrong owner family
- Rephrased query: `prompt container shutdown on shell exit`
- Correct result: `src/runner/container_command/closeout.rs`

That means the graph is too sensitive to exact wording. Agents should not need
to guess the internal vocabulary before the graph becomes useful.

## Scope

- review current query tokenization, role inference, synonym handling, and path
  scoring
- add a repo-agnostic behavior vocabulary layer for common action concepts:
  prompt, confirm, ask, shutdown, start, stop, open, close, exit, login,
  redirect, validate, migrate, deploy, upload, download, cache, index
- keep domain vocabulary extensible through code, not per-repo hard-coded hacks
- bias behavior queries toward implementation owners and away from incidental
  docs/tests unless the query is docs/test-shaped
- add gold queries that include Effigy plus at least two non-Effigy repo shapes

## Guardrails

- no Effigy-only phrase table
- no one-off boost for specific file paths like `src/runner`
- no LLM-generated semantic expansion
- no broad fuzzy matching that turns every behavior query into huge result sets
- every new ranking rule must explain itself in `reasons`

## Acceptance Criteria

- the shell-exit prompt query lands on the correct owner without special-casing
  that file
- at least three behavioral gold queries improve or stay correct across
  multiple repos
- result reasons explain behavior-vocabulary boosts clearly
- exact-token queries do not regress

## Next Task

Execute `1024`.
