# Dependability Proof Matrix First Slice

Date: 2026-05-05

## Summary

Completed card `399`.

## Decision

The first proof slice is DecodeLabs mysql seed execution through Rhai
`exec::run(...)`.

## Rationale

This directly targets the failure mode where a Rhai script guesses incorrectly
about host/container execution and loses path correctness. The proof will keep
the SQL seed path structured as `stdin_file` and require container execution.

## Next

Implement card `400`.
