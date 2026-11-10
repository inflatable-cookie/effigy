#!/bin/sh
# Task-829 exploratory provisioning on the selected VM only. The 2 GiB pilot
# measures peak use; it is not the final E12 capacity admission or a receipt.
set -eu
umask 077
test "${1-}" = --exploratory-829
test "$(id -u)" = 0
test "$(blkid -s UUID -o value /dev/sda2)" = d488be7b-682f-41ac-b79b-57bbf495a01d
root=/var/lib/effigy-829
mount="$root/rootful"
image="$root/engine-rootful.ext4"
test "$(readlink -f "$root")" = "$root"
test "$(stat -c '%u:%a' "$root")" = 0:750
test ! -e "$image"
test ! -L "$image"
test ! -e "$mount"
test ! -L "$mount"
test ! -e /etc/systemd/system/effigy-829-rootful.service
test ! -L /etc/systemd/system/effigy-829-rootful.service
# Do not disturb the VM's existing Docker, containerd, firewall or routes.
ip -j route | python3 -c 'import sys,json,ipaddress; pools=[ipaddress.ip_network(x) for x in ("172.29.0.0/24","172.30.0.0/16")]; assert all(not any(ipaddress.ip_network(r["dst"],strict=False).overlaps(p) for p in pools) for r in json.load(sys.stdin) if r.get("dst") not in (None,"default"))'
test ! -e /sys/class/net/effigy829br0
ip link add name effigy829br0 type bridge
ip address add 172.29.0.1/24 dev effigy829br0
ip link set effigy829br0 up
fallocate -l 2147483648 "$image"
chmod 600 "$image"
mkfs.ext4 -q -F -m 0 -E nodiscard -L effigy829rootful "$image"
loop=$(losetup --find --show "$image")
printf '%s\n' "$loop" > "$root/rootful.loop"
mkdir -m 750 "$mount"
mount -o nodev,nosuid "$loop" "$mount"
chown root:effigy531 "$mount"
chmod 750 "$mount"
install -d -m 700 "$mount/data" "$mount/containerd" "$mount/containerd/root"
install -d -m 750 -o root -g effigy531 "$mount/run" "$mount/client"
install -d -m 700 -o effigy531 -g effigy531 "$mount/client/home" "$mount/client/tmp" "$mount/client/config"
cat > "$mount/containerd.toml" <<'CONFIG'
version = 3
root = "/var/lib/effigy-829/rootful/containerd/root"
state = "/var/lib/effigy-829/rootful/run/containerd"
disabled_plugins = ["io.containerd.cri.v1.images", "io.containerd.cri.v1.runtime"]
[grpc]
address = "/var/lib/effigy-829/rootful/run/containerd.sock"
CONFIG
printf '{}\n' > "$mount/dockerd.json"
cat > /usr/local/libexec/effigy829-rootful.sh <<'DAEMON'
#!/bin/sh
set -eu
# OCI root directories inherit the daemon's mask when a layer omits '.'.
# The normal daemon mask keeps images usable by their non-root users.
umask 022
root=/var/lib/effigy-829/rootful
/usr/bin/containerd --config "$root/containerd.toml" &
child=$!
printf '%s\n' "$child" > "$root/run/containerd.pid"
count=0
until test -S "$root/run/containerd.sock"; do
    kill -0 "$child"
    count=$((count + 1))
    test "$count" -lt 100
    sleep .1
done
exec /usr/bin/dockerd --config-file="$root/dockerd.json" --host="unix://$root/run/docker.sock" --group=effigy531 \
    --data-root="$root/data" --exec-root="$root/run/docker" --pidfile="$root/run/docker.pid" \
    --containerd="$root/run/containerd.sock" --containerd-namespace=effigy829 \
    --containerd-plugins-namespace=effigy829-plugins --bridge=effigy829br0 \
    --default-address-pool=base=172.30.0.0/16,size=24 --host-gateway-ip=172.29.0.1 \
    --iptables=false --ip6tables=false --ip-forward=false --ip-masq=false \
    --exec-opt=native.cgroupdriver=cgroupfs \
    --cgroup-parent=/system.slice/effigy-829-rootful.service/workloads \
    --log-driver=local --log-opt=max-size=1m --log-opt=max-file=2
DAEMON
chmod 700 /usr/local/libexec/effigy829-rootful.sh
cat > /etc/systemd/system/effigy-829-rootful.service <<'UNIT'
[Unit]
Description=Task 829 dedicated capacity-bounded rootful Docker pilot
After=network-online.target
[Service]
Type=exec
ExecStart=/usr/local/libexec/effigy829-rootful.sh
Environment=TMPDIR=/tmp
Delegate=yes
DelegateSubgroup=manager
ProtectSystem=strict
ProtectHome=yes
# One cgroupfs mount: a bind over only the delegated subtree breaks runc's
# RESOLVE_NO_XDEV reads. The trusted daemon's cgroup-parent stays its own unit.
# This kernel-control filesystem consumes RAM, not persisted engine allocation.
ReadWritePaths=/var/lib/effigy-829/rootful /Volumes/Stash/scratch/effigy/linux-native-829/qualification /sys/fs/cgroup
TemporaryFileSystem=/tmp:rw,nosuid,nodev,size=64M,mode=1777
TemporaryFileSystem=/var/tmp:rw,nosuid,nodev,size=64M,mode=1777
TemporaryFileSystem=/run:rw,nosuid,nodev,size=64M,mode=755
# PrivateDevices drops CAP_MKNOD, which runc's default capability set needs.
# Workloads keep Docker's own private devices, seccomp and AppArmor policy.
MemoryMax=3G
MemorySwapMax=0
TasksMax=1024
LimitCORE=0
KillMode=control-group
TimeoutStopSec=30
Restart=no
StandardOutput=append:/var/lib/effigy-829/rootful/daemon.log
StandardError=append:/var/lib/effigy-829/rootful/daemon.log
UNIT
systemctl daemon-reload
systemctl start effigy-829-rootful.service
count=0
until env -i PATH=/usr/bin:/bin HOME="$mount/client/home" DOCKER_CONFIG="$mount/client/config" \
    /usr/bin/docker --host="unix://$mount/run/docker.sock" info --format '{{.DockerRootDir}}'; do
    count=$((count + 1))
    test "$count" -lt 30
    sleep 1
done
systemctl show effigy-829-rootful.service -p MainPID -p ActiveState -p SubState -p Result
findmnt "$mount"
stat -c '%n %u %g %a %s %b' "$image"
