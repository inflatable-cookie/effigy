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

## Batch 23.1: Track 16 - Secure Secrets Management

**Date:** 2026-03-07  
**Tools studied:** Varlock, Mozilla SOPS, git-crypt, age, Doppler  
**Track:** 16 - Secure Secrets Management (user-requested)  

**Deliverables created:**
- Tool dossier: `tool-dossiers/varlock.md`
- Tool dossier: `tool-dossiers/mozilla-sops.md`
- Tool dossier: `tool-dossiers/git-crypt.md`
- Tool dossier: `tool-dossiers/age-encryption.md`
- Tool dossier: `tool-dossiers/doppler.md`
- Value track: `value-tracks/16-secure-secrets-management.md`
- Translation memo: `translation-memos/016-secure-secrets-management.md`

**Key findings:**
- Varlock: Schema validation + external providers, but doesn't encrypt (yet)
- SOPS: Industry standard for encrypted files in git, complex key management
- git-crypt: Transparent git encryption, but GPG complexity and maintenance mode
- age: Modern encryption, simple UX, no configuration - best default choice
- Doppler: Good UX but cloud-only, subscription cost, vendor lock-in

**Recommendation for Effigy:**
- Default: age-based encryption (modern, simple, self-hosted)
- Optional: SOPS integration for enterprise/KMS users
- Optional: External provider integration (1Password, Bitwarden)
- Add: Schema validation inspired by Varlock

**Outcome:** research processed  
**Next:** Implementation phase

---

## Refined Recommendation: External Provider Focus

**Date:** 2026-03-07  
**Based on:** User feedback from Track 16 research  

**Original recommendation:** age-based file encryption (git-committable)  
**Refined recommendation:** External provider integration (1Password, Bitwarden, Infisical)

**Rationale:**
- Most developers already use password managers
- No key management needed (provider handles it)
- Centralized secret rotation
- Simpler team onboarding (grant vault access vs share keys)
- No copies of secrets in repo or filesystem

**New deliverable:**
- Translation memo: `translation-memos/016b-external-provider-secrets.md`

**Recommended providers (in order):**
1. **1Password** - Best UX, most popular, biometric unlock
2. **Bitwarden Secrets Manager** - Free tier, open source
3. **Infisical** - Open source, self-hostable

---

## Final Recommendation: Custom @env-spec Implementation

**Date:** 2026-03-07  
**Based on:** Deeper analysis of Varlock architecture + in-process requirements  

**Original refined recommendation:** Integrate Varlock as external dependency  
**Final recommendation:** Implement custom @env-spec parser/resolver in Rust

**Rationale:**
- Varlock is JS/Node-based - spawning processes for every task is slow
- Effigy needs **in-process access** to env vars for conditional logic
- Full control over security (zeroization, timeouts, caching)
- No external binary dependency
- @env-spec is a well-designed DSL we can implement ourselves

**Implementation strategy:**
- Custom Rust parser for @env-spec (using nom)
- Resolution engine for `exec()`, `env()`, templates
- `SecretString` type with `zeroize::Zeroize`
- Integration with Effigy runtime for task execution

**New deliverables:**
- Translation memo: `translation-memos/016c-varlock-integration.md`
- Implementation handoff: `varlock-integration-implementation` (handoff deleted at
  closeout; see git history before 2026-09-05)

**Status:** Research complete, ready for implementation

---

## watch

*Items waiting for a review trigger.*

---

## lead only

*Items missing primary sources.*

---

## reject

*Items excluded with reason.*

---

## research now (pending processing)

*Items approved for research but not yet processed.*

