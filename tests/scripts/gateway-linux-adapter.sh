#!/bin/sh
# All changes, including the VM and executable xattr, live in fresh fixtures.
# Run through proof:gateway:operator:linux for host-wide admission.
set -eu
# This fixture hosts a Lima VM, and a Lima instance socket path must stay under
# the platform's 104-byte limit, so a long platform temp directory (macOS
# `/private/var/folders/.../T`) cannot hold it. A run's `[scratch]` TMPDIR is
# short and is used when it fits; the platform's `/tmp` is the fallback.
temp_root=${TMPDIR:-/tmp}
[ ${#temp_root} -le 40 ] || temp_root=/tmp
proof_dir=$(mktemp -d "$temp_root/g219-linux.XXXXXX")
printf 'Linux gateway adapter artifacts: %s\n' "$proof_dir"
export COLIMA_HOME="$proof_dir/c" COLIMA_CACHE_HOME="$proof_dir/cache" LIMA_HOME="$proof_dir/l"
export DOCKER_CONFIG="$proof_dir/docker" XDG_CONFIG_HOME="$proof_dir/config" XDG_CACHE_HOME="$proof_dir/xdg-cache"
mkdir -p "$COLIMA_HOME" "$DOCKER_CONFIG"
source_dir=$(mktemp -d "$proof_dir/source.XXXXXX")
git ls-files -c -o --exclude-standard -z | tar --null -T - -cf "$proof_dir/source.tar"
tar -xf "$proof_dir/source.tar" -C "$source_dir"
trap 'result=$?; colima stop --force > "$proof_dir/stop.log" 2>&1 || true; printf "exit=%s\n" "$result" >> "$proof_dir/result.log"; exit "$result"' EXIT
colima start --runtime docker --activate=false --ssh-config=false --template=false --binfmt=false \
    --cpus 2 --memory 4 --disk 12 --root-disk 10 --mount "$proof_dir:w" --mount-type virtiofs
export DOCKER_HOST="unix://$COLIMA_HOME/default/docker.sock"
docker run --rm --init --cpus 2 --memory 3g --sysctl net.ipv4.ip_unprivileged_port_start=1024 \
    --mount "type=bind,source=$source_dir,target=/proof" --workdir /proof \
    rust:1.97.1-slim-bookworm sh -ec '
        apt-get update -qq
        apt-get install -y -qq curl util-linux libcap2-bin > /dev/null
        curl --fail --location --proto "=https" --tlsv1.2 https://get.nexte.st/latest/linux-arm | tar xz -C /usr/local/cargo/bin
        # Guest-side temp: this runs inside the container, whose own /tmp is
        # not the host temp, so it deliberately leaves TMPDIR alone.
        native_dir=$(mktemp -d)  # mktemp-exempt: container guest temp
        export CARGO_TARGET_DIR="$native_dir/target"
        cargo build -p effigy-gateway --example linux-adapter-proof
        chmod 755 "$native_dir"
        mkdir "$native_dir/bin"
        fixture="$native_dir/bin/effigy"
        cp "$CARGO_TARGET_DIR/debug/examples/linux-adapter-proof" "$fixture"
        chmod 755 "$fixture"
        # A failing control proves the namespace does not already allow binds.
        if setpriv --reuid=501 --regid=501 --clear-groups "$fixture" probe; then
            echo "negative low-port control unexpectedly succeeded" >&2; exit 1
        fi
        "$fixture" install
        getcap "$fixture"
        test "$("$fixture" report)" = '"installed"'
        # The production unit definition is a pure function; this container has
        # no user manager, so the launch proof is the unit's own content.
        "$fixture" unit | grep -qx 'ExecStart=".*" gateway up'
        "$fixture" unit | grep -qx 'ExecStop=".*" gateway down'
        if "$fixture" unit | grep -q 'Restart'; then
            echo "the launch unit declares a second lifecycle authority" >&2; exit 1
        fi
        setpriv --reuid=501 --regid=501 --clear-groups "$fixture" probe
        "$fixture" remove
        test "$("$fixture" report)" = '"missing"'
        if setpriv --reuid=501 --regid=501 --clear-groups "$fixture" probe; then
            echo "removed capability still permits low-port binds" >&2; exit 1
        fi
        cargo nextest run --config-file tests/config/gateway-nextest.toml --user-config-file none \
            -p effigy-gateway --lib -E "test(setup::tests::native_boot_jobs) | test(setup::tests::managed_setup) | test(setup::tests::setup_symlink) | test(linux_operator::tests::)" \
            --no-fail-fast --retries 0 --no-tests fail
    '
