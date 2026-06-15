# 098 - Effigy Uninstall Command

Status: Initial implementation
Owner: Platform
Created: 2026-06-15

## Intent

Add a top-level `effigy uninstall` command for operators who want Effigy to
remove its local machine state before they stop using the tool.

This is broader than `effigy container profile purge`. Profile purge deletes
the managed Colima runtime data only. `effigy uninstall` should inventory and
remove Effigy-owned local state across command domains.

## Proposed Command Shape

```sh
effigy uninstall [--yes] [--json]
effigy uninstall --plan [--json]
```

Initial scope stays machine-local. It does not edit repos unless a future
explicit repo-scoped flag is added.

## Cleanup Targets

`effigy uninstall --plan` reports:

- user-global config path: `~/.effigy/config.toml`
- user-global catalog directory: `~/.effigy/catalog/`
- managed Colima profile state, equivalent to
  `effigy container profile purge`

Repo-local `.effigy/` directories are out of initial scope because a top-level
machine uninstall cannot safely enumerate every repo the user has ever used.

The Effigy binary itself is out of scope for the first version. Homebrew,
curl-installed binaries, and `cargo install` each have different ownership and
permission rules. The command may print follow-up uninstall hints later.

## Safety Rules

- default mode is plan-only when no `--yes` is supplied on a TTY
- non-TTY mutation requires `--yes`
- JSON mode must never prompt
- deletion must be limited to Effigy-owned paths or state with an ownership
  marker
- Colima profile cleanup must use the same managed-profile guard as
  `container profile purge`
- failures should be itemized; one failed cleanup target should not hide the
  rest of the plan/result

Deferred cleanup targets:

- user-global Effigy home state beyond config/catalog that is clearly
  Effigy-owned
- local gateway resolver/routes state that Effigy can prove it owns
- installed shell completions or hooks only when Effigy can identify the exact
  files it created

## Output Contract Sketch

JSON payload:

```json
{
  "schema": "effigy.uninstall.v1",
  "schema_version": 1,
  "ok": true,
  "mode": "plan",
  "targets": [
    {
      "kind": "user_config",
      "path": "~/.effigy/config.toml",
      "exists": true,
      "owned": true,
      "action": "delete"
    }
  ]
}
```

Mutation result should keep the same `targets` shape and add per-target
`status`, `removed`, and `error` fields.

## Open Questions

- Should `effigy uninstall` remove all of `~/.effigy/` when every child path is
  known, or only known files/directories?
- Should profile purge be opt-out from uninstall, or is managed Colima profile
  cleanup always part of uninstall?
- Should Homebrew/cargo uninstall hints be detected and printed, or left to
  install docs?

## Promotion Targets

- durable cleanup ownership rules belong in
  `docs/contracts/032-secret-and-local-config-management-contract.md` or a new
  local-state cleanup contract
- command surface belongs in `docs/guides/025-command-reference-matrix.md`
- operator docs belong in `docs/guides/010-path-installation-and-release.md`
