# PHP Graph Proof Depth

Date: 2026-05-18
Roadmap: `g07.008`
Batch card: `908`

## What Changed

- rewrote the PHP extractor to emit stronger first-party graph facts for:
  - namespaces
  - classes, interfaces, traits
  - methods and global functions
  - constants
  - imports
  - static include and require edges
  - call-site references
- added generic front-controller classification for `index.php`
- added parse-error diagnostics that keep the file indexed instead of dropping it
- hardened include resolution so static literal includes resolve to repo files when
  the target exists
- fixed semicolon-style namespace handling so later top-level declarations keep
  the active namespace

## Current State

- `908` is complete
- next ready card is `909`
- legacy PHP files now index into useful ownership and call/include facts instead
  of only raw file-level references

## Validation

- `cargo test -p effigy-codegraph`
- `cargo check -p effigy-cli -p effigy`
- `cargo test graph -- --nocapture`
- `cargo fmt --all -- --check`

## Vision Target Delta

- tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved:
  - PHP graph depth: raw calls/includes only -> namespaces/classes/methods/constants/imports/static includes
  - failure posture: parse issues could collapse utility -> warning diagnostics with retained indexing
  - legacy entry files: plain php-file -> generic front-controller classification
- remains open:
  - `909` JavaScript/TypeScript proof depth
  - `911` bounded context-pack ranking proof
  - `912` performance closeout
