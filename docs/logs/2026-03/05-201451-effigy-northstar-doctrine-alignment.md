Status: complete
Created: 2026-03-05
Roadmap: g01.013
Batch: doctrine-alignment

## Summary

Applied the Northstar documentation migration to Effigy without rolling to a new
roadmap generation.
Roadmaps now live under `docs/roadmaps/g01/`, logs are month-sharded under
`docs/logs/`, and vision rollout history lives under `docs/vision/history/`.

## Changes

- Moved flat roadmap files into `docs/roadmaps/g01/` and retained backlog under `docs/roadmaps/backlog/`.
- Moved dated report artifacts into `docs/logs/YYYY-MM/` with deterministic day-time filename prefixes.
- Moved vision rollout history from `docs/reports/vision-history/` to `docs/vision/history/`.
- Updated root and docs-level readmes to reflect the new canonical structure.

## Validation Performed

- `find docs/roadmaps -maxdepth 3 -type f -name '*.md' | sort`
- `find docs/logs -maxdepth 2 -type f -name '*.md' | sort`
- `rg -n 'docs/roadmap|docs/reports|g02/' docs README.md`

## Evidence

- `g01` remains the active roadmap generation and the next milestone is `g01.014`.
- All migrated report artifacts now have month-segmented paths and day-time filename prefixes.
- No compatibility shim folders were left behind.

## Risks

- Historical log bodies still contain some period-authentic wording such as
  "report" where it describes the artifact at the time; this batch focused on
  structural normalization and live path correctness.

## Next Task

Open `g01.014` for the next net-new Effigy milestone and keep future logs tied
to active roadmap IDs.
