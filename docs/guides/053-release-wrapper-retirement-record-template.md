# 053 - Release Wrapper Retirement Record Template

Use this guide when maintainers are deciding whether the remaining release
compatibility wrappers should stay for another cycle or be retired.

The current wrappers under evaluation are:

- `scripts/prepare-release.sh`
- `scripts/check-release-gates.sh`
- `scripts/check-release-install-from-tag.sh`

Do not use this record for the durable shell boundaries:

- `scripts/check-release-smoke.sh`
- `scripts/check-distribution-first-publish.sh`
- `scripts/effigy-dev`
- `scripts/install-local-bin-links.sh`

## 1) Decision Rule

Retire the three release compatibility wrappers only when all of the following
are true:

- at least two consecutive real Effigy releases completed through the built-in
  `effigy release ...` flow without wrapper fallback
- hosted release workflows stayed green across that evaluation window
- tag-install validation stayed green across that evaluation window
- no active CI, docs, or downstream operator contract still points to the
  wrapper path as the primary entrypoint
- maintainers explicitly approve the retirement decision

## 2) Template

Copy this into the dated release checkpoint log when a wrapper-retirement
decision is being evaluated:

```md
## Release Wrapper Retirement Record

- Release evaluated: `v0.__.__`
- Prior built-in release in comparison window: `v0.__.__`
- Built-in release path used for both releases:
  - [ ] `effigy release prepare`
  - [ ] `effigy release execute`
- Wrapper fallback used:
  - [ ] no
  - [ ] yes, with explanation recorded
- Hosted release workflows green on both releases:
  - [ ] `CI`
  - [ ] `JSON Contracts`
  - [ ] `Release Binaries`
- Tag install validation green on both releases:
  - [ ] `effigy release verify-install`
- Any active CI/docs/downstream contract still points to wrapper scripts:
  - [ ] no
  - [ ] yes, list them explicitly
- Maintainer decision:
  - [ ] keep wrappers for another release cycle
  - [ ] retire `scripts/prepare-release.sh`
  - [ ] retire `scripts/check-release-gates.sh`
  - [ ] retire `scripts/check-release-install-from-tag.sh`
- Decision owner: `name/team`
- Notes:
```

## 3) Where To Record It

- Put the filled record in the dated release checkpoint log under
  `docs/logs/YYYY-MM/`
- Link that log from `docs/logs/README.md`
- Update release roadmap/protocol status if the decision changes wrapper
  policy

## 4) Outcome Rules

- If any box in the decision rule is not satisfied, keep the wrappers for
  another release cycle.
- If all boxes are satisfied but maintainers still want the wrappers as an
  explicit backstop, record `keep wrappers for another release cycle` and state
  why.
- If the wrappers are retired, remove active docs/CI references in the same
  batch as the deletion so operator guidance does not drift.

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)

## Next Step

When the next real Effigy release closes, paste this record into the checkpoint
log and decide whether the wrapper window stays open for one more cycle.
