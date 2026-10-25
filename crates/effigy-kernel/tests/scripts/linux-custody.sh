#!/bin/sh
# Run only through a heavy-admitted test:kernel:*:linux, test:task-engine:*:linux,
# test:fixtures:foreign-generation:linux or proof:runtime-*:linux route (host
# admission). Every host
# write, VM, image and fixture belongs to this fresh directory. No existing
# Colima profile, Docker context or checkout is mounted or changed.
set -eu
proof_mode=${1:-custody}
case "$proof_mode" in
    custody|gate-shared|probe|helpers|runtime-storage|qualification|final-runtime|task-engine|fixtures|task-engine-fixtures) ;;
    *) printf 'Unknown Linux proof mode: %s\n' "$proof_mode" >&2; exit 2 ;;
esac
# This fixture hosts a Lima VM, and a Lima instance socket path must stay under
# the platform's 104-byte limit, so a long platform temp directory (macOS
# `/private/var/folders/.../T`) cannot hold it. A run's `[scratch]` TMPDIR is
# short and is used when it fits; the platform's `/tmp` is the fallback.
temp_root=${TMPDIR:-/tmp}
[ ${#temp_root} -le 40 ] || temp_root=/tmp
proof_dir=$(mktemp -d "$temp_root/k208.XXXXXX")
printf 'Linux qualification artifacts: %s\n' "$proof_dir"
export COLIMA_HOME="$proof_dir/c"
export COLIMA_CACHE_HOME="$proof_dir/cache"
export LIMA_HOME="$proof_dir/l"
export DOCKER_CONFIG="$proof_dir/docker"
export XDG_CONFIG_HOME="$proof_dir/config"
export XDG_CACHE_HOME="$proof_dir/xdg-cache"
mkdir -p "$DOCKER_CONFIG" "$COLIMA_HOME"
source_dir=$(mktemp -d "$proof_dir/source.XXXXXX")
# Colima 0.10 requires COLIMA_HOME to exist; otherwise it falls back to ~/.colima.
git ls-files -c -o --exclude-standard -z | tar --null -T - -cf "$proof_dir/source.tar"
tar -xf "$proof_dir/source.tar" -C "$source_dir"
# Stop only this invocation's private VM, even when validation fails. Retain
# files and logs for inspection; never prune another worker's resources.
trap 'result=$?; colima stop --force > "$proof_dir/stop.log" 2>&1 || true; printf "exit=%s\\n" "$result" >> "$proof_dir/result.log"; exit "$result"' EXIT
colima start --runtime docker --activate=false --ssh-config=false --template=false \
    --binfmt=false --cpus 2 --memory 3 --disk 10 --root-disk 10 --mount "$proof_dir:w" --mount-type virtiofs
# Teardown fault affordance: with the private VM already running, fail before
# the proof so the trap has to stop it and record the exit. Used only by the
# teardown check; no route sets it.
if [ "${EFFIGY_LINUX_PROOF_INJECT_FAILURE:-0}" = 1 ]; then
    printf 'injected failure after VM start: %s\n' "$proof_mode" >&2
    false
fi
mkdir -p "$proof_dir/cargo" "$proof_dir/target"
export DOCKER_HOST="unix://$COLIMA_HOME/default/docker.sock"
set --
if [ "$proof_mode" = gate-shared ]; then
    # Hosted CI is unprivileged: root bypasses the permission-denied proof.
    # Match the disposable bind mounts' owner and use a writable Cargo home.
    # The image already contains this toolchain; avoid root-only rustup updates.
    set -- --user "$(id -u):$(id -g)" --env CARGO_HOME=/proof-cargo \
        --env RUSTUP_TOOLCHAIN=1.97.1 \
        --mount "type=bind,source=$proof_dir/cargo,target=/proof-cargo"
fi
docker run --rm --init --cpus 2 --memory 2g "$@" \
    --env "PROOF_UID=$(id -u)" --env "PROOF_GID=$(id -g)" --env "KERNEL_PROOF_MODE=$proof_mode" \
    --mount "type=bind,source=$source_dir,target=/proof" --workdir /proof \
    --mount "type=bind,source=$proof_dir/cargo,target=/usr/local/cargo/registry" \
    --mount "type=bind,source=$proof_dir/target,target=/proof/target" \
    rust:1.97.1-slim-bookworm sh -ec '
        if [ "$KERNEL_PROOF_MODE" = gate-shared ]; then
            # Same default-feature, concurrent libtest model as hosted CI.
            # libtest has no timeout flag; bound and force-exit its owned tree.
            RUSTFLAGS="--cfg effigy_shared_process" timeout --kill-after=5s 600s cargo test -p effigy-kernel --test gate -- --test-threads=8
            exit
        fi
        apt-get update -qq
        apt-get install -y -qq curl procps git python3 strace lld pkg-config libssl-dev > /dev/null
        curl --fail --location --proto "=https" --tlsv1.2 https://get.nexte.st/latest/linux-arm | tar xz -C /usr/local/cargo/bin
        rustup component add clippy
        if [ "$KERNEL_PROOF_MODE" = task-engine ] || [ "$KERNEL_PROOF_MODE" = task-engine-fixtures ] \
            || [ "$KERNEL_PROOF_MODE" = fixtures ]; then
            # The root task engine against the real Effigy binary, and the
            # fixture ledger refusal of a foreign generation. Both need the root
            # package build; the container is the Linux host, not a guest of
            # another runtime, so no container backend is exercised.
            export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
            export RUSTFLAGS="-C link-arg=-fuse-ld=lld"
            if [ "$KERNEL_PROOF_MODE" != fixtures ]; then
                cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy --test task_engine_kill_matrix --test-threads 2 --no-fail-fast --retries 0 --no-tests fail
            fi
            if [ "$KERNEL_PROOF_MODE" != task-engine ]; then
                cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-test-support --test leak_assertion -E "test(unknown_custody_keeps_fixture_files_and_never_signals_a_foreign_generation) | test(unknown_kernel_state_preserves_fixture_files)" --no-fail-fast --retries 0 --no-tests fail
            fi
            exit
        fi
        if [ "$KERNEL_PROOF_MODE" = qualification ] || [ "$KERNEL_PROOF_MODE" = final-runtime ]; then
            export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
            export RUSTFLAGS="-C link-arg=-fuse-ld=lld"
        fi
        cargo build -p effigy-kernel --features fault-injection --bin kernel-custody-fixture
        cargo clippy -p effigy-kernel --all-targets --features fault-injection -- -D warnings
        if [ "$KERNEL_PROOF_MODE" = final-runtime ]; then
            chown -R "$PROOF_UID:$PROOF_GID" /proof /usr/local/cargo/registry
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups env EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --test probe --lib -E "test(/^probe::/) | binary(probe)" --no-fail-fast --retries 0 --no-tests fail
            cargo build --bin effigy
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups python3 tests/scripts/runtime-write-budget.py target/debug/effigy
            exit
        fi
        if [ "$KERNEL_PROOF_MODE" = qualification ]; then
            chown -R "$PROOF_UID:$PROOF_GID" /proof /usr/local/cargo/registry
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups env EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --lib --test runtime_routing --test custody --test store --test gate --test probe -E "test(/^closure::/) | test(/^supervise::/) | test(/^store::/) | test(/^root::/) | binary(runtime_routing) | binary(custody) | binary(store) | binary(gate) | binary(probe)" --no-fail-fast --retries 0 --no-tests fail
            cargo build --bin effigy
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups python3 tests/scripts/runtime-write-budget.py target/debug/effigy
            exit
        fi
        if [ "$KERNEL_PROOF_MODE" = runtime-storage ]; then
            # Permission refusals must run as the fixture owner, not root.
            chown -R "$PROOF_UID:$PROOF_GID" /proof /usr/local/cargo/registry
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups env EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --lib --test runtime_routing --test custody --test store --test gate -E "test(/^closure::/) | test(/^supervise::/) | test(/^store::/) | test(/^root::/) | binary(runtime_routing) | binary(custody) | binary(store) | binary(gate)" --no-fail-fast --retries 0 --no-tests fail
            exit
        fi
        if [ "$KERNEL_PROOF_MODE" = helpers ]; then
            EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --lib -E "test(/^supervise::tests::/)" --no-fail-fast --retries 0 --no-tests fail
            exit
        fi
        if [ "$KERNEL_PROOF_MODE" = probe ]; then
            chown -R "$PROOF_UID:$PROOF_GID" /proof /usr/local/cargo/registry
            setpriv --reuid="$PROOF_UID" --regid="$PROOF_GID" --clear-groups env EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --test probe --lib -E "test(/^probe::/) | binary(probe)" --no-fail-fast --retries 0 --no-tests fail
            exit
        fi
        EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --test custody --test foundation --lib -E "test(/^custody::tests::/) | test(/^signal::tests::/) | test(/^run::lifecycle::tests::/) | test(/^run::tests::/) | binary(custody) | test(identity_) | test(context_)" --no-fail-fast --retries 0 --no-tests fail
    '
