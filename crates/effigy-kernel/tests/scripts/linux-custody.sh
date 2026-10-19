#!/bin/sh
# Run only through test:kernel:custody:linux (host admission). Every host
# write, VM, image and fixture belongs to this fresh directory. No existing
# Colima profile, Docker context or checkout is mounted or changed.
set -eu
proof_dir=$(mktemp -d /tmp/k208.XXXXXX)
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
mkdir -p "$proof_dir/cargo" "$proof_dir/target"
export DOCKER_HOST="unix://$COLIMA_HOME/default/docker.sock"
docker run --rm --init --cpus 2 --memory 2g \
    --mount "type=bind,source=$source_dir,target=/proof" --workdir /proof \
    --mount "type=bind,source=$proof_dir/cargo,target=/usr/local/cargo/registry" \
    --mount "type=bind,source=$proof_dir/target,target=/proof/target" \
    rust:1.97.1-slim-bookworm sh -ec '
        apt-get update -qq
        apt-get install -y -qq curl procps > /dev/null
        curl --fail --location --proto "=https" --tlsv1.2 https://get.nexte.st/latest/linux-arm | tar xz -C /usr/local/cargo/bin
        rustup component add clippy
        cargo build -p effigy-kernel --features fault-injection --bin kernel-custody-fixture
        cargo clippy -p effigy-kernel --all-targets --features fault-injection -- -D warnings
        EFFIGY_KERNEL_CUSTODY_FIXTURE=/proof/target/debug/kernel-custody-fixture cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none -p effigy-kernel --features fault-injection --test custody --test foundation --lib -E "test(/^custody::tests::/) | test(/^signal::tests::/) | test(/^run::lifecycle::tests::/) | test(/^run::tests::/) | binary(custody) | test(identity_) | test(context_)" --no-fail-fast --retries 0 --no-tests fail
    '
