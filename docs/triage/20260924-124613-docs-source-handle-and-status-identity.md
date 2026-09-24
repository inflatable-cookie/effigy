# Cross-repository docs-source handle and status identity

Raised: 2026-09-24. Release-readiness audit requested by the operator.
Status: open; not execution authority.
Owner: Effigy Chatterbox for planning; docs-context source routing for repair.

## Evidence

- Portfolio directories may each contain a checkout with the same basename.
  The basename is used as the handle, and `--only <HANDLE>` filters by that
  handle without a cross-directory uniqueness check. One selection can query
  two repositories and return duplicate handles.
- `read_dir` failures are all reported as `missing` with an absent-directory
  instruction. Guide `079` reserves `missing` for an absent directory or
  checkout; permission and other I/O failures need accurate evidence.

## Known direction and open checks

- Keep one-level, explicit portfolio enumeration and the documented handle
  contract. Decide whether duplicate handles fail at portfolio validation or
  receive a stable disambiguated identity; avoid selecting a second repository
  by accident.
- Distinguish absence from unreadable directory state without leaking data or
  hiding a healthy sibling's result.

## Promotion condition

Settle handle collision behavior with the operator, then promote a bounded
source-routing task if it belongs in the pre-release frontier. This decision
is independent of committed consent and excerpt provenance.

## Next check

Ask the operator whether this is a release gate or a retained follow-up.
