"""Collection gate and owned gzip data only; no real payload or target observer."""
import copy
import datetime
import gzip
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import payload_collection as p
import payload_collect_once as driver
from recovery_boundaries import deadline


class PayloadBoundaries(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='531-payload-boundary-', dir=os.environ['TMPDIR'])
        self.addCleanup(self.temp.cleanup);self.root = Path(self.temp.name)
        self.head = '1'*40
        self.path = str(Path('/opt')/'homebrew'/'share'/'lima'/'lima-guestagent.Linux-aarch64.gz')
        raw = ('94093 1 501 Mon Nov 2 23:37:10 2026 synthetic-comm synthetic-alias '
               'hostagent --guestagent '+self.path+' synthetic-profile\n').encode()
        retained = self.root/'synthetic-retained.private';retained.write_bytes(raw)
        self.plan = dict(schema='effigy.guestagent-evidence.v1', source_head=self.head,
            programs={n:p.digest(p.PACKET/n) for n in p.PROGRAMS}, target_path=self.path, owner_uid=501,
            retained_stdout=dict(path=str(retained),sha256=p.digest(retained)),budget_seconds=600)
        self.go = dict(schema='effigy.workspace-private-native-go.v1',operator='Tom',kind='native',
            operator_authority='operator (board)',decision_id='synthetic-payload-collection',source_head=self.head,
            backend='colima',budget_seconds=600,engine_identity=p.r.PROFILE,effects=p.EFFECT,action=p.ACTION,
            count=1,allowance='new-guestagent-evidence',target_path_sha256=hashlib.sha256(self.path.encode()).hexdigest(),
            started_at=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(seconds=1)).isoformat(),
            expires_at=(datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=30)).isoformat())
        for name,value in [('source_head',lambda _:self.head),('RETAINED_SHA',p.digest(retained))]:
            mocked=patch.object(p,name,value);mocked.start();self.addCleanup(mocked.stop)

    def files(self):
        def write(name,value):
            file=self.root/name;file.write_text(json.dumps(value));return str(file),p.digest(file)
        inputs,ih=write('inputs.json',self.plan);self.go['inputs_sha256']=ih
        go,gh=write('go.json',self.go);index,bh=write('index.json',dict(colima=dict(path=go,sha256=gh)))
        return inputs,ih,index,bh

    def evidence(self):
        return Path(tempfile.mkdtemp(prefix='evidence-',dir=self.root))

    def test_old_wrong_unbound_expired_authority_refuses_before_target_access(self):
        original=copy.deepcopy(self.go)
        for change in [dict(effects='workspace-private-531-preflight-r1'),dict(effects='workspace-private-531-admin-sigint-r1'),
                       dict(operator='planner'),dict(operator_authority='worker'),dict(action='admit-payload'),
                       dict(count=2),dict(allowance='consumed-8335884c'),dict(target_path_sha256='0'*64),
                       dict(expires_at='2020-01-01T00:00:00+00:00')]:
            self.go=dict(original,**change)
            with patch.object(p,'NativeCollector') as native:
                result=p.collect(*self.files(),deadline(),self.evidence())
                self.assertEqual(result['attempts'],0);self.assertFalse(result['admitted']);native.assert_not_called()
        self.go=original;args=list(self.files());args[1]='0'*64
        with patch.object(p,'NativeCollector') as native:
            self.assertEqual(p.collect(*args,deadline(),self.evidence())['attempts'],0);native.assert_not_called()
        self.plan['target_path']=str(self.root/'foreign.gz')
        with patch.object(p,'NativeCollector') as native:
            self.assertEqual(p.collect(*self.files(),deadline(),self.evidence())['attempts'],0);native.assert_not_called()

    def test_unarmed_driver_cannot_call_collector(self):
        binding=self.root/'binding.json';binding.write_text(json.dumps(dict(armed=False)))
        with patch.object(p,'collect',side_effect=AssertionError('no native collection')) as collect:
            with self.assertRaises(p.r.Refused):driver.main([str(binding),p.digest(binding)])
            collect.assert_not_called()
        started=time.monotonic()
        child=subprocess.Popen([sys.executable,'-I','-B',str(p.PACKET/'payload_collect_once.py'),
                                str(binding),p.digest(binding)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            out,err=child.communicate(timeout=3)
        finally:
            if child.poll() is None:child.kill()
            child.wait()
        print(json.dumps(dict(owned_payload_driver_fixture_pid=child.pid,waited=True,exit=child.returncode,
                              elapsed_monotonic=time.monotonic()-started,fixture_only=True)),flush=True)
        self.assertEqual(child.returncode,1);self.assertEqual(json.loads(out)['status'],'refused');self.assertFalse(err)

    def test_owned_gzip_hash_is_compressed_domain_and_regular_type_is_required(self):
        raw=b'benign synthetic payload';compressed=gzip.compress(raw,mtime=0)
        file=self.root/'owned.gz';file.write_bytes(compressed);file.chmod(0o600)
        result=p.hash_regular(file,os.geteuid(),deadline(),self.evidence())
        self.assertEqual(result['compressed_file_sha256'],hashlib.sha256(compressed).hexdigest())
        self.assertNotEqual(result['compressed_file_sha256'],hashlib.sha256(raw).hexdigest())
        with self.assertRaises(p.r.Refused):p.hash_regular(self.root,os.geteuid(),deadline(),self.evidence())
        link=self.root/'alias.gz';link.symlink_to(file.name)
        with self.assertRaises(p.r.Refused):p.hash_regular(link,os.geteuid(),deadline(),self.evidence())
        file.chmod(0o666)
        with self.assertRaises(p.r.Refused):p.hash_regular(file,os.geteuid(),deadline(),self.evidence())

    def test_collection_is_one_attempt_evidence_never_admission_or_v2_input(self):
        data=gzip.compress(b'owned benign gzip',mtime=0);file=self.root/'fixture.gz';file.write_bytes(data);file.chmod(0o600)
        class Fixture:
            fixture_only=True
            def collect(self,plan,canonical,bound,directory):
                return p.hash_regular(file,os.geteuid(),bound,directory)
        # Reference data is a synthetic independent fixture, never a host digest.
        public=dict(payload_name='lima-guestagent.Linux-aarch64.gz',artifacts=[dict(kind='fixture',compressed_payload_sha256=hashlib.sha256(data).hexdigest())])
        actual_load=p.load
        def load(path):return public if Path(path)==p.PACKET/'payload-public-digests.json' else actual_load(path)
        with patch.object(p,'load',side_effect=load):
            evidence=self.evidence();result=p.collect(*self.files(),deadline(),evidence,_collector=Fixture())
            self.assertEqual(result['status'],'reference-match-evidence-only');self.assertEqual(result['attempts'],1)
            self.assertFalse(result['admitted']);self.assertFalse(result['installed_package_provenance_verified'])
            self.assertEqual((result['target_signals'],result['watches'],result['sigint_claims'],result['resumes']),(0,0,0,0))
            with self.assertRaises(p.r.Refused):p.collect(*self.files(),deadline(),evidence,_collector=Fixture())
            public['artifacts'][0]['compressed_payload_sha256']=hashlib.sha256(b'outer archive domain').hexdigest()
            refused=p.collect(*self.files(),deadline(),self.evidence(),_collector=Fixture())
            self.assertEqual(refused['status'],'refused');self.assertFalse(refused['admitted'])
