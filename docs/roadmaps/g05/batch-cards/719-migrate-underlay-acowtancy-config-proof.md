# 719 - Migrate Underlay Acowtancy Config Proof

Roadmap: [`../006-underlay-and-acowtancy-config-migration-proof.md`](../006-underlay-and-acowtancy-config-migration-proof.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Prove the Effigy secret/config model against Underlay and Acowtancy without
adding app-specific behavior to Effigy.

## Scope

- audit Acowtancy `.env` and local configuration files
- classify values into ordinary config, generated runtime config, true secrets,
  and legacy compatibility
- declare true secrets in Acowtancy `[secrets]`
- move ordinary local config toward Underlay-approved config surfaces
- document the Underlay source-of-truth convention for bundle apps
- update Acowtancy docs with the local setup and secret injection workflow
- validate `effigy secrets doctor` and the local container path where feasible

## Non-Goals

- no Acowtancy-specific logic in Effigy
- no production secret migration
- no provider-hosted secret creation
- no forced removal of every legacy `.env` compatibility path before the proof
  is stable

## Acceptance

- Acowtancy has a documented config/secrets split
- Underlay docs describe the standard local config and secret declaration
  pattern for Underlay bundle apps
- Acowtancy declares required true secrets in Effigy config
- no required secret values are added to committed files
- any remaining `.env` use is documented as compatibility or non-secret config

## Completed

- Audited Acowtancy root, Farmyard, Cream, and Dairy env/config surfaces without
  printing local secret values.
- Added Acowtancy root `[secrets]` declarations for Farmyard runtime, media
  state/artifact, migration, and Render deploy credentials.
- Kept Acowtancy secret declarations optional during the bridge window so
  existing local tasks do not break before each developer initialises a vault.
- Documented the Acowtancy config/secrets split in the Ledger operator docs.
- Linked the new Acowtancy operator doc from the root README and state/deploy
  runbook.
- Updated Underlay config, contract, and state/Effigy policy docs so Underlay
  remains the source of truth for consuming apps.

## Validation Notes

- `git diff --check` passed in Acowtancy.
- `git diff --check` passed in Underlay.
- Acowtancy `effigy secrets doctor` parsed 14 declarations and correctly
  blocked because `.effigy/secrets/local.vault` has not been initialised.
- Local container startup with vault injection was not run because no local
  Acowtancy vault exists yet; the documented next operator step is
  `effigy secrets init`.

## Validation

- Acowtancy `effigy secrets doctor`
- Acowtancy local container startup check where available
- grep/audit checks for known secret names in committed files
- Underlay docs validation
- Effigy docs path checks for any touched Effigy docs

## Next Task

Execute `720` to decide Varlock adapter or deferral.
