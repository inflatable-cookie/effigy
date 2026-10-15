#!/usr/bin/env bash
set -euo pipefail

: "${EFFIGY_HOST_RUN_PRIVATE_SERVER:?set this to the reviewed private Queue checkout}"

fixture_root=$(mktemp -d /tmp/effigy-parent-isolation.XXXXXX)
if [ ! -d "$fixture_root" ] || [ -L "$fixture_root" ]; then
  printf 'mktemp did not return an owned fixture directory\n' >&2
  exit 1
fi

cleanup() {
  local status=$?
  trap - EXIT
  if [ -d "$fixture_root" ] && [ ! -L "$fixture_root" ]; then
    rm -rf -- "$fixture_root" || status=$?
  else
    printf 'fixture root is missing or ambiguous; refusing cleanup\n' >&2
    status=1
  fi
  exit "$status"
}
trap cleanup EXIT

export TMPDIR="$fixture_root"

# Build the lib executable without running it: the private admitted runner
# first reproduces old outside-root state placement, then exercises every
# selected fixture under the corrected authenticated checkout root.
if [[ "${1:-}" != contracts ]]; then
cargo nextest run \
  --config-file tests/config/gateway-nextest.toml \
  --user-config-file none \
  -p effigy \
  --lib \
  --test bootstrap_cli_tests \
  --test env_schema_tests \
  --test secrets_local_dev_cli_tests \
  --test cli_output_tests \
  --no-run
fi

filter='test(fixture_parent_runner_selector_succeeds_with_authenticated_parent_root) | test(binary_cli_fixtures_succeed_with_authenticated_parent_root) | test(forged_expired_and_outside_root_tokens_exit_77_and_never_queue) | test(admitted_container_selector_stays_on_host_and_executes_its_resolved_payload) | test(complete_cli_fixture_family_succeeds_with_authenticated_parent_root)'
if [[ "${1:-}" == contracts ]]; then
  filter='test(complete_contract_fixture_family_succeeds_with_authenticated_parent_root) | test(full_contract_fixture_family_succeeds_with_authenticated_parent_root) | test(forged_expired_and_outside_root_tokens_exit_77_and_never_queue)'
elif [[ "${1:-}" == workspace ]]; then
  filter='test(workspace_fixture_family_) | test(workspace_parent_membership_) | test(forged_expired_and_outside_root_tokens_exit_77_and_never_queue)'
elif [[ "${1:-}" == cli ]]; then
  filter='test(binary_cli_fixtures_succeed_with_authenticated_parent_root) | test(complete_cli_fixture_family_succeeds_with_authenticated_parent_root)'
elif [[ "${1:-}" == all-cli ]]; then
  filter='test(complete_cli_fixture_family_succeeds_with_authenticated_parent_root)'
elif [[ "${1:-}" == non-cli ]]; then
  filter='test(fixture_parent_runner_selector_succeeds_with_authenticated_parent_root) | test(binary_cli_fixtures_succeed_with_authenticated_parent_root) | test(forged_expired_and_outside_root_tokens_exit_77_and_never_queue) | test(admitted_container_selector_stays_on_host_and_executes_its_resolved_payload)'
fi

ignored_options=(--run-ignored only)
failure_options=(--max-fail 1:immediate)
if [[ "${1:-}" == workspace ]]; then
  failure_options=(--no-fail-fast)
  ignored_options=(--run-ignored all)
fi

EFFIGY_REQUIRE_HOST_RUN_PRIVATE_SERVER=1 \
  cargo nextest run \
    --config-file tests/config/gateway-nextest.toml \
    --user-config-file none \
    -p effigy \
    --test host_scheduler_integration \
    "${ignored_options[@]}" \
    --filterset "$filter" \
    --no-capture \
    "${failure_options[@]}" \
    --retries 0 \
    --no-tests fail \
    -j 1
