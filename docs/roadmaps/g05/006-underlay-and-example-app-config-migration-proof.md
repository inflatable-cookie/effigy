# g05.006 - Underlay And Example App Config Migration Proof

Status: Complete
Depends on: `g05.005`

## Goal

Prove the new config/secrets model against real Underlay-based applications,
starting with Example App.

## Scope

- Audit Example App `.env` and equivalent local files.
- Classify values as:
  - ordinary config
  - generated runtime config
  - true secrets
  - legacy compatibility only
- Move ordinary config into Underlay-approved config surfaces such as
  `config/local.toml` or bundle defaults.
- Declare true secrets in Effigy `[secrets]`.
- Document the Underlay source-of-truth convention.
- Update Example App docs to explain local setup, container injection, and
  deploy-provider credential handling.
- Validate the local container workflow without plaintext repo-root `.env`
  secrets.

## Underlay Rule

Effigy is the tool. Underlay is the application convention authority.

Any pattern that Underlay sites are expected to follow must be documented in
Underlay, even when Effigy implements the command behavior.

## Non-Goals

- No Example App-specific secret logic in Effigy.
- No production secret migration.
- No provider-hosted secret creation.
- No forced removal of every legacy compatibility path before the proof is
  stable.

## Acceptance Criteria

- Example App has a documented split between config and secrets.
- Underlay docs describe the standard local config and secret declaration
  pattern for bundle apps.
- Example App local container development has declared `containers` targets ready
  for Effigy injection once the local vault is initialised.
- No required secret values live in committed files.
- Any remaining `.env` use is documented as legacy compatibility or
  non-secret config.

## Closeout

Completed by card `719`.

Example App now declares true secret keys in root `effigy.toml`, documents the
bridge window for existing `.env` files, and links the operator workflow from
the root README and state/deploy runbook. Underlay now documents the same
Effigy-backed local vault posture as the source-of-truth convention for
Underlay-based apps.

The proof intentionally leaves Example App secret declarations optional until
operators initialise `.effigy/secrets/local.vault`; tightening selected keys to
`required = true` is the next app-local rollout step, not an Effigy core change.

## Test Strategy

- Example App `effigy secrets doctor`.
- Example App `effigy container up` against the local backend.
- Example App app smoke task using injected secrets.
- Underlay docs validation.
- Grep/audit checks for known secret names in committed files.

## Next Task

Close the `g05` secret-management suite in `721`.
