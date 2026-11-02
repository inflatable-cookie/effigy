#!/usr/bin/env python3
"""Strict estimator for ONLY the fixed ASCII #524 synthetic render fixtures.

This independent source transcription is not a general Compose/YAML parser and
does not represent candidate-preflight output or a runtime measurement.
Unexpected syntax refuses rather than guessing.
"""
import json
import re
import sys
from pathlib import Path

render=json.loads(Path(sys.argv[1]).read_text())['result']
env=json.loads(next(x['content'] for x in render['files'] if x['name']=='inputs/environment.json'))
contents={x['path']:x['content'] for x in render['files']}
mounts={}
for path in render['compose_files']:
    text=contents[path]
    lines=text.splitlines()
    active=False
    current=None
    def commit():
        if current is None: return
        source=current.get('source',''); target=current.get('target','')
        kind=current.get('type','volume'); ro=current.get('read_only','false')=='true'
        if not re.fullmatch(r'/workspace-root/synthetic/[0-9]{2}',target): raise RuntimeError('unexpected target')
        if kind=='bind':
            if source and not re.fullmatch(r'/[A-Za-z0-9_./-]+',source): raise RuntimeError('unexpected bind source')
            resolved=source
        elif kind=='volume':
            if source and not re.fullmatch(r'[A-Za-z0-9_.-]+',source): raise RuntimeError('unexpected volume source')
            volume='anonymous' if not source else env['project_name']+'-'+source
            resolved='/var/lib/nerdctl/'+'0'*64+'/volumes/default/'+volume+'/_data'
        else: raise RuntimeError('unexpected mount kind')
        mounts[target]=(resolved,ro)
    for line in lines:
        if line=='    volumes:':
            if active: raise RuntimeError('duplicate volumes section')
            active=True
            continue
        if not active: continue
        if line.startswith('    - '):
            commit()
            item=line[6:]
            if item.startswith('type: '):
                current={'type':item[6:]}
            else:
                parts=item.split(':')
                if len(parts)!=3 or parts[2] not in ('ro','rw'): raise RuntimeError('unexpected short mount')
                current={'source':parts[0],'target':parts[1],'type':'bind' if parts[0].startswith('/') else 'volume','read_only':str(parts[2]=='ro').lower()}
        elif line.startswith('      ') and current is not None:
            m=re.fullmatch(r'      (source|target|read_only): (.*)',line)
            if not m: raise RuntimeError('unexpected long mount field')
            current[m.group(1)]=m.group(2)
        else:
            commit(); current=None; active=False
    if active: commit()

def go_json(value):
    return json.dumps(value,ensure_ascii=False,separators=(',',':')).replace('&',r'\u0026').replace('<',r'\u003c').replace('>',r'\u003e').replace('\u2028',r'\u2028').replace('\u2029',r'\u2029')
sizes=[]
for target,(source,ro) in sorted(mounts.items()):
    mode='ro' if ro else ''
    rw='false' if ro else 'true'
    raw='{"Type":'+go_json('bind')+',"Source":'+go_json(source)+',"Destination":'+go_json(target)+',"Mode":'+go_json(mode)+',"RW":'+rw+',"Propagation":""}'
    sizes.append(len(raw.encode()))
total=len('nerdctl/mounts')+2+sum(sizes)+max(0,len(sizes)-1)
margin=(total+49)//50
print(json.dumps({'reserve_bytes':margin,'guarded_bytes':total+margin,'interpretation':'independent source-model estimate, not candidate preflight or runtime','mount_count':len(sizes),'estimated_bytes':total,'budget':3900,'fits':total+margin<3900,'largest_contribution_bytes':sorted(sizes,reverse=True)[:6]}))
