# g07.027 - Role-Aware Context Ranker

Status: Complete
Depends on: `g07.026`

## Goal

Make `graph context` prefer the file role implied by the request.

## Scope

- classify files into generic roles:
  - implementation
  - test
  - docs
  - roadmap/planning
  - fixture/example
  - generated/cache/vendor
- derive request intent from stable lexical cues:
  - implementation: `trace`, `implement`, `owner`, `runtime`, `command`,
    `flow`, `where`, `how`
  - tests: `test`, `regression`, `fixture`, `coverage`
  - docs: `docs`, `guide`, `contract`, `roadmap`, `skill`
- apply small role boosts/penalties rather than hard filters
- normalize request tokens:
  - lowercase
  - split snake/camel/kebab words where useful
  - remove high-noise stop words such as `trace`, `find`, `where`, `how` from
    direct match scoring after intent extraction
- cap repeated symbol-hit scoring per file
- prefer multi-token co-occurrence and exact phrase matches

## Guardrails

- do not make the ranker Effigy-only by hardcoding `src/runner` or
  `crates/effigy-*`
- do not hide tests/docs when they are the best answer
- keep reasons human-readable and specific
- keep scoring deterministic

## Acceptance Criteria

- implementation tasks rank implementation files before tests/docs
- docs tasks rank docs before implementation
- repeated-symbol files no longer dominate just because they contain dozens of
  matching test names
- reasons explain role and match contributions without flooding output

## Next Task

After `972`, execute `973`.
