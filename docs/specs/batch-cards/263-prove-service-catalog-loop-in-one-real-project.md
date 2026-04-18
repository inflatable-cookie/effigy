# 263 Prove Service Catalog Loop In One Real Project

Status: landed
Updated: 2026-04-18
Roadmap: `g02.011`
Spec: `docs/specs/011-service-catalog-and-compose-assembly-strict-lane.md`

## Objective

Close the remaining credibility gap in `g02.011` by proving the generated
compose loop in one real project through Effigy itself.

## Scope

- pick one real project with a catalog-backed container stack
- exercise the operator-facing `g02.011` path end to end
- record the proof outcome and any product gaps exposed by that run
- keep widening bounded to issues exposed by the proof itself

## Acceptance

- one real project uses the landed catalog-backed container surface through
  Effigy
- the proof covers generated compose ownership rather than only crate tests
- any gaps found are either fixed in-batch or written down as explicit follow-on
  product work
- `g02.011` can then close or narrow to one final cleanup decision

## Outcome

The proof landed through `/Users/tom/Dev/projects/underlay-reference`.

- a real repo declared a catalog-backed Colima stack under
  `[containers.stack.services.*]`
- `effigy catalog list --repo ... --json` confirmed the visible catalog surface
- `effigy container up --repo ... --detach --json` generated and used
  `infra/dev/.effigy-compose.generated.yml`
- `effigy container status --repo ... --json` reported the live generated
  compose path and running `postgres`, `minio`, and `mailpit` services
- `effigy container eject --repo ... --json` exposed one product gap: eject
  copied compose output but did not rewrite `effigy.toml` away from catalog
  ownership
- that gap was fixed in-batch by rewriting the manifest to
  `compose_file = "infra/dev/docker-compose.yml"` and removing the nested
  `services` table during eject
- the proof then reran cleanly: status switched to the permanent compose file
  and the generated compose artifact no longer remained the source of truth

`g02.011` is now complete on a trustworthy product boundary.

## Next Task

Hand off to `264` and reopen `g02.012` for the first bounded
context-routing integration slice.
