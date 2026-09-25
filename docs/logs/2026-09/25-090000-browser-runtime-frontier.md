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

## 2026-09-25 publication closeout and retained-worker ruling

Protected run `36124764718` published pack `v1.1.0` from source commit `c932c58f64cafd10a70d24907dc77fb81230bb01` under annotated tag object `72f5d7551dc0430fcc83af36066463bd9f1aab82`. OCI `v1.1.0` and `stable` resolve to manifest digest `sha256:5699fcb8641424cc6365feb2a4c4cc7f6056de385fc9dc49f771aec63f6078ba`; unpacked content identity is `sha256:e92cc2f217fa2ba4de302b8376ec558afb042acd3a83e4d33ecfb03dc40606a3`. Independent review of evidence PR #8 verified the attestation, anonymous exact-byte pull, 42 matching source files, and workflow approvals. The evidence PR merged and Queue closed `g10.019`. These facts answer the provenance blocker. Queue's documented versioned `resume_worker` control is the operator path for the retained pre-PR `g10.017` worker after its missing-callback attention episode; the old callback itself must not be forged. Preserve all dirty work and instruct the worker to replace direct snapshot edits with a verified import from this artifact, regenerate the honest lock and pinned identities, then finish review.

## 2026-09-25 lifecycle pin repair

Queue resumed the same `g10.017` worker under a new authenticated run. Its PR #120 passed independent review and merged as `0e7b77dfda715f3c565a68241b92e1d48c995706`, but task closeout initially refused the later human-prose edits to `docs/roadmaps/g10/017-opt-in-chromium-workspace-runtime.md`: the hook requires that task card to match the pinned planning blob outside its generated lifecycle block. The card was restored byte-for-byte to its original planning version. This log and `g10.018`/`g10.019` retain the approved provenance correction: the worker was authorized to import the generated `catalog/` snapshot, lock, and pinned baseline constants only from the verified published artifact, with 42 paths/bytes, content identity, source tag/commit, and digest-bound attestation checked. No `v1.0.1` provenance was claimed for edited bytes. The hook's generated terminal block remains Queue-owned.
