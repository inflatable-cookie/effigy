# 420 - Acowtancy Proof And Closeout

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Close the artifact substrate planning/implementation round with an Acowtancy
proof boundary and the next implementation handoff.

## Scope

- inspect the current Acowtancy seed-bundle install/build/publish flow again
- map the Effigy artifact substrate onto the first Farmyard replacement seam
- document how UAT should apply explicit artifact refs
- document how UAT capture should be represented without making Effigy part of
  request serving
- list remaining implementation hooks for public artifact commands, OCI live
  transport, seed/dump JSON reports, and Farmyard adoption
- close `g03.036` only if no implementation-ready gap remains in this round

## Non-Goals

- no Acowtancy file edits
- no live private registry proof
- no production deployment orchestration
- no release work

## Exit Condition

This card is complete when the first Acowtancy operator flow is documented
against the new artifact substrate and the lane either closes cleanly or names
one precise next card.

## Acowtancy Proof Boundary

Current Farmyard flow:

- `seed-bundle-build.sh` packages `migration/dist/seed-bundles/<name>` into
  `.oci` files under `migration/dist/oci`.
- `seed-bundle-publish.sh` publishes those `.oci` files to the local OCI store.
- `seed-bundle-install.sh` reads `seed-bundles.sources.json`, accepts either
  `oci_ref` or `bundle_file`, pulls/unpacks into
  `migration/dist/seed-bundles/<name>`, then regenerates local post-SQL hook
  artifacts.
- `seed-bundles.sources.sample.json` already models digest-pinned refs for
  `spine` and `content`.
- `bundle-set.json` declares families, priority, replay hooks, and patch
  overlay hooks.

Effigy replacement seam:

- replace the transport/staging half of `seed-bundle-install.sh`
- keep `seed-bundles.sources.json` or an equivalent explicit source manifest
  as app/operator input
- resolve each source through `effigy-artifacts`
- emit `effigy.artifact.v1` metadata for each staged artifact
- hand Farmyard a staged path/root plus metadata
- let Farmyard regenerate `bundle-set.json` and hook artifacts after staging

Effigy must not absorb:

- seed-bundle family ordering
- `bundle-set.json`
- content/media/exam post-SQL handlers
- patch overlays
- residual queues
- migration closeout gates
- DB-level idempotency

## UAT Operator Flow

First UAT apply shape:

1. operator supplies explicit artifact refs, preferably digest-pinned
2. Effigy resolves and stages the artifact refs
3. Effigy writes artifact metadata and an apply operation report
4. Effigy invokes the app-owned Farmyard install/apply task with staged paths
5. Farmyard applies/replays, writes migration state, and records app-level
   idempotency results

First UAT capture shape:

1. Farmyard produces the SQL/OCI-ready payload or snapshot directory
2. Effigy stages and packages it as an artifact
3. Effigy records capture metadata and digest
4. publishing remains an explicit operator action

Effigy can be installed on UAT as an operator tool. It is not part of normal
request serving.

## Remaining Hooks

The next implementation round should add one card for public artifact and
Farmyard handoff work:

- `effigy artifact inspect <REF|PATH>`
- `effigy artifact stage <REF|PATH>`
- local artifact operation ledger
- Farmyard-compatible staged source manifest output
- no live registry requirement yet

OCI live transport and private-registry proof should be a separate card after
the local command/handoff path is stable.

## Next Task

Open the next artifact implementation card for public `artifact inspect/stage`
and Farmyard handoff output.
