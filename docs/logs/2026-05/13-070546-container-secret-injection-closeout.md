# Container Secret Injection Closeout

Closed `g05.005`.

## Changed

- Marked `g05.005` complete.
- Marked strict lane `080` complete.
- Added ready card `719` for the Underlay/Acowtancy config migration proof.
- Updated roadmap/spec front doors to point at `719`.

## Current State

Container startup now uses declared `targets = ["containers"]` vault secrets
through compose process environment injection. Compatibility export exists as
an explicit plaintext bridge and refuses repo-root `.env`.

## Validation

- docs path checks
- `git diff --check`

## Next

Execute `719` to prove the config/secrets split in Underlay and Acowtancy.
