# g10.018 — Browser runtime catalog-pack source

Owner: canonical `inflatable-cookie/effigy-catalog-pack` source
Created: 2026-09-25
Governing refs: `docs/contracts/043-feature-placement-and-surface-migration-contract.md`, `docs/roadmaps/g10/017-opt-in-chromium-workspace-runtime.md`, catalog-pack `AGENTS.md`
Depends on: none; precedes `g10.019` and completion of `g10.017`

## Outcome

The canonical pack source contains the default-off Chromium system-library option already proven by the retained `g10.017` worker, with a new pack version and compatibility with released Effigy 0.13.0. A reviewed source PR merges before any tag or artifact publication.

## Ready-state rubric

- [x] Tom approved the source PR and separately gated publication on 2026-09-25.
- [x] The pack repository is the sole editable catalog source; Effigy's checked-in catalog is a generated recovery snapshot.
- [x] The retained `g10.017` worktree has implementation, tests, and linux-arm64 non-root Playwright smoke evidence; no PR or committed implementation.
- [x] Publication is reserved for `g10.019`, after this source PR merges.

## Dispatch manifest

- **State:** ready; one source PR in `effigy-catalog-pack`.
- **Completion:** one independently reviewed PR merges the source, version, compatibility, and pack-owned documentation/tests; no release mutation in this task.
- **Owned mutable paths:** `pack/workspace-rust-bun/{Dockerfile,service.toml,compose.fragment.yml}`, `pack/pack.toml`, pack-owned tests and documentation directly required for this option.
- **Reserved surfaces:** Effigy's generated catalog, lock, pinned constants, `g10.017` worker worktree, protected publication workflow, release tags, OCI package and `stable` channel.
- **Worker:** automatic Queue pool; independent exact-head review.
- **Concurrent sibling:** the retained `g10.017` worker remains blocked and its worktree preserved. Source PR work must not commit or clear that worktree.
- **Escalation:** Chatterbox for a pack version or compatibility conflict.

## Work

1. Inspect the retained `g10.017` diff and smoke log as evidence. Recreate the approved service option in `pack/`, treating pack source as authoritative. Keep `browser_runtime = "none"` by default, support `"chromium"`, and reject unknown values.
2. Choose the next SemVer version for an additive option and declare compatibility that includes released Effigy 0.13.0. Do not claim compatibility with an untested older release. Keep the image free of Playwright, Node, and browser binaries.
3. Run pack validation and an Effigy 0.13.0 consumer assembly proof. Compare source bytes and behavior against the retained arm64 launch evidence; repeat the real launch if the effective Dockerfile or package list changes.
4. Open a non-draft PR for independent review and current-base CI. Do not create a tag, dispatch publication, or move `stable`.

## Acceptance and review oracle

| Invariant | Counterexample | Proof |
| --- | --- | --- |
| Canonical source | Only Effigy's generated copy changes | Reviewed `pack/` diff in the source repository |
| Default-off option | Ordinary workspaces gain browser libraries | Default Compose arg and image path remain `none` |
| Real browser | Library list passes but Chromium fails as `dev` | Retained linux-arm64 Playwright 1.55.1 launch evidence, or repeated smoke if bytes change |
| Version truth | Pack advertises old bytes or unsupported Effigy | New pack version and verified 0.13.0 compatibility |
| Publication boundary | Source worker tags or pushes an artifact | PR and CI only; no publication mutation |

## Validation

Pack `effigy qa`, focused service assembly and source tests, diff check, hosted CI, and exact-head independent review. Record the source commit and effective source byte identity for `g10.019`.

## Stop conditions

- Stop on unresolved version/compatibility policy, changed image bytes without a real arm64 proof, or a need to modify Effigy, Underlay bundle, Acowtancy, publication workflows, tags, OCI package, or `stable`.

## Next task

After this PR merges, `g10.019` performs the separately approved, protected pack publication. `g10.017` remains blocked until the published artifact can be imported with honest provenance.
