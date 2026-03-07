# Research Batch 20.1: Track 01 Completion

Date: 2026-03-07
Roadmap: g01.020
Batch: 20.1

## Summary

Completed Batch 20.1 of Research Phase 1 (Core Execution). Three tool dossiers and Track 01 value track synthesis completed.

## Deliverables

### Tool Dossiers (3)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [Make](../../research/tool-dossiers/make.md) | Complete | Tab sensitivity is #1 pain point; custom syntax creates learning barrier |
| [Just](../../research/tool-dossiers/just.md) | Complete | Built-in help is essential; custom syntax still a barrier; no caching |
| [Task](../../research/tool-dossiers/task.md) | Complete | YAML is verbose; Go templates add complexity; DAG execution is valuable |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 01: Task Configuration Formats](../../research/value-tracks/01-task-configuration-formats.md) | Complete | TOML validated as correct choice for Effigy |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [001: TOML Configuration Validation](../../research/translation-memos/001-toml-configuration-validation.md) | Complete | Promote to concept work |

## Key Findings

### TOML Validation

Comparative analysis confirms Effigy's TOML choice is correct:

1. **Custom syntax** (Make, Just): Creates learning barriers, limited tooling
2. **YAML** (Task): Verbose, indentation-sensitive, template complexity
3. **TOML**: Balanced, human-friendly, Rust-ecosystem native

### Patterns to Adopt

- **Built-in help** (`just --list`, `effigy tasks`): Essential discoverability
- **No tab/whitespace sensitivity**: Avoid Make's mistake
- **Simple interpolation**: Effigy's `{var}` > Task's `{{.VAR}}`
- **Explicit over implicit**: No magic rules (reject Make's implicit rules)

### Patterns to Reject

- Tab-sensitive syntax
- Custom syntax (learning barrier)
- Go template complexity in configs
- Implicit/magic behavior

## Evidence Quality

| Source Type | Count | Confidence |
|-------------|-------|------------|
| Official documentation | 8 | high |
| Source code repos | 3 | high |
| Academic/industry papers | 2 | high |
| Community issues/discussions | 3 | medium |

## Next Batch

**Batch 20.2**: Track 02 — Caching Strategies

Tools to study:
- Bazel (content-addressable caching)
- Turbo (incremental builds)
- sccache (compiler caching)

## Acceptance Criteria

- [x] 3 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] TOML choice validated with evidence

## Outcome

Batch 20.1 complete. Track 01 validated that TOML is the correct configuration format choice for Effigy. Ready to proceed to Batch 20.2 (Caching Strategies).

