# 002 - Manifest Composition and Override Contract

Generation: `g02`

Status: In Progress
Owner: Platform
Created: 2026-04-11
Depends on: 027, 028

## Vision Alignment

Effigy currently treats `effigy.toml` as one root manifest file. That keeps the
entrypoint simple, but it pushes larger repos toward one of two bad outcomes:

- oversized root manifests with unrelated concerns jammed together
- feature-specific escape hatches where one surface invents its own external
  file loading semantics

The next product cycle needs one general composition model for repo-owned
configuration so features can scale without each inventing a different include
story.

This roadmap defines that general model.

## Primary Tags

- `CONTRACT`
- `ROUTE`
- `OPERATE`
- `MAINT`

## Target Envelope

- Effigy keeps `effigy.toml` as the canonical repo entrypoint.
- Effigy gains one first-class composition mechanism for loading additional
  manifest fragments.
- Composition remains feature-agnostic: tasks, docs policy, release config,
  env schema, demos, and future surfaces all use the same rules.
- Ordered composition and explicit override behavior are defined, inspectable,
  and failure-first.
- Feature design can assume both inline and split-file config without inventing
  feature-local import semantics.

## Vision Target Delta

- Move from `one root file or feature-specific external config escape hatches`
  toward `one canonical root manifest with a reusable composition and override
  model`.

## 1) Problem

Large or multi-owner repos need to split configuration for maintainability, but
the split must not become a semantic function of whichever feature happened to
need it first.

If demos get a custom scenario loader, release later gets a separate import
story, and docs policy eventually gets another include mechanism, Effigy stops
having a coherent manifest model.

The composition system therefore has to solve:

- how extra config files are discovered
- how paths resolve
- how multiple fragments merge
- how overrides are declared intentionally
- how conflicts fail
- how tooling explains the effective manifest

## 2) Goals

- [ ] Define one manifest composition mechanism for arbitrary Effigy config
- [ ] Keep `effigy.toml` as the root entry surface
- [ ] Support ordered inclusion of partial manifest fragments
- [ ] Define explicit override behavior rather than silent last-write wins
- [ ] Keep path resolution deterministic and local to the including file
- [ ] Detect and fail on cycles, ambiguous merges, and unsupported conflicts
- [ ] Make the effective manifest inspectable in plan/help/schema tooling
- [ ] Keep feature lanes like demos compatible with both inline and split-file
      manifest shapes

## 3) Non-Goals

- [ ] No feature-specific include/import model for demos only
- [ ] No hidden machine-global config registry
- [ ] No remote manifest composition in this lane
- [ ] No attempt to solve every future package/module boundary up front
- [ ] No silent override behavior that hides conflicts by default

## 4) Design Rules

### 4.1 Root stays canonical

`effigy.toml` remains the only required repo entrypoint. Composition extends the
manifest; it does not replace the root with a directory crawl or alternate
registry.

### 4.2 Composition is feature-agnostic

The composition layer must not know or care whether a fragment contains tasks,
docs policy, release config, demos, or future surfaces.

### 4.3 File-relative resolution

Included fragment paths should resolve relative to the file that declared the
include, not to cwd and not to a hidden workspace root.

### 4.4 Ordered and explicit

Composition order must be visible and deterministic. If later fragments can
override earlier ones, that needs to be declared as an explicit contract, not an
accidental merge side effect.

### 4.5 Conflict-first failure

When two fragments define incompatible values, Effigy should fail with a clear
merge conflict instead of inventing magical precedence.

## 5) Questions To Settle

### 5.1 Include vs require vs import

We need to decide naming and semantics:

- `include`
  - reads naturally for additive fragments
- `require`
  - reads more like a contract dependency
- `import`
  - may imply a stronger module system than we actually want

The product decision should optimize for clarity rather than novelty.

Decision for Batch `02.1`:

- use `include`
- do not introduce separate `require` or `import` semantics in the first
  contract

Reason:

- `include` reads clearly for ordered manifest composition
- missing or unreadable fragments can still fail hard without needing a second
  keyword just to mean “required”
- `import` suggests a larger module system than this lane is trying to design

### 5.2 Fragment shape

Need to choose whether included files are:

- partial manifest fragments
- or full independent manifests with root sections

Current bias: partial fragments are cleaner because they preserve one real root
and avoid pretending that nested config files are standalone repos.

Decision for Batch `02.1`:

- included files are partial manifest fragments
- they extend the effective manifest; they are not independent repo roots

### 5.3 Override model

Need an explicit override system in the contract. Areas to define:

- additive merge defaults
- which structures can merge cleanly
- when override is required
- whether override is declared at include-site, key-site, or both
- how override intent is surfaced in plan/schema tooling

This lane should assume we need overrides and design them deliberately instead
of hoping additive merge alone is enough.

Decision for Batch `02.1`:

- override intent belongs at the include-site, not inside arbitrary key/value
  leaves
- the exact override granularity and merge rules are the next bounded batch

## 6) Preferred Contract Shape

Batch `02.1` decision:

- root composition surface: `[manifest]`
- composition keyword: `include`
- fragment model: partial manifest fragments
- path resolution: relative to the file declaring the include
- nested composition: allowed, with cycle detection and failure-first behavior
- override intent: declared on include entries, not hidden inside feature data

Illustrative shape:

```toml
[manifest]
include = [
  "effigy.tasks.toml",
  "effigy.docs.toml",
  "effigy.demos.toml",
]
```

Preferred override direction for the next batch:

```toml
[manifest]
include = [
  "effigy.base.toml",
  { path = "effigy.local.toml", override = ["tasks.dev", "release.sync_files"] },
]
```

This lane has now settled the root contract direction. The next batch should
define what path-scoped override authorizes, what conflicts still fail, and how
that is explained in tooling.

## 7) Execution Plan

### Batch 02.1 - Contract Design

- [x] Define root composition section and naming
- [x] Define fragment shape and path resolution rules
- [x] Define the initial conflict-first posture and include-site override boundary
- [x] Define cycle/error behavior as failure-first, including nested include cycles

### Batch 02.2 - Override Model

- [x] Define explicit override semantics
- [x] Decide where override intent lives in config
- [x] Define additive vs replace behavior by structure class
- [x] Define diagnostics for invalid or ambiguous overrides

### Batch 02.3 - Tooling and Explainability

- [x] Define how effective-manifest inspection should work
- [x] Define doctor/schema/help visibility for composed config
- [x] Define JSON/text output expectations for composed manifests

### Batch 02.4 - Feature Compatibility Proof

- [x] Prove that at least tasks/docs/release or another cross-feature slice can
      use the same composition model cleanly
- [ ] Prove that demo planning can rely on the contract without needing a
      demo-only loader

### Batch 02.5 - First Implementation Slice

- [x] Add root and nested manifest composition loading for `effigy.toml` plus
      included partial fragments
- [x] Enforce the decided conflict/override rules during composed-manifest load
- [x] Surface composition failures through normal manifest parse/doctor paths
- [x] Add one minimal effective-manifest inspection surface under `effigy config`
- [x] Prove the first implementation against at least one cross-feature split
      such as `tasks + docs_policy` or `tasks + release`

### Batch 02.6 - Explainability and Operator UX Hardening

- [ ] Improve conflict diagnostics so they identify the path and competing
      source fragments clearly
- [ ] Improve inspection output so effective sources and overridden paths are
      easier to trace quickly
- [ ] Decide whether one narrower `effigy config` query surface is warranted
      before `g02.003` depends on composition
- [ ] Prove the explainability contract in both text and JSON output

## 8) Acceptance Criteria

- [x] Effigy has one documented root composition direction for arbitrary
      manifest content
- [x] Override behavior is explicit rather than implied
- [ ] Split-file config does not require feature-specific semantics
- [x] The design is inspectable enough that operators and agents can understand
      the effective config shape

## 9) Risks and Mitigations

- [ ] Risk: overdesigning a full module system instead of a practical
      composition contract
  - Mitigation: keep one root, partial fragments, and explicit failure rules
- [ ] Risk: silent merge behavior hides mistakes
  - Mitigation: make conflicts fail unless explicitly overridden
- [ ] Risk: demos or another feature rush ahead with bespoke loading rules
  - Mitigation: treat composition as a foundation lane and block feature-local
    import semantics

## 10) Batch `02.2` / `02.3` Decisions

### 10.1 Override Granularity

The boolean `override = true` direction is too coarse for the common case where
an included fragment only needs to replace one or two values.

Decision:

- keep override intent at the include-site
- make override path-scoped, not whole-fragment scoped
- initial override unit is a full config path, not an arbitrary inline patch

Illustrative direction:

```toml
[manifest]
include = [
  "effigy.base.toml",
  { path = "effigy.local.toml", override = ["tasks.dev", "release.sync_files"] },
]
```

This preserves one clear ownership point for override intent while avoiding the
awkwardness of replacing a whole included fragment just to change one value.

### 10.2 Merge Posture

Initial merge model:

- additive merge by default for distinct keys in tables/maps
- scalar conflicts fail unless the path is explicitly listed in `override`
- array/list conflicts fail unless the full array-valued path is explicitly
  listed in `override`
- initial overrides replace the whole addressed value at that path

Not in the first contract:

- partial array element patching
- deep inline patch expressions inside feature data
- merge folklore where some conflicts silently replace and others silently
  append without an explicit contract

### 10.3 Illegal Conflict Classes

These should still fail even when override exists:

- type-shape conflicts (`string` vs table, array vs scalar, etc.)
- override paths that do not resolve to a full manifest value boundary
- override requests for paths not present in the including fragment
- override requests that attempt array-element or mid-object patching rather
  than whole-value replacement
- nested include cycles or unreadable fragment paths

### 10.4 Explainability Contract

Minimum operator/tooling posture:

- Effigy must expose the include graph and evaluation order
- Effigy must expose the final source for each overridden path
- conflicts must name the path, the earlier source, and the later source
- text and JSON output should describe the same composition facts

The likely product surface is an extension of `effigy config`, not a second
manifest-specific app. Exact command spelling can remain implementation-batch
work so long as the explainability requirements are fixed now.

## 11) Batch `02.4` Decision: First Implementation Slice

### 11.1 Narrowest Honest First Ship

The first implementation slice should cover:

- loading `effigy.toml`
- recursively loading included partial fragments
- composing the effective manifest with the agreed conflict/override model
- failing clearly on unreadable fragments, cycles, invalid override paths, and
  illegal conflicts
- making the composed result inspectable enough that operators are not forced
  to debug merge behavior from guesswork

It should not try to ship:

- fragment generation helpers
- init/migrate support for composed manifests
- automatic fragment refactors
- broad repo migrations
- demo-specific config support

### 11.2 Minimum Inspection Surface

The first implementation must ship with one minimal effective-manifest
inspection surface under `effigy config`.

Required capabilities:

- show include graph and evaluation order
- show effective source file for resolved values
- show which paths were overridden and by which later fragment
- expose the same facts in text and JSON

The exact final command spelling can remain implementation work, but the first
implementation is under-scoped if it only loads composed manifests without
providing this inspection surface.

### 11.3 First Proof Boundary

The first proof boundary should be one synthetic or fixture-backed cross-feature
slice that demonstrates the model is not task-only.

Preferred proof:

- split `tasks` and either `docs_policy` or `release` across included fragments
- prove Effigy can load, route, and validate against the composed result
- prove the inspection surface shows the include graph and effective sources

That is enough to validate feature-agnostic composition before `g02.003`
depends on it. A real-repo migration can remain a later proof wave.

## Next Task

Use the active `g02.002` strict lane to harden composition explainability and
operator UX next, then decide whether the surface is ready for `g02.003`
planning to depend on directly.
