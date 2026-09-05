# Deps CLI JSON Foundation Closeout

Status: complete
Created: 2026-08-05
Roadmap: `g08.019`
Batch: `1053`

## Summary

Exposed the read-only dependency domain through `effigy deps`, completed its
JSON/help/completion contract, and closed the foundation milestone before
Cargo mutation begins.

## Changes

- added bare `deps` and `deps status [cargo|bun]` over one report path
- reserved and parsed `deps link|unlink <manager> <path> [--dry-run]` while
  keeping both operations explicitly non-mutating
- wired repo targeting, global JSON, command labels, dispatch, help, completion,
  built-in inventory, and deferral collision handling
- added deterministic text output and `effigy.deps.status.v1` inside the
  standard `effigy.command.v1` envelope
- added parser, help, runner, integration, selection-index, JSON example, and
  command-reference coverage
- promoted Cargo planning card `1054` as the strict lane's only ready work

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: dependency-link state was library-only -> operators and agents can
  now inspect the same typed state through stable text and JSON commands
- Remaining gap: Cargo/Bun mutation, doctor hygiene, and portfolio proof remain
  in `g08.020` through `g08.023`

## Validation Performed

- `cargo test -p effigy-cli`
  - result: 10 tests passed
- `cargo test -p effigy-deps`
  - result: 24 tests and doc tests passed
- focused parser, help, runner, and CLI JSON tests
  - result: passed
- `effigy qa:ci:json`
  - result: all 23 selected command contracts passed, including deps status
- `effigy qa:ci:fast`
  - result: 1,618 tests passed, 1 skipped; compatibility and JSON checks passed
- `cargo clippy --all-targets -- -D warnings`
  - result: passed
- `effigy qa:docs`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- `link` and `unlink` are intentionally unavailable until their manager
  adapters can plan, apply, and verify the full closure safely
- Cargo lockfile and doctor hygiene remain later milestones, not status claims
  in the foundation command

## Next Task

Execute ready batch card `1054`.
