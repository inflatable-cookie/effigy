"""Fixed read-only guest observations for the Linux writer-bound capability facts.

Observation source only. Each entry is one reviewed, fixed argv that reads kernel
or documented tool output and writes nothing; its exact bytes are retained by the
caller and decoded here into schema-bounded facts with one explicit status. This
module cannot start a unit, mount, enable quota, download, install, open a
session or correlate a host disk with a guest device, and no fact it produces
grants setup, disk-mechanism, hard-limit, association or trust credit.
"""
import hashlib
import json
import re
import time

# Status vocabulary: exactly one per read.
OBSERVED = 'observed'              # completed and decoded under the fixed schema
ABSENT = 'absent'                  # the documented tool or capability is not installed
UNSUPPORTED = 'unsupported'        # the documented invocation or filesystem is not supported here
NOT_FOUND = 'not-found'            # the documented inventory has no entry for this device
AMBIGUOUS = 'ambiguous'            # more than one candidate, or a mapper the fixed schema cannot resolve
FAILED = 'failed'                  # the read ran and did not complete
MALFORMED = 'malformed'            # duplicate, oversize, truncated or otherwise invalid output
NOT_ATTEMPTED = 'not-attempted'    # the shared observation window ended before this read

# An unfinished read: the observation is incomplete and the caller records a finding. Absence,
# unsupported invocations, a missing inventory entry and an ambiguous mapper are negative
# observations of this guest, not broken reads, so they stay facts without a finding.
INCOMPLETE = frozenset({FAILED, MALFORMED, NOT_ATTEMPTED})

BUDGET_SECONDS = 90
MIN_ROUTE_SECONDS = 1
MAX_ROUTE_BYTES = 262144
MAX_LINES = 4096
MAX_BLOCKS = 1024
MAX_FEATURES = 128
MAX_VALUE = 512

NAME = re.compile(r'^[A-Za-z0-9._:-]{1,64}$')
MAJMIN = re.compile(r'^[0-9]{1,7}:[0-9]{1,7}$')
COUNT = re.compile(r'^[0-9]{1,20}$')
BINARY = re.compile(r'^(0|1)$')
SYSFS_PATH = re.compile(r'^/sys/(devices|module)/[A-Za-z0-9._:/+@-]{1,512}$')
DEVICE_PATH = re.compile(r'^/[A-Za-z0-9._:/+-]{1,255}$')
UUID_TEXT = re.compile(r'^[0-9a-fA-F-]{1,64}$')
FEATURE = re.compile(r'^[a-z0-9_]{1,32}$')
FSTYPE = re.compile(r'^[a-z0-9._+-]{1,32}$')
OPTION = re.compile(r'^[A-Za-z0-9_=+.:-]{1,64}$')
CONTROLLER = re.compile(r'^[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$')
HEX = re.compile(r'^[0-9a-f]{0,2048}$')
PROPERTY = re.compile(r'^(infinity|[0-9]{1,20})$')
IO_DEVICE = re.compile(r'^/[A-Za-z0-9._:/+@-]{1,255}$')
IO_RATE = re.compile(r'^(infinity|[0-9]{1,20})$')

# A device-map, software-raid, loop or otherwise virtual controller cannot be resolved to one
# physical controller by this schema; the facts say so instead of guessing.
CONTROLLER_KINDS = (('virtio', re.compile(r'^virtio[0-9]+$')), ('ata', re.compile(r'^ata[0-9]+$')),
                    ('nvme', re.compile(r'^nvme[0-9]+$')), ('usb', re.compile(r'^usb[0-9]+$')),
                    ('mmc', re.compile(r'^mmc[0-9]+$')), ('scsi', re.compile(r'^host[0-9]+$')),
                    ('pci', CONTROLLER), ('virtual', re.compile(r'^(dm|md|loop|nbd|zram|ram)[0-9]*$')))

# Documented systemd resource-control properties. The IO entries are a(st) device+rate lists, so
# they decode as bounded device=rate pairs and never as a scalar byte count.
SCALAR_PROPERTY_KEYS = ('MemoryMax', 'MemoryHigh', 'MemorySwapMax', 'TasksMax', 'CPUWeight')
ARRAY_PROPERTY_KEYS = ('IOReadBandwidthMax', 'IOWriteBandwidthMax', 'IOReadIOPSMax', 'IOWriteIOPSMax')
PROPERTY_KEYS = SCALAR_PROPERTY_KEYS + ARRAY_PROPERTY_KEYS
QUOTA_TOOLS = ('repquota', 'quota', 'quotacheck', 'quotaon', 'edquota')
CAPABILITY_TOOLS = ('lsblk', 'findmnt', 'blkid', 'dumpe2fs', 'tune2fs', 'repquota', 'quota', 'quotacheck',
                    'quotaon', 'edquota', 'systemctl', 'systemd-run', 'readlink', 'od')
LSBLK_FIELDS = frozenset({'name', 'maj:min', 'pkname', 'type', 'size', 'fstype', 'uuid', 'mountpoint',
                          'children'})
LSBLK_TYPES = re.compile(r'^[a-z0-9]{1,16}$')

# The facts below stay observations. Each string restates what the observation cannot establish so a
# later reader cannot upgrade a byte bound or a rate into an allocation quota.
LIMITS = {
    'aggregate_filesystem_allocation': 'not established',
    'hard_quota': 'not established',
    'writer_association': 'not established',
    'guest_disk_association': 'unknown',
    'setup_credit': 'none',
    'disk_mechanism_credit': 'none',
    'hard_limit_credit': 'none',
    'trust_credit': 'none',
    'root_device_name': 'may match the reviewed guest history; a kernel name is not an association',
    'host_guest_serial_equality': 'not established and not inferred from a similar size, model or name',
    'memory_max': 'byte bound of one control group; bounds memory and is not an aggregate allocation',
    'memory_high': 'throttling byte threshold of one control group; not an allocation',
    'io_write_bandwidth_max': ('per-device write rate in bytes per second (the brief IOWriteMax '
                               'shorthand); a rate, not an aggregate allocation'),
    'io_read_bandwidth_max': ('per-device read rate in bytes per second; a rate, not an aggregate allocation'),
    'io_read_iops_max': 'per-device read operations per second; a rate, not an allocation',
    'io_write_iops_max': 'per-device write operations per second; a rate, not an allocation',
    'tasks_max': 'task count bound of one control group; not an allocation',
    'rlimit_fsize': 'a per-file size limit is not a filesystem allocation quota',
    'sampling_and_post_growth': 'sampled counters and post-growth checks are not an allocation quota',
    'filesystem_features': 'a feature bit is a documented capability of this filesystem, not a bound',
}

# ----------------------------------------------------------------- fixed read routes

BLOCK_SYSFS = '''# effigy-linux-observation: block-sysfs
LC_ALL=C
export LC_ALL
if command -v od >/dev/null 2>&1; then encoding=hex; else encoding=unavailable; fi
printf 'encoding %s\\n' "$encoding"
encode() {
  if [ "$encoding" = hex ]; then od -An -v -tx1 2>/dev/null | tr -d ' \\n'; fi
}
for d in /sys/class/block/*; do
  [ -e "$d" ] || continue
  n=${d##*/}
  case $n in ''|*[!A-Za-z0-9._:-]*) printf 'invalid-name\\n'; exit 71;; esac
  v=$(cat "/sys/class/block/$n/dev" 2>/dev/null) || v=
  case $v in [0-9]*:[0-9]*) ;; *) v=invalid;; esac
  p=0
  [ -e "/sys/class/block/$n/partition" ] && p=1
  z=$(cat "/sys/class/block/$n/size" 2>/dev/null) || z=
  case $z in ''|*[!0-9]*) z=invalid;; esac
  r=$(cat "/sys/class/block/$n/ro" 2>/dev/null) || r=
  case $r in 0|1) ;; *) r=invalid;; esac
  m=$(cat "/sys/class/block/$n/removable" 2>/dev/null) || m=
  case $m in 0|1) ;; *) m=invalid;; esac
  s=$(readlink -f "/sys/class/block/$n" 2>/dev/null) || s=
  case $s in /sys/devices/*|/sys/module/*) ;; *) s=invalid;; esac
  case $s in *[!A-Za-z0-9._:/+@-]*) s=invalid;; esac
  printf 'block %s %s %s %s %s %s %s\\n' "$n" "$v" "$p" "$z" "$r" "$m" "$s"
  if [ "$p" = 0 ]; then
    for a in serial model vendor; do
      f="/sys/class/block/$n/device/$a"
      if [ ! -e "$f" ]; then printf 'attr %s %s missing\\n' "$n" "$a"; continue; fi
      if [ "$encoding" != hex ]; then printf 'attr %s %s unavailable\\n' "$n" "$a"; continue; fi
      h=$(cat "$f" 2>/dev/null | encode)
      if [ -z "$h" ]; then printf 'attr %s %s empty\\n' "$n" "$a"
      else printf 'attr %s %s hex:%s\\n' "$n" "$a" "$h"; fi
    done
  fi
done
'''

BLOCK_LSBLK = '''# effigy-linux-observation: block-lsblk
LC_ALL=C
export LC_ALL
command -v lsblk >/dev/null 2>&1 || exit 127
# The documented invocation is checked before the evidence read. A build that reports the option
# itself as unknown is unsupported (126); any other failed check stays a failed read (125) so a
# broken read is never reported as an unsupported command. -a keeps size-zero devices visible.
error=$(lsblk -J -a -b -o NAME 2>&1 >/dev/null) || case $error in
  *"unrecognized option"*|*"invalid option"*|*"unknown option"*) exit 126;;
  *) exit 125;;
esac
exec lsblk -J -a -b -o NAME,MAJ:MIN,PKNAME,TYPE,SIZE,FSTYPE,UUID,MOUNTPOINT
'''

MOUNTINFO = '''# effigy-linux-observation: mountinfo
LC_ALL=C
export LC_ALL
[ -r /proc/self/mountinfo ] || exit 66
while IFS= read -r line; do printf '%s\\n' "$line"; done < /proc/self/mountinfo
'''

FILESYSTEM = '''# effigy-linux-observation: filesystem
LC_ALL=C
export LC_ALL
majmin=
fstype=
count=0
while IFS= read -r line; do
  set -f
  set -- $line
  [ "$5" = "/" ] || continue
  count=$((count + 1))
  if [ "$count" -gt 1 ]; then printf 'obs status duplicate-root\\n'; exit 65; fi
  majmin=$3
  seen=0
  for field in "$@"; do
    if [ "$seen" = 1 ]; then fstype=$field; break; fi
    [ "$field" = "-" ] && seen=1
  done
done < /proc/self/mountinfo
printf 'obs root_majmin %s\\n' "${majmin:-unknown}"
printf 'obs root_fstype %s\\n' "${fstype:-unknown}"
case $majmin in [0-9]*:[0-9]*) ;; *) printf 'obs status ambiguous-root\\n'; exit 65;; esac
if ! command -v readlink >/dev/null 2>&1; then printf 'obs tool absent\\n'; exit 69; fi
link=/sys/dev/block/$majmin
if [ ! -e "$link" ]; then printf 'obs status no-block-device\\n'; exit 66; fi
resolved=$(readlink -f "$link" 2>/dev/null) || resolved=
name=${resolved##*/}
case $name in ''|*[!A-Za-z0-9._:-]*) printf 'obs status invalid-device-name\\n'; exit 67;; esac
printf 'obs root_device %s\\n' "$name"
case $fstype in
  ext2|ext3|ext4) ;;
  *) printf 'obs status unsupported-filesystem\\n'; exit 68;; esac
if ! command -v dumpe2fs >/dev/null 2>&1; then printf 'obs tool absent\\n'; exit 69; fi
printf 'obs tool present\\n'
if [ ! -r "/dev/$name" ]; then printf 'obs status read-failed\\n'; exit 70; fi
exec dumpe2fs -h "/dev/$name"
'''

DEVICE_UUIDS = '''# effigy-linux-observation: device-uuids
LC_ALL=C
export LC_ALL
command -v readlink >/dev/null 2>&1 || exit 127
for k in by-uuid by-partuuid; do
  d=/dev/disk/$k
  if [ ! -d "$d" ]; then printf 'dir %s absent\\n' "$k"; continue; fi
  printf 'dir %s present\\n' "$k"
  for p in "$d"/*; do
    [ -e "$p" ] || [ -L "$p" ] || continue
    id=${p##*/}
    case $id in ''|*[!A-Za-z0-9._:-]*) printf 'invalid-entry %s\\n' "$k"; continue;; esac
    t=$(readlink -f "$p" 2>/dev/null) || t=
    n=${t##*/}
    case $n in ''|*[!A-Za-z0-9._:-]*) n=unresolved;; esac
    printf 'entry %s %s %s\\n' "$k" "$id" "$n"
  done
done
'''

CAPABILITIES = '''# effigy-linux-observation: capabilities
LC_ALL=C
export LC_ALL
for t in lsblk findmnt blkid dumpe2fs tune2fs repquota quota quotacheck quotaon edquota systemctl systemd-run readlink od; do
  p=$(command -v "$t" 2>/dev/null) || p=
  case $p in
    /*) case $p in *[!A-Za-z0-9._:/+-]*) p=invalid;; esac ;;
    '') p=absent ;;
    *) p=invalid ;;
  esac
  printf 'tool %s %s\\n' "$t" "$p"
done
for t in lsblk findmnt dumpe2fs; do
  command -v "$t" >/dev/null 2>&1 || continue
  case $t in
    dumpe2fs) flag=-V ;;
    *) flag=--version ;;
  esac
  # Some tools (dumpe2fs) print their banner on stderr, so both documented streams are read. The
  # record names the stream it came from; no temp file is written by this read-only probe.
  v=$("$t" "$flag" 2>/dev/null | while IFS= read -r l; do printf '%s' "$l"; break; done)
  stream=stdout
  if [ -z "$v" ]; then
    v=$("$t" "$flag" 2>&1 >/dev/null | while IFS= read -r l; do printf '%s' "$l"; break; done)
    stream=stderr
  fi
  if [ -z "$v" ]; then printf 'version %s unavailable\\n' "$t"; continue; fi
  printf 'version %s %s %s\\n' "$t" "$stream" "$v"
done
'''

CGROUP_UNITS = '''# effigy-linux-observation: cgroup-units
LC_ALL=C
export LC_ALL
for f in cgroup.controllers cgroup.subtree_control; do
  p=/sys/fs/cgroup/$f
  if [ ! -r "$p" ]; then printf 'file %s unreadable\\n' "$f"; continue; fi
  v=$(cat "$p" 2>/dev/null) || v=
  if [ -z "$v" ]; then printf 'file %s empty\\n' "$f"; continue; fi
  # Controller names are space separated; the decoder validates every name.
  printf 'file %s %s\\n' "$f" "$v"
done
if ! command -v systemctl >/dev/null 2>&1; then printf 'unit_observation absent\\n'; exit 0; fi
for u in system.slice user.slice; do
  out=$(systemctl show -p MemoryMax -p MemoryHigh -p MemorySwapMax -p IOReadBandwidthMax -p IOWriteBandwidthMax -p IOReadIOPSMax -p IOWriteIOPSMax -p TasksMax -p CPUWeight -- "$u" 2>/dev/null) || out=
  if [ -z "$out" ]; then printf 'unit %s unreadable\\n' "$u"; continue; fi
  printf 'unit %s present\\n' "$u"
  printf '%s\\n' "$out" | while IFS= read -r l; do
    k=${l%%=*}
    v=${l#*=}
    case $k in
      MemoryMax|MemoryHigh|MemorySwapMax|TasksMax|CPUWeight) ;;
      IOReadBandwidthMax|IOWriteBandwidthMax|IOReadIOPSMax|IOWriteIOPSMax) ;;
      *) continue;;
    esac
    if [ -z "$v" ]; then printf 'property %s %s empty\\n' "$u" "$k"; continue; fi
    case $k in
      IO*)
        set -f
        set -- $v
        if [ "$(($# % 2))" -ne 0 ]; then printf 'property %s %s invalid\\n' "$u" "$k"; continue; fi
        while [ $# -ge 2 ]; do printf 'property %s %s %s=%s\\n' "$u" "$k" "$1" "$2"; shift 2; done
        continue;;
    esac
    case $v in *[!A-Za-z0-9._+-]*) v=invalid;; esac
    printf 'property %s %s %s\\n' "$u" "$k" "$v"
  done
done
'''

ROUTES = (
    {'id': 'block-sysfs', 'script': BLOCK_SYSFS, 'timeout': 20},
    {'id': 'block-lsblk', 'script': BLOCK_LSBLK, 'timeout': 20},
    {'id': 'mountinfo', 'script': MOUNTINFO, 'timeout': 20},
    {'id': 'filesystem', 'script': FILESYSTEM, 'timeout': 20},
    {'id': 'device-uuids', 'script': DEVICE_UUIDS, 'timeout': 20},
    {'id': 'capabilities', 'script': CAPABILITIES, 'timeout': 30},
    {'id': 'cgroup-units', 'script': CGROUP_UNITS, 'timeout': 30},
)


class Bad(ValueError):
    """One read's bytes cannot be decoded under the fixed schema."""

    def __init__(self, status, reason):
        super().__init__(reason)
        self.status = status
        self.reason = reason


def route_ids():
    return tuple(route['id'] for route in ROUTES)


def route_argv(route):
    return ['/bin/sh', '-c', route['script']]


def routes_digest():
    """Bind the exact fixed argv of every route, not a caller-supplied list."""
    rows = [[route['id'], route_argv(route), route['timeout'], MAX_ROUTE_BYTES] for route in ROUTES]
    return hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


# ----------------------------------------------------------------- decoding helpers

def _decode(raw, cap=MAX_ROUTE_BYTES):
    """Split one read into space-separated records; a partial or oversize read is not decoded."""
    if not isinstance(raw, bytes) or len(raw) > cap:
        raise Bad(MALFORMED, 'output exceeds the fixed read cap')
    if not raw:
        raise Bad(MALFORMED, 'output is empty')
    if not raw.endswith(b'\n'):
        raise Bad(MALFORMED, 'output is truncated without its final newline')
    try:
        text = raw.decode('utf-8', 'strict')
    except UnicodeDecodeError as exc:
        raise Bad(MALFORMED, 'output is not UTF-8') from exc
    lines = text.split('\n')[:-1]
    if len(lines) > MAX_LINES:
        raise Bad(MALFORMED, 'output has more records than the fixed schema allows')
    rows = []
    for line in lines:
        if not line or line != line.strip():
            raise Bad(MALFORMED, 'record is blank or padded')
        rows.append(line.split(' '))
    return rows


def _unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Bad(MALFORMED, 'duplicate JSON key')
        value[key] = item
    return value


def _octal(match):
    return {'040': ' ', '011': '\t', '012': '\n', '134': '\\'}.get(match.group(1))


def _unescape(value):
    """Undo the documented mountinfo escapes; an undocumented one is not guessed."""
    def replace(match):
        decoded = _octal(match)
        if decoded is None:
            raise Bad(MALFORMED, 'mount field uses an undocumented octal escape')
        return decoded
    result = re.sub(r'\\([0-7]{3})', replace, value)
    if '\\' in result:
        raise Bad(MALFORMED, 'mount field leaves an unescaped backslash')
    return result


def _bounded(value, limit=MAX_VALUE):
    if not isinstance(value, str) or len(value) > limit:
        raise Bad(MALFORMED, 'value is missing or longer than the fixed schema allows')
    if any(ord(char) < 0x20 or ord(char) == 0x7f for char in value):
        raise Bad(MALFORMED, 'value carries a control character')
    return value


def _controller(syspath):
    """The sysfs controller path and kind above /block/, never a guessed controller."""
    if syspath == 'invalid':
        return {'path': 'unknown', 'kind': 'unknown', 'components': []}
    head = syspath.split('/block/')[0]
    components = [part for part in head.split('/') if part][-4:]
    kind = 'unknown'
    parts = [part for part in head.split('/') if part]
    for part in reversed(parts):
        for name, pattern in CONTROLLER_KINDS:
            if pattern.match(part):
                kind = name
                break
        if kind != 'unknown':
            break
    if kind == 'unknown' and 'virtual' in parts:
        # /sys/devices/virtual holds device-mapper, software-raid, loop and other synthetic
        # controllers; this schema cannot resolve them to one controller and says so.
        kind = 'virtual'
    return {'path': head, 'kind': kind, 'components': components}


def _attribute(row):
    """A decoded sysfs attribute: its exact hex bytes plus text only when it is UTF-8."""
    if row is None:
        return {'status': 'missing'}
    if row['kind'] != 'hex':
        return {'status': row['kind']}
    data = bytes.fromhex(row['hex'])
    try:
        text = data.decode('utf-8')
    except UnicodeDecodeError:
        text = None
    return {'status': 'observed', 'hex': row['hex'], 'bytes': len(data), 'text': text}


# ----------------------------------------------------------------- route parsers

def parse_block_sysfs(raw):
    rows = _decode(raw)
    blocks, attributes, encoding = {}, {}, None
    for row in rows:
        kind = row[0]
        if kind == 'encoding':
            if len(row) != 2 or row[1] not in ('hex', 'unavailable') or encoding is not None:
                raise Bad(MALFORMED, 'invalid or duplicate encoding record')
            encoding = row[1]
            continue
        if kind == 'invalid-name':
            raise Bad(MALFORMED, 'the guest block inventory carries an invalid device name')
        if kind == 'block':
            if len(row) != 8:
                raise Bad(MALFORMED, 'block record has the wrong field count')
            name, majmin, partition, size, ro, removable, syspath = row[1:]
            if not NAME.match(name) or name in blocks:
                raise Bad(MALFORMED, 'invalid or duplicate block name')
            if not MAJMIN.match(majmin) or not BINARY.match(partition) or not COUNT.match(size):
                raise Bad(MALFORMED, 'block record carries an invalid identity or counter')
            if not BINARY.match(ro) or not BINARY.match(removable):
                raise Bad(MALFORMED, 'block record carries an invalid flag')
            if syspath != 'invalid' and not SYSFS_PATH.match(syspath):
                raise Bad(MALFORMED, 'block record carries an invalid sysfs path')
            if len(blocks) >= MAX_BLOCKS:
                raise Bad(MALFORMED, 'the block inventory exceeds the fixed schema')
            blocks[name] = {'name': name, 'maj_min': majmin, 'partition': int(partition),
                            'sectors': int(size), 'read_only': int(ro), 'removable': int(removable),
                            'syspath': syspath}
            continue
        if kind == 'attr':
            if len(row) != 4:
                raise Bad(MALFORMED, 'attribute record has the wrong field count')
            name, attribute, value = row[1], row[2], row[3]
            if name not in blocks or attribute not in ('serial', 'model', 'vendor'):
                raise Bad(MALFORMED, 'attribute record does not follow its block or names an unknown field')
            if (name, attribute) in attributes:
                raise Bad(MALFORMED, 'duplicate attribute record')
            if encoding == 'unavailable':
                documented = ('missing', 'empty', 'unavailable')
            else:
                documented = ('missing', 'empty')
            if value in documented:
                attributes[(name, attribute)] = {'kind': value, 'hex': ''}
            elif value.startswith('hex:') and HEX.match(value[4:]):
                if len(value[4:]) % 2:
                    raise Bad(MALFORMED, 'attribute hex payload is truncated')
                attributes[(name, attribute)] = {'kind': 'hex', 'hex': value[4:]}
            else:
                raise Bad(MALFORMED, 'attribute record carries an invalid value')
            continue
        raise Bad(MALFORMED, 'unknown record in the block inventory')
    if encoding is None:
        raise Bad(MALFORMED, 'the block inventory has no encoding record')
    if encoding == 'unavailable' and any(row['kind'] == 'hex' for row in attributes.values()):
        raise Bad(MALFORMED, 'hex attributes were emitted without an encoder')
    disks, partitions, unresolved = [], [], []
    for name in sorted(blocks):
        record = dict(blocks[name])
        resolved = _controller(record['syspath'])
        record.update(controller_path=resolved['path'], controller_kind=resolved['kind'],
                      controller_components=resolved['components'])
        blocks[name] = record
        if record['partition']:
            components = record['syspath'].split('/')
            parent = components[-2] if len(components) >= 2 else ''
            record['parent'] = parent if parent in blocks and blocks[parent]['partition'] == 0 else None
            if record['parent'] is None:
                unresolved.append(name)
            partitions.append(record)
        else:
            record['parent'] = None
            record['serial'] = _attribute(attributes.get((name, 'serial')))
            record['model'] = _attribute(attributes.get((name, 'model')))
            record['vendor'] = _attribute(attributes.get((name, 'vendor')))
            disks.append(record)
    return {'encoding': encoding, 'blocks': blocks, 'disks': disks, 'partitions': partitions,
            'parent_unresolved': sorted(unresolved), 'count': len(blocks)}


def _lsblk_node(item, nodes, depth):
    if not isinstance(item, dict) or depth > 8:
        raise Bad(MALFORMED, 'lsblk node is not an object or is nested too deeply')
    if set(item) - LSBLK_FIELDS:
        raise Bad(MALFORMED, 'lsblk node carries an unknown field')
    name = item.get('name')
    if not isinstance(name, str) or not NAME.match(name) or name in nodes:
        raise Bad(MALFORMED, 'lsblk node has an invalid or duplicate name')
    majmin = item.get('maj:min')
    if not isinstance(majmin, str) or not MAJMIN.match(majmin):
        raise Bad(MALFORMED, 'lsblk node has an invalid major:minor')
    parent = item.get('pkname')
    if parent in (None, ''):
        parent = None
    elif not isinstance(parent, str) or not NAME.match(parent):
        raise Bad(MALFORMED, 'lsblk node has an invalid parent name')
    kind = item.get('type')
    if not isinstance(kind, str) or not LSBLK_TYPES.match(kind):
        raise Bad(MALFORMED, 'lsblk node has an invalid type')
    size = item.get('size')
    if type(size) is str and COUNT.match(size):
        size = int(size)
    if type(size) is not int or size < 0:
        raise Bad(MALFORMED, 'lsblk node has an invalid size')
    for field in ('fstype', 'uuid', 'mountpoint'):
        value = item.get(field)
        if value is not None and not isinstance(value, str):
            raise Bad(MALFORMED, 'lsblk node carries a non-text ' + field)
        if value is not None:
            _bounded(value)
    uuid = item.get('uuid') or None
    if uuid is not None and not UUID_TEXT.match(uuid):
        raise Bad(MALFORMED, 'lsblk node carries an invalid filesystem UUID')
    fstype = item.get('fstype') or None
    if fstype is not None and not FSTYPE.match(fstype):
        raise Bad(MALFORMED, 'lsblk node carries an invalid filesystem type')
    children = item.get('children')
    if children is None:
        children = []
    if not isinstance(children, list):
        raise Bad(MALFORMED, 'lsblk node children are not a list')
    if len(nodes) >= MAX_BLOCKS:
        raise Bad(MALFORMED, 'the lsblk inventory exceeds the fixed schema')
    nodes[name] = {'name': name, 'maj_min': majmin, 'parent': parent, 'type': kind, 'bytes': size,
                   'fstype': fstype, 'uuid': uuid, 'mountpoint': item.get('mountpoint') or None}
    for child in children:
        _lsblk_node(child, nodes, depth + 1)


def parse_lsblk(raw):
    if not isinstance(raw, bytes) or len(raw) > MAX_ROUTE_BYTES:
        raise Bad(MALFORMED, 'output exceeds the fixed read cap')
    if not raw.endswith(b'\n'):
        raise Bad(MALFORMED, 'output is truncated without its final newline')
    try:
        data = json.loads(raw.decode('utf-8', 'strict'), object_pairs_hook=_unique)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise Bad(MALFORMED, 'lsblk output is not valid JSON') from exc
    if not isinstance(data, dict) or set(data) != {'blockdevices'}:
        raise Bad(MALFORMED, 'lsblk output is not the fixed document shape')
    devices = data['blockdevices']
    if not isinstance(devices, list) or not devices or len(devices) > MAX_BLOCKS:
        raise Bad(MALFORMED, 'lsblk output has no bounded device list')
    nodes = {}
    for device in devices:
        _lsblk_node(device, nodes, 0)
    return {'nodes': nodes, 'count': len(nodes)}


def parse_mountinfo(raw):
    rows = _decode(raw)
    mounts, root = [], None
    for row in rows:
        if len(row) < 10 or '-' not in row[6:]:
            raise Bad(MALFORMED, 'mount record has the wrong field count or no separator')
        separator = row.index('-', 6)
        if len(row) < separator + 4:
            raise Bad(MALFORMED, 'mount record does not carry type, source and options')
        if not COUNT.match(row[0]) or not COUNT.match(row[1]) or not MAJMIN.match(row[2]):
            raise Bad(MALFORMED, 'mount record carries an invalid identity')
        root_field = _unescape(row[3])
        point = _unescape(row[4])
        options = _unescape(row[5])
        if any(not OPTION.match(option) for option in options.split(',')):
            raise Bad(MALFORMED, 'mount record carries an invalid mount option')
        fstype = row[separator + 1]
        if not FSTYPE.match(fstype):
            raise Bad(MALFORMED, 'mount record carries an invalid filesystem type')
        source = _unescape(_bounded(row[separator + 2]))
        super_options = _unescape(row[separator + 3])
        if any(not OPTION.match(option) for option in super_options.split(',')):
            raise Bad(MALFORMED, 'mount record carries an invalid superblock option')
        record = {'maj_min': row[2], 'root': root_field, 'mount_point': point,
                  'mount_options': options.split(','), 'fstype': fstype, 'source': source,
                  'super_options': super_options.split(','),
                  'optional_fields': separator - 6}
        mounts.append(record)
        if point == '/':
            if root is not None:
                raise Bad(AMBIGUOUS, 'more than one mount claims the root mount point')
            root = record
    if root is None:
        raise Bad(MALFORMED, 'no mount record claims the root mount point')
    block_mounts = [record['mount_point'] for record in mounts
                    if record['maj_min'] != '0:0' and record is not root]
    return {'root': root, 'mounts': len(mounts), 'block_backed_mounts': len(block_mounts),
            'other_mount_points': sorted(block_mounts)[:64], 'truncated_mount_points': len(block_mounts) > 64}


FS_STATUS = {'ambiguous-root': AMBIGUOUS, 'duplicate-root': AMBIGUOUS,
             'no-block-device': UNSUPPORTED, 'unsupported-filesystem': UNSUPPORTED,
             'invalid-device-name': MALFORMED, 'read-failed': FAILED}


def parse_filesystem(raw):
    if not isinstance(raw, bytes) or len(raw) > MAX_ROUTE_BYTES:
        raise Bad(MALFORMED, 'output exceeds the fixed read cap')
    if not raw.endswith(b'\n'):
        raise Bad(MALFORMED, 'output is truncated without its final newline')
    try:
        lines = raw.decode('utf-8', 'strict').split('\n')[:-1]
    except UnicodeDecodeError as exc:
        raise Bad(MALFORMED, 'output is not UTF-8') from exc
    if len(lines) > MAX_LINES:
        raise Bad(MALFORMED, 'output has more records than the fixed schema allows')
    observed, start = {}, 0
    while start < len(lines) and lines[start].startswith('obs '):
        fields = lines[start][4:].split(' ')
        if len(fields) != 2:
            raise Bad(MALFORMED, 'observation record has the wrong field count')
        if fields[0] in observed:
            raise Bad(MALFORMED, 'duplicate observation record')
        observed[fields[0]] = fields[1]
        start += 1
    dump = lines[start:]
    if any(line.startswith('obs ') for line in dump):
        raise Bad(MALFORMED, 'an observation record follows the tool output')
    if not observed:
        raise Bad(MALFORMED, 'the filesystem read carries no observation record')
    status = FS_STATUS.get(observed.get('status'))
    facts = {}
    for field in ('root_majmin', 'root_fstype', 'root_device'):
        if field in observed and observed[field] != 'unknown':
            facts[field] = observed[field]
    if 'root_majmin' in facts and not MAJMIN.match(facts['root_majmin']):
        raise Bad(MALFORMED, 'the root major:minor is malformed')
    if 'root_fstype' in facts and not FSTYPE.match(facts['root_fstype']):
        raise Bad(MALFORMED, 'the root filesystem type is malformed')
    if 'root_device' in facts and not NAME.match(facts['root_device']):
        raise Bad(MALFORMED, 'the root device name is malformed')
    if observed.get('tool') == 'absent':
        status = ABSENT
    elif observed.get('tool') not in (None, 'present'):
        raise Bad(MALFORMED, 'the filesystem read carries an invalid tool record')
    if status is not None:
        if dump and status != AMBIGUOUS:
            raise Bad(MALFORMED, 'a refused filesystem read carries tool output')
        reported = observed.get('status') or ('tool ' + observed['tool'])
        return status, facts, ['the fixed read reported ' + reported]
    if not dump:
        raise Bad(MALFORMED, 'the filesystem tool produced no output')
    features, uuid, defaults, state = None, None, None, None
    for line in dump:
        if line.startswith('Filesystem features:'):
            if features is not None:
                raise Bad(MALFORMED, 'duplicate filesystem features line')
            features = line.split(':', 1)[1].split()
            if not features or len(features) > MAX_FEATURES:
                raise Bad(MALFORMED, 'the filesystem feature list is empty or oversized')
            if len(set(features)) != len(features) or any(not FEATURE.match(item) for item in features):
                raise Bad(MALFORMED, 'the filesystem feature list is malformed or duplicated')
        elif line.startswith('Filesystem UUID:'):
            if uuid is not None:
                raise Bad(MALFORMED, 'duplicate filesystem UUID line')
            uuid = line.split(':', 1)[1].strip()
            if not UUID_TEXT.match(uuid):
                raise Bad(MALFORMED, 'the filesystem UUID is malformed')
        elif line.startswith('Default mount options:'):
            defaults = line.split(':', 1)[1].split()
            if any(not OPTION.match(option) for option in defaults):
                raise Bad(MALFORMED, 'the default mount options are malformed')
        elif line.startswith('Filesystem state:'):
            state = _bounded(line.split(':', 1)[1].strip(), 64)
    if features is None:
        raise Bad(MALFORMED, 'the dump does not carry a documented feature line')
    facts.update(features=features, uuid=uuid, default_mount_options=defaults, state=state,
                 project_feature='present' if 'project' in features else 'absent',
                 quota_feature='present' if 'quota' in features else 'absent')
    return OBSERVED, facts, []


def parse_device_uuids(raw):
    rows = _decode(raw)
    directories, entries = {}, {}
    for row in rows:
        kind = row[0]
        if kind == 'invalid-entry':
            raise Bad(MALFORMED, 'the guest device-uuid inventory carries an invalid entry name')
        if kind == 'dir':
            if len(row) != 3 or row[1] not in ('by-uuid', 'by-partuuid') or row[2] not in ('present', 'absent'):
                raise Bad(MALFORMED, 'invalid device-uuid directory record')
            if row[1] in directories:
                raise Bad(MALFORMED, 'duplicate device-uuid directory record')
            directories[row[1]] = row[2]
            continue
        if kind == 'entry':
            if len(row) != 4 or row[1] not in ('by-uuid', 'by-partuuid'):
                raise Bad(MALFORMED, 'invalid device-uuid entry record')
            key, identifier, name = row[1], row[2], row[3]
            if not NAME.match(identifier) or (name != 'unresolved' and not NAME.match(name)):
                raise Bad(MALFORMED, 'device-uuid entry carries an invalid name')
            if len(entries) >= MAX_BLOCKS:
                raise Bad(MALFORMED, 'the device-uuid inventory exceeds the fixed schema')
            entries.setdefault(key, []).append({'uuid': identifier, 'device': name})
            continue
        raise Bad(MALFORMED, 'unknown record in the device-uuid inventory')
    if not directories:
        raise Bad(MALFORMED, 'the device-uuid read carries no directory record')
    for key in entries:
        if key not in directories:
            raise Bad(MALFORMED, 'device-uuid entries without their directory record')
    return {'directories': directories, 'entries': entries}


def parse_capabilities(raw):
    rows = _decode(raw)
    tools, versions = {}, {}
    for row in rows:
        kind = row[0]
        if kind == 'tool':
            if len(row) != 3 or row[1] not in CAPABILITY_TOOLS or row[1] in tools:
                raise Bad(MALFORMED, 'invalid or duplicate tool capability record')
            value = row[2]
            if value in ('absent', 'invalid') or DEVICE_PATH.match(value):
                tools[row[1]] = value
            else:
                raise Bad(MALFORMED, 'tool capability record carries an invalid path')
            continue
        if kind == 'version':
            if len(row) < 4 or row[1] not in CAPABILITY_TOOLS or row[1] in versions:
                raise Bad(MALFORMED, 'invalid or duplicate tool version record')
            if row[2] == 'unavailable':
                if len(row) != 4:
                    raise Bad(MALFORMED, 'an unavailable version record carries text')
                versions[row[1]] = {'status': 'unavailable'}
            elif row[2] in ('stdout', 'stderr'):
                versions[row[1]] = {'status': 'observed', 'stream': row[2],
                                    'text': _bounded(' '.join(row[3:]), 200)}
            else:
                raise Bad(MALFORMED, 'a tool version record names an undocumented stream')
            continue
        raise Bad(MALFORMED, 'unknown record in the capability inventory')
    if not tools:
        raise Bad(MALFORMED, 'the capability read carries no tool record')
    return {'tools': tools, 'versions': versions,
            'quota_tools': {name: tools.get(name, 'unobserved') for name in QUOTA_TOOLS}}


def parse_cgroup_units(raw):
    rows = _decode(raw)
    files, units, properties, observation = {}, {}, {}, None
    for row in rows:
        kind = row[0]
        if kind == 'unit_observation':
            if len(row) != 2 or row[1] not in ('absent',) or observation is not None:
                raise Bad(MALFORMED, 'invalid or duplicate unit observation record')
            observation = row[1]
            continue
        if kind == 'file':
            if len(row) < 3 or row[1] not in ('cgroup.controllers', 'cgroup.subtree_control') or row[1] in files:
                raise Bad(MALFORMED, 'invalid or duplicate control-group file record')
            value = ' '.join(row[2:])
            if value in ('unreadable', 'empty'):
                files[row[1]] = {'status': value}
            elif value == 'invalid':
                raise Bad(MALFORMED, 'a control-group file carries an invalid value')
            else:
                names = value.split()
                if any(not FEATURE.match(name) for name in names):
                    raise Bad(MALFORMED, 'a control-group controller name is malformed')
                files[row[1]] = {'status': 'observed', 'controllers': names}
            continue
        if kind == 'unit':
            if len(row) != 3 or row[1] not in ('system.slice', 'user.slice') or row[1] in units:
                raise Bad(MALFORMED, 'invalid or duplicate unit record')
            if row[2] not in ('present', 'unreadable'):
                raise Bad(MALFORMED, 'invalid unit state record')
            units[row[1]] = {'status': row[2]}
            continue
        if kind == 'property':
            if len(row) != 4 or row[1] not in units or row[2] not in PROPERTY_KEYS:
                raise Bad(MALFORMED, 'invalid property record')
            key, value = (row[1], row[2]), row[3]
            if row[2] in ARRAY_PROPERTY_KEYS:
                # One record per documented device=rate pair; a list value never carries a space.
                if value in ('empty', 'invalid'):
                    if key in properties:
                        raise Bad(MALFORMED, 'duplicate or mixed IO property record')
                    properties[key] = {'status': value, 'devices': []}
                    continue
                device, separator, rate = value.partition('=')
                if not separator or not IO_DEVICE.match(device) or not IO_RATE.match(rate):
                    raise Bad(MALFORMED, 'an IO property carries an invalid device+rate pair')
                record = properties.setdefault(key, {'status': 'observed', 'devices': []})
                if record['status'] != 'observed' or any(item['device'] == device
                                                         for item in record['devices']):
                    raise Bad(MALFORMED, 'duplicate or mixed IO property record')
                record['devices'].append({'device': device, 'rate': rate})
                continue
            if key in properties:
                raise Bad(MALFORMED, 'duplicate property record')
            if value in ('empty', 'invalid'):
                properties[key] = {'status': value}
            elif PROPERTY.match(value):
                properties[key] = {'status': 'observed', 'value': value}
            else:
                raise Bad(MALFORMED, 'a unit property carries an invalid value')
            continue
        raise Bad(MALFORMED, 'unknown record in the control-group inventory')
    if not files and observation is None:
        raise Bad(MALFORMED, 'the control-group read carries no file record')
    for unit, record in units.items():
        record['properties'] = {key: properties.get((unit, key), {'status': 'unobserved'})
                                for key in PROPERTY_KEYS}
    if observation == 'absent' and units:
        raise Bad(MALFORMED, 'unit observation is absent but unit records are present')
    return {'files': files, 'units': units, 'unit_observation': observation or 'present'}


PARSERS = {'block-sysfs': parse_block_sysfs, 'block-lsblk': parse_lsblk, 'mountinfo': parse_mountinfo,
           'filesystem': parse_filesystem, 'device-uuids': parse_device_uuids,
           'capabilities': parse_capabilities, 'cgroup-units': parse_cgroup_units}


# ----------------------------------------------------------------- per-read supervision

def decode(route, result, raw):
    """One read's status and facts; a broken read never becomes a positive observation."""
    disposition, code = result.get('disposition'), result.get('waited_exit')
    if disposition == 'output-cap':
        return MALFORMED, {}, ['output exceeded the fixed read cap']
    if disposition in ('timeout', 'resource-refusal', 'unknown', 'local-error'):
        return FAILED, {}, ['the fixed read did not complete: ' + str(disposition)]
    if code == 127:
        return ABSENT, {}, ['the documented tool is not installed']
    if code == 126:
        return UNSUPPORTED, {}, ['the documented invocation is not supported by this guest build']
    if code == 125 and route['id'] == 'block-lsblk':
        return FAILED, {}, ['the documented invocation could not be verified, so the read is not an observation']
    if route['id'] == 'filesystem':
        try:
            status, facts, reasons = parse_filesystem(raw)
        except Bad as exc:
            return exc.status, {}, [exc.reason]
        if code in (0, None):
            if status != OBSERVED:
                return MALFORMED, {}, ['the read exited successfully but reported ' + status]
            return OBSERVED, facts, reasons
        if status == OBSERVED:
            # A complete dump from a read that still failed is a partial observation: the facts
            # are retained but the read never becomes an observation.
            return FAILED, facts, ['the fixed read exited ' + str(code) + ' after its dump']
        return status, facts, reasons
    if code not in (0, None):
        reasons = ['the fixed read exited ' + str(code)]
        try:
            return FAILED, PARSERS[route['id']](raw), reasons
        except Bad:
            return FAILED, {}, reasons
    try:
        return OBSERVED, PARSERS[route['id']](raw), []
    except Bad as exc:
        return exc.status, {}, [exc.reason]


def observe(run, retain, guard, deadline, budget=BUDGET_SECONDS):
    """Run every fixed read inside one shared window; nothing here repairs a failed read."""
    end = time.monotonic() + budget
    rows = []
    for route in ROUTES:
        guard()
        now = time.monotonic()
        limit = min(end, deadline)
        row = {'id': route['id'], 'script_sha256': hashlib.sha256(route['script'].encode()).hexdigest(),
               'timeout_seconds': route['timeout'], 'cap_bytes': MAX_ROUTE_BYTES}
        if limit - now < MIN_ROUTE_SECONDS:
            row.update(status=NOT_ATTEMPTED, exit=None, disposition='not-attempted',
                       reasons=['the shared observation window ended before this read'], facts={},
                       original=None, sha256=None, bytes=0)
            rows.append(row)
            continue
        try:
            result, raw = run(route_argv(route), min(route['timeout'], limit - now), MAX_ROUTE_BYTES, limit)
        except Exception as exc:
            guard()
            row.update(status=FAILED, exit=None, disposition='local-error',
                       reasons=['the fixed read could not run: ' + str(exc)], facts={},
                       original=None, sha256=None, bytes=0)
            rows.append(row)
            continue
        sha = hashlib.sha256(raw).hexdigest()
        try:
            original = retain(route['id'], raw)
        except Exception as exc:
            guard()
            original = None
            retained_error = 'the original was not retained: ' + str(exc)
        else:
            retained_error = None
        try:
            status, facts, reasons = decode(route, result, raw)
        except Exception as exc:                                  # a decoder bug is not a fact
            status, facts, reasons = MALFORMED, {}, ['the read could not be decoded: ' + str(exc)]
        if retained_error is not None:
            status, reasons = FAILED, reasons + [retained_error]
        row.update(status=status, exit=result.get('waited_exit'), disposition=result.get('disposition'),
                   reasons=reasons, facts=facts, original=original, sha256=sha, bytes=len(raw))
        rows.append(row)
    guard()
    return summarize(rows)


# ----------------------------------------------------------------- correlation

def facts_of(rows, route_id):
    for row in rows:
        if row['id'] == route_id:
            return row['facts'] if row['status'] in (OBSERVED, AMBIGUOUS, NOT_FOUND) else {}
    return {}


def _root_facts(mounts, filesystem, block):
    """The root mount, its block device and that device's own sysfs ancestry."""
    root = {'status': 'unknown', 'reasons': []}
    mount = mounts.get('root')
    if mount:
        root.update(source=mount['source'], fstype=mount['fstype'], maj_min=mount['maj_min'],
                    mount_point=mount['mount_point'], mount_options=mount['mount_options'],
                    super_options=mount['super_options'])
    device = filesystem.get('root_device')
    blocks = block.get('blocks', {})
    if device is None and mount:
        matches = [name for name, record in blocks.items() if record['maj_min'] == mount['maj_min']]
        device = matches[0] if len(matches) == 1 else None
        if len(matches) > 1:
            root['reasons'].append('more than one block device carries the root major:minor')
    if device is not None:
        root['device'] = device
    if filesystem.get('root_majmin'):
        root['filesystem_maj_min'] = filesystem['root_majmin']
    if filesystem.get('uuid'):
        root['filesystem_uuid'] = filesystem['uuid']
    if filesystem.get('state'):
        root['filesystem_state'] = filesystem['state']
    if filesystem.get('default_mount_options'):
        root['default_mount_options'] = filesystem['default_mount_options']
    if device is not None and device in blocks:
        chain, current, seen = [], device, set()
        while current is not None and current in blocks and current not in seen:
            chain.append(current)
            seen.add(current)
            current = blocks[current].get('parent')
        root['ancestry'] = chain
        record = blocks[chain[-1]]
        root['ancestry_syspath'] = record['syspath']
        root['controller_path'] = record['controller_path']
        root['controller_kind'] = record['controller_kind']
        root['controller_components'] = record['controller_components']
        if record['partition']:
            if record['parent'] is None:
                root['reasons'].append('the root partition parent is unresolved')
            root['reasons'].append('the root ancestry does not reach a whole disk')
        if record['controller_kind'] in ('virtual', 'unknown'):
            root['reasons'].append('the root controller is ' + record['controller_kind'] +
                                   ' and this schema cannot resolve it to one controller')
    elif device is not None:
        root['reasons'].append('the root device is absent from the block inventory')
    if mount and block.get('parent_unresolved'):
        root['reasons'].append('the block inventory has unresolved parents')
    if mount is None:
        root['reasons'].append('no root mount was observed')
    if root['reasons']:
        root['status'] = 'ambiguous' if 'device' in root else 'unknown'
    else:
        root['status'] = 'resolved'
    return root


def _disk_facts(block):
    disks = []
    for record in block.get('disks', []):
        disks.append({'name': record['name'], 'maj_min': record['maj_min'],
                      'bytes': record['sectors'] * 512, 'read_only': record['read_only'],
                      'removable': record['removable'], 'syspath': record['syspath'],
                      'controller_path': record['controller_path'],
                      'controller_kind': record['controller_kind'],
                      'controller_components': record['controller_components'],
                      'serial': record['serial'], 'model': record['model'], 'vendor': record['vendor']})
    partitions = []
    for record in block.get('partitions', []):
        partitions.append({'name': record['name'], 'maj_min': record['maj_min'],
                           'parent': record['parent'], 'bytes': record['sectors'] * 512,
                           'syspath': record['syspath'], 'controller_kind': record['controller_kind']})
    return disks, partitions


def _inventory_agreement(block, lsblk):
    """Compare the fixed sysfs inventory with the documented lsblk one; a difference stays a fact."""
    if not block or not lsblk:
        return {'status': 'unavailable', 'reasons': ['one of the two block inventories is unavailable']}
    ours, theirs = block.get('blocks', {}), lsblk.get('nodes', {})
    reasons = []
    if set(ours) != set(theirs):
        reasons.append('device name sets differ: ' + ','.join(sorted(set(ours) ^ set(theirs))[:16]))
    for name in sorted(set(ours) & set(theirs)):
        if ours[name]['maj_min'] != theirs[name]['maj_min']:
            reasons.append('major:minor differs for ' + name)
        if ours[name]['partition'] and ours[name].get('parent') != theirs[name]['parent']:
            reasons.append('parent differs for ' + name)
    return {'status': 'differs' if reasons else 'agrees', 'reasons': reasons}


def correlate(rows):
    """Every writer-bound capability fact as an observation, with missing parts left unknown."""
    block = facts_of(rows, 'block-sysfs')
    lsblk = facts_of(rows, 'block-lsblk')
    mounts = facts_of(rows, 'mountinfo')
    filesystem = facts_of(rows, 'filesystem')
    uuids = facts_of(rows, 'device-uuids')
    capabilities = facts_of(rows, 'capabilities')
    groups = facts_of(rows, 'cgroup-units')
    root = _root_facts(mounts, filesystem, block)
    disks, partitions = _disk_facts(block)
    ancestry = set(root.get('ancestry', []))
    additional_disks = [disk for disk in disks if disk['name'] not in ancestry]
    additional_partitions = [part for part in partitions if part['name'] not in ancestry]
    entries = uuids.get('entries', {})
    root_uuids = {'by-uuid': [row['uuid'] for row in entries.get('by-uuid', [])
                              if root.get('device') is not None and row['device'] == root['device']],
                  'by-partuuid': [row['uuid'] for row in entries.get('by-partuuid', [])
                                  if root.get('device') is not None and row['device'] == root['device']]}
    if not uuids:
        uuid_status = 'unknown'
    elif len(root_uuids['by-uuid']) > 1 or len(root_uuids['by-partuuid']) > 1:
        uuid_status = AMBIGUOUS
    elif any(root_uuids.values()):
        uuid_status = OBSERVED
    else:
        uuid_status = NOT_FOUND
    return {
        'root': root,
        'root_uuids': {'status': uuid_status, 'entries': root_uuids},
        'disks': disks,
        'partitions': partitions,
        'additional_disks': additional_disks,
        'additional_partitions': additional_partitions,
        'block_inventory': {'count': block.get('count'), 'sysfs_encoding': block.get('encoding'),
                            'parent_unresolved': block.get('parent_unresolved', [])},
        'lsblk_inventory': {'count': lsblk.get('count')},
        'inventory_agreement': _inventory_agreement(block, lsblk),
        'filesystem_features': {'status': OBSERVED if filesystem.get('features') is not None else 'unknown',
                                'features': filesystem.get('features'),
                                'project_feature': filesystem.get('project_feature'),
                                'quota_feature': filesystem.get('quota_feature')},
        'quota_tools': capabilities.get('quota_tools', {}),
        'tool_versions': capabilities.get('versions', {}),
        'tool_versions_note': ('a version banner is read from stdout first and from stderr when stdout is '
                               'empty; the record names that stream'),

        'control_groups': {'status': 'observed' if groups else 'unknown',
                           'files': groups.get('files', {}), 'units': groups.get('units', {}),
                           'unit_observation': groups.get('unit_observation')},
    }


def summarize(rows):
    """The retained observation document: routes, facts, limits and the findings they imply."""
    try:
        facts = correlate(rows)
    except Exception as exc:                                  # a correlation bug is not a fact
        facts = {'status': 'unknown', 'error': type(exc).__name__ + ': ' + str(exc)}
    findings = []
    for row in rows:
        if row['status'] in INCOMPLETE:
            detail = '; '.join(row['reasons']) or 'no reason recorded'
            findings.append('writer-bound observation %s: %s (%s)' % (row['id'], row['status'], detail))
    agreement = facts.get('inventory_agreement', {})
    if agreement.get('status') == 'differs':
        findings.append('writer-bound observation: block inventories differ (' +
                        '; '.join(agreement['reasons']) + ')')
    root = facts.get('root', {})
    ambiguity = root.get('status') == 'ambiguous'
    if ambiguity:
        findings.append('writer-bound observation: the root controller or ancestry is ambiguous (' +
                        '; '.join(root.get('reasons', [])) + ')')
    elif root.get('status') != 'resolved':
        findings.append('writer-bound observation: the root device or mount is unresolved')
    complete = not findings and not any(row['status'] in INCOMPLETE for row in rows)
    if complete:
        eligibility = 'eligible for a later reviewed correlation mechanism'
    elif ambiguity:
        eligibility = ('not eligible: the root controller or ancestry is ambiguous, so no writer '
                       'bound is established')
    else:
        eligibility = 'incomplete: missing facts stay unknown and grant nothing'
    return {'schema': 'linux-writer-bound-observation-v1', 'routes_sha256': routes_digest(),
            'routes': rows, 'facts': facts, 'limits': dict(LIMITS),
            'observation_eligibility': eligibility,
            'complete': complete,
            'findings': findings}
