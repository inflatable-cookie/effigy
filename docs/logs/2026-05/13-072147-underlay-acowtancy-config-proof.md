# Underlay Acowtancy Config Proof

Completed `719`.

Changes:

- added Acowtancy root `[secrets]` declarations for Farmyard runtime, media
  state/artifact, migration, and Render deploy credentials
- documented Acowtancy's config/secrets split in the Ledger operator docs
- linked the new Acowtancy policy from the root README and state/deploy runbook
- updated Underlay config and migration policy docs so Underlay remains the
  source of truth for consuming apps
- opened `720` as the next ready card for the Varlock adapter/deferral decision

Validation:

- Acowtancy `git diff --check`
- Underlay `git diff --check`
- Acowtancy `effigy secrets doctor` parsed the declarations and blocked on the
  expected missing local vault

Follow-up:

- Acowtancy operators should run `effigy secrets init` before tightening bridge
  declarations to `required = true`.
