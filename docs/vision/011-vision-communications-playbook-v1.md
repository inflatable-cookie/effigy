# 011 Vision Communications Playbook v1

Status: Draft
Owner: Platform Lead + Docs Owners
Purpose: define how vision strategy should be communicated into roadmap, guides, and reports without dilution or drift.

## 1. Communication Goals

1. Keep high-level vision constraints visible in delivery artifacts.
2. Make strategic intent easy to trace from planning to release evidence.
3. Reduce interpretation drift across repositories and contributors.

## 2. Audience Map

1. `Maintainers`: need vision constraints translated into implementation priorities.
2. `Contributors`: need clear expectations for aligning PRs and docs with vision tags.
3. `Release Owners`: need concise signal on vision movement and exception risk.
4. `Operators`: need confidence that behavior remains predictable and actionable.

## 3. Translation Rules

### Vision -> Roadmap

1. Every roadmap item should identify primary vision tags.
2. Acceptance criteria should include at least one target-envelope movement.
3. Major tradeoffs should reference decision principles (`008`).

### Vision -> Guides

1. High-traffic guides should include a short "Vision Alignment" note.
2. Operator-facing guidance should preserve deterministic and actionable behavior language.
3. Terminology should remain consistent with vision canon (`012`).

### Vision -> Reports

1. Release and validation reports should include "Vision Target Delta" summaries.
2. Reports should surface risk and exception status when relevant (`004`, `005`).
3. Governance review template (`009`) should be used for cadence reporting.

## 4. Communication Cadence

1. Weekly: concise alignment notes in active implementation streams.
2. Monthly: summarized movement against key vision tags and risks.
3. Per release: explicit statement of vision target movement and unresolved exceptions.
4. Quarterly: strategic refresh of message framing and stale artifact cleanup.

## 5. Message Quality Checklist

1. Is the affected vision tag explicit?
2. Is the change framed as movement against a target envelope?
3. Are tradeoffs and exceptions named and bounded?
4. Is the language consistent with canonical terms?
5. Is owner and follow-up action clear?

## Next Task

Define a lightweight vision message template set for roadmap updates, guide revisions, and release notes.
