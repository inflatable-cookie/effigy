# Flat Command Execution Planning

Status: complete
Created: 2026-09-02
Roadmap: g09.002
Batch: remove-executable-command-namespaces

## Summary

- The operator found the executable job namespaces overbearing after the
  g09.001 preview landed.
- General-help grouping remains useful, but it no longer defines execution
  grammar.
- Direct built-in invocation is restored as the canonical target; the preview
  aliases and migration warnings are removed before any v1 removal.

## Decisions

- Keep `effigy --help` grouping and `effigy help <group>`.
- Teach and execute `effigy deps`, `effigy version`, `effigy graph`, and the
  other direct built-ins.
- Remove executable `local`, `repo`, `deliver`, `extend`, and `admin` aliases.
- Preserve genuine command-owned subcommands and pre-preview selector
  precedence.
- Keep the g09.001 artifacts as historical evidence rather than rewriting them.

## Validation Performed

- `effigy tasks`
- `effigy doctor` — `err:0`; stale graph and seven god-file warnings only
- canonical planning, contract, architecture, preview, and open-triage review
- current namespace occurrence inventory across code, tests, docs, config, and
  both managed skill surfaces

## Risks

- Removing grouped explicit built-in escape restores the earlier shadowing
  behavior for deferred built-ins. Card 1110 cannot invent a replacement.
- Current guides and the managed Effigy skill contain many preview spellings;
  source/install parity and generated-reference checks are required.

## Next Task

Execute ready card `1110` under active spec `117` and roadmap `g09.002`.
