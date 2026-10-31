#!/bin/sh
# The real post-capability custody shape (effigy#476), natively on Linux.
#
# The installed `effigy` carries `cap_net_bind_service=ep`, so every custody
# exec of it is a secure exec that also leaves the child with a permitted
# capability. An unprivileged run owner may not read such a child's
# `/proc/<pid>/environ`, which is where custody reads the anchor nonce. This
# script allocates exactly that shape and runs the ignored regression as an
# unprivileged account:
#
#   sudo sh crates/effigy-kernel/tests/scripts/custody-capability.sh
#
# It builds the product binary, installs a single-link copy of it with the
# capability, and hands that path to the test. Every write lives in a fresh
# mktemp directory; only the copy gets a capability, and the copy is removed
# before exit. Run through an admitted Effigy route or directly on a Linux host;
# the owner's one-run native packet runs it in the proof VM.
set -eu
if [ "$(id -u)" -ne 0 ]; then
    printf '%s\n' 'custody-capability: run as root so only the copy can hold a capability' >&2
    exit 2
fi
owner=${EFFIGY_CAPABILITY_OWNER:-${SUDO_UID:-}}
if [ -z "$owner" ] || [ "$owner" = 0 ]; then
    printf '%s\n' 'custody-capability: set EFFIGY_CAPABILITY_OWNER (or run through sudo) to a non-root account' >&2
    exit 2
fi
command -v setcap >/dev/null 2>&1 || {
    printf '%s\n' 'custody-capability: setcap is missing (install libcap2-bin)' >&2
    exit 2
}
work=$(mktemp -d "${TMPDIR:-/tmp}/custody-capability.XXXXXX")
printf 'custody-capability artifacts: %s\n' "$work"
cleanup() {
    result=$?
    rm -f "$work/effigy"
    rmdir "$work" 2>/dev/null || true
    printf 'exit=%s\n' "$result"
    exit "$result"
}
trap cleanup EXIT
cargo build -p effigy --bin effigy
# A single link: gateway setup refuses a multiply-linked image, and the anchor
# must exec this exact file.
cp target/debug/effigy "$work/effigy"
chmod 755 "$work/effigy"
setcap cap_net_bind_service=ep "$work/effigy"
getcap "$work/effigy"
links=$(stat -c '%h' "$work/effigy")
[ "$links" = 1 ] || { printf 'custody-capability: image has %s links\n' "$links" >&2; exit 1; }
# The unprivileged owner needs the checkout and the Cargo home it reads.
chown -R "$owner" target
owner_gid=$(id -g "$owner")
setpriv --reuid="$owner" --regid="$owner_gid" --clear-groups \
    env EFFIGY_KERNEL_CUSTODY_CAPABILITY_IMAGE="$work/effigy" \
    cargo nextest run --config-file tests/config/gateway-nextest.toml \
    --user-config-file none -p effigy-kernel --features fault-injection \
    --test custody \
    -E 'test(custody_settles_two_setup_probes_from_a_capability_bearing_image)' \
    --run-ignored only --no-capture --no-fail-fast --retries 0 --no-tests fail
