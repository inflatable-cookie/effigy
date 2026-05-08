# 314 Decide Post-Production-Metadata Widening

Status: archived
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Decision

Do one provider-export planning batch before any adapter implementation.

The neutral model is now strong enough for the next seam to stop being
`deploy.model.v1` itself and start being the first adapter contract.

That next seam should be:

- Render first
- contract and file-shape planning before implementation

## Why

- `deploy.model.v1` now carries the main production metadata the first adapter
  needs:
  - static output ownership
  - release-hook promotion
  - health-probe promotion
- opening real adapter code without an explicit target contract would just move
  the ambiguity downstream into templates
- Render is the cleanest first provider because its file surface is explicit
  and its service model maps closely onto the current neutral model

## Result

The next ready card is:

- [`315-plan-first-render-export-contract.md`](./315-plan-first-render-export-contract.md)

## Next Task

Execute `315`.
