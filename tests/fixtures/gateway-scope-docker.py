#!/usr/bin/env python3
"""Private, file-backed Compose fixture for the linked gateway scope proof."""
import hashlib
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
root = Path(os.environ['HOME'])
state = root / 'fake-runtime.json'
records = json.loads(state.read_text()) if state.exists() else {}
with open(os.environ['FAKE_DOCKER_LOG'], 'a') as log:
    log.write(repr(args) + '\n')

def save():
    state.write_text(json.dumps(records))

def selected():
    project = next((x.split('=', 2)[-1] for x in args if x.startswith('label=com.docker.compose.project=')), None)
    service = next((x.split('=', 2)[-1] for x in args if x.startswith('label=com.docker.compose.service=')), None)
    return [r for r in records.values() if (project is None or r['project'] == project) and (service is None or r['service'] == service)]

if args and args[0] == 'ps':
    fmt = args[args.index('--format') + 1] if '--format' in args else ''
    for r in selected():
        if 'shared-render' in fmt:
            fields = [r['name'], 'Up 1 minute', r.get('render', ''), r['project']]
        elif '{{.ID}}' in fmt and '{{.Status}}' in fmt:
            fields = [r['id'], 'Up 1 minute', 'False', r['checkout'], r['checkout'], r.get('scope', '')]
        elif '{{.ID}}' in fmt:
            fields = [r['id'], r['project'], r['checkout'], r['checkout'], r.get('scope', '')]
        else:
            ports = '127.0.0.1:' + (root / 'fake-db-port').read_text().strip() + '->3306/tcp' if r['service'] in ('db', 'client') else ''
            fields = [r['name'], 'Up 1 minute', ports, r['project'], r['checkout'], r['service'], 'False']
        print('\t'.join(fields))
elif args and args[0] == 'inspect':
    r = next((r for r in records.values() if args[1] in (r['id'], r['name'])), None)
    if r is None:
        sys.exit(1)
    labels = {'com.docker.compose.project': r['project'], 'com.docker.compose.service': r['service'], 'com.docker.compose.oneoff': 'False', 'com.effigy.checkout': r['checkout']}
    if r.get('scope'):
        labels['com.effigy.scope'] = r['scope']
    ports = {'3306/tcp': [{'HostIp': '127.0.0.1', 'HostPort': (root / 'fake-db-port').read_text().strip()}]} if r['service'] in ('db', 'client') else {}
    print(json.dumps([{'Id': r['id'], 'Name': '/' + r['name'], 'State': {'Status': 'running', 'Running': True}, 'Config': {'Labels': labels}, 'NetworkSettings': {'Ports': ports}}]))
elif args and args[0] == 'compose':
    project = args[args.index('-p') + 1]
    files = [args[i + 1] for i, value in enumerate(args[:-1]) if value == '-f']
    action = next((x for x in args if x in ('up', 'down', 'ps', 'exec')), '')
    if action == 'up':
        shared = project.startswith('effigy-shared-')
        checkout = os.environ.get('FAKE_CHECKOUT', '')
        scope_file = os.environ.get('FAKE_SCOPE_FILE', '')
        scope = Path(scope_file).read_text().strip() if scope_file and Path(scope_file).exists() else ''
        for service in (['backing'] if shared else ['client', 'db']):
            name = project + '-' + service + '-1'
            if name not in records:
                records[name] = {'id': hashlib.sha256(name.encode()).hexdigest(), 'name': name, 'project': project, 'service': service, 'checkout': checkout, 'scope': scope, 'render': Path(files[0]).parent.name if shared else ''}
        save()
    elif action == 'down':
        records = {key: r for key, r in records.items() if r['project'] != project}
        save()
    elif action == 'ps':
        print('[]')
