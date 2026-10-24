#!/bin/sh
# Explicit proof: all VM/container/binary/state changes stay in this fresh tree.
set -eu
proof_dir=$(mktemp -d "${TMPDIR:-/tmp}/g237-downgrade.XXXXXX")
printf 'Gateway downgrade proof artifacts: %s\n' "$proof_dir"
target/debug/examples/gateway-downgrade-proof "$proof_dir/home"
cp tests/scripts/gateway-downgrade.py "$proof_dir/proof.py"
case $(uname -m) in
    arm64|aarch64) arch=aarch64; digest=5a1f66b26b6be8962be64f07a50c4f9e2c972fa9a7e2e602c0d80432c182a859 ;;
    x86_64) arch=x86_64; digest=18c6e6b1626cdeafa05c97547d9da6c177825592814f837c9119980f9aa1fc34 ;;
    *) echo 'unsupported proof architecture' >&2; exit 1 ;;
esac
curl --fail --location --max-time 120 --proto '=https' --tlsv1.2 \
    "https://github.com/inflatable-cookie/effigy/releases/download/v0.14.1/effigy-$arch-unknown-linux-gnu" -o "$proof_dir/effigy-0.14.1"
printf '%s  %s\n' "$digest" "$proof_dir/effigy-0.14.1" | shasum -a 256 -c -
chmod 755 "$proof_dir/effigy-0.14.1"
export COLIMA_HOME="$proof_dir/c" COLIMA_CACHE_HOME="$proof_dir/cache" LIMA_HOME="$proof_dir/l"
export DOCKER_CONFIG="$proof_dir/docker" XDG_CONFIG_HOME="$proof_dir/config" XDG_CACHE_HOME="$proof_dir/xdg-cache"
mkdir -p "$COLIMA_HOME" "$DOCKER_CONFIG"
trap 'result=$?; colima stop --force > "$proof_dir/stop.log" 2>&1 || true; printf "exit=%s\n" "$result" >> "$proof_dir/result.log"; exit "$result"' EXIT
colima start --runtime docker --activate=false --ssh-config=false --template=false --binfmt=false \
    --cpus 2 --memory 3 --disk 12 --root-disk 10 --mount "$proof_dir:w" --mount-type virtiofs
export DOCKER_HOST="unix://$COLIMA_HOME/default/docker.sock"
docker run --rm --init --cpus 2 --memory 2g --mount "type=bind,source=$proof_dir,target=/artifacts" ubuntu:24.04 sh -ec '
    apt-get update -qq
    apt-get install -y -qq python3 ca-certificates > /dev/null
    python3 /artifacts/proof.py
'
