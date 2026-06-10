# 2026-03-12 Source-of-Truth Consolidation

## Summary

Closed the remaining source-of-truth drift after the consumer-adoption and
starter-bundle proof work.

The core contract and product boundary were already decided earlier in the day,
but several landing pages and roadmap indexes were still describing `g01.029`
as a future migration milestone instead of an active consolidation milestone.

This batch aligned the human-facing front doors with the now-proven reality:

- Effigy owns generic validation, JSON/runtime surfaces, and release checks
- the `northstar-effigy` skill owns bootstrap/scaffolding and repo-shape choice
- `g01.029` is no longer “the next adoption roadmap”; it is the active
  product-boundary and stabilization milestone

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `MAINT`
- Moved: landing pages and roadmap summaries now match the finished
  Northstar + Effigy contract instead of lagging behind the implementation
- Remaining open: decide later, from observed adoption pain only, whether any
  bootstrap/init surface should move from the skill layer into Effigy itself

## Files Aligned

- root front doors:
  - `README.md`
  - `AGENTS.md`
- docs front doors:
  - `docs/README.md`
  - `docs/guides/README.md`
  - `docs/guides/047-agent-and-cross-repo-adoption.md`
- roadmap indexes:
  - `docs/roadmaps/README.md`
  - `docs/roadmaps/g01/README.md`
  - `docs/roadmaps/generation-index.md`

## Result

Effigy now tells one coherent story across the README, docs landing pages,
agent-adoption guidance, and roadmap indexes:

- use Effigy for the reusable validation engines and release/runtime surfaces
- use the `northstar-effigy` skill to scaffold the repo contract
- keep an Effigy-native `init` or repo-contract surface out of scope unless
  future consumer adoption shows repeated gaps the skill cannot cover

## Validation

- `cargo run --bin effigy -- docs check-links README.md AGENTS.md docs/README.md docs/guides/README.md docs/guides/047-agent-and-cross-repo-adoption.md docs/logs/README.md docs/logs/archive/2026-03/12-235900-source-of-truth-consolidation.md docs/roadmaps/README.md docs/roadmaps/g01/README.md docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md docs/roadmaps/generation-index.md CHANGELOG.md`
- `cargo run --bin effigy -- docs check-index --dir docs/logs --index docs/logs/README.md`
