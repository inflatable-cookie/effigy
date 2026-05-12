# g05 Roadmaps

Status: Active
Theme: Secret and local configuration management

## Purpose

`g05` makes Effigy a safer operator surface for agent-heavy development by
splitting ordinary config from true secrets and adding a portable, human-gated
secret management model.

The generation starts from the Underlay and Acowtancy problem: `.env` files
contain too much non-secret configuration, while real credentials need stronger
handling than plaintext files can provide.

## Roadmap Sequence

- [`001-secret-and-local-config-contract.md`](./001-secret-and-local-config-contract.md) (complete)
- [`002-secret-manifest-and-doctor-surface.md`](./002-secret-manifest-and-doctor-surface.md)
- [`003-local-encrypted-vault.md`](./003-local-encrypted-vault.md)
- [`004-task-rhai-and-deploy-secret-injection.md`](./004-task-rhai-and-deploy-secret-injection.md)
- [`005-container-secret-injection.md`](./005-container-secret-injection.md)
- [`006-underlay-and-acowtancy-config-migration-proof.md`](./006-underlay-and-acowtancy-config-migration-proof.md)
- [`007-varlock-adapter-and-closeout.md`](./007-varlock-adapter-and-closeout.md)

## Execution Rule

Each roadmap should open strict batch cards only when implementation starts.

Do not implement vault crypto, manifest parsing, or runtime injection until
`g05.001` has promoted the contract and closed the exact safety boundary.

## Batch Card Shape

Recommended first cards:

```text
700-open-secret-config-generation-lane.md
701-promote-secret-config-contract.md
702-audit-env-config-secret-boundaries.md
703-add-secrets-manifest-parser.md
704-add-secrets-list-and-doctor.md
705-close-secret-manifest-doctor-surface.md
706-open-local-encrypted-vault-lane.md
707-add-secret-domain-and-vault-file-model.md
708-add-vault-crypto-round-trip.md
709-add-secrets-init-set-unset.md
710-add-secrets-unlock-lock-and-doctor-vault-diagnostics.md
711-close-local-encrypted-vault.md
712-add-task-and-rhai-secret-injection.md
713-add-deploy-provider-secret-injection.md
714-add-container-secret-injection.md
715-add-compat-env-export.md
716-migrate-underlay-acowtancy-config-proof.md
717-decide-varlock-adapter-or-deferral.md
718-close-g05-secret-management-suite.md
```

## Current State

`g05.001` and `g05.002` are complete. Strict lanes `076` and `077` are closed.
Strict lane `078` is open for `g05.003`.

Completed cards:

- [`702-audit-env-config-secret-boundaries.md`](./batch-cards/702-audit-env-config-secret-boundaries.md)
- [`703-add-secrets-manifest-parser.md`](./batch-cards/703-add-secrets-manifest-parser.md)
- [`704-add-secrets-list-and-doctor.md`](./batch-cards/704-add-secrets-list-and-doctor.md)
- [`705-close-secret-manifest-doctor-surface.md`](./batch-cards/705-close-secret-manifest-doctor-surface.md)
- [`706-open-local-encrypted-vault-lane.md`](./batch-cards/706-open-local-encrypted-vault-lane.md)
- [`707-add-secret-domain-and-vault-file-model.md`](./batch-cards/707-add-secret-domain-and-vault-file-model.md)
- [`708-add-vault-crypto-round-trip.md`](./batch-cards/708-add-vault-crypto-round-trip.md)
- [`709-add-secrets-init-set-unset.md`](./batch-cards/709-add-secrets-init-set-unset.md)

Ready card:

- [`710-add-secrets-unlock-lock-and-doctor-vault-diagnostics.md`](./batch-cards/710-add-secrets-unlock-lock-and-doctor-vault-diagnostics.md)

## Next Task

Execute `710` to add unlock/lock and doctor vault diagnostics.
