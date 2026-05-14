# g05 Roadmaps

Status: Active
Theme: Secret and local configuration management plus post-release ownership cleanup

## Purpose

`g05` makes Effigy a safer operator surface for agent-heavy development by
splitting ordinary config from true secrets and adding a portable, human-gated
secret management model.

The generation starts from the Underlay and Acowtancy problem: `.env` files
contain too much non-secret configuration, while real credentials need stronger
handling than plaintext files can provide.

Final posture from `g05.001` through `g05.007`: `[secrets]` plus the built-in
vault is the supported local secret path. `.env.schema` remains
validation/task-env compatibility. Varlock is deferred as a live backend
adapter.

`g05` is now reopened for the next post-release cleanup tranche because the new
work is directly coupled to the just-landed secret/runtime/container/Rhai
surfaces and does not justify a generation rollover yet.

## Roadmap Sequence

- [`001-secret-and-local-config-contract.md`](./001-secret-and-local-config-contract.md) (complete)
- [`002-secret-manifest-and-doctor-surface.md`](./002-secret-manifest-and-doctor-surface.md)
- [`003-local-encrypted-vault.md`](./003-local-encrypted-vault.md)
- [`004-task-rhai-and-deploy-secret-injection.md`](./004-task-rhai-and-deploy-secret-injection.md)
- [`005-container-secret-injection.md`](./005-container-secret-injection.md)
- [`006-underlay-and-acowtancy-config-migration-proof.md`](./006-underlay-and-acowtancy-config-migration-proof.md)
- [`007-varlock-adapter-and-closeout.md`](./007-varlock-adapter-and-closeout.md)
- [`008-post-release-reference-grade-follow-through-suite.md`](./008-post-release-reference-grade-follow-through-suite.md)
- [`009-state-command-thin-shell-follow-through.md`](./009-state-command-thin-shell-follow-through.md)
- [`010-shared-secrets-vault-access-boundary.md`](./010-shared-secrets-vault-access-boundary.md)
- [`011-container-lifecycle-owner-split.md`](./011-container-lifecycle-owner-split.md)
- [`012-rhai-internal-boundary-follow-through.md`](./012-rhai-internal-boundary-follow-through.md)
- [`013-cli-help-topic-descriptor-convergence.md`](./013-cli-help-topic-descriptor-convergence.md)
- [`014-area-local-test-builder-cleanup.md`](./014-area-local-test-builder-cleanup.md)
- [`015-active-docs-reference-refresh-and-g05-closeout.md`](./015-active-docs-reference-refresh-and-g05-closeout.md)

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
712-open-task-rhai-deploy-secret-injection-lane.md
713-add-task-secret-injection.md
714-add-rhai-secret-api.md
715-add-deploy-state-artifact-secret-injection.md
716-close-task-rhai-deploy-secret-injection.md
717-add-container-secret-injection.md
718-add-compat-env-export.md
719-migrate-underlay-acowtancy-config-proof.md
720-decide-varlock-adapter-or-deferral.md
721-close-g05-secret-management-suite.md
```

## Current State

`g05.001` through `g05.015` are complete. Strict lanes `076`, `077`, `078`,
`079`, `080`, and `081` are closed.

Completed cards:

- [`702-audit-env-config-secret-boundaries.md`](./batch-cards/702-audit-env-config-secret-boundaries.md)
- [`703-add-secrets-manifest-parser.md`](./batch-cards/703-add-secrets-manifest-parser.md)
- [`704-add-secrets-list-and-doctor.md`](./batch-cards/704-add-secrets-list-and-doctor.md)
- [`705-close-secret-manifest-doctor-surface.md`](./batch-cards/705-close-secret-manifest-doctor-surface.md)
- [`706-open-local-encrypted-vault-lane.md`](./batch-cards/706-open-local-encrypted-vault-lane.md)
- [`707-add-secret-domain-and-vault-file-model.md`](./batch-cards/707-add-secret-domain-and-vault-file-model.md)
- [`708-add-vault-crypto-round-trip.md`](./batch-cards/708-add-vault-crypto-round-trip.md)
- [`709-add-secrets-init-set-unset.md`](./batch-cards/709-add-secrets-init-set-unset.md)
- [`710-add-secrets-unlock-lock-and-doctor-vault-diagnostics.md`](./batch-cards/710-add-secrets-unlock-lock-and-doctor-vault-diagnostics.md)
- [`711-close-local-encrypted-vault.md`](./batch-cards/711-close-local-encrypted-vault.md)
- [`712-open-task-rhai-deploy-secret-injection-lane.md`](./batch-cards/712-open-task-rhai-deploy-secret-injection-lane.md)
- [`713-add-task-secret-injection.md`](./batch-cards/713-add-task-secret-injection.md)
- [`714-add-rhai-secret-api.md`](./batch-cards/714-add-rhai-secret-api.md)
- [`715-add-deploy-state-artifact-secret-injection.md`](./batch-cards/715-add-deploy-state-artifact-secret-injection.md)
- [`716-close-task-rhai-deploy-secret-injection.md`](./batch-cards/716-close-task-rhai-deploy-secret-injection.md)
- [`717-add-container-secret-injection.md`](./batch-cards/717-add-container-secret-injection.md)
- [`718-add-compat-env-export.md`](./batch-cards/718-add-compat-env-export.md)
- [`719-migrate-underlay-acowtancy-config-proof.md`](./batch-cards/719-migrate-underlay-acowtancy-config-proof.md)
- [`720-decide-varlock-adapter-or-deferral.md`](./batch-cards/720-decide-varlock-adapter-or-deferral.md)
- [`721-close-g05-secret-management-suite.md`](./batch-cards/721-close-g05-secret-management-suite.md)

## Next Task

No active `g05` task right now. The reopened cleanup suite is closed, but the
generation remains open for future work.
