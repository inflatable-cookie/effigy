"""Fixed read-only guest observations for the resident engine/builder artifact
identities and packaged launch metadata (the accepted 783 §3.4 pin route).

Observation source only. Each entry is one reviewed, fixed argv that reads
packaged file facts, the dpkg database and packaged systemd unit files and
writes nothing. No engine/builder binary is executed to learn its version, no
daemon, socket, journal or process inventory is contacted, and nothing outside
the allowlisted packaged units and the documented CLI-plugin directories is
read. A package version string is metadata, never publisher authentication; a
packaged unit body is a potential launch relationship, never observed ancestry.
No fact here grants setup, E12 admission, confinement, hard-limit, association
or trust credit.
"""
import hashlib
import json
import re
import time

from linux_diagnostic_probe import (
    ABSENT, AMBIGUOUS, BUDGET_SECONDS, FAILED, INCOMPLETE, MALFORMED,
    MAX_ROUTE_BYTES, MIN_ROUTE_SECONDS, NOT_ATTEMPTED, OBSERVED, UNSUPPORTED,
    Bad, _decode, facts_of,
)

SCHEMA = 'linux-engine-inputs-observation-v1'

ENGINE_ARTIFACTS = '''# effigy-linux-observation: engine-artifacts
LC_ALL=C
export LC_ALL
for t in readlink stat sha256sum dpkg-query; do
  command -v "$t" >/dev/null 2>&1 || { printf 'support %s absent\\n' "$t"; exit 127; }
done
pin() {
  t=$1
  p=$2
  case $p in
    absent) ;;
    /*) case $p in *[!A-Za-z0-9._:/+-]*) p=invalid;; esac ;;
    *) p=invalid ;;
  esac
  printf 'tool %s %s\\n' "$t" "$p"
  case $p in /*) ;; *) return 0 ;; esac
  if [ -L "$p" ]; then l=1; else l=0; fi
  printf 'link %s %d\\n' "$t" "$l"
  r=$(readlink -f "$p" 2>/dev/null) || r=
  case $r in
    /*) case $r in *[!A-Za-z0-9._:/+-]*) r=invalid;; esac ;;
    '') r=unavailable ;;
    *) r=invalid ;;
  esac
  printf 'resolve %s %s\\n' "$t" "$r"
  case $r in /*) ;; *) return 0 ;; esac
  if [ -e "$r" ]; then
    st=$(stat -c '%f %u %g %s %d %i' "$r" 2>/dev/null) || st=
    if [ -z "$st" ]; then printf 'stat %s unavailable\\n' "$t"; else
      set -f
      set -- $st
      if [ "$#" -eq 6 ]; then printf 'stat %s %s %s %s %s %s %s\\n' "$t" "$1" "$2" "$3" "$4" "$5" "$6"
      else printf 'stat %s invalid\\n' "$t"; fi
    fi
    h=$(sha256sum "$r" 2>/dev/null) || h=
    set -f
    set -- $h
    if [ "$#" -eq 2 ]; then v=$1; else v=; fi
    case $v in ''|*[!0-9a-f]*) v=invalid ;; esac
    printf 'hash %s %s\\n' "$t" "$v"
  else
    printf 'stat %s missing\\n' "$t"
    printf 'hash %s missing\\n' "$t"
  fi
  own=$(dpkg-query -S "$r" 2>/dev/null); orc=$?
  if [ "$orc" -eq 1 ]; then printf 'owner %s none\\n' "$t"; return 0; fi
  if [ "$orc" -ne 0 ]; then printf 'owner %s unavailable\\n' "$t"; return 0; fi
  first=$(printf '%s\\n' "$own" | head -n 1)
  extra=$(printf '%s\\n' "$own" | tail -n +2)
  if [ -n "$extra" ]; then printf 'owner %s ambiguous\\n' "$t"; return 0; fi
  case $first in
    *": $r") pkgs=${first%": $r"} ;;
    *) printf 'owner %s invalid\\n' "$t"; return 0 ;;
  esac
  case $pkgs in ''|*[!a-z0-9+.:_-]*) printf 'owner %s invalid\\n' "$t"; return 0 ;; esac
  names=$(printf '%s\\n' "$pkgs" | tr ',' ' ')
  set -f
  set -- $names
  if [ "$#" -gt 4 ]; then printf 'owner %s ambiguous\\n' "$t"; return 0; fi
  printf 'owner %s %d\\n' "$t" "$#"
  i=0
  for pkg in "$@"; do
    i=$((i+1))
    case $pkg in
      *:*) pa=${pkg%%:*}; pb=${pkg#*:}
           case $pa in ''|*[!a-z0-9+.-]*) printf 'owner-pkg %s %d invalid\\n' "$t" "$i"; continue;; esac
           case $pb in ''|*[!a-z0-9.-]*) printf 'owner-pkg %s %d invalid\\n' "$t" "$i"; continue;; esac ;;
      *) case $pkg in ''|*[!a-z0-9+.-]*) printf 'owner-pkg %s %d invalid\\n' "$t" "$i"; continue;; esac ;;
    esac
    printf 'owner-pkg %s %d %s\\n' "$t" "$i" "$pkg"
    q=$(dpkg-query -W -f='${Version}\\t${db:Status-Status}\\n' "$pkg" 2>/dev/null); qrc=$?
    if [ "$qrc" -ne 0 ] || [ -z "$q" ]; then printf 'version %s %d unavailable\\n' "$t" "$i"; continue; fi
    extra=$(printf '%s\\n' "$q" | tail -n +2)
    if [ -n "$extra" ]; then printf 'version %s %d ambiguous\\n' "$t" "$i"; continue; fi
    ver=$(printf '%s\\n' "$q" | head -n 1)
    oldifs=$IFS
    IFS=$(printf '\\t')
    set -f
    set -- $ver
    IFS=$oldifs
    if [ "$#" -ne 2 ]; then printf 'version %s %d invalid\\n' "$t" "$i"; continue; fi
    v=$1
    s=$2
    case $v in ''|*[!A-Za-z0-9.+~:-]*) v=invalid ;; esac
    case $s in ''|*[!a-z-]*) s=invalid ;; esac
    printf 'version %s %d %s %s\\n' "$t" "$i" "$v" "$s"
  done
  return 0
}
for t in dockerd containerd runc buildx; do
  p=$(command -v "$t" 2>/dev/null) || p=
  if [ -z "$p" ]; then p=absent; fi
  pin "$t" "$p"
done
# buildx is a Docker CLI plugin, not a PATH command: the CLI looks it up in these
# documented system directories in this order, and the first hit wins.
source=
for d in /usr/local/lib/docker/cli-plugins /usr/local/libexec/docker/cli-plugins /usr/lib/docker/cli-plugins /usr/libexec/docker/cli-plugins; do
  if [ -e "$d/docker-buildx" ]; then source=$d/docker-buildx; break; fi
done
if [ -z "$source" ]; then
  printf 'tool docker-buildx absent\\n'
else
  printf 'plugin-source docker-buildx %s\\n' "$source"
  pin docker-buildx "$source"
fi
exit 0
'''

ENGINE_UNITS = '''# effigy-linux-observation: engine-units
LC_ALL=C
export LC_ALL
emitval() {
  if [ -z "$4" ]; then printf '%s %s %s bare\\n' "$1" "$2" "$3"
  else printf '%s %s %s %s\\n' "$1" "$2" "$3" "$4"; fi
}
dirs='/etc/systemd/system:admin /run/systemd/system:runtime /usr/local/lib/systemd/system:local /usr/lib/systemd/system:packaged /lib/systemd/system:legacy'
for u in docker.service docker.socket containerd.service buildkit.service buildkit.socket; do
  for d in $dirs; do
    dir=${d%%:*}
    role=${d##*:}
    if [ "$role" = legacy ] && [ -e /usr/lib/systemd/system/"$u" ]; then continue; fi
    if [ -e "$dir/$u" ]; then printf 'present %s %s\\n' "$u" "$role"; fi
    if [ -d "$dir/$u.d" ]; then printf 'dropin %s %s\\n' "$u" "$role"; fi
  done
  body=
  for spec in /usr/lib/systemd/system:packaged /lib/systemd/system:legacy; do
    dir=${spec%%:*}
    role=${spec##*:}
    if [ "$role" = legacy ] && [ -e /usr/lib/systemd/system/"$u" ]; then continue; fi
    if [ ! -e "$dir/$u" ]; then continue; fi
    if [ -L "$dir/$u" ]; then printf 'read %s symlink\\n' "$u"
    elif [ ! -f "$dir/$u" ]; then printf 'read %s nonregular\\n' "$u"
    else printf 'read %s %s\\n' "$u" "$dir"; body=$dir/$u; fi
    break
  done
  if [ -z "$body" ]; then continue; fi
  n=0
  overrun=0
  while IFS= read -r line || [ -n "$line" ]; do
    n=$((n+1))
    if [ "$n" -gt 512 ]; then overrun=1; break; fi
    merges=0
    while :; do
      case $line in
        *\\\\) ;;
        *) break ;;
      esac
      line=${line%\\\\}
      merges=$((merges+1))
      seg=
      IFS= read -r seg
      n=$((n+1))
      if [ "$merges" -ge 4 ] || [ "$n" -gt 512 ]; then overrun=1; break; fi
      if [ $((${#line} + ${#seg})) -gt 4096 ]; then overrun=1; break; fi
      line=$line$seg
    done
    if [ "$overrun" -eq 1 ]; then break; fi
    case $line in
      ExecStart=*)
        value=${line#ExecStart=}
        set -f
        set -- $value
        if [ "$#" -eq 0 ]; then printf 'launch %s none\\n' "$u"; continue; fi
        printf 'launch %s %s\\n' "$u" "$1"
        shift
        while [ "$#" -gt 0 ]; do
          tok=$1
          case $tok in
            --config-file=*|--config=*|--root=*|--state-root=*|--data-root=*|--graph=*|--exec-root=*|--containerd=*)
              emitval flag "$u" "${tok%%=*}" "${tok#*=}" ;;
            --config-file|--config|--root|--state-root|--data-root|--graph|--exec-root|--containerd)
              if [ "$#" -gt 1 ]; then emitval flag "$u" "$tok" "$2"; shift
              else printf 'flag %s %s bare\\n' "$u" "$tok"; fi ;;
          esac
          shift
        done ;;
      ListenStream=*)
        value=${line#ListenStream=}
        set -f
        set -- $value
        if [ "$#" -eq 1 ]; then printf 'listen %s %s\\n' "$u" "$1"
        else printf 'listen %s invalid\\n' "$u"; fi ;;
    esac
  done < "$body"
  if [ "$overrun" -eq 1 ]; then printf 'overrun %s\\n' "$u"; fi
done
exit 0
'''

ROUTES = (
    {'id': 'engine-artifacts', 'script': ENGINE_ARTIFACTS, 'timeout': 30},
    {'id': 'engine-units', 'script': ENGINE_UNITS, 'timeout': 15},
)

ARTIFACT_TOOLS = frozenset({'dockerd', 'containerd', 'runc', 'buildx', 'docker-buildx'})
PLUGIN_DIRS = ('/usr/local/lib/docker/cli-plugins', '/usr/local/libexec/docker/cli-plugins',
               '/usr/lib/docker/cli-plugins', '/usr/libexec/docker/cli-plugins')
PLUGIN_NAME = 'docker-buildx'
SUPPORT_TOOLS = frozenset({'readlink', 'stat', 'sha256sum', 'dpkg-query'})
UNIT_NAMES = frozenset({'docker.service', 'docker.socket', 'containerd.service',
                        'buildkit.service', 'buildkit.socket'})
UNIT_ROLES = frozenset({'admin', 'runtime', 'local', 'packaged', 'legacy'})
PACKAGED_DIRS = {'/usr/lib/systemd/system', '/lib/systemd/system'}
FLAG_GROUPS = {
    '--config-file': 'config', '--config': 'config',
    '--root': 'root', '--state-root': 'root', '--data-root': 'root',
    '--graph': 'root', '--exec-root': 'root',
    '--containerd': 'address',
}

PATH = re.compile(r'^/[A-Za-z0-9._:/+-]{1,255}$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
HEX = re.compile(r'^[0-9a-f]{1,16}$')
DECIMAL = re.compile(r'^[0-9]{1,20}$')
VERSION_TOKEN = re.compile(r'^[A-Za-z0-9.+~:-]{1,64}$')
PKG_NAME = re.compile(r'^[a-z0-9][a-z0-9+.-]*$')
PKG_ARCH = re.compile(r'^[A-Za-z0-9][A-Za-z0-9-]*$')
DEB_STATUS = re.compile(r'^[a-z-]{1,32}$')
LAUNCH_TOKEN = re.compile(r'^[!+:-]{0,2}/[A-Za-z0-9._:/+@%=-]{0,250}$')

# The facts below stay observations. Each string restates what the observation
# cannot establish so a later reader cannot upgrade a pin into a confinement or
# an ancestry claim.
LIMITS = {
    'artifact_identity': ('hash and stat of the resolved file bytes during this one boot; not '
                          'publisher authentication and not build provenance'),
    'package_version': ('dpkg database metadata about the owning package; a version string is not '
                        'publisher authentication'),
    'launch_relationship': ('packaged unit metadata is a potential launch relationship; no process '
                            'ancestry, capability or user-namespace readback exists here, so no '
                            'descendant closure is established'),
    'effective_units': ('a custom override or drop-in replaces or edits the packaged unit; its body '
                        'is never read, so the effective launch arguments stay unknown'),
    'resident_processes': ('no daemon, socket, journal or process inventory was contacted; the '
                           'resident running engine is unobserved'),
    'engine_storage': ('unobserved; no resident engine or builder storage location is established, '
                       'inside or outside BASE'),
    'rootless_or_rootful': ('not decidable from packaged metadata alone; the separately pinned '
                            'observation and its review own that decision'),
    'confinement_credit': 'none',
    'setup_credit': 'none',
    'e12_admission': 'none',
    'quota_bound': 'none',
}


def route_ids():
    return tuple(route['id'] for route in ROUTES)


def route_argv(route):
    return ['/bin/sh', '-c', route['script']]


def routes_digest():
    """Bind the exact fixed argv of every route, not a caller-supplied list."""
    rows = [[route['id'], route_argv(route), route['timeout'], MAX_ROUTE_BYTES] for route in ROUTES]
    return hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


# ----------------------------------------------------------------- parsing helpers

def _one(row, kinds):
    if len(row) != kinds + 1:
        raise Bad(MALFORMED, 'record does not fit the fixed schema')
    return row[1:]


def _token(value, pattern, reason):
    if not isinstance(value, str) or not pattern.fullmatch(value):
        raise Bad(MALFORMED, reason)
    return value


def _status_token(value):
    return value in ('unavailable', 'invalid', 'missing', 'ambiguous', 'none')


def _package(value):
    """A dpkg owner token: `name` or `name:arch`; anything else is a grammar violation."""
    if ':' in value:
        name, _, arch = value.partition(':')
        if not PKG_NAME.fullmatch(name) or not PKG_ARCH.fullmatch(arch):
            raise Bad(MALFORMED, 'package record does not fit the fixed schema')
        return value
    if not PKG_NAME.fullmatch(value):
        raise Bad(MALFORMED, 'package record does not fit the fixed schema')
    return value


def _flag_value(value):
    """A directive value: an absolute path, the bare marker, or an invalid fact kept verbatim."""
    if value == 'bare':
        return {'status': 'invalid', 'value': None}
    if not isinstance(value, str):
        raise Bad(MALFORMED, 'flag record does not fit the fixed schema')
    if PATH.fullmatch(value):
        return {'status': 'observed', 'value': value}
    return {'status': 'invalid', 'value': value}


def _entry(status, value=None):
    return {'status': status, 'value': value}


# ----------------------------------------------------------------- route parsers

def parse_engine_artifacts(raw):
    rows = _decode(raw)
    tools = {}
    plugin_source = {}
    for row in rows:
        kind = row[0]
        if kind == 'tool':
            name, target = _one(row, 2)
            if name not in ARTIFACT_TOOLS or name in tools:
                raise Bad(MALFORMED, 'tool record is unknown, duplicate or out of inventory')
            if target not in ('absent', 'invalid'):
                _token(target, PATH, 'tool path is not a fixed-schema absolute path')
            tools[name] = {'path': target, 'symlink': None, 'resolved': None,
                           'stat': None, 'sha256': None, 'package': None}
            continue
        if kind == 'plugin-source':
            name, origin = _one(row, 2)
            if name != PLUGIN_NAME or name in plugin_source:
                raise Bad(MALFORMED, 'plugin-source record is unknown or duplicate')
            if origin not in [directory + '/' + PLUGIN_NAME for directory in PLUGIN_DIRS]:
                raise Bad(MALFORMED, 'plugin-source is not a documented CLI-plugin directory')
            plugin_source[name] = origin
            continue
        name = row[1] if len(row) > 1 else None
        if name not in tools:
            raise Bad(MALFORMED, 'record names a tool without its inventory record')
        if kind == 'link':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if value not in ('0', '1'):
                raise Bad(MALFORMED, 'link record is not the fixed token')
            if tools[name]['symlink'] is not None:
                raise Bad(MALFORMED, 'duplicate link record')
            tools[name]['symlink'] = value == '1'
        elif kind == 'resolve':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if tools[name]['resolved'] is not None:
                raise Bad(MALFORMED, 'duplicate resolve record')
            if value in ('unavailable', 'invalid'):
                tools[name]['resolved'] = _entry(value)
            else:
                tools[name]['resolved'] = _entry(
                    OBSERVED, _token(value, PATH, 'resolved path is not a fixed-schema path'))
        elif kind == 'stat':
            if tools[name]['stat'] is not None:
                raise Bad(MALFORMED, 'duplicate stat record')
            if len(row) == 3:
                if row[2] not in ('unavailable', 'invalid', 'missing'):
                    raise Bad(MALFORMED, 'stat record is not the fixed token')
                tools[name]['stat'] = {'status': row[2]}
                continue
            if len(row) != 8:
                raise Bad(MALFORMED, 'stat record does not fit the fixed schema')
            mode, uid, gid, size, device, inode = row[2:]
            _token(mode, HEX, 'stat mode is not the fixed hex token')
            for value in (uid, gid, size, inode):
                _token(value, DECIMAL, 'stat field is not the fixed numeric token')
            _token(device, HEX, 'stat device is not the fixed hex token')
            tools[name]['stat'] = {'status': OBSERVED, 'mode': mode, 'uid': uid, 'gid': gid,
                                   'bytes': size, 'device': device, 'inode': inode}
        elif kind == 'hash':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if tools[name]['sha256'] is not None:
                raise Bad(MALFORMED, 'duplicate hash record')
            if value in ('unavailable', 'invalid', 'missing'):
                tools[name]['sha256'] = _entry(value)
            else:
                tools[name]['sha256'] = _entry(
                    OBSERVED, _token(value, HEX64, 'hash is not the fixed SHA-256 token'))
        elif kind == 'owner':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if tools[name]['package'] is not None:
                raise Bad(MALFORMED, 'duplicate owner record')
            if value in ('none', 'unavailable', 'invalid', 'ambiguous'):
                tools[name]['package'] = {'status': value, 'count': None, 'entries': []}
            elif not DECIMAL.fullmatch(value):
                raise Bad(MALFORMED, 'owner record is not the fixed token')
            else:
                tools[name]['package'] = {'status': OBSERVED, 'count': int(value), 'entries': []}
        elif kind == 'owner-pkg':
            _, index, package = _one(row, 3)
            record = tools[name]['package']
            if record.get('count') is None:
                raise Bad(MALFORMED, 'owner-pkg record without a counted owner')
            if index != str(len(record['entries']) + 1) or int(index) > record['count']:
                raise Bad(MALFORMED, 'owner-pkg index is out of sequence')
            record['entries'].append({'package': _package(package), 'version': None,
                                      'status': 'unobserved'})
        elif kind == 'version':
            record = tools[name]['package']
            if record is None or not record.get('entries'):
                raise Bad(MALFORMED, 'version record without a package entry')
            entry = record['entries'][-1]
            if entry['status'] != 'unobserved':
                raise Bad(MALFORMED, 'duplicate version record for one package entry')
            if len(row) == 4:
                if row[3] not in ('unavailable', 'invalid', 'ambiguous'):
                    raise Bad(MALFORMED, 'version record is not the fixed token')
                entry['status'] = row[3]
                continue
            if len(row) != 5:
                raise Bad(MALFORMED, 'version record does not fit the fixed schema')
            version, status = row[3], row[4]
            if status not in ('invalid',) and not DEB_STATUS.fullmatch(status):
                raise Bad(MALFORMED, 'version status is not the fixed token')
            if version == 'invalid' or not VERSION_TOKEN.fullmatch(version):
                entry.update(status='invalid', version=None)
            else:
                entry.update(status=OBSERVED, version=version)
            entry['dpkg_status'] = status
        else:
            raise Bad(MALFORMED, 'unknown record kind')
    if len(tools) != len(ARTIFACT_TOOLS):
        raise Bad(MALFORMED, 'the artifact inventory is partial')
    for record in tools.values():
        if record['path'] in ('absent', 'invalid'):
            if any(part is not None for part in (record['symlink'], record['resolved'],
                                                 record['stat'], record['sha256'], record['package'])):
                raise Bad(MALFORMED, 'an absent or invalid tool carries further records')
            continue
        if record['package'] is None:
            raise Bad(MALFORMED, 'a present tool carries no owner record')
        for entry in record['package']['entries']:
            if entry['status'] == 'unobserved':
                raise Bad(MALFORMED, 'a package entry carries no version record')
    return {'tools': tools, 'plugin_source': plugin_source,
            'support': sorted(SUPPORT_TOOLS)}


def parse_engine_units(raw):
    rows = _decode(raw)
    units = {name: {'status': 'absent', 'present': [], 'dropins': [], 'read': None,
                    'launch': {'status': 'unobserved', 'tokens': []},
                    'flags': {group: {'status': 'unobserved', 'entries': []}
                              for group in ('config', 'root', 'address')},
                    'listen': {'status': 'unobserved', 'values': []},
                    'truncated': False}
             for name in UNIT_NAMES}
    for row in rows:
        kind = row[0]
        if kind in ('present', 'dropin'):
            name, role = _one(row, 2)
            if name not in UNIT_NAMES or role not in UNIT_ROLES:
                raise Bad(MALFORMED, 'presence record is unknown or out of inventory')
            unit = units[name]
            listing = unit['present'] if kind == 'present' else unit['dropins']
            if role in listing:
                raise Bad(MALFORMED, 'duplicate presence record')
            listing.append(role)
            if unit['status'] == 'absent':
                unit['status'] = 'present'
            continue
        if kind == 'read':
            name, origin = _one(row, 2)
            if name not in UNIT_NAMES or units[name]['read'] is not None:
                raise Bad(MALFORMED, 'read record is unknown or duplicate')
            if origin in ('symlink', 'nonregular'):
                units[name]['read'] = {'status': origin, 'source': None}
            else:
                _token(origin, PATH, 'read source is not a fixed-schema path')
                if origin not in PACKAGED_DIRS:
                    raise Bad(MALFORMED, 'read source is not a packaged unit directory')
                units[name]['read'] = {'status': OBSERVED, 'source': origin}
            continue
        name = row[1] if len(row) > 1 else None
        if name not in units or units[name]['read'] is None or units[name]['read']['status'] != OBSERVED:
            raise Bad(MALFORMED, 'directive record without a read packaged unit')
        unit = units[name]
        if kind == 'launch':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if value == 'none':
                unit['launch']['tokens'].append(_entry('none'))
            elif LAUNCH_TOKEN.fullmatch(value):
                unit['launch']['tokens'].append(_entry(OBSERVED, value))
            else:
                unit['launch']['tokens'].append(_entry('invalid', value))
            if len(unit['launch']['tokens']) > 1:
                unit['launch']['status'] = AMBIGUOUS
            elif unit['launch']['status'] != AMBIGUOUS:
                unit['launch']['status'] = unit['launch']['tokens'][0]['status']
        elif kind == 'flag':
            name, flag, value = _one(row, 3)
            if flag not in FLAG_GROUPS:
                raise Bad(MALFORMED, 'flag record is not allowlisted')
            group = unit['flags'][FLAG_GROUPS[flag]]
            if any(entry['flag'] == flag for entry in group['entries']):
                group['status'] = AMBIGUOUS
            group['entries'].append(dict(_flag_value(value), flag=flag))
            if group['status'] != AMBIGUOUS:
                group['status'] = 'observed'
        elif kind == 'listen':
            if len(row) != 3:
                raise Bad(MALFORMED, 'record does not fit the fixed schema')
            value = row[2]
            if value == 'invalid':
                unit['listen']['values'].append(_entry('invalid'))
            else:
                unit['listen']['values'].append(_flag_value(value))
            if len(unit['listen']['values']) > 1:
                unit['listen']['status'] = AMBIGUOUS
            elif unit['listen']['status'] != AMBIGUOUS:
                unit['listen']['status'] = unit['listen']['values'][0]['status']
        elif kind == 'overrun':
            _one(row, 1)
            if unit['truncated']:
                raise Bad(MALFORMED, 'duplicate overrun record')
            unit['truncated'] = True
        else:
            raise Bad(MALFORMED, 'unknown record kind')
    for unit in units.values():
        unit['effective'] = 'absent' if unit['status'] == 'absent' else (
            'overridden' if any(role in unit['present'] for role in ('admin', 'runtime', 'local'))
            else 'drop-in-edited' if any(role in unit['dropins'] for role in ('admin', 'runtime', 'local'))
            else 'packaged' if unit['read'] is not None and unit['read']['status'] == OBSERVED
            else 'packaged-source-unreadable')
    return {'units': units}


def _parse(route, raw):
    if route['id'] == 'engine-artifacts':
        return parse_engine_artifacts(raw)
    return parse_engine_units(raw)


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
    if code not in (0, None):
        reasons = ['the fixed read exited ' + str(code)]
        try:
            return FAILED, _parse(route, raw), reasons
        except Bad:
            return FAILED, {}, reasons
    try:
        return OBSERVED, _parse(route, raw), []
    except Bad as exc:
        return exc.status, {}, [exc.reason]


# ----------------------------------------------------------------- per-read supervision

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

def correlate(rows):
    """Every engine-input fact as an observation, with missing parts left unknown."""
    artifacts = facts_of(rows, 'engine-artifacts')
    units = facts_of(rows, 'engine-units')
    return {
        'artifacts': artifacts.get('tools', {}),
        'plugin_source': artifacts.get('plugin_source', {}),
        'units': units.get('units', {}),
        'unit_effectiveness_note': ('a custom override or drop-in means the packaged body is not the '
                                    'effective unit; its content is unknown and never read, so the '
                                    'effective launch arguments stay unknown'),
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
            findings.append('engine-input observation %s: %s (%s)' % (row['id'], row['status'], detail))
    for name, unit in facts.get('units', {}).items():
        if unit.get('truncated'):
            findings.append('engine-input observation engine-units: %s body overrun before completion'
                            % name)
    complete = not findings and not any(row['status'] in INCOMPLETE for row in rows)
    if complete:
        eligibility = ('eligible for the later separately pinned original-stream observation; '
                       'no E12 admission, confinement, quota-bound or launch credit')
    else:
        eligibility = 'incomplete: missing facts stay unknown and grant nothing'
    return {'schema': SCHEMA, 'routes_sha256': routes_digest(),
            'routes': rows, 'facts': facts, 'limits': dict(LIMITS),
            'observation_eligibility': eligibility,
            'complete': complete,
            'findings': findings}
