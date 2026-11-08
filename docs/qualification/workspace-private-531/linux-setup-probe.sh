# Read-only preflight probe for the Linux setup adapter (v2). POSIX sh.
# Emits key=value lines only, parsed by linux_setup.parse_probe. It writes
# nothing, starts nothing, makes no network call and reads no user home.
# Run as root through the bound VM route only at activation; never from source.
LC_ALL=C
export LC_ALL
echo "uid=$(id -u)"
echo "boot_id=$(cat /proc/sys/kernel/random/boot_id 2>/dev/null)"
echo "system_state=$(systemctl is-system-running 2>/dev/null)"
command -v systemctl >/dev/null 2>&1 && echo "systemctl=present" || echo "systemctl=absent"
command -v systemd-run >/dev/null 2>&1 && echo "systemd_run=present" || echo "systemd_run=absent"
echo "systemd_version=$(systemctl --version 2>/dev/null | sed -n '1s/^systemd \([0-9]*\).*/\1/p')"
echo "cgroup2=$(stat -fc %T /sys/fs/cgroup 2>/dev/null)"
echo "ssh_host_ed25519_fp=$(ssh-keygen -l -f /etc/ssh/ssh_host_ed25519_key.pub 2>/dev/null | awk '{print $2}')"
f=/usr/share/man/man8/sshd.8.gz
if [ -f "$f" ]; then echo "sshd_expiry_time_mentions=$(zcat "$f" 2>/dev/null | grep -c 'expiry-time')"; else echo "sshd_expiry_time_mentions=0"; fi
getent passwd effigy531 >/dev/null 2>&1; echo "effigy531_passwd_exit=$?"
getent group effigy531 >/dev/null 2>&1; echo "effigy531_group_exit=$?"
if [ -e /home/effigy531 ]; then echo "effigy531_home=present"; else echo "effigy531_home=absent"; fi
echo "docker_group=$(getent group docker 2>/dev/null | cut -d: -f1)"
echo "root_source=$(findmnt -no SOURCE / 2>/dev/null)"
echo "root_fstype=$(findmnt -no FSTYPE / 2>/dev/null)"
echo "root_uuid=$(findmnt -no UUID / 2>/dev/null)"
echo "root_parent=$(lsblk -no PKNAME "$(findmnt -no SOURCE / 2>/dev/null)" 2>/dev/null | head -n 1)"
echo "root_serial=$(lsblk -dno SERIAL /dev/sda 2>/dev/null | head -n 1)"
echo "root_model=$(lsblk -dno MODEL /dev/sda 2>/dev/null | head -n 1)"
echo "root_size=$(lsblk -bdno SIZE /dev/sda 2>/dev/null | head -n 1)"
echo "ssh_listen_ipv4=$(ip -4 -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | head -n 1)"
