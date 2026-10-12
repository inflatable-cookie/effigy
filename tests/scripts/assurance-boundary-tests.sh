#!/usr/bin/env bash
# Membership only; the following matrix members execute each family once.
set -euo pipefail
listing=$(mktemp "${TMPDIR:-/tmp}/effigy-assurance-list.XXXXXX")
trap 'rm -f "$listing"' EXIT
cargo nextest list -p effigy --lib --message-format json > "$listing"
python3 - "$listing" <<'PYTHON'
import json,sys
payload=json.load(open(sys.argv[1]))
actual={name for suite in payload['rust-suites'].values() for name in suite.get('testcases', {})}
required=['runner::container_command::bounded_exec::tests::public_child_guard_reaps_on_unwind_and_preserves_foreign_child', 'runner::container_command::bounded_exec::tests::public_entry_bounds_discovery_preparation_and_fallback_on_the_full_path', 'runner::container_command::bounded_exec::tests::public_runtime_deadline_refuses_before_command', 'runner::container_command::bounded_exec::tests::public_runtime_flood_refuses_before_command', 'runner::container_command::bounded_exec::tests::public_runtime_identity_mismatch_refuses_before_command', 'runner::container_command::bounded_exec::tests::public_workspace_identity_invalid_requires_refresh', 'runner::container_command::bounded_exec::tests::public_workspace_deadline_refuses_before_command', 'runner::container_command::bounded_exec::tests::public_workspace_flood_refuses_before_command', 'runner::container_command::bounded_exec::tests::public_fallback_draws_from_the_same_cumulative_budget', 'runner::container_command::bounded_exec::tests::public_launch_failure_redacts_human_and_json_diagnostics', 'runner::container_command::bounded_exec::tests::public_earlier_deadline_is_incomplete_launch_evidence', 'runner::container_command::bounded_exec::tests::public_nonzero_inner_failure_emits_no_captured_secrets', 'runner::container_command::bounded_exec::tests::expired_public_entry_refuses_before_the_first_discovery_spawn', 'runner::exec_command::bounded::tests::closed_streams_still_wait_for_exit_or_deadline', 'runner::exec_command::bounded::tests::inherited_writer_is_terminated_after_its_leader_exits', 'runner::exec_command::transport::capture_deadline_tests::legacy_deadline_bounds_inherited_writer_final_drain']
missing=[name for name in required if name not in actual]
if missing:
    raise SystemExit('required assurance proofs absent: '+', '.join(missing))
print(f'{len(required)} exact required assurance proof names present')
PYTHON
