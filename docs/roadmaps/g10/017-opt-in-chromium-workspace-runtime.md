# g10.017 — Opt-in Chromium workspace runtime

Owner: catalog workspace image and container assembly
Created: 2026-09-25
Governing refs: `docs/contracts/001-working-rules.md`, `docs/guides/063-container-system-guide.md`, `docs/guides/067-catalog-services-reference.md`, `docs/guides/065-external-bundle-adoption.md`
Depends on: none; `g10.016` is terminal

## Outcome

The built-in `workspace-rust-bun` service supports an explicit Chromium system-library layer, default off. A linux-arm64 image built with the option launches a project-pinned Playwright Chromium as non-root `dev`. Consumer bundles can select the parameter without taking ownership of a per-project Dockerfile.

## Ready-State Rubric

- [x] Tom approved the opt-in browser-runtime lane on 2026-09-25 after the Acowtancy g05.195 blocker was presented.
- [x] The shared catalog service and acceptance boundary are known; bundle wiring is separate downstream work.
- [x] The image installs only OS libraries and fonts, while each repo owns its Playwright package and matching browser download.
- [x] Default behavior, arm64 launch proof, mutable paths, review, and stop conditions are explicit.

## Decisions

- Add a string `browser_runtime` service parameter with default `"none"` and supported value `"chromium"`. Pass it into the Compose build args. Reject unknown values clearly at build; no arbitrary apt package input.
- Install Chromium's Debian Bookworm system dependencies and a basic font set as root at image build, before the non-root `dev` runtime. Use the Acowtancy `ldd` list as a starting set and verify the final package list by launch, not assumption.
- Do not bake Chromium, Playwright, Node, `npx`, or a browser revision into the image. The consumer installs its own matching browser in `dev`'s user cache.
- Defer `underlay-effigy-bundle` input and compatibility floor to its own repository. Its current `minimum_effigy_version = "0.10.0"` cannot honestly advertise this opt-in until a supporting Effigy version is available.

## Dispatch manifest

- **State:** ready; one catalog-image lane.
- **Completion:** one independently reviewed current-base PR merges the catalog option, docs, and targeted assembly checks, with a recorded linux-arm64 build and non-root Chromium launch smoke.
- **Owned mutable paths:** `crates/effigy-catalog/catalog/workspace-rust-bun/{service.toml,compose.fragment.yml,Dockerfile}`; focused catalog tests under `crates/effigy-catalog/tests/integration/workspace.rs`; `docs/guides/067-catalog-services-reference.md` and directly related container guidance; `CHANGELOG.md` if the option is user-facing; this task's evidence under `docs/logs/2026-09/`; `PAPERCUTS.md` for execution friction.
- **Reserved closeout surfaces:** task status, lifecycle records/projections, generation and docs front doors, and the submitted handoff are Queue/hook/Chatterbox owned.
- **Worker:** general Rust/catalog and Docker work in the automatic Queue pool; independent exact-head review.
- **Excluded:** bundle repository edits, Acowtancy app changes, runtime Node installation, browser binary installation in the image, cross-browser support, arbitrary package injection, release mutation, and workflow edits.
- **Escalation:** Chatterbox if Debian arm64 needs a broader base-image or container-policy change.

## Work

1. Inspect catalog parameter and Compose assembly patterns. Add `browser_runtime` default `none`, the build argument, and a bounded Dockerfile branch for `chromium`; fail on unsupported values.
2. Install only the necessary Debian runtime libraries and fonts in the opt-in branch, with apt metadata removed afterward. Keep the default image path free of those optional packages.
3. Add focused assembly assertions for default and explicit modes. Document the input, its image-size tradeoff, consumer-owned browser download, and rebuild requirement.
4. On linux-arm64, build the opted-in image and run a smoke as `dev`: install Chromium for pinned `@playwright/test` 1.55.1 into the user cache, check the browser executable for unresolved shared libraries, launch headless Chromium, render local text, and close cleanly. Record architecture, UID, browser revision, and command/result. Check a default-off build remains usable.
5. Run focused and coherent repository validation; open one non-draft PR for independent review and current-base CI.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Opt-in boundary | Normal Rust/Bun workspaces gain the browser layer | Default Compose arg is `none`; default image builds and retains its normal toolchain; explicit `chromium` selects the apt branch |
| Bounded input | A typo silently becomes a browserless image | Unsupported `browser_runtime` fails the build with a named error |
| Real runtime | `ldd` passes a partial binary but Playwright still cannot launch as `dev` | linux-arm64 image smoke launches the pinned headless browser as non-root, renders local text, and exits; unresolved library check is empty |
| Version ownership | Shared image contains one Chromium revision that disagrees with the repo package | Dockerfile installs system packages only; browser download is performed under the smoke's pinned consumer package |
| Shared-catalog delivery | A project override hides the fix from other consumers | Change lands in the built-in service and its reference docs; bundle remains a separate downstream lane |

## Validation

- Focused `effigy-catalog` integration tests for `workspace-rust-bun` and Compose args.
- One real linux-arm64 image/Playwright smoke as specified above; record a clear blocker if the host cannot build or launch containers.
- `cargo test --workspace --locked`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `effigy qa:docs`, `git diff --check`, and hosted CI on the reviewed head.

## Stop conditions

- Stop if the opt-in cannot launch without changing the Rust/Bun base, container privileges, mount policy, or unrelated catalog services; return a specific proposal.
- Stop if only an x86_64 or root smoke is available, or if the browser launch result is replaced by `ldd` alone.
- Stop on unrelated package/version movement, unowned paths, or stale exact-head review.

## Evidence

Record the package list, default and opt-in Compose args, arm64 image build, non-root Playwright smoke, repository validation, PR/reviewed head/merge, and limits. Queue owns lifecycle closeout.

## Next task

After terminal closeout, Chatterbox advances the external Underlay bundle input with an explicit supporting Effigy version gate; a release remains separately authorized.
