# Product Boundary and Verify-Install SSH Closeout

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: product-boundary-and-verify-install-ssh-closeout

## Summary

Closed two remaining adoption-kit boundary items in one batch:

- defined the reusable starter shape for `qa:northstar`
- fixed `effigy release verify-install` so auto-detected SSH remotes work
  without manual URL rewriting

This moves the roadmap from `what should the starter bundle be?` to
`which remaining pieces should stay in templates versus become product
surface?`

## Changes

- normalized scp-style SSH remotes such as
  `git@github.com:owner/repo.git` into `ssh://git@github.com/owner/repo.git`
  before the `cargo install --git` step used by
  `effigy release verify-install`
- added focused release-command coverage for:
  - SSH remote normalization
  - explicit non-SSH URL pass-through
  - auto-detected `origin` SSH remote normalization
- updated the consumer contract guide to define the starter `qa:northstar`
  bundle explicitly around:
  - `effigy docs check-index`
  - `effigy docs check-next-action`
  - `effigy docs check-headings`
  - `effigy docs check-forbidden`
- recorded the product boundary decision:
  - Effigy owns generic validation engines and release/install behavior
  - the `northstar-effigy` skill/templates own repo-shape choice, starter file
    creation, and repo-specific heading/policy content
- updated the release guide to document automatic SSH remote normalization for
  `verify-install`

## Decision

The starter `qa:northstar` bundle does not need new product-specific built-ins.

The current reusable surface is already enough:

- `docs check-index`
- `docs check-next-action`
- `docs check-headings`
- `docs check-forbidden`

That means the next productization step should not be `add more validators
first`. It should be:

- package starter policy/config more cleanly
- decide whether contract scaffolding belongs in `effigy init` or stays in the
  skill/template layer
- add contract-drift checks only where the current generic engines are not
  enough

## Validation

Validated with focused coverage:

- `cargo test --lib normalize_verify_install_repo_url_rewrites_scp_style_ssh_remotes`
- `cargo test --lib normalize_verify_install_repo_url_keeps_supported_non_ssh_forms`
- `cargo test --lib resolve_verify_install_repo_url_normalizes_origin_ssh_remote`
- `cargo run --bin effigy -- docs check-links CHANGELOG.md docs/guides/051-release-orchestration.md docs/guides/056-northstar-effigy-consumer-repo-contract.md docs/logs/README.md docs/logs/2026-03/12-220500-consumer-adoption-closeout-matrix.md docs/logs/2026-03/12-223500-product-boundary-and-verify-install-ssh-closeout.md docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `cargo run --bin effigy -- docs check-index --dir docs/logs --index docs/logs/README.md`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `RELEASE`, `MAINT`
- Movement: baseline `consumer adoption was proven, but the starter validation
  bundle and release-install SSH remote behavior were still partly implicit` ->
  current `the starter `qa:northstar` bundle is explicit and released
  `verify-install` now handles auto-detected SSH remotes cleanly`
- Remaining gap: package a starter `[docs_policy]` consumer config, add any
  missing non-Effigy-repo-specific contract-drift rules, and decide whether
  scaffolding belongs in templates only or in a future `effigy init` surface

## Next Task

Use this decision to finish Wave 3 in a concrete batch: package the starter
`[docs_policy]` consumer config, prove it on one more repo or fixture without
Effigy-repo assumptions, and then make the Wave 5 call on whether any of that
should become first-class `init` or repo-contract product surface.
