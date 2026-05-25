# g08.003 - Existing Scan Graph Enrichment

Status: Complete
Depends on: `g08.002`

## Goal

Enrich current scan findings with graph context when a ready index exists.

This should make existing findings more actionable without changing their base
meaning.

## Candidate Enrichments

`god-files`:

- add inbound/outbound edge counts
- identify whether the file appears to be an entrypoint, shared owner, test
  owner, generated file, or low-impact bulk file
- rank oversized files by blast radius as a secondary field

`duplicate-blocks`:

- distinguish repeated production logic from repeated test fixtures or docs
- highlight duplicate blocks in files that share graph neighborhoods
- avoid raising severity for generated or fixture-only duplication

`attention-markers`:

- raise priority when markers sit on central files, public entrypoints, or
  boundary-crossing owners
- attach likely owners and likely tests where graph can identify them

`generated-in-src` and `generated-assets`:

- show whether generated files are referenced from live code
- attach dependent files when available

## Guardrails

- enrichment must be additive
- original scan severity must remain available
- graph-derived severity changes must carry a reason
- no graph enrichment should make a clean filesystem scan fail by itself unless
  the command explicitly asks for graph-aware policy

## Acceptance Criteria

- at least two existing scan families receive useful graph context
- fixture tests prove graph-ready and no-graph behavior
- output remains understandable in text mode and structured in JSON mode
- docs explain that enrichment is context, not a replacement for scan findings

## Next Task

Start `g08.004`.
