# Consumer Proof Of Optional Distribution Surface

Date: 2026-04-15
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Batch Card: `docs/specs/batch-cards/103-implement-consumer-proof-of-optional-distribution-surface.md`

## Summary

Ran one bounded consumer proof in `convergence` to test whether the current
optional `[distribution]` surface works outside Effigy's self-hosting release
flow.

The proof was intentionally narrow:

- add minimal consumer-owned `[distribution.package]`
- add minimal consumer-owned `[distribution.publish]`
- add minimal consumer-owned `[distribution.closeout]`
- capture real local proof logs from a `cargo install --path` install and
  `converge --help`
- exercise `distribution validate-artifacts`
- exercise `distribution generate-closeout`
- probe `distribution validate-metadata` to record the remaining gap honestly

## Consumer Repo

- repo: `~/Dev/projects/convergence`
- worktree state before proof: clean
- adopted manifest surface:
  - `[distribution.package] name = "converge"`
  - `[distribution.publish] binary-name = "converge"`
  - `[distribution.publish] registry-label = "local cargo install"`
  - `[distribution.closeout] owner = "operators"`
  - `[distribution.closeout] related = "docs/operators/releases-and-retention.md"`
  - `[distribution.closeout] next-step = "Decide whether to widen Effigy's first-publish and metadata surface before treating Convergence as a fuller distribution consumer."`

## Proof Commands

```sh
cargo install --path ~/Dev/projects/convergence --root /tmp/effigy-convergence-install-root --force
cargo run --manifest-path ~/Dev/projects/effigy/Cargo.toml --bin effigy -- \
  distribution validate-artifacts \
  --repo ~/Dev/projects/convergence \
  --artifacts-dir /tmp/effigy-convergence-distribution-proof
cargo run --manifest-path ~/Dev/projects/effigy/Cargo.toml --bin effigy -- \
  distribution generate-closeout \
  --repo ~/Dev/projects/convergence \
  --tag v0.1.0 \
  --artifacts-dir /tmp/effigy-convergence-distribution-proof \
  --output /tmp/effigy-convergence-distribution-closeout.md
cargo run --manifest-path ~/Dev/projects/effigy/Cargo.toml --bin effigy -- \
  distribution validate-metadata \
  --repo ~/Dev/projects/convergence \
  --tag v0.1.0
```

## What Worked

- The optional manifest contract was strong enough for a real consumer to own
  package, publish, and closeout identity without inheriting Effigy's exact
  release labels.
- `distribution validate-artifacts` accepted the consumer-shaped
  `registry-label = "local cargo install"` and passed against the captured
  proof logs.
- `distribution generate-closeout` used the consumer-owned closeout owner,
  related reference, and next-step language instead of Effigy defaults.
- The closeout wording stayed generic enough to remain useful outside Effigy's
  own release roadmap.

## Remaining Effigy-Shaped Gaps

- `distribution validate-metadata` still hard-fails on
  `.github/workflows/release-binaries.yml`, which blocks repos that do not use
  Effigy's release workflow layout.
- `distribution validate-metadata` still assumes a root `Cargo.toml` package
  shape rather than broader consumer variants.
- The fuller `distribution first-publish` path still assumes an
  Effigy-compatible CLI self-inspection surface such as `--json tasks`.
- In the `convergence` proof, `converge --json tasks` failed with
  `unexpected argument '--json'`, which is valid consumer behavior today but
  not yet compatible with Effigy's first-publish matrix.

## Outcome

The optional distribution surface is now proven enough to trust for
artifact-validation and closeout-evidence reuse in another repo.

The lane should not pause yet. The consumer proof exposed one bounded widening
cluster clearly enough that the next valid move is implementation, not another
decision loop:

- widen metadata validation away from hardcoded Effigy workflow assumptions
- widen first-publish away from mandatory Effigy CLI self-inspection

## Validation

- consumer proof logs captured under `/tmp/effigy-convergence-distribution-proof`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution validate-artifacts --repo /Users/tom/Dev/projects/convergence --artifacts-dir /tmp/effigy-convergence-distribution-proof`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution generate-closeout --repo /Users/tom/Dev/projects/convergence --tag v0.1.0 --artifacts-dir /tmp/effigy-convergence-distribution-proof --output /tmp/effigy-convergence-distribution-closeout.md`
- `cargo run --manifest-path /Users/tom/Dev/projects/effigy/Cargo.toml --bin effigy -- distribution validate-metadata --repo /Users/tom/Dev/projects/convergence --tag v0.1.0` (fails honestly on missing `.github/workflows/release-binaries.yml`)

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Moved: optional distribution support is now proven in one real consumer repo
  for artifact validation and closeout evidence, not just in Effigy's own
  self-hosting lane
- Remaining open: metadata validation and fuller first-publish still carry
  concrete Effigy-shaped assumptions that block broader consumer adoption

## Next Task

Execute `docs/specs/batch-cards/104-implement-consumer-driven-distribution-gap-widening.md`
to widen the named metadata and first-publish gaps exposed by the
`convergence` proof.
