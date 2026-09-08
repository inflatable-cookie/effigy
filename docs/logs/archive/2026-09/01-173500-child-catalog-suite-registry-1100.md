# Child-Catalog Suite Registry 1100 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.045
Card: 1100
Handoff: `20260901-175827-child-catalog-suite-registry-1100.md`
Papercut: Child-catalog suite task refs lose ancestor `[containers]` registry

## Summary

- Nested host-launched catalog task refs now pin `--repo <originating-root>`
  when the already loaded catalog graph has a shallower ancestor than the
  selected child.
- Child catalog cwd stays on the wrap (`cd <child> && …`). Selector identity
  is unchanged.
- Direct invocation from the child still discovers only that child. No
  Acowtancy, Cream, or manifest-grammar special case.

## Synthetic shape

Parent declares `[containers] default = "workspace"` and
`[test.suites.api] run = [{ task = "api/test:unit" }]`. Child `api` declares
`run_in = "container"` on `test:unit` and has no containers of its own.

Plan command:

`(cd '<child>' && env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 <effigy> 'api/test:unit' --repo '<parent>')`

Suite target root stays the suite catalog. Expanded task cwd is the child.
`--repo` reloads the parent graph so ancestor `[containers]` is present at
nested execution.

## Review oracle → proof

1. Recurrence only passes by moving the child task cwd back to the parent —
   falsified by `run_manifest_task_builtin_test_child_task_ref_pins_ancestor_container_registry`
   and the child-owned suite twin: wrap cwd is the child, `--repo` is the
   ancestor, and `printf inherited-container` is not inlined. Nested preflight
   keeps that child `invocation_cwd`.
2. Ancestor registry is still absent when the task ref reaches execution —
   falsified by feeding the planned nested semantics (child cwd, child
   selector, pinned `--repo`) through `build_execution_preflight`,
   `select_catalog_and_task`, and `effective_task_binding_inputs`. Loaded
   catalogs include ancestor `default = "workspace"`; the inherited-only
   fixtures resolve that default.
3. Preserving the ancestor overrides an explicit child registry —
   falsified by `run_manifest_task_builtin_test_child_explicit_container_registry_still_nests`:
   nested discovery still loads ancestor `workspace`, then the effective
   binding default is `child`. Plan text still keeps `--repo` as fallback
   only.
4. Direct child invocation begins discovering ambient undeclared ancestors —
   falsified by `run_manifest_task_direct_child_task_does_not_inherit_undeclared_ancestor_containers`
   (`test:unit` from the child cwd still errors with `no container target is defined`).
5. The fix special-cases Acowtancy/Cream names or changes manifest grammar —
   falsified by diff scope (`references/resolve.rs` plus suite-selection tests);
   pin uses catalog depth, not names. No Acowtancy files. No TOML grammar change.
6. Plan text/JSON claims a different cwd, task source, or container target than
   execution uses — falsified by `--plan --json`: parent suite target root stays
   the parent; child-owned suite target root stays `…/api`; the rendered command
   is what execution shells. Command-form and host-inlined child refs do not
   grow `--repo`.

## Validation

| Check | Result |
| --- | --- |
| `cargo test --lib -- suite_selection_tests` | 18 passed after nested binding assertions |
| `effigy graph affected` (changed source) | index current after auto-refresh (1729 files); first closeout ran the exact changed module plus full `effigy qa` |
| `effigy test --plan` | cargo-nextest workspace plan |
| `effigy qa` | first closeout: 3649 passed, 1 skipped; docs and JSON contracts passed |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --all-targets -- -D warnings` | passed on first closeout; follow-up is test/log only |
| `git diff --check` | passed |
| rust-quality closeout | RUST-READ-001, RUST-ERR-001; tool status `warning` from crates.io future-incompat noise, not this tranche |

PR75 requested a nested discovery/binding proof rather than plan-text `--repo` presence alone. The renderer assertions remain; the two registry-precedence rows now also inspect typed effective container defaults.

## Downstream boundary

Acowtancy card `162` keeps its workspace-root re-entry workaround until that
owner revalidates against this head. This card does not remove it.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`
- Movement: child-catalog suite task-ref expansion discarded the loaded
  ancestor `[containers]` graph → nested re-entry pins originating `--repo`
  while keeping child cwd
- Remaining gap: Acowtancy revalidation; orchestrator merge of this PR after
  the nested binding proof

## Next Task

Return the exact-head PR to the Effigy orchestrator.
