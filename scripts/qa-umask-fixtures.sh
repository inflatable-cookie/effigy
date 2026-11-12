#!/bin/sh
# Linux-sensitive fixture suites under one explicit umask (effigy#520, effigy#949).
#
# A fixture inherits the process umask when it creates its directories and
# files. Linux native qualification found fixture homes and a legacy log
# fixture whose default modes were group-writable under Ubuntu's usual 0002, so
# the kernel's private-state trust refused them. This route re-runs the fixture
# families that own custody, object, publication, serving-host, environment and
# test-support state under 0002, 0027 and 0077: a fixture that leans on the
# ambient umask fails here instead of only on a Linux host. It is host umask
# coverage only and never replaces native Linux qualification.
set -eu

mask=${1:-}
case "$mask" in
  0002 | 0027 | 0077) ;;
  *)
    echo "usage: $0 0002|0027|0077" >&2
    exit 2
    ;;
esac
umask "$mask"
cd "$(dirname "$0")/.."

# Custody, signal and run-lifecycle fixtures under fault injection.
sh scripts/oxide-selector.sh build -p effigy-kernel --features fault-injection --bin kernel-custody-fixture
EFFIGY_KERNEL_CUSTODY_FIXTURE=$PWD/target/debug/kernel-custody-fixture \
  sh scripts/oxide-selector.sh nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --test custody --lib -E 'test(/^custody::tests::/) | test(/^signal::tests::/) | test(/^run::lifecycle::tests::/) | binary(custody)' --no-fail-fast --retries 0 --no-tests fail

# Object store, serving-host session and file-trust fixtures.
sh scripts/oxide-selector.sh nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --lib -E 'test(/^objects::tests::/) | test(/^workspace_session::tests::/) | test(/^fs::/)' --no-fail-fast --retries 0 --no-tests fail

# Publication and environment fixtures in the container policy.
sh scripts/oxide-selector.sh nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-containers --lib -E 'test(/^environment::tests::/) | test(/^policy::render::tests::/)' --no-fail-fast --retries 0 --no-tests fail

# The fixture helpers themselves, including the suite guard.
sh scripts/oxide-selector.sh nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-test-support --no-fail-fast --retries 0 --no-tests fail

# The runner's environment fixtures, mirroring the `test:fixtures:environment`
# selection: these write vaults and logs that stand in for private state.
sh scripts/oxide-selector.sh nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy -p effigy-builtin -p effigy-cli -p effigy-containers -p effigy-core -p effigy-gateway -p effigy-managed -p effigy-release -p effigy-rhai -p effigy-test-support --lib --filterset 'package(effigy-rhai) | test(/^build_info::tests::/) | test(/^header::tests::/) | (package(effigy-builtin) & test(/^completion::/)) | (package(effigy-gateway) & test(/^tls::tests::/)) | (package(effigy-containers) & test(/mount_spec::tests::|workspace::.*host.*tests::/)) | test(env_resolution::tests::) | test(failing_and_passing_gates_persist_logs_and_redacted_environment) | test(/^runner::bootstrap_command::tests::/) | test(/^runner::container_command::secret_env::tests::/) | test(/^runner::state_command::tests::/) | test(permission_reuse_headless_flag_grants_no_privilege_and_implies_no_tty) | test(generation_flag_is_cleared_after_the_hook_succeeds_or_fails)' --no-fail-fast --retries 0 --no-tests fail
