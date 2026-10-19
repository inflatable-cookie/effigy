import sys,os,json,pathlib,subprocess,time,signal
root=pathlib.Path(os.environ['MATRIX_ROOT']);state=root/'engine';args=sys.argv[2:];op=sys.argv[1]
def kill_recorded():
 records=list(root.glob('*.pid'))
 # A real container down ends every workload, including non-group leaders.
 # Keep group termination for descendants, but always signal the PID too.
 for sig in (signal.SIGTERM,signal.SIGKILL):
  for p in records:
   try: pid=int(p.read_text())
   except (FileNotFoundError,ValueError): continue
   for send in (os.killpg,os.kill):
    try: send(pid,sig)
    except ProcessLookupError: pass
  if sig==signal.SIGTERM: time.sleep(.1)
 for p in records: p.unlink(missing_ok=True)
if op=='down':
 with (state/'down-events').open('a') as events: events.write(args[0]+'\n')
 if (state/'deny-down').exists(): sys.exit(71)
 if (state/'slow-down').exists(): time.sleep(float((state/'slow-down').read_text()))
 kill_recorded()
 (state/('active-'+args[0])).unlink(missing_ok=True)
 sys.exit(0)
if op=='payload' or (op=='exec' and '.pgid' in args[-1]):
 payload=args[-1].replace('/usr/local/bin/effigy',str(root/'effigy-wrapper'))
 with open(root/('remote-'+str(os.getpid())+'.out'),'wb') as log:
  child=subprocess.Popen(['sh','-c',payload],start_new_session=True,stdout=log,stderr=log)
  (root/('remote-'+str(os.getpid())+'.pid')).write_text(str(child.pid))
  sys.exit(child.wait())
if op=='exec':
 if (state/'deny-exec').exists(): sys.exit(72)
 if args[-1].startswith('alive()'): sys.exit(subprocess.call(['sh','-c',args[-1]]))
 sys.exit(0)
if op=='inspect':
 proof=json.loads((state/('inspect-'+args[-1]+'.json')).read_text());project=proof[0]['Config']['Labels']['com.docker.compose.project']
 if (state/('stopped-'+project)).exists(): proof[0]['State']={'Status':'exited','Running':False}
 print(json.dumps(proof));sys.exit(0)
if op=='ps':
 project=next((a.rsplit('=',1)[1] for a in args if a.startswith('label=com.docker.compose.project=')),None)
 service=next((a.rsplit('=',1)[1] for a in args if a.startswith('label=com.docker.compose.service=')),None)
 if not project: sys.exit(0)
 stopped=(state/('stopped-'+project)).exists()
 if not (state/('active-'+project)).exists() or (stopped and '--all' not in args): sys.exit(0)
 fmt=args[args.index('--format')+1] if '--format' in args else ''
 for line in (state/('ps-'+project+'.txt')).read_text().splitlines():
  name,status,ports,proj,wd,svc,oneoff=line.split('\t')
  if (service and service!=svc) or ((state/('missing-'+project)).exists() and svc=='app'): continue
  if stopped: status='Exited (1)'
  proof=json.loads((state/('inspect-'+name+'.json')).read_text())[0];labels=proof['Config']['Labels']
  if '{{.ID}}' in fmt:
   values=[proof['Id'],status,oneoff,'',labels['com.effigy.checkout'],''] if '{{.Status}}' in fmt else [proof['Id'],proj,'',labels['com.effigy.checkout'],'']
  else: values=[name,status,ports,proj,wd,svc,oneoff]
  print('\t'.join(values))
