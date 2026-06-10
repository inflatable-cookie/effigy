# Managed Gateway Seeded Start And Idle Shutdown Correction

Date: 2026-04-20
Roadmap: `g02.013`, `g02.020`

## Summary

Extended the consumer-proof correction batch after re-running the
`underlay-reference` DNS setup against the live `v0.3` surfaces.

The proof exposed three real Effigy lifecycle gaps:

- `managed.gateway = true` did not start the gateway on the workspace-seeded
  TUI path used by `effigy dev`
- the gateway daemon stayed running after the last container route was removed,
  even though the route table was empty
- interrupted or abandoned managed dev sessions could leave an idle
  host-side lifecycle owner shell behind

## What Changed

- moved managed gateway auto-start onto the seeded TUI workspace handoff path,
  not just the later in-process managed runtime branch
- added idle gateway shutdown after container route deregistration when the
  shared route table becomes empty
- kept idle shutdown narrow: stop the daemon, keep resolver setup intact
- hardened managed lifecycle owner shells so they exit when their parent
  runtime disappears, even if the shutdown signal path does not land cleanly
- added optional per-route `service` ownership on container DNS routes, so
  stricter gateway provenance can bind a hostname to one runtime service
  instead of only one compose project
- added focused runner tests around the empty-route shutdown decision

## Consumer Proof Findings

- `underlay-reference` host-side `.test` routing works once the gateway is up
- container-side `.test` callback routing also works once the workspace uses
  the Colima host address (`192.168.5.2` here) instead of
  `extra_hosts = ... :host-gateway`
- gateway registration still trusts declared host ports and can therefore point
  a project hostname at an unrelated listener on the same host port

This batch closes the seeded gateway start gap, the empty-route gateway idle
shutdown gap, the orphaned lifecycle-owner gap, and the host-port ownership
validation gap, and adds an opt-in path for stricter service-level provenance.

## Validation

- `cargo test idle_gateway_shutdown`
  Result: pass
- `cargo test workspace_seeded_task_command_preserves_passthrough_args`
  Result: pass
- `cargo test managed_lifecycle_cleanup_notice_is_stable`
  Result: pass
- `cargo test -p effigy-containers managed_lifecycle_command_renders_one_shot_snapshot_without_screen_clear_loop`
  Result: pass
- `cargo check -p effigy --lib --tests`
  Result: pass
- `cargo test -p effigy-manifest container_dns_config_accepts_additional_routes`
  Result: pass
- `underlay-reference` clean end-to-end repro on updated build
  Result: pass
  Notes: `effigy dev` reached the workspace shell, container-side `.test`
  URLs returned `200`, `Ctrl+C` shut down the managed profile, `exit` closed
  the workspace shell, then gateway status returned `running: false` and
  `route_count: 0`, no underlay containers remained, and
  `.effigy/runtime/managed-lifecycle/dev-stack.state` ended as `stopped`
- `underlay-reference` host-port collision repro on updated build
  Result: pass
  Notes: with a temporary `Python` listener holding `41003`,
  `effigy container stack up --detach` failed with
  `host port 41003 is already held by 'Python'`, gateway status stayed at
  `running: false` and `route_count: 0`, and the updated bring-up path rolled
  the partial stack back down before returning the error
- focused service-provenance runner tests
  Result: pass
  Notes: routes that declare `service = "..."` now validate against the
  matching runtime service; mismatched services fail before route registration

## Next Task

Resume the gateway expansion follow-up from a stronger baseline:

- decide whether repos like `underlay-reference` should start opting into
  per-route `service = "..."` declarations for their multi-surface stacks
