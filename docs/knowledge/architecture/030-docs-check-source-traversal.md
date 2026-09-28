# Docs-check source traversal

Owner: `effigy-docs-policy`

Recursive `effigy docs check` walks inspect owned Markdown. They skip
generated Cargo and package-manager output at directory entry, not after
collecting files.

## Default and explicit scope

- `docs check links` with no paths checks `README.md` (if present) and every
  Markdown file under `docs/`.
- Explicit file arguments stay in the check set, including a file that sits
  inside `target/` or `node_modules/`.
- A missing or unreadable owned Markdown file is a failure with the path and
  read error. Directory arguments stay ignored.

`docs check workflow-paths` and `docs check index` use the same walk for
their configured directories.

## Generated trees

Skip a directory named `target` or `node_modules` when it is not the walk
root. Nested trees such as `docs/packages/gpui/preview/target/` are not
scanned. A declared source root that is itself named `target` is still
walked; only nested generated directories are skipped.

A generated entry that vanishes during a parallel build is not a check
failure. The walk does not descend into those trees and ignores walk errors
whose path is under a skipped name.

## Command surface

Parser shape and kind names stay with
[021-docs-check-subcommand-consolidation-contract.md](../contracts/021-docs-check-subcommand-consolidation-contract.md).
This page owns traversal behavior only.
