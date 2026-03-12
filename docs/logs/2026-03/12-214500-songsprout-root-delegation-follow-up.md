# Songsprout Root Delegation Follow-Up

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: songsprout-root-delegation-follow-up

## Summary

Completed the second half of the Songsprout migration by upgrading the
workspace root after the earlier `trellis` authority-only pilot.

The earlier authority-only batch was a conservative response to the root
guardrail that said not to edit root files. Once that guardrail was explicitly
overridden, Songsprout was brought up to the same workspace-root standard as
`contact-patch` and `underlay-reference`.

## Changes

- normalized root teaching surfaces in:
  - `songsprout/AGENTS.md`
  - `songsprout/README.md`
  - `songsprout/package.json`
  - `songsprout/effigy.toml`
- removed the stale root no-edit rule from `songsprout/AGENTS.md`
- added root-level:
  - `qa:docs`
  - `qa:northstar`
- kept `trellis` as the docs authority and delegated root docs checks there
  rather than creating a competing root docs surface

## Validation

Validated directly in `songsprout` against released `effigy v0.2.6`:

- `effigy qa:docs`
- `effigy qa:northstar`

Both passed.

## Decision

Songsprout no longer needs to be treated as an authority-only exception.

Its final adopted shape is now:

- thin workspace root
- root-level docs orchestration
- native `qa:docs` / `qa:northstar` delegation into `trellis`
- `trellis` remains the sole docs authority

The earlier authority-only pilot remains useful as a record of the interim
state, but it is no longer the current contract.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `songsprout adopted only at the trellis authority layer`
  -> current `songsprout root and trellis now match the standard
  workspace-container contract on released 0.2.6`
- Remaining gap: `none specific to songsprout contract shape`

## Next Task

Use the completed Songsprout migration to collapse the remaining rollout plan
into one final classification of untouched repos: full-contract ready, docs
authority missing, or not worth migrating yet.
