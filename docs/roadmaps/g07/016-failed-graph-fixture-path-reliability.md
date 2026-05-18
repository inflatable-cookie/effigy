# g07.016 - Failed Graph Fixture Path Reliability

Status: Complete
Depends on: `g07.013`

## Goal

Remove the known failed-path set from full-repo graph indexing, or reclassify
each path explicitly if it should not be indexed.

## Scope

- investigate the seven failed paths recorded in the `g07.012` closeout
- separate parser gaps from intended exclusions
- fix structural graph support for valid bundle/export/manifest fixture shapes
- add regression coverage for each fixed class of failure

## Guardrails

- do not hide failures by broad ignore rules
- do not special-case only the current fixture filenames
- keep the resulting behavior reusable for real repo/bundle content
- if a path should stay unsupported, document why and expose that reason

## Acceptance

- the known failed-path list is reduced to zero, or every retained path has a
  deliberate unsupported classification with tests and docs
- full-repo index evidence is rerun after the fixes

## Next Task

Execute `935`.
