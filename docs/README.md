# Effigy Docs

Effigy documentation is organized into four top-level categories:

- `architecture/`: stable design and package/runtime boundaries.
- `contracts/`: machine-readable schema/index contracts for tooling consumers.
- `guides/`: operational runbooks and implementation guides.
- `roadmap/`: numbered phase plans and progress tracking.
- `roadmap/backlog/`: unnumbered exploratory plans not yet scheduled.
- `reports/`: dated execution reports, checkpoints, and sweeps.

Use architecture docs as the source of truth, roadmap docs for execution planning, and reports for evidence/history.

Current JSON contract:
- Canonical JSON mode is `effigy --json <command>`.
- Top-level schema is `effigy.command.v1`.
- See `guides/017-json-output-contracts.md` for payload details.
- See `contracts/json-schema-index.json` for validation coverage entries.

Notable guide additions:
- `guides/013-testing-orchestration.md`
- `guides/014-release-checklist-template.md`
- `guides/016-task-routing-precedence.md`
- `guides/017-json-output-contracts.md`
- `guides/019-watch-init-migrate-phase-1.md`
- `guides/020-dag-lock-policy-baseline.md`
