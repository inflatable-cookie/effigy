#!/bin/sh
# The real post-capability custody shape (effigy#476), natively on Linux.
#
# The installed `effigy` carries `cap_net_bind_service=ep`, so every custody exec
# of it is a secure exec *and* leaves the child with a permitted capability. An
# unprivileged run owner may not read such a child's `/proc/<pid>/environ`, which
# is where custody reads the anchor nonce. This script allocates exactly that
# shape and runs the ignored regression as an ordinary account:
#
#   sudo -v   # once, so the single `sudo -n setcap` below is warm
#   sh crates/effigy-kernel/tests/scripts/custody-capability.sh
#
# Every build and test goes through `scripts/oxide-selector.sh`, the repository's
# admitted Oxide route, so no raw Cargo runs and no selector bypass is possible.
# Everything the script writes lives in one fresh mktemp fixture: the staged
# single-link image, and nothing else. Nothing in the checkout is rewritten and
# no ownership anywhere is changed; the only privileged step is `setcap` on that
# one disposable copy, which is the shape Linux gateway setup itself installs.
# The script refuses to run as root, because root would satisfy the ptrace read
# and prove nothing. The owner's one-run native packet runs it in the proof VM.
set -eu
if [ "$(id -u)" -eq 0 ]; then
    printf '%s\n' 'custody-capability: run as the unprivileged owner, not root (root would satisfy the ptrace read)' >&2
    exit 2
fi
command -v setcap >/dev/null 2>&1 || {
    printf '%s\n' 'custody-capability: setcap is missing; this is a Linux proof (install libcap2-bin)' >&2
    exit 2
}
command -v sudo >/dev/null 2>&1 || {
    printf '%s\n' 'custody-capability: sudo is missing; the disposable copy needs one setcap' >&2
    exit 2
}
owner=$(id -un)
fixture=$(mktemp -d "${TMPDIR:-/tmp}/custody-capability.XXXXXX")
printf 'custody-capability fixture: %s (owner %s)\n' "$fixture" "$owner"
cleanup() {
    result=$?
    rm -rf "$fixture"
    printf 'exit=%s\n' "$result"
    exit "$result"
}
trap cleanup EXIT
# The owner creates it, so the owner can traverse it; 0755 also lets the
# privileged setcap and any transcript follow the same path.
chmod 0755 "$fixture"
sh scripts/oxide-selector.sh build --bin effigy
# A single link: gateway setup refuses a multiply-linked image, and the anchor
# must exec this exact file.
cp target/debug/effigy "$fixture/effigy"
chmod 0755 "$fixture/effigy"
if ! sudo -n setcap cap_net_bind_service=ep "$fixture/effigy"; then
    printf '%s\n' 'custody-capability: `sudo -n setcap` was refused; run `sudo -v` first' >&2
    exit 1
fi
getcap "$fixture/effigy"
links=$(stat -c '%h' "$fixture/effigy")
staged_owner=$(stat -c '%U' "$fixture/effigy")
[ "$links" = 1 ] || {
    printf 'custody-capability: staged image has %s links\n' "$links" >&2
    exit 1
}
[ "$staged_owner" = "$owner" ] || {
    printf 'custody-capability: staged image is owned by %s, not %s\n' "$staged_owner" "$owner" >&2
    exit 1
}
EFFIGY_KERNEL_CUSTODY_CAPABILITY_IMAGE="$fixture/effigy" \
    sh scripts/oxide-selector.sh nextest run \
    --config-file tests/config/gateway-nextest.toml --user-config-file none \
    -p effigy-kernel --features fault-injection --test custody \
    -E 'test(custody_settles_two_setup_probes_from_a_capability_bearing_image)' \
    --run-ignored only --no-capture --no-fail-fast --retries 0 --no-tests fail
