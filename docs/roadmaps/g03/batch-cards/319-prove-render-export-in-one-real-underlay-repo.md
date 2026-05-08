# 319 Prove Render Export In One Real Underlay Repo

Status: archived
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Prove the new Render export surface in one real Underlay repo before widening
to Railway planning.

## In Scope

- run the Render export against one real Underlay consumer
- inspect the generated `render.yaml` against the repo's real structure
- fix any remaining model-to-export drift exposed by that proof
- document the real proof boundary in the planning lane

## Out Of Scope

- Railway planning
- Decodelabs production export work
- Render API integration or live provisioning

## Acceptance Criteria

- one real Underlay repo exports a coherent first `render.yaml`
- any proof-discovered drift is fixed on the product path
- the lane records whether Render is now honest enough to pause while Railway
  planning opens

## Result

The proof passed against `underlay-reference`.

What it verified:

- `front`, `admin`, `api`, and optional `jobs` export coherently into
  `render.yaml`
- static publish paths match the real package roots plus emitted build dirs
- SPA fallback rewrites match the real `svelte.config.*` fallback values
- managed Postgres and `DATABASE_URL` mapping stay coherent in one real repo

No product-path drift was exposed by the proof, so the next honest seam is now
Railway planning rather than more Render widening.

## Validation

- targeted deploy/export tests as needed
- `./target/debug/effigy deploy export render --repo <REAL_UNDERLAY_REPO> --path <TMP_DIR> --plan`
- `./target/debug/effigy deploy export render --repo /Users/tom/Dev/projects/underlay-reference --path /tmp/effigy-render-proof-underlay-reference`
- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/g03/batch-cards/318-decide-post-render-export-foundation-boundary.md docs/roadmaps/g03/batch-cards/319-prove-render-export-in-one-real-underlay-repo.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

After `319`, execute:

- [`320-decide-post-render-proof-provider-widening.md`](./320-decide-post-render-proof-provider-widening.md)
