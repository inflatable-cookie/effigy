# Mixed-Repo Suite Selection Validation

Date: 2026-02-27
Owner: Platform
Related roadmap: 007 - Test Runner Selection and Environment Policy

## Scope

Validate deterministic suite selection behavior across mixed-suite repositories and prefixed catalog paths.

## Validation Matrix

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test smoke`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result: fails with expected ambiguity error (`cargo-nextest, vitest`) and explicit remediation to provide suite token.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/underlay`
  - result: detects mixed suites (`vitest`, `cargo-nextest`) and shows ordered fallback chain/evidence.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test smoke`
  - cwd: `/Users/betterthanclay/Dev/projects/underlay`
  - result: fails with expected ambiguity error for named invocation in mixed-suite context.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test vitest smoke`
  - cwd: `/Users/betterthanclay/Dev/projects/underlay`
  - result: executes selected suite only (`root/vitest`), no cross-suite fanout; returned non-zero because no test files matched the filter, with expected no-match hint.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- farmyard/test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result: prefixed routing resolves to `farmyard` target root and selects `cargo-nextest` deterministically.

## Findings

- Ambiguous named invocations in mixed-suite repos are now deterministic and guarded.
- Positional suite selection correctly narrows execution to the selected suite.
- Prefixed catalog/sub-repo test routing remains functional with built-in suite selection behavior.
- Error messaging includes direct remediation examples and no-match hinting.

## Conclusion

Roadmap 007 mixed-repo validation requirement is satisfied.
