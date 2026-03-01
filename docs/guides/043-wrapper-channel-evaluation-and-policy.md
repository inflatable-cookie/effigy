# 043 - Wrapper Channel Evaluation and Policy

This guide records the decision framework for npm/JS wrapper channels and defines strict delegation rules if a wrapper is retained.

## 1) Current Decision (Phase E Baseline)

Status: no separate npm wrapper channel is required as the default distribution path right now.

Reasoning:
- CI and automation are covered by pinned binary install guidance.
- macOS operator flow is covered by Homebrew channel guidance.
- direct binary invocation reduces drift and support overhead.

Result:
- primary channels remain Rust install + Homebrew + source-run dev flow.
- wrapper support is treated as optional, not default.

## 2) Reassessment Triggers

Re-open wrapper decision only if one or more conditions are true:
- JS-first repositories cannot adopt direct `effigy` binary in CI or local automation.
- onboarding metrics show repeated friction around binary installation.
- Homebrew/Rust channel constraints materially block adoption for target teams.

If none of these conditions are met, keep wrapper channel disabled.

## 3) Wrapper Contract (If Enabled)

If a wrapper is introduced, it must be thin and deterministic:
- delegates all execution to the canonical Effigy binary.
- does not reimplement routing, parsing, or schema behavior.
- requires explicit binary version pinning support.
- preserves exit codes and stdout/stderr behavior from delegated binary.
- includes clear fallback instructions to direct `effigy` invocation.

## 4) Operational Constraints

- one owner for wrapper maintenance and release updates.
- wrapper release cadence must follow Effigy release tags.
- wrapper docs must include deprecation/rollback path.
- wrapper must pass the same smoke matrix as direct binary usage.

## 5) Deprecation Path (If Wrapper Exists)

If wrapper channel is later removed:
1. announce deprecation window and target removal version.
2. provide migration snippets replacing wrapper calls with direct `effigy`.
3. keep compatibility notes in release notes for one full release cycle.
4. remove wrapper docs only after migration checkpoint confirms cutover.

## 6) Validation Checklist

- direct binary path remains documented as primary.
- wrapper usage in docs is explicitly marked optional.
- release notes include wrapper status (retained, optional, deprecated, removed).
- no CI critical path depends exclusively on wrapper channel.

## Related Guides

- [`041-distribution-ci-pinning-and-wrapper-migration.md`](./041-distribution-ci-pinning-and-wrapper-migration.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)

## Next Step

After one full release cycle with current channels, review adoption evidence and either keep wrapper disabled or open a dedicated implementation batch for a strict thin-wrapper channel.
