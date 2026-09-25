# g10.020 — Underlay bundle Chromium input

Status: ready
Owner: `inflatable-cookie/underlay-effigy-bundle` export and input schema
Created: 2026-09-25
Governing refs: `docs/contracts/001-working-rules.md`, `docs/guides/065-external-bundle-adoption.md`, `docs/guides/067-catalog-services-reference.md`, `docs/roadmaps/g10/017-opt-in-chromium-workspace-runtime.md`
Depends on: g10.017 terminal; Effigy v0.13.1 published and install-verified

## Outcome

The Underlay bundle exposes the built-in `workspace-rust-bun` Chromium
system-library option as a default-off typed input. Its exported manifest
requires Effigy 0.13.1, so an older binary cannot silently ignore the opt-in.
One reviewed bundle PR merges with rendered default and opt-in proof.

## Ready-State Rubric

- [x] Tom approved the core and bundle lane with “Go for it” on 2026-09-25.
- [x] The catalog option and non-root linux-arm64 Playwright smoke are complete
  in g10.017; published pack v1.1.0 supplies the exact service bytes.
- [x] Effigy v0.13.1 is published with four binaries; the tagged install check
  and direct ARM64 macOS binary smoke passed.
- [x] The bundle repository is clean, has no active Queue task, and owns only
  its input/export/docs change. No open design choice remains.

## Decisions

- Add a top-level typed string input `browser_runtime` with default `"none"`.
  The operator may set `"chromium"`; the catalog image build rejects unknown
  values. Do not introduce arbitrary package lists or a second Dockerfile.
- Pass the input to the `workspace-rust-bun` service parameter in `export.toml`.
- Raise the bundle-wide `minimum_effigy_version` from `0.10.0` to `0.13.1`.
  This is intentional for default and opted-in consumers: only the released
  binary that understands the service option may load this bundle revision.
- Document the image rebuild and consumer-owned Playwright/browser download.
  Do not add Node, Playwright, or a browser binary to the shared image.

## Dispatch manifest

- **State:** ready; one external bundle lane, no parallel owner of these files.
- **Completion:** one non-draft PR in `underlay-effigy-bundle` passes independent
  exact-head review and current-base validation, then merges.
- **Owned mutable paths:** bundle `bundle.toml`, `export.toml`, `README.md`, and
  focused bundle-owned validation evidence if needed.
- **Reserved closeout surfaces:** Effigy g10 planning and lifecycle state,
  catalog-pack source/artifact, Effigy release, and Acowtancy app evidence.
- **Worker:** automatic Queue pool; independent PR review.
- **Excluded:** Effigy image/runtime edits, pack changes, Acowtancy changes,
  browser binaries or Node in the image, per-project overrides, workflow edits,
  and another release.
- **Escalation:** Chatterbox for a version-floor conflict or inability to prove
  released-binary composition.

## Work

1. Add the typed default-off input and pass it into the selected
   `workspace-rust-bun` service. Set the manifest floor to 0.13.1.
2. Update bundle usage docs with `browser_runtime = "chromium"`, the rebuild
   step, and project-owned `@playwright/test` browser installation.
3. Use the published 0.13.1 binary to compose a temporary path-sourced
   consumer with required inputs. Prove default `none` and explicit `chromium`
   both reach the workspace service/build arg. Use released 0.13.0 to prove the
   version floor rejects the bundle before an opted-in build can be ignored.
4. Run focused bundle validation and diff checks, open a non-draft PR, and
   obtain independent exact-head review before merge.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Default remains off | Existing consumers gain browser libraries | Omitted input renders `browser_runtime = "none"` and the default Compose build arg |
| Opt-in reaches the image | Input exists but is not passed to the service | Explicit `"chromium"` renders the service parameter and Compose build arg |
| Older binary fails closed | 0.13.0 ignores the new parameter | Released 0.13.0 rejects `minimum_effigy_version = "0.13.1"` |
| Consumer owns browser revision | Bundle pins a browser or installs Node | Diff and docs show only the catalog parameter; Playwright/browser install remains project-owned |

## Validation

Run released-binary default, explicit, and old-version probes; bundle
composition/inspection commands; `git diff --check`; and current-base PR review.
The core g10.017 arm64 non-root launch is accepted as image evidence because
this task changes no effective image bytes.

## Stop conditions

- Stop if the bundle template cannot pass this parameter with existing typed
  inputs, or if the published 0.13.1 binary does not render it.
- Stop on a need to alter catalog image bytes, package versions, container
  privileges, Acowtancy, or the release; return the concrete blocker.

## Evidence

Record released binary identities, rendered default and opt-in values, old
version rejection, PR/reviewed head/merge, and any limits. Queue owns its task
record; Chatterbox updates this cross-repository planning card after closeout.

## Next task

After bundle merge, notify the Acowtancy Chatterbox to enable the option,
rebuild its workspace, and resume g05.195 browser gesture and request evidence.
