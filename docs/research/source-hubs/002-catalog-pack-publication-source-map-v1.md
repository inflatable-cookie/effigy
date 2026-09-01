# Source Map 002: Catalog-Pack Publication

Status: Active
Coverage: GHCR, ORAS, GitHub Actions trust, attestations, and GitHub App PR proposals
Last updated: 2026-09-01
Owner: Effigy catalog-pack publication lane

## Purpose

Preserve the primary-source evidence used to constrain Effigy's official
catalog-pack publication and generated-baseline design. Architecture `026` and
contract `043` own the decisions; this file owns the source map.

## Registry And Artifact Identity

| Source | Evidence used |
| --- | --- |
| [GitHub container registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry) | GHCR supports OCI artifacts, digest-addressed pulls, repository linkage, and anonymous pulls for public packages. A first package is private until visibility is changed. |
| [GitHub package publication](https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions) | A source-repository workflow may publish with `GITHUB_TOKEN`; the narrow write boundary is `contents: read` plus `packages: write`. |
| [GitHub package access and visibility](https://docs.github.com/en/packages/learn-github-packages/configuring-a-packages-access-control-and-visibility) | Package linkage, inherited permissions, and public visibility are distinct controls. |
| [ORAS push and pull](https://oras.land/docs/1.2/how_to_guides/pushing_and_pulling/) | ORAS can publish generic OCI artifacts, attach version/channel tags, and pull by immutable digest. |
| [ORAS formatted output](https://oras.land/docs/how_to_guides/format_output/) | Publication can capture resolved references and digests as machine-readable evidence. |
| [ORAS annotations](https://oras.land/docs/1.2/how_to_guides/manifest_annotations/) | OCI annotations can carry source and revision facts without making a tag the immutable identity. |
| [ORAS OCI layouts](https://oras.land/docs/how_to_guides/distributing_oci_layouts/) | A deterministic local OCI layout can be built and inspected before registry mutation. |

## Trust And Mutation Control

| Source | Evidence used |
| --- | --- |
| [GitHub artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations) | A workflow can bind provenance to an OCI subject digest when granted the documented attestation and identity permissions. The generic artifact shape still requires live proof. |
| [GitHub ruleset rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets) | Matching source tags can reject update and deletion; bypass actors must be explicit. |
| [GitHub deployment environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments) | A protected environment can require review, prevent self-review, and restrict eligible branches. |
| [GitHub Actions settings](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository) | Third-party actions should be pinned to full commit SHAs. |
| [GitHub Releases API](https://docs.github.com/en/rest/releases/releases?apiVersion=latest) | Release existence and latest non-draft/non-prerelease identity can be checked independently of Effigy's support policy. |

## Cross-Repository Proposal Boundary

| Source | Evidence used |
| --- | --- |
| [GitHub App permissions](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app) | An App starts with no repository permissions and can be narrowed to the minimum contents and pull-request scope. |
| [GitHub App token action](https://github.com/actions/create-github-app-token) | A short-lived installation token can be narrowed to named repositories and permissions, then revoked after the job. |

## Effigy Implications

- The OCI manifest digest is the immutable release identity. Source and OCI
  `vX.Y.Z` tags are protected, checked pointers.
- Build the candidate deterministically before mutation. Treat absent,
  same-digest, and different-digest remote version states separately.
- Keep validation read-only. Grant package and attestation writes only to the
  protected manual publication job.
- Verify public anonymous pull, digest-bound attestation, exact bytes, and the
  unchanged Effigy compatibility input before moving `stable`.
- Use a narrow GitHub App only for generated baseline PR proposals. It cannot
  approve, merge, or release Effigy.

## Proof Boundary

Documentation establishes available platform controls, not that the selected
generic OCI artifact shape works end to end. The first-publication gate must
prove attestation, anonymous pull, deterministic replay, tag collision handling,
and stable rollback before Effigy exposes public update behavior.
