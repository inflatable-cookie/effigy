"""Private qualification driver; binding path/hash MUST come from authenticated owner dispatch."""
import datetime
import json
import os
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import payload_collection as p


def main(argv):
    r = p.r
    r.require(len(argv) == 2, 'exact owner binding path and SHA256 required')
    binding = p.load(p.a.bound_file(argv[0], argv[1]))
    r.require(set(binding) == {'schema', 'armed', 'noAutomaticRetry', 'source_head', 'inputs', 'inputs_sha256',
        'go_index', 'go_index_sha256', 'evidence', 'started_at', 'expires_at', 'latest_start', 'report_reserve_seconds'}
        and binding['schema'] == 'effigy.guestagent-collection-binding.v1' and binding['armed'] is True
        and binding['noAutomaticRetry'] is True, 'fresh explicit owner binding required')
    now = datetime.datetime.now(datetime.timezone.utc)
    start, end, latest = [datetime.datetime.fromisoformat(binding[k]) for k in ('started_at', 'expires_at', 'latest_start')]
    reserve = binding['report_reserve_seconds']
    r.require(all(t.tzinfo is not None for t in (start,end,latest))
        and type(reserve) is int and reserve >= 60 and start <= now <= latest < end
        and 0 < (end-start).total_seconds() <= 600 and (end-latest).total_seconds() >= reserve+10,
        'expired or insufficient inclusive/report window')
    r.require(p.source_head(p.PACKET.parents[2]) == binding['source_head'], 'source differs')
    for path,sha in [('inputs','inputs_sha256'),('go_index','go_index_sha256')]:
        p.a.bound_file(binding[path],binding[sha])
    directory = Path(binding['evidence'])
    # The owner prepares parent0700 and empty observation0700 before the window.
    parent = directory.parent
    r.require(directory.is_absolute() and directory.is_relative_to(p.a.SCRATCH)
              and not directory.is_relative_to(Path(r.CONTROL)) and not directory.is_relative_to(Path(r.SHARED))
              and not directory.is_relative_to(p.a.SCRATCH/'q') and directory.resolve() == directory
              and parent.stat().st_uid == os.geteuid() and parent.stat().st_mode & 0o777 == 0o700,
              'private owner evidence parent required')
    p.exclusive(parent/'payload-driver-call-intent.json', dict(source_head=binding['source_head'],
                inputs_sha256=binding['inputs_sha256'], pid=os.getpid(), started=p.clock()))
    deadline = r.Deadline(min(end-datetime.timedelta(seconds=reserve),now+datetime.timedelta(seconds=9)),
                          time.monotonic()+9)
    result = p.collect(binding['inputs'],binding['inputs_sha256'],binding['go_index'],
                       binding['go_index_sha256'],deadline,directory)
    p.exclusive(parent/'payload-driver-result.json',dict(result=result,pid=os.getpid(),argv=sys.argv,
                                                       finished=p.clock()))
    print(json.dumps({k:result.get(k) for k in ('status','attempts','admitted','failure_type')}))
    return 0 if result['status'] == 'reference-match-evidence-only' else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main(sys.argv[1:]))
    except Exception as error:
        print(json.dumps(dict(status='refused',failure_type=type(error).__name__)))
        raise SystemExit(1)
