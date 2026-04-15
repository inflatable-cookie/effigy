# Consumer Adoption Landscape Scan

Date: 2026-03-12
Owner: Platform

## Summary

Scanned the repositories under `~/Dev/projects` to assess
current adoption of Effigy, Northstar-style documentation structure, and
agent-facing execution contracts.

The ecosystem already has broad Effigy presence, but the full
Northstar-plus-Effigy contract is only implemented in the Effigy repo itself.
Most consuming repos have some of the pieces, but not the combined package:

- Effigy manifest present
- AGENTS contract present
- Northstar-style `docs/vision` and `docs/roadmaps`
- changelog/release discipline
- repo-owned validation for docs/contracts/release policy

## Scan Scope

- `~/Dev/projects/acowtancy`
- `~/Dev/projects/compli-me`
- `~/Dev/projects/contact-patch`
- `~/Dev/projects/convergence`
- `~/Dev/projects/effigy`
- `~/Dev/projects/finch`
- `~/Dev/projects/jetstream`
- `~/Dev/projects/loophole`
- `~/Dev/projects/monkey`
- `~/Dev/projects/northstar`
- `~/Dev/projects/nucleus`
- `~/Dev/projects/pug`
- `~/Dev/projects/signal`
- `~/Dev/projects/songsprout`
- `~/Dev/projects/underlay`
- `~/Dev/projects/underlay-reference`

## Quantitative Snapshot

- Projects scanned: `16`
- Repos with `effigy.toml`: `15`
- Repos with `AGENTS.md`: `16`
- Repos with `CHANGELOG.md`: `3`
- Repos with `docs/guides/README.md`: `3`
- Repos with `docs/vision/`: `9`
- Repos with `docs/roadmaps/`: `9`
- Repos with `qa:docs`: `1`
- Repos with `qa:json`: `1`
- Repos with `[release]` config in `effigy.toml`: `1`
- Repos with `[docs_policy]` config in `effigy.toml`: `1`
- AGENTS files mentioning `effigy tasks`: `13`
- AGENTS files mentioning `effigy doctor` or `effigy health`: `13`
- AGENTS files mentioning `effigy test --plan`: `7`
- AGENTS files mentioning Northstar explicitly: `2`
- AGENTS files still teaching `--repo .`: `13`

## Maturity Bands

### 1. Full doctrine only in Effigy

Effigy is currently the only repo with the full self-hosted doctrine:

- docs guides hub
- docs policy config
- docs QA bundle
- JSON QA bundle
- changelog
- release config
- release orchestration

### 2. Strong Northstar-ish docs, weak Effigy validation

These repos already have real `docs/vision` and `docs/roadmaps`, but their
Effigy layer is mostly light task routing rather than doctrine enforcement:

- `compli-me`
- `convergence`
- `finch`
- `jetstream`
- `signal`

These are the best near-term pilot candidates for a consumer adoption kit.

### 3. Strong Effigy task surface, weak Northstar structure

These repos already rely on Effigy but do not yet expose the full Northstar
documentation skeleton:

- `acowtancy`
- `contact-patch`
- `loophole`
- `nucleus`
- `songsprout`
- `underlay-reference`

These need scaffolding and doctrine migration more than new task semantics.

### 4. Mixed foundation candidates

- `underlay` has strong guides and a meaningful Effigy task surface, but does
  not yet expose the same Northstar-style vision/roadmap layer used elsewhere.
- `monkey` has guides plus vision/roadmaps and some Effigy usage, but not the
  broader validation/release doctrine. It is also a strong first pilot because
  the repo is structurally mature without carrying the workspace-scale
  complexity of `compli-me`.
- `pug` has Northstar-style docs structure but does not yet have `effigy.toml`.

## Representative Findings

### Workspace AGENTS drift

`acowtancy`, `compli-me`, and `underlay` all have real Effigy-first agent
instructions, but they still teach `--repo .` as a default. That means the
agent contract has spread faster than the corrected default semantics.

### Validation gap

Outside Effigy itself, the scan did not find consumer repos with a repo-owned
`qa:docs` / `qa:json` / `[release]` / `[docs_policy]` bundle that enforces the
documentation and release contract in the same way Effigy now does.

### Northstar naming gap

Several repos already use Northstar-like docs structures, but almost none
explicitly tell agents what “use northstar and effigy” means as an operational
contract. The structure exists, the phrase contract does not.

## Recommended Pilot Sequence

1. `monkey`
   - Has `docs/vision`, `docs/roadmaps`, `docs/guides`, and a simple
     single-repo Effigy surface.
   - Missing changelog/release doctrine, which makes it a strong end-to-end
     proving ground for the adoption kit.
2. `compli-me`
   - Has `docs/vision`, `docs/roadmaps`, and a docs authority repo shape.
   - Good second pilot after the contract is proven in a simpler repo.
3. `underlay`
   - Strong Effigy usage and guides.
   - Good candidate for testing how the contract applies to a shared library
     repo rather than an app workspace.
4. `acowtancy`
   - Good workspace-scale pilot once the repo contract works in a simpler repo.

## Conclusion

The ecosystem is ready for a reusable Northstar + Effigy adoption kit, but not
for an assumption that consuming repos already understand or enforce the full
doctrine. The next milestone should package:

- a repo contract
- an agent skill
- scaffolding/templates
- Effigy validation tasks
- at least one real consumer pilot

## Vision Target Delta

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`, `MAINT`
- Moved from: `adoption discussion based on intuition and isolated repo memory`
  to `explicit cross-repo scan showing which doctrine pieces already exist and
  which are still missing in consumer repos`
- Remaining open:
  - define the reusable Northstar + Effigy repo contract
  - ship the agent skill and scaffolding bundle
  - validate the contract on real consumer pilots outside Effigy itself
