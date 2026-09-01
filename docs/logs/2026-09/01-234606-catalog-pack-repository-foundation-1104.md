# Catalog-Pack Repository Foundation 1104 Closeout

Status: complete
Created: 2026-09-01
Roadmap: `g08.048`
Spec: `115`
External repository: `inflatable-cookie/effigy-catalog-pack`

## Outcome

- Public source repository foundation merged through external PR
  [`#1`](https://github.com/inflatable-cookie/effigy-catalog-pack/pull/1).
- Accepted exact head:
  `ee242cbc55dbd6ac3f4ae635ecdbd011effcc518`.
- Merge commit: `168b9f530d51f666007663215207a4d9dcfc9c8b`.
- `pack/` is the sole editable catalog root. Version `1.0.0` contains 42
  regular files and has Effigy content identity
  `sha256:511d120f181505f8ecced7687b564c4663663eca8f6f68b2b562c9b676feb29e`.
- One-time import proof pins Effigy commit
  `055595340c2219d3d47296072f5818c524c341f0`, catalog tree
  `539471162c4976551ac720fdcffe6a1de33cef0f`, and support-policy blob
  `20d0194d52c0bbf46677f8d77ca96fb4505df50e`.
- Routine validation is pack-owned and independent. Exact import equality is a
  separate historical proof.
- Local OCI layout, exact-byte pull replay, current-Effigy install and assembly
  smoke, and absent/same-digest/collision no-push cases passed.

## Hosted Controls

- Final hosted validation run
  [`33567574175`](https://github.com/inflatable-cookie/effigy-catalog-pack/actions/runs/33567574175)
  passed on the accepted exact head.
- Actions are restricted to the selected, full-SHA-pinned checkout action with
  read-only workflow permissions.
- The manual rehearsal environment requires the sole operator, permits that
  operator to approve, and forbids administrator bypass.
- Active ruleset `22050144` protects exactly `refs/tags/v*` from update and
  deletion with no bypass actors.
- The committed provider snapshot is explicitly static. A separate GET-only
  live verifier rechecked Actions, environment, and ruleset state on the
  accepted head.
- Workflow inputs cross into shell through quoted environment variables. A
  recurrence guard rejects raw `inputs.*` interpolation in `run` blocks.

## Review

Round one blocked on four classes: duplicate canonical authority, Effigy-based
OCI source identity, missing hosted controls, and a personal-path fallback.
Repairs separated one-time import proof from routine validation, derived OCI
identity from pack source/tag facts, installed protected hosted controls, and
made authority lookup portable.

Round two blocked on three classes: sole-operator environment deadlock, raw
workflow-input shell interpolation, and a static snapshot presented as live
proof. Repairs enabled protected sole-operator approval, bound and guarded
inputs, and split static intended policy from live read-only verification.

Final exact-head review reran `workflow-check`, `provider-controls`, and
`git diff --check`; all passed. The accepted verdict is recorded on PR `#1`.

## Mutation Boundary

Card `1104` created no source tag, GitHub Release, GHCR package, package
visibility, attestation, or `stable` channel state. Those mutations belong only
to card `1105` after a fresh explicit operator instruction.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement: Effigy-owned concrete catalog source -> dedicated public,
  independently versioned pack source with deterministic no-push proof
- Remaining gap: first verified publication, generated Effigy baseline and
  provenance lock, public update cutover, and narrow proposal automation

## Next Task

Request explicit operator authority for card `1105`. Do not begin first
publication until the instruction names the annotated `v1.0.0` tag, GHCR
package/public visibility, attestation, and `stable` movement.
