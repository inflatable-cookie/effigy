# Browser runtime frontier

Recorded: 2026-09-25
Roadmap: g10.017

## Decision and provenance

Tom approved the opt-in Playwright/Chromium workspace-runtime plan after the Acowtancy Chatterbox reported g05.195 blocked on a browser oracle. The worker's evidence is in Acowtancy PR #345 at `docs/logs/2026-09/25-034300-g05-195-cream-question-figure-delivery.md`: Playwright 1.55.1 downloaded Chromium revision 1193, but the non-root linux-arm64 container lacked 16 shared libraries.

The approved lane has two repository owners. Effigy `g10.017` delivers the default-off catalog image option and a non-root arm64 launch proof. `underlay-effigy-bundle` then exposes the input and documents a rebuild, with a minimum-version or equivalent capability gate so older Effigy binaries cannot silently ignore an opt-in. The bundle change is a separate PR and does not authorize a release. Acowtancy owns its eventual gesture, keyboard, named-state, and request-log evidence after its workspace rebuild.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`.
- Movement: a shared-image browser blocker has a bounded core task and serial bundle follow-up instead of a per-project override.
- Remaining gap: core implementation/review/merge, supporting Effigy version decision, bundle implementation/review/merge, and Acowtancy browser proof.

## Next Task

Dispatch the core catalog task through Queue. Prepare the bundle follow-up against the merged core contract, then apply the version gate before bundle publication.

## 2026-09-25 provenance correction

The retained `g10.017` worker proved the non-root linux-arm64 Chromium launch, but `cargo test --workspace --locked` rejected its direct edit to Effigy's generated catalog: the snapshot still claims published catalog-pack `v1.0.1` source commit and content identity. The worker left its changes uncommitted and reported a blocker. Tom approved a canonical pack source PR and, after review, separately gated publication. `g10.018` owns that source PR; `g10.019` owns publication; `g10.017` then imports the exact published bytes and lock provenance. The original Queue run's callback identity/ticket gap remains a separate operator recovery problem. Preserve its worker and worktree. No pack tag, OCI mutation, or generated lock hand edit is authorized by this planning update alone.

## 2026-09-25 source closeout and publication readiness

Pack PR #7 merged after independent exact-head review. The pack's `1.1.0` source files match the retained `g10.017` browser-smoke image byte-for-byte; the merged and closed-out source is `c932c58f64cafd10a70d24907dc77fb81230bb01`. Read-only checks on that head passed: live provider controls, Effigy `v0.13.0` release freshness and support policy, publication model, and no-push candidate rehearsal. The `v1.1.0` source tag was absent at readiness, and the organization package was public. Tom's approval covers publication after source review; the protected workflow still enforces its environment checkpoint and exact-source, attestation, public-pull, and channel gates. `g10.019` is ready to dispatch. No tag or OCI write occurred during this readiness check.
