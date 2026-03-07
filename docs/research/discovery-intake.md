# Discovery Intake and Frontier Triage

Purpose: define how low-authority secondary channels feed into the research program without polluting the primary-source corpus.

## Why This Exists

The research program's source hierarchy requires official docs, release notes, source trees, and technical talks before secondary commentary. But secondary channels surface signals faster than primary-source indexing — new tools, feature announcements, performance comparisons, and workflow insights often appear on Twitter, Hacker News, or Discord before formal documentation. Without an intake process, the program either misses timely signals or absorbs unvetted claims into the corpus.

## Discovery Channel Registry

### Tier A — Curated Aggregators (weekly check cadence)

These channels have editorial judgment and consistent track records of linking to primary sources:

| Channel | Signal Type | Timeliness | Primary Failure Mode |
|---------|-------------|------------|---------------------|
| Hacker News (Show HN, tooling threads) | new tool announcements, performance claims | 0-3 days | hype cycles, shallow evaluation |
| lobste.rs (tag: cli, rust, devtools) | curated dev tool discussion | 0-7 days | smaller community, niche focus |
| Console.dev (developer tools newsletter) | new tool launches, reviews | 1-2 weeks | editorial selection bias |
| Changelog podcasts/newsletter | OSS project updates | 1-2 weeks | audio format, limited depth |
| Rust Weekly / This Week in Rust | Rust ecosystem tools | 1 week | Rust-only focus |

### Tier B — Production Testing and Analysis (event-driven check)

These channels provide empirical evidence about shipped implementations:

| Channel | Signal Type | Timeliness | Primary Failure Mode |
|---------|-------------|------------|---------------------|
| Martin Fowler's bliki | build system patterns, CI/CD | months | high-level, not tool-specific |
| Charity Majors / Honeycomb blog | observability, developer experience | weeks | observability lens |
| Dan Luu blog | performance analysis | months | deep but sporadic |
| Earthly blog (comparative) | build tool comparisons | weeks | vendor content |
| Temporal / Serverless.com blogs | workflow patterns | weeks | platform-specific |

### Tier C — Technical Explainers (as-needed reference)

These channels provide implementation-level education, not discovery. Use them as entry points when a topic needs background, not as signal sources:

| Channel | Signal Type | Timeliness | Primary Failure Mode |
|---------|-------------|------------|---------------------|
| Fireship (YouTube) | quick tool overviews | weeks behind | superficial coverage |
| Traversy Media | tutorial content | months behind | beginner-focused |
| System Design Primer | architectural patterns | not event-driven | theoretical |
| Various "X vs Y" blog posts | comparison tables | varies | often outdated, vendor-biased |

### Tier D — Community Forums and Chat (ephemeral signal, never cite directly)

These channels surface practitioner reactions and implementation feedback but are ephemeral and uncurated:

| Channel | Signal Type | Timeliness | Primary Failure Mode |
|---------|-------------|------------|---------------------|
| r/rust, r/programming, r/devops | discussion, frustration signals | same-day | extreme noise, recency bias |
| Twitter/X devtools community | hot takes, announcements | same-day | hype, vendor marketing |
| Discord (Rust, various OSS) | implementation questions | same-day | ephemeral, not searchable |
| GitHub Discussions (various tools) | user feedback | ongoing | specific to project |
| Stack Overflow | usage patterns, problems | ongoing | problem-biased |

### Tier E — Conference Recap Channels (post-conference check)

These are secondary summaries of conferences that also have primary source archives:

| Channel | Signal Type | Primary Source Archive |
|---------|-------------|----------------------|
| RustConf / Rust Belt Rust recaps | Rust ecosystem tools | conference recordings, slides |
| QCon / GOTO conference blogs | industry patterns | InfoQ recordings |
| KubeCon / CloudNativeCon | CI/CD, build infrastructure | CNCF recordings |
| FOSDEM (devtools track) | open source tooling | video archive |

## Triage Rules

Every signal from a secondary channel must be triaged before it can enter the research corpus. Triage produces exactly one outcome per signal.

### Triage Outcomes

| Outcome | Meaning | What Happens Next |
|---------|---------|-------------------|
| `research now` | primary source exists, Effigy-relevant, strong enough to enter a value track or memo | trace to primary source, add to relevant source map and value track |
| `lead only` | interesting signal but primary source is missing, incomplete, or unverified | record in the triage log with the claim and the missing primary source; do not add to corpus |
| `watch` | credible primary source exists but the technique is too early, too niche, or too uncertain to act on | record in the triage log with the primary source and a review trigger condition |
| `reject` | not Effigy-relevant, or the claim does not survive primary-source tracing | record in the triage log with the reason for rejection; do not add to corpus |

### Triage Decision Tree

1. **Does a primary source exist?** (docs, release notes, source tree, first-party writeup)
   - No → `lead only` (record what the claim is and what source is missing)
   - Yes → continue

2. **Is the tool/signal Effigy-relevant?** (does it address a problem in one of the value tracks?)
   - No → `reject`
   - Yes → continue

3. **Is the primary source strong enough?** (tier 1-2 in the source hierarchy)
   - No → `lead only` (record the weak source and what would strengthen it)
   - Yes → continue

4. **Is the technique ready for Effigy action?** (shipping in production, has users, or has multiple independent evaluations)
   - No → `watch` (record the tool/signal, primary source, and what would make it ready: v1.0 release, production usage report, performance benchmarks, etc.)
   - Yes → `research now`

### Triage Quality Rules

- Never add a `lead only` or `watch` item directly to a value track or translation memo. These stay in the triage log until they are promoted through re-triage.
- Never cite a secondary channel as a source in the research corpus. Always trace to the primary source.
- When a `watch` item's review trigger fires (e.g., a tool reaches v1.0, a benchmark study publishes), re-triage it.
- When a `lead only` item's missing primary source appears, re-triage it.
- Triage is not permanent — items can move between outcomes as evidence changes.

## Triage Log Format

Each triage entry records:

```markdown
## [Signal Title]

- Source channel: [which secondary channel surfaced this]
- Date triaged: [YYYY-MM-DD]
- Claim: [what the signal claims, in one sentence]
- Primary source: [link to primary source if found, or "missing" with what would constitute one]
- Effigy relevance: [which value track(s) this relates to, or "none"]
- Outcome: [research now | lead only | watch | reject]
- Reason: [one sentence explaining the triage decision]
- Review trigger: [for watch items only — what event would cause re-triage]
```

## Check Cadence

| Channel Tier | Check Frequency | Who |
|--------------|-----------------|-----|
| Tier A (curated aggregators) | weekly | research session |
| Tier B (production testing) | event-driven (major releases, conference season) | research session |
| Tier C (technical explainers) | as-needed when a topic requires background | research session |
| Tier D (community forums) | never systematically; only when a specific signal is reported | research session |
| Tier E (conference recaps) | post-conference (RustConf, FOSDEM, etc.) | research session |

## Integration with Research Corpus

- `research now` items get added to the relevant source map and value track following normal research batch procedures.
- `lead only` items stay in the triage log until their primary source appears.
- `watch` items stay in the triage log until their review trigger fires.
- `reject` items stay in the triage log permanently as a record of what was considered and why it was excluded.
- The triage log is not a value track — it is a staging area. Nothing in the triage log is citable by concept work or roadmaps.

## Next Task

Run the initial triage pass on current secondary signals to populate the triage log and validate the intake process. Focus on:

1. Recent task runner announcements on Hacker News
2. Rust ecosystem tooling in This Week in Rust
3. Comparative benchmarks on build systems
4. Any existing GitHub issues or discussions mentioning competing tools

