# Release Binaries Changelog Extract Cutover Review

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-binaries-changelog-extract-cutover-review

## Summary

- Audited `.github/workflows/release-binaries.yml` against the shipped
  `effigy changelog extract` contract.
- Confirmed the workflow still extracts release notes with inline `sed`
  instead of the built-in changelog command.
- Prepared the exact review-only workflow change set without modifying
  `.github/workflows/`, because workflow edits remain human-gated.

## Current State

Current workflow behavior in `.github/workflows/release-binaries.yml`:

- `Create GitHub Release` checks out the repo on `ubuntu-latest`
- it derives `VERSION="${GITHUB_REF_NAME#v}"`
- it runs inline `sed` over `CHANGELOG.md` to capture the matching version body
- if the extracted body is empty, it falls back to `gh release create --generate-notes`
- otherwise it writes `release-notes.md` and passes `--notes-file release-notes.md`

Current extraction command:

```bash
NOTES="$(sed -n "/^## \[$VERSION\]/,/^## \[/{/^## \[$VERSION\]/d;/^## \[/d;p;}" CHANGELOG.md)"
```

This is functionally close to the built-in extractor, but it duplicates logic
that now already exists in the shipped CLI.

## Proposed Workflow Delta

Human-review patch target:

1. Add an Effigy build step in the `release` job before note extraction:

```yaml
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Build effigy
        run: cargo build --release --bin effigy
```

2. Replace the inline `sed` extraction step with the built-in command:

```yaml
      - name: Extract release notes from CHANGELOG.md
        id: notes
        run: |
          VERSION="${GITHUB_REF_NAME#v}"
          if ./target/release/effigy changelog extract CHANGELOG.md --version "$VERSION" > release-notes.md; then
            if [ ! -s release-notes.md ]; then
              echo "::warning::Extractor returned an empty release note body for $VERSION, falling back to generated notes"
              echo "fallback=true" >> "$GITHUB_OUTPUT"
            else
              echo "fallback=false" >> "$GITHUB_OUTPUT"
            fi
          else
            echo "::warning::No CHANGELOG.md section found for $VERSION, falling back to generated notes"
            rm -f release-notes.md
            echo "fallback=true" >> "$GITHUB_OUTPUT"
          fi
```

3. Keep the existing `gh release create` fallback behavior unchanged.

## Review Notes

- This is a workflow-only cutover. No CLI or release-command code changes are
  required for the workflow to adopt the built-in extractor.
- The `release` job currently does not install Rust, so it cannot call the
  built binary yet; the small build/bootstrap step above is required.
- Using `./target/release/effigy` keeps the workflow pinned to the repo state
  being released rather than assuming a preinstalled global binary.
- The existing fallback to generated GitHub release notes should remain in
  place so a missing or empty changelog section does not hard-fail publish.
- This cutover should be reviewed together with release-pipeline runtime cost,
  because it adds one small compile step to the `release` job even though the
  build matrix already compiled artifacts earlier.

## Recommendation

- Approve the workflow cutover only when a human is ready to review
  `.github/workflows/release-binaries.yml`.
- Apply the extractor swap as a single workflow PR with no unrelated release
  process edits.
- After merge, remove the corresponding roadmap item in `g01.027` and note the
  workflow cutover in the next release/adoption checkpoint log.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `the built-in changelog extractor is shipped but workflow cutover still lives as an unchecked roadmap item` -> current `the exact human-review workflow diff is documented and approval-ready without bypassing the workflow-edit policy`
- Remaining gap: human approval and merge of the workflow change itself

## Validation Performed

- command: `sed -n '1,260p' .github/workflows/release-binaries.yml`
  - result: confirmed the release job still uses inline `sed` extraction
- command: `rg -n "changelog extract|release notes|release-binaries.yml" .github/workflows docs/guides`
  - result: confirmed docs already treat `effigy changelog extract` as the preferred release-note baseline
- command: `cargo run --bin effigy -- qa:docs --repo .`
  - result: pass after indexing this log in `docs/logs/README.md`

## Risks

- The proposed workflow delta adds one local binary build to the `release` job;
  reviewers may prefer an alternate implementation path if they want to avoid
  that incremental CI cost.
- If the workflow change is applied without preserving the generated-notes
  fallback, a malformed changelog could block tag-publish unexpectedly.

## Next Task

- Prepare the actual workflow PR diff for human approval only when a maintainer
  explicitly authorizes edits under `.github/workflows/`.
