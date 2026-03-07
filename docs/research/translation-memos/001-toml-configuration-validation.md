# Translation Memo 001: TOML Configuration Validation

Status: Complete
Memo: 001
Owner: Research
Last updated: 2026-03-07
Related track: Track 01 — Task Configuration Formats

## 1) Effigy problem statement

Effigy uses TOML for its configuration format (`effigy.toml`). This decision was made early in development but hadn't been systematically validated against alternatives. The research question: Is TOML the right choice compared to custom syntax (Make, Just), YAML (Task), or JSON (npm)?

## 2) External evidence summary

From comparative analysis of Make, Just, and Task:

**Make (custom syntax)**:
- Tab sensitivity causes cryptic errors (`missing separator`)
- Help generation requires boilerplate
- String manipulation functions are arcane
- Portability issues between GNU and BSD Make

**Just (custom syntax)**:
- Better than Make: no tab sensitivity, built-in help
- Still requires learning a new syntax
- Limited IDE/editor support
- No standard library (reinvent common patterns)

**Task (YAML)**:
- Familiar to many developers
- Verbose: 5 lines for what Make does in 2
- Go template syntax (`{{.VAR}}`) adds complexity
- Indentation sensitivity (though less than Make's tabs)

**Pattern observed**: Custom syntax creates adoption barriers; YAML adds verbosity and template complexity.

## 3) Recommendation

**Validate and retain TOML as Effigy's configuration format.**

TOML provides the best balance:
1. **Standard format** — Not custom syntax, parser libraries exist
2. **Human-friendly** — Less indentation-sensitive than YAML, clearer than JSON
3. **Rust ecosystem native** — Both Effigy and likely users are Rust-oriented
4. **Comment support** — Unlike JSON, supports explanatory comments
5. **Type preservation** — Distinct strings, integers, booleans, arrays, tables

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Less ubiquitous than JSON | Some users unfamiliar | Good documentation, examples |
| Not as compact as custom syntax | More typing for simple cases | Task aliases, sensible defaults |
| Rust-centric | Other ecosystems use YAML/JSON | Clear documentation helps all users |

## 5) What must be true before adoption

Already validated:
- [x] TOML parser library available (Rust `toml` crate)
- [x] Schema can be defined for validation
- [x] Supports all needed data types (strings, arrays, tables)

Recommended additions:
- [ ] Schema validation in `effigy doctor`
- [ ] Better error messages for TOML parse failures
- [ ] Migration guide from Make/Just/Task

## 6) Required prototype or validation work

**None required for format decision** — TOML is validated.

Future work (not blocking):
- Schema validation tool
- Migration helper from other formats
- IDE extension with TOML schema support

## 7) Promotion target

- [x] `concept contract work` — Document in `docs/concepts/configuration-format.md`
- [ ] `roadmap execution planning` — Schema validation is roadmap material
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable (already implemented)

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Make dossier | high | Custom syntax problems |
| Just dossier | high | Custom syntax learning curve |
| Task dossier | high | YAML verbosity, template complexity |
| TOML specification | high | Format validation |

## 9) Rejected alternatives

| Alternative | Reason for rejection |
|-------------|---------------------|
| Custom syntax (like Just) | Learning barrier, limited tooling |
| YAML (like Task) | Verbose, indentation-sensitive, template complexity |
| JSON (like npm) | No comments, rigid, limited for human writing |
| Embedded in code (like cargo xtask) | Too heavy for simple task definitions |

## Next Task

1. Document this decision in `docs/concepts/configuration-format.md`
2. Implement TOML schema validation in `effigy doctor`
3. Begin Track 02: Caching Strategies

