# Agent and Cross-Repo Adoption Contract

Status: complete
Created: 2026-03-06
Roadmap: g01.015
Batch: 15.5-agent-and-cross-repo-adoption

## Summary

Published the Effigy-first agent adoption contract for consumer repos, including
reusable `AGENTS.md` guidance, minimum adoption criteria, and rollout waves.

## Changes

- added guide:
  - `docs/guides/047-agent-and-cross-repo-adoption.md`
- updated docs navigation:
  - `docs/README.md`
  - `docs/guides/README.md`
- closed the final open batch in roadmap `g01.015`

## Vision Target Delta

- Primary tags: `OPERATE`, `ROUTE`, `MAINT`
- Movement: baseline `Effigy self-hosting existed but agent usage across consumer
  repos was still implicit` -> current `Effigy now has an explicit agent-first
  adoption contract with reusable repo instructions and rollout criteria`
- Remaining gap: `None within roadmap g01.015`

## Validation Performed

- command: `./scripts/check-doc-links.sh docs/README.md docs/guides/README.md docs/guides/047-agent-and-cross-repo-adoption.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md docs/logs/README.md`
  - result: pass
- command: `zsh -ic 'effigy-dev qa:docs'`
  - result: pass
- command: `git diff --check -- docs/README.md docs/guides/README.md docs/guides/047-agent-and-cross-repo-adoption.md docs/logs/README.md docs/logs/archive/2026-03/06-103500-agent-and-cross-repo-adoption-contract.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
  - result: pass

## Risks

- Consumer repos may still claim Effigy adoption too early if they copy the
  `AGENTS.md` snippet without meeting the minimum task/doctor/test coverage bar.
- Some external release or packaging paths will continue to require wrapper
  exceptions; those should remain explicit to avoid retraining agents toward raw
  scripts.

## Next Task

Roadmap `g01.015` is complete. Open the next roadmap milestone only when there
is a new implementation batch beyond self-hosting and agent-first adoption.
