# Codebase sweep procedure

Use a read-only sweep after a release or before choosing broad maintainability work. Its output is evidence for `docs/plan.md`, not authority to change code.

1. Inspect the current [package map](010-package-map.md), [feature placement](026-feature-placement-and-command-surface.md), and relevant contracts. Run `effigy graph` for ownership questions and `effigy test --plan` for test shape.
2. Run bounded scans such as `effigy scan god-files --json`, `duplicate-blocks`, `comment-ratio`, and `attention-markers`. If a scan fails, record the limitation and inspect manually.
3. Look for duplicate models, split execution paths, hidden orchestration, oversized modules, stale abstractions, and repeated JSON/text rendering. Name concrete call sites. More or fewer crates is not a goal by itself.
4. Produce a short report: observed code and path, why the boundary costs work, a bounded improvement, tests that would prove it, and uncertainty. Do not edit code during the sweep.
5. Tom chooses which finding, if any, enters `docs/plan.md`. The resulting Queue brief owns implementation and review.
