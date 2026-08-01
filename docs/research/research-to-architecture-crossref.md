# Research-to-Architecture Cross-Reference

Status: Active
Last updated: 2026-08-01
Purpose: Map translation memo findings to Effigy architecture, guides, and roadmap work so promotion status stays explicit.

## Gap Analysis Results

Each memo below is read against the current docs surface and classified as `Aligned`, `Partially Aligned`, `Missing`, or `Follow-up Needed`.

### Memo 001: TOML Configuration Validation

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| retain TOML as Effigy's configuration format | `docs/guides/022-manifest-cookbook.md`, `src/runner/manifest.rs` | Aligned | Format decision is already reflected in the manifest docs and implementation. |
| schema validation should be part of the product contract | `docs/contracts/`, doctor schema docs, roadmap `g01.023` | Partially Aligned | Validation direction exists but the memo's explicit recommendation still needs clearer surfaced contract and QA guidance. |
| parse failures should produce strong diagnostics | `docs/guides/023-troubleshooting-and-failure-recipes.md`, diagnostics work | Follow-up Needed | The direction exists, but research-backed parse diagnostics still need stronger consolidation. |

### Memo 002: Caching Strategy

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| content-addressable cache with local + remote tiers | roadmap `g01.020`, cache code paths | Partially Aligned | Cache work exists, but the memo remains draft and the docs do not yet present the full research-backed strategy as settled contract. |
| explicit invalidation and statistics matter | doctor/QA surfaces, cache tests | Partially Aligned | Some support exists, but operator-facing guidance is still incomplete. |
| remote caching should remain optional | docs and implementation direction | Aligned | Current direction does not force remote cache dependence. |

### Memo 003: File Watching Strategy

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| smart defaults and debounce are essential | `docs/guides/019-watch-init-migrate-foundation.md`, watch implementation | Partially Aligned | Watch UX is documented, but research-backed debounce and ignore policy still need clearer explicit contract. |
| cross-platform watch behavior should be library-backed | watch implementation, roadmap `g01.011` | Follow-up Needed | The memo is still draft; the library and behavior contract remain under active implementation. |

### Memo 004: DAG Execution Strategy

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| retain current DAG model with stronger cycle detection | `docs/architecture/000-overview.md`, roadmap `g01.010` | Partially Aligned | DAG direction is present, but the memo's specific synthesis still needs cleaner promotion into docs. |
| scheduling and cycle errors should be explicit and deterministic | diagnostics and planner code/docs | Partially Aligned | The implementation is moving in this direction; docs should reflect the research basis more directly. |

### Memo 005: TUI Patterns

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| retain multi-pane TUI with targeted refinements | `docs/architecture/011-multiprocess-tui-config-contract.md`, `docs/guides/012-dev-process-manager-tui.md` | Aligned | The current TUI docs already reflect this direction. |

### Memo 017: Apple Containers Runtime Backend

| Research Finding | Primary Docs | Alignment | Gap Description |
| --- | --- | --- | --- |
| Apple Containers is a viable native runtime candidate, not a Compose backend | `docs/architecture/020-container-infrastructure-design.md`, `docs/contracts/006-compose-backend-compatibility.md` | Aligned | The watch-only boundary is explicit; the supported backend set is unchanged. |
| backend semantics must be expressed as a typed stack plan before native execution | `docs/contracts/012-container-manager-contract.md`, `crates/effigy-catalog`, `crates/effigy-containers` | Aligned | Generated catalog assembly and native planning now share `EffectiveStackPlan`; full manager operation integration remains deliberately unclaimed. |
| service discovery, readiness, recovery, and resource cost gate adoption | `docs/research/translation-memos/017-apple-containers-runtime-backend.md` | Prototype complete | Recovery and lifecycle pass; boot-time aliases and gateway integration keep the backend watch-only. |
| direct Compose files should remain on Docker or Colima | `docs/contracts/006-compose-backend-compatibility.md` | Aligned | The candidate scope is limited to Effigy-generated catalog stacks. |

## Critical Gaps

| Gap | Related Research | Primary Area | Status |
| --- | --- | --- | --- |
| research-backed schema/parse diagnostics need clearer promotion into docs | Memo 001, Memo 007 | guides, doctor, manifest contract | Open |
| cache strategy is still draft despite active implementation and operator impact | Memo 002 | cache code, guides, roadmap | Open |
| watch strategy remains draft and needs explicit docs alignment | Memo 003 | watch implementation and guide | Open |
| Apple Containers lacks safe boot-time service aliases and complete gateway/runtime-prep integration | Memo 017 | container manager, runtime prep, upstream networking | Watch only |

## Areas Already Aligned

| Finding | Research Source | Primary Docs |
| --- | --- | --- |
| TOML is the correct configuration format | Memo 001 | manifest docs and implementation |
| multi-pane TUI remains the right operator surface | Memo 005 | architecture 011 and guide 012 |
| cross-platform considerations must stay explicit | Memo 009 | install and portability docs |

## Next Task

Update Memo 017 after an approved live prototype records pass/fail evidence for
service discovery, readiness, recovery, gateway networking, data lifecycle, and
multi-service resource cost.
