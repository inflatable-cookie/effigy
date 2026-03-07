# Discovery Triage Log

Staging area for signals from secondary channels awaiting promotion to the research corpus.

---

## Template Example (Delete after first real entry)

### [Tool Name / Signal Title]

- Source channel: Hacker News / lobste.rs / Twitter / etc.
- Date triaged: 2026-03-07
- Claim: This new tool claims to be 10x faster than Make for large projects
- Primary source: [link to GitHub repo, or "missing — need benchmark methodology"]
- Effigy relevance: Track 2 (Caching Strategies), Track 5 (Process Management)
- Outcome: watch
- Reason: Interesting performance claims but only v0.3, no production usage evidence
- Review trigger: Re-triage when tool reaches v1.0 or published benchmarks with methodology

---

## Batch 22.2: Track 12 - CI/CD Integration

**Date:** 2026-03-07  
**Tools studied:** GitHub Actions, pre-commit  
**Track:** 12 - CI/CD Integration  

**Deliverables created:**
- Tool dossier: `tool-dossiers/github-actions.md`
- Tool dossier: `tool-dossiers/pre-commit.md`
- Value track: `value-tracks/12-ci-cd-integration.md`
- Translation memo: `translation-memos/012-ci-cd-integration.md`

**Key findings:**
- GitHub Actions: YAML workflows, event-driven, vendor lock-in, excellent ecosystem
- pre-commit: Multi-language hooks, configuration-driven, Python dependency
- Common pattern: Configuration drift between local and CI environments
- Opportunity: Single source of truth (effigy.toml) driving all environments

**Outcome:** research processed  
**Next:** Track 13 (IDE Integration)

---

## Batch 22.3: Track 13 - IDE Integration

**Date:** 2026-03-07  
**Tools studied:** VS Code Tasks, cargo IDE integration  
**Track:** 13 - IDE and Editor Integration  

**Deliverables created:**
- Tool dossier: `tool-dossiers/vscode-tasks.md`
- Tool dossier: `tool-dossiers/cargo-ide-integration.md`
- Value track: `value-tracks/13-ide-integration.md`
- Translation memo: `translation-memos/013-ide-integration.md`

**Key findings:**
- VS Code Tasks: Flexible but requires configuration, problem matchers for error parsing
- cargo: Excellent JSON output, error codes, precise spans, rust-analyzer integration
- Common pattern: Machine-readable output essential for IDE integration
- Opportunity: Standard JSON interfaces (`--list --format json`, `--format json`)

**Outcome:** research processed  
**Next:** Track 14 (Plugin Architecture)

---

## Batch 22.4: Track 14 - Plugin Architecture

**Date:** 2026-03-07  
**Tools studied:** ESLint plugins, Bazel rules  
**Track:** 14 - Plugin and Extension Architecture  

**Deliverables created:**
- Tool dossier: `tool-dossiers/eslint-plugins.md`
- Tool dossier: `tool-dossiers/bazel-rules.md`
- Value track: `value-tracks/14-plugin-architecture.md`
- Translation memo: `translation-memos/014-plugin-architecture.md`

**Key findings:**
- ESLint: Simple function API enables large ecosystem; config hell is a risk
- Bazel: Powerful but steep learning curve; hermetic builds are valuable
- Pattern: Simple APIs beat powerful APIs for adoption
- Opportunity: Task templates + lifecycle hooks for simple extensibility

**Outcome:** research processed  
**Next:** Track 15 (Telemetry) - Final Phase 3 track

---

## Batch 22.5: Track 15 - Telemetry

**Date:** 2026-03-07  
**Tools studied:** Homebrew analytics, VS Code telemetry  
**Track:** 15 - Telemetry and Observability  

**Deliverables created:**
- Tool dossier: `tool-dossiers/homebrew-analytics.md`
- Tool dossier: `tool-dossiers/vscode-telemetry.md`
- Value track: `value-tracks/15-telemetry-and-observability.md`
- Translation memo: `translation-memos/015-telemetry-and-observability.md`

**Key findings:**
- Homebrew: Opt-out with easy disable, public dashboards, anonymous data
- VS Code: Granular controls, multi-channel telemetry, detailed documentation
- Pattern: Transparency + easy opt-out builds trust
- Recommendation: First-run prompt instead of opt-out default, anonymous by design, self-hosted

**Outcome:** research processed  
**Next:** Research program complete - transition to implementation phase

---

## watch

*Items waiting for a review trigger. Empty on initial creation.*

---

## lead only

*Items missing primary sources. Empty on initial creation.*

---

## reject

*Items excluded with reason. Empty on initial creation.*

---

## research now (pending processing)

*Items approved for research but not yet processed. Empty on initial creation.*

