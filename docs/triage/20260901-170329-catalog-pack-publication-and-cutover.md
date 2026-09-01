---
title: Catalog-pack publication and concrete-asset cutover
kind: triage
status: ready-for-review
created: 2026-09-01
updated: 2026-09-01
owner: Tom / catalog-pack publication planning delegate
handoff: docs/handoffs/20260901-170329-catalog-pack-publication-planning-delegate.md
tags: [catalog-pack, publication, ghcr, asset-ownership, distribution]
---

# Catalog-Pack Publication And Concrete-Asset Cutover

Reviewable planning packet. This records evidence and operator choices for
later orchestrator promotion. It is not implementation or release authority.

## Scope

- official OCI repository:
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`
- canonical ownership and editable source for concrete catalog assets
- compiled-baseline regeneration and drift proof
- independent pack identity, stable-channel mapping, and immutable digests
- supported automatic availability without registry probes in ordinary commands
- pack build, validation, provenance, and publication workflow boundaries
- public `effigy service pack update` exposure and first-publication gate

Out of scope: implementation, workflow edits in this delegate, OCI publication,
Effigy release mutation, retention/garbage collection, S3, general extension
transport, command grouping, and generation rollover.

## Checkout Evidence

- worktree:
  `/Users/tom/.paseo/worktrees/310mya31/catalog-pack-publication-planning`
- branch: `planning/catalog-pack-publication`
- clean at startup: yes
- recorded base: `0f40f7f2b1692628b078d76674f43fc2b4b79e46`
- startup `HEAD` and `origin/main`:
  `1c0ebe9c8e929d8fcf87a02da2102d2059e27e18`
- base is an ancestor of `HEAD`: yes
- absolute handoff matches the tracked `HEAD` blob: yes
- drift since the recorded base: only the dispatch handoff commit
- open Effigy PRs at discovery start: none

## Operator-Confirmed Decisions

These decisions are operator-confirmed and stay closed:

- The official package repository is
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`.
- Canonical editable catalog assets live in the dedicated
  `inflatable-cookie/effigy-catalog-pack` source repository.
- Pack releases use independent semantic versions. Effigy compatibility stays
  explicit in the pack manifest rather than being encoded by matching Effigy
  release numbers.
- Effigy retains only a generated, pinned recovery snapshot. That snapshot
  records the source repository commit, pack version, and published OCI digest,
  and a deterministic drift check rejects hand edits or a stale snapshot.
- A pack release may propose a baseline-update PR to Effigy. Acceptance and
  release of that Effigy change remain separate repository-owned decisions.
- The generated compiled snapshot is the automatic catalog-pack availability
  contract for every supported Homebrew, GitHub Release, source-build,
  `effigy init`, and `effigy bootstrap` path.
- Those install and repository-initialization paths never contact GHCR or
  silently activate user-state pack content. Registry-backed acquisition is
  only an explicit `effigy service pack update` or explicit digest install.
- The `stable` channel moves only through a protected manual dispatch against
  an existing annotated pack `vX.Y.Z` tag.
- The dedicated pack repository owns pack validation, publication, provenance,
  and baseline-update PR generation. Effigy independently reviews and accepts
  or rejects the proposed generated snapshot.
- First publication is a separate operator-gated external mutation.
- Digest-bound provenance is mandatory for the first official artifact. The
  publication lane stops rather than silently weakening that requirement if
  the chosen generic OCI shape cannot be attested and verified.
- Anonymous pulls through both `stable` and the resolved digest must reproduce
  and validate the published pack before any Effigy build exposes
  `effigy service pack update`.
- Cross-repository baseline PR proposals use a narrowly installed GitHub App
  with short-lived repository-scoped permissions, not a maintainer PAT.
- The first official pack release is `1.0.0`; its annotated source tag and OCI
  version tag are `v1.0.0`.
- `stable` remains compatible with every still-supported Effigy release that
  publicly exposes `service pack update`. A pack that raises the Effigy floor
  cannot move `stable` until support policy advances explicitly. Parallel
  compatibility channels require later planning.
- Scoped GitHub Actions changes for pack build, validation, and publication are
  authorized for a later implementation lane.
- Every supported Effigy installation keeps a permanent compiled baseline.
- Selection order remains project override, user override, active installed
  pack, compiled baseline.
- Ordinary catalog-backed commands never fetch, probe the registry, or check
  freshness.
- Explicit immutable OCI and local-path install, validate-before-activate,
  atomic activation, visible fallback, rollback, and reset remain settled.
- Installed content cannot redirect the baseline-owned official channel.
- Public no-argument `service pack update` appears only when the official
  channel exists and can succeed from its first release.
- No automatic pruning belongs to this lane.

## Current Repository Evidence

- `crates/effigy-catalog/catalog/README.md` declares
  `crates/effigy-catalog/catalog/` the current maintainer-facing source of truth.
- `crates/effigy-catalog/src/fragment.rs` embeds that directory with
  `rust_embed`; it is the permanent compiled baseline today.
- `crates/effigy-catalog/build.rs` invalidates the crate build when the catalog
  directory changes. There is no pack-generation or baseline-drift command.
- `PackManifest` already gives a pack its own semantic version and an Effigy
  compatibility requirement. The prototype describes that version as
  independently owned.
- `OfficialPackChannel::baseline()` currently compiles in repository
  `packs.invalid/effigy/default-catalog`, channel `stable`, and
  `published = false`.
- The modeled update planner accepts a channel-resolved digest and turns it into
  an immutable `oci://<repository>@sha256:...` candidate. Tag resolution itself
  is not implemented.
- Current workflows are `ci.yml`, `json-contracts.yml`, and
  `release-binaries.yml`. None publishes packages. The binary release workflow
  is manually dispatched from an existing annotated Effigy version tag and has
  only `contents: write`; coupling pack publication to it would couple the pack
  to Effigy releases by construction.
- The existing OCI adapter pulls a generic OCI artifact with ORAS into a
  destination directory and records the registry-resolved digest. A published
  pack must therefore unpack to one discoverable pack root containing
  `pack.toml` plus the existing catalog fragment layout.
- Supported binary distribution is one self-contained executable through
  GitHub Releases, Homebrew, or a source build. Each path therefore receives
  the embedded generated snapshot without an extra installer contract.
- `effigy init` scaffolds and checks a target repository. `effigy bootstrap`
  clones and brings up a target repository. Neither currently owns global
  catalog-pack selection, and making either activate a pack would add an
  unrelated user-state mutation to a repository operation.

## Sourced Publication Evidence

- GHCR supports OCI artifacts, public packages can be pulled anonymously, and
  a workflow in the source repository can publish with `GITHUB_TOKEN`. GitHub
  recommends `contents: read` plus `packages: write`; a workflow-created package
  is linked to that repository. A first package is private by default, so making
  the official channel public is a separate package-setting action.
  [GitHub container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry),
  [GitHub Actions package publication](https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions)
- Package permissions and visibility can either inherit from a linked repository
  or be managed granularly. Linking before publication matters when inheritance
  is intended; OCI source annotations are one supported linkage mechanism.
  [GitHub package access and visibility](https://docs.github.com/en/packages/learn-github-packages/configuring-a-packages-access-control-and-visibility)
- ORAS can push a directory as an OCI artifact, set a distinct artifact type and
  annotations, apply multiple tags to one manifest, and return the immutable
  digest/reference in JSON. Those properties can support one version tag plus a
  movable `stable` tag whose value is recorded as a digest.
  [ORAS push/pull](https://oras.land/docs/1.2/how_to_guides/pushing_and_pulling/),
  [ORAS formatted output](https://oras.land/docs/how_to_guides/format_output/),
  [ORAS annotations](https://oras.land/docs/1.2/how_to_guides/manifest_annotations/)
- GitHub can produce build-provenance attestations for OCI container subjects
  when the workflow has `contents: read`, `packages: write`,
  `attestations: write`, and `id-token: write`, and the attestation is bound to
  the pushed subject digest. Whether GitHub accepts the chosen generic pack
  artifact shape must be proved in the publication lane rather than assumed.
  [GitHub artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
- GitHub recommends pinning third-party actions to full commit SHAs. A pack
  workflow should either pin the ORAS setup action and other non-GitHub actions
  or install a fixed ORAS release with a verified checksum.
  [GitHub Actions settings](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository),
  [ORAS setup action](https://github.com/oras-project/setup-oras)
- A GitHub Actions environment can hold the publication job behind required
  reviewers, prevent the initiator from self-approving, and restrict eligible
  branches. This supplies a repository-native operator gate without giving the
  validation jobs package-write credentials.
  [GitHub deployment environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)
- A GitHub App has no repository permissions by default and should receive only
  the minimum required permissions. Git access needs `contents`; a short-lived
  installation token can be narrowed to named repositories and explicit
  permissions, then is revoked by the official token action after the job.
  This fits a pack-release proposal into Effigy better than a broad personal
  access token.
  [GitHub App permissions](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app),
  [GitHub App token action](https://github.com/actions/create-github-app-token)

## Delegate Recommendations

Not operator decisions yet:

1. Keep the canonical asset tree under `pack/` in the dedicated source
   repository: top-level `pack.toml` plus the exact current catalog tree,
   including non-fragment support files. Repository-level docs, workflows, and
   tooling stay outside that tree.
2. Copy that tree byte-for-byte into Effigy as the generated baseline. Keep a
   sidecar lock with source repository, source commit, pack version, OCI
   manifest digest, and Effigy's existing pack content identity. Mark the
   snapshot generated and reject direct edits.
3. Split drift proof in two. Normal offline QA recomputes manifest/version and
   content identity from the checked-in snapshot and compares them with the
   lock. A publication or baseline-PR provenance check additionally pulls the
   public artifact by recorded digest, verifies its attestation, and compares
   exact paths and bytes with the snapshot.
4. Publish the immutable `vX.Y.Z` manifest first, attest and revalidate it, then
   promote `stable` to that same digest. This avoids moving the public channel
   before immutable-version proof completes while keeping both operations in
   one protected release run.
5. Make `service pack update` report the resolved channel and digest. If that
   digest is already active and verified, return a deterministic no-op. Any
   resolution, pull, compatibility, validation, or activation failure leaves
   the current active pack and `stable` channel metadata unchanged.

## Derived Source And Artifact Shape

- Source repository: `inflatable-cookie/effigy-catalog-pack`.
- Canonical asset root: `pack/`.
- First manifest identity/version: `effigy-default-catalog` / `1.0.0`.
- Git and OCI immutable version tag: `v1.0.0`.
- OCI repository: `ghcr.io/inflatable-cookie/effigy-catalog-pack`.
- OCI channel tag: `stable`.
- Recommended artifact type:
  `application/vnd.inflatable-cookie.effigy.catalog-pack.v1`.
- Recommended OCI annotations: source repository, source revision, version,
  description, created time, and license using standard OCI keys where they
  exist.
- Effigy generated snapshot: the exact `pack/` tree, including `pack.toml`.
  The existing embed owner may keep its current path if that minimizes churn,
  but its README must say generated recovery snapshot rather than source of
  truth.
- Effigy provenance lock: repository-owned typed data beside the snapshot, not
  metadata inside installed content that could redirect the official channel.

The OCI manifest digest and Effigy's pack content identity are different facts.
The former pins registry transport bytes and metadata; the latter proves the
unpacked tree. Record and verify both rather than pretending one can be
recomputed from the other.

## Stable Channel And Update Semantics

1. A protected manual dispatch accepts an existing annotated source tag.
2. The workflow proves the checked-out commit equals that tag and
   `pack.toml` declares the same SemVer without the `v` prefix.
3. It validates the source tree, compatibility range, and artifact shape with
   no package mutation.
4. The publish job writes immutable `vX.Y.Z`, records the returned manifest
   digest, attaches provenance, pulls by digest, and reruns validation.
5. Only after that proof does the gated promotion move `stable` to the same
   digest. It then verifies anonymous pull through `stable` and by digest.
6. `service pack update` uses the existing ORAS-backed artifact boundary to
   resolve `stable` to a digest. It passes only the digest-addressed candidate
   into the settled acquire, validate, store, and activate transaction.
7. A pack release may move `stable` only if its compatibility range admits the
   oldest still-supported update-capable Effigy and the current Effigy release.
8. A channel rollback revalidates a previously published immutable digest, then
   moves only `stable` through the same protected promotion path. It never
   deletes or overwrites an immutable version.

No Homebrew, binary download, source build, `init`, `bootstrap`, service,
container, system, workspace, or task path resolves `stable` implicitly.

## Workflow Trust And Permissions

### Validation

- runs on pull requests and manual dry-runs in the pack repository
- read-only source permissions; no package, attestation, or Effigy write token
- pins third-party actions to full commits or installs a fixed ORAS binary with
  a verified checksum
- validates manifest, fragments, compatibility, exact file inventory, pack
  content identity, and reproducible generated-snapshot input

### Publication

- manual dispatch only, with an existing annotated `vX.Y.Z` tag
- concurrency keyed by tag, with cancellation disabled
- protected publication environment with required reviewer and no self-review
- publish job permissions limited to `contents: read`, `packages: write`,
  `attestations: write`, and `id-token: write`
- no delete permission and no Effigy repository credential in the publish job
- immutable-version failure never rewrites the tag; repair publishes the next
  SemVer patch

### Baseline PR proposal

- starts only after immutable publication, provenance, and channel proof pass
- uses a separate GitHub App installation token narrowed to the Effigy
  repository and the minimum contents/pull-request permissions
- writes one generated snapshot, one provenance lock, and required generated
  evidence; no product-code or workflow change is smuggled into the proposal
- cannot approve, merge, or release the Effigy PR
- Effigy CI independently verifies snapshot drift and public artifact
  provenance before review

## First Publication Authority

Workflow implementation and first publication are separate lanes.

1. Merge the pack repository, source import, validation, and publication
   workflow with every push path disabled except protected manual dispatch.
2. Prove read-only validation and a no-push release rehearsal.
3. An operator creates the annotated `v1.0.0` source tag and explicitly
   authorizes the protected publication job.
4. The job may create only immutable `v1.0.0`; it records the digest and
   provenance before channel promotion.
5. The operator confirms repository linkage and changes the GHCR package to
   public. The first public package mutation is not delegated to this planning
   thread or inferred from workflow-edit authority.
6. The gated promotion verifies anonymous pulls, exact bytes, manifest,
   compatibility, digest, and attestation, then moves `stable` to the proved
   digest and verifies it again.
7. Record workflow run, source tag/commit, pack version, OCI digest,
   attestation, visibility/linkage, anonymous pull, and rollback target.
8. Only that evidence may unblock Effigy's coordinate cutover and public
   `service pack update` implementation.

If the initial immutable push partially succeeds, do not overwrite or re-tag
`v1.0.0`. Preserve evidence, leave `stable` absent or unchanged, fix the cause,
and publish the next valid patch after operator review.

## Implementation Sequence

1. **Canonical promotion in Effigy.** The orchestrator promotes accepted
   ownership, channel, automatic-availability, trust, migration, and gate rules
   into architecture `026`, contract `043`, a strict spec, roadmap, card, and
   front doors. No implementation starts from this packet alone.
2. **Pack repository foundation.** Create the dedicated repository; import the
   current concrete assets without byte changes; add `pack/pack.toml` at
   `1.0.0`; add repo-owned validation, release tasks, read-only CI, publication
   workflow, and evidence format. Stop before publication.
3. **No-push proof.** Validate the imported pack with the current released
   Effigy acquisition boundary and prove an isolated local install, full
   fragment inventory, representative assembly, and generated-snapshot input.
4. **First-publication gate.** Follow the operator-owned sequence above. The
   result is a public, attested immutable `v1.0.0` plus `stable` at the same
   verified digest.
5. **Effigy cutover and update lane.** Generate the pinned baseline and lock
   from the published pack; change the current catalog directory from editable
   authority to generated recovery snapshot; add offline drift and online
   provenance checks; replace the placeholder official coordinate; resolve
   `stable` through the existing artifact adapter; expose `service pack update`;
   preserve all settled acquisition and layering behavior.
6. **Future pack-release automation.** Enable the narrow GitHub App proposal
   only after its generated branch and PR scope are proved. Pack publication
   does not wait for Effigy to merge or release the proposed baseline update.

No step runs an Effigy binary release. Releasing an Effigy build containing the
public update command remains a later operator-authorized release operation.

## Migration And Rollback Proof

The implementation plan must falsify these cases:

1. The initial source import or generated Effigy snapshot changes any existing
   catalog path or byte other than adding pack/provenance metadata.
2. A direct edit to Effigy's snapshot passes offline drift checks or becomes a
   second editable authority.
3. The provenance lock names a source commit, version, content identity, or OCI
   digest that does not reproduce the checked-in snapshot exactly.
4. Homebrew, binary, source, `init`, bootstrap, or ordinary catalog-backed use
   performs a registry call or requires an installed pack.
5. `stable` resolves to mutable tag input at activation time instead of an
   immutable digest, or installed content redirects the repository/channel.
6. A failed resolution, pull, attestation, compatibility check, validation, or
   activation changes active or previous local state.
7. A candidate that excludes a still-supported update-capable Effigy moves
   `stable` anyway.
8. Publication moves `stable` before immutable-tag, digest, attestation,
   anonymous-pull, and exact-byte proof complete.
9. A baseline-update workflow receives broader repository authority, edits
   product code, approves its own PR, or makes pack publication depend on merge.
10. A bad channel promotion cannot be restored to the previous verified digest,
    or a user cannot recover through installed-pack rollback/reset and the
    permanent compiled baseline.
11. `service pack update` is advertised before the official artifact exists, or
    its first released form cannot succeed against the public channel when ORAS
    and network access are available.

Required evidence includes focused pack/source validation, current-baseline byte
parity, anonymous stable/digest pulls, provenance verification, supported-version
compatibility, isolated `HOME` update/no-op/failure tests, representative
service/container/system/workspace/task regression, offline source install, full
Effigy QA, workflow run URLs, exact source and artifact identities, and a tested
stable rollback target.

## Alternatives And Dispositions

- **Canonical source inside Effigy:** rejected; concrete assets would remain
  coupled to the core repository.
- **Effigy-coupled pack versions:** rejected; asset-only releases would still
  wait for binary releases.
- **Implicit install/init/bootstrap update:** rejected; it adds surprise network
  and global-state mutation where the compiled baseline already supplies the
  no-ceremony floor.
- **Automatic tag-push or main-push publication:** rejected; `stable` movement
  and first publication require a protected manual gate.
- **Maintainer PAT for baseline PRs:** rejected; a narrow GitHub App provides a
  shorter-lived and repository-scoped boundary.
- **Provenance after the first release:** rejected; the first official public
  artifact must establish the durable trust model.
- **Parallel compatibility channels:** deferred until one is required by an
  explicit support-floor change.
- **Automatic installed-pack pruning:** out of scope and unchanged.

## Unresolved Questions

No operator-owned question blocks this packet.

Future support-floor changes, parallel compatibility channels, and installed
pack retention remain separate decisions. The implementation lane must stop if
GHCR cannot attest the chosen generic OCI artifact shape, anonymous pull differs
from authenticated proof, the GitHub App cannot be narrowed as planned, or
exact-byte snapshot reproduction is not deterministic.

## Proposed Canonical Destinations

To be chosen by the orchestrator after this packet is accepted:

- architecture `026`: long-lived source ownership, baseline derivation, and
  release independence
- contract `043`: stable-to-digest resolution, automatic-availability
  invariant, drift gate, workflow trust, and first-publication authority
- a new strict spec and roadmap/card lane: implementation order, validation,
  rollback proof, workflow changes, and public update exposure
- guides `067`, `071`, `072`, and release/distribution guidance: maintainer and
  operator behavior after implementation
- the dedicated pack repository: source README, manifest/version policy,
  validation tasks, publication workflow, recovery runbook, and evidence format
- Effigy generated surfaces: snapshot ownership marker, typed provenance lock,
  offline drift task, and registry-backed provenance verification task
