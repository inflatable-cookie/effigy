# Docs Policy Task-Chain Closeout

Date: 2026-03-12
Owner: Platform

## Summary

The remaining active docs-policy shell wrapper has been removed.

Current state:

- generic engines now exist as built-ins:
  - `effigy docs check-headings`
  - `effigy docs check-contains`
  - `effigy docs check-index --policy-index ...`
  - `effigy docs check-next-action --policy ...`
  - `effigy docs check-workflow-paths`
- repo-specific docs policy now lives visibly in `effigy.toml` task composition
  under `qa:docs:vision`
- fixture-style negative next-action cases live in Rust CLI tests instead of a
  shell regression harness

Deleted entrypoints in this closeout:

- `docs/scripts/check-vision-metadata.sh`
- `docs/scripts/check-vision-next-task-regression.sh`

## Vision Target Delta

- Primary tags touched: `CONTRACT`, `MAINT`, `OPERATE`
- Moved from `one remaining docs-policy wrapper plus native engines` to
  `fully visible manifest task composition over native docs validators`
- Remains open: historical logs and older roadmap artifacts still mention the
  removed scripts as evidence of the path that existed at the time

## Result

`qa:docs:vision` is now the canonical policy bundle for this repo, and it is
composed entirely from native Effigy commands:

- roadmap heading requirements
- guide heading requirements
- report-policy substring checks
- cutoff-date substring checks
- workflow-path validation
- vision index validation
- next-action validation

This preserves the generic-engine vs repo-policy boundary:

- built-ins provide reusable validators
- `effigy.toml` expresses this repo's doctrine
- no dedicated docs-policy bash wrapper is required anymore
