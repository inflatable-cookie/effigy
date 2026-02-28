# JSON Envelope Removal Release Note

Date: 2026-02-28
Owner: Platform
Related roadmap: 008 - Universal JSON Command Coverage

## Summary

Legacy JSON mode flags have been removed.

Canonical JSON mode is now:
- `--json` for command-envelope output (`effigy.command.v1`).

## Breaking Change

- Removed global flags:
  - `--json-envelope`
  - `--json-raw`
- Calling removed flags now fails with:
  - `unknown argument: --json-envelope`
  - `unknown argument: --json-raw`
  - exit code `2`

## Migration

Before:

```bash
effigy --json-envelope tasks
effigy --json-envelope doctor
effigy --json-envelope build --repo /path/to/workspace
```

After:

```bash
effigy --json tasks
effigy --json doctor
effigy --json build --repo /path/to/workspace
```

## Validation

- `cargo test -q --test cli_output_tests` passed.
- `bash ./scripts/check-json-contracts.sh --fast` passed.
- `cargo test -q` passed.
