#!/bin/sh
# The caller's Effigy selector owns admission. Every VM, socket, checkout and
# backend resource belongs to this fresh fixture; no user backend is contacted.
set -eu
# This fixture hosts a Lima VM, and a Lima instance socket path must stay under
# the platform's 104-byte limit, so a long platform temp directory (macOS
# `/private/var/folders/.../T`) cannot hold it. A run's `[scratch]` TMPDIR is
# short and is used when it fits; the platform's `/tmp` is the fallback.
temp_root=${TMPDIR:-/tmp}
[ ${#temp_root} -le 40 ] || temp_root=/tmp
proof_dir=$(mktemp -d "$temp_root/q276.XXXXXX")
printf 'Private retirement proof: %s\n' "$proof_dir"
export COLIMA_HOME="$proof_dir/colima" COLIMA_CACHE_HOME="$proof_dir/cache" LIMA_HOME="$proof_dir/lima"
export DOCKER_CONFIG="$proof_dir/docker" XDG_CONFIG_HOME="$proof_dir/config" XDG_CACHE_HOME="$proof_dir/xdg-cache"
mkdir -p "$COLIMA_HOME" "$DOCKER_CONFIG/cli-plugins"
# Private Docker config still needs the installed Compose executable.
cp -L "$(command -v docker-compose)" "$DOCKER_CONFIG/cli-plugins/docker-compose"
cleanup() {
    result=$?
    colima stop --profile alternate --force > "$proof_dir/alternate-stop.log" 2>&1 || true
    colima delete --profile alternate --force > "$proof_dir/alternate-delete.log" 2>&1 || true
    colima stop --force > "$proof_dir/stop.log" 2>&1 || true
    colima delete --force > "$proof_dir/delete.log" 2>&1 || true
    printf 'exit=%s\n' "$result" > "$proof_dir/result.log"
    exit "$result"
}
trap cleanup EXIT
colima start --runtime docker --activate=false --ssh-config=false --template=false --binfmt=false \
    --cpus 2 --memory 3 --disk 10 --root-disk 10 --mount "$proof_dir:w" --mount-type virtiofs
colima start --profile alternate --runtime docker --activate=false --ssh-config=false --template=false --binfmt=false \
    --cpus 1 --memory 2 --disk 10 --root-disk 10 --mount "$proof_dir:w" --mount-type virtiofs
export EFFIGY_PROOF_ALTERNATE_DOCKER="unix://$COLIMA_HOME/alternate/docker.sock"
export DOCKER_HOST="unix://$COLIMA_HOME/default/docker.sock"
unset DOCKER_CONTEXT
python3 tests/scripts/runtime-retire-dead.py "$PWD/target/debug/effigy" "$proof_dir"
