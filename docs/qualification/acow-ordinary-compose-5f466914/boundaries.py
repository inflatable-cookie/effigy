#!/usr/bin/env python3
"""Committed refusal-boundary battery for the Acow ordinary-Compose packet.

Queue task effigy#540 (amended by effigy#543: the battery proves the
selected-current-head consumer qualification and the approved bundle
source-identity transition; amended by effigy#545: the prerequisite is the
actual effigy#544 r18 proof receipt, and the old bare green-#539 receipt is
proven not to be the real prerequisite). This is the only file the installed-Effigy
selector `test:qualification:acow` executes. It runs the packet's fail-closed
boundaries against fake backends and disposable fixture metadata under
/Volumes/Builds/scratch/effigy/540/ — no real runtime call, no consumer
stack, no vault read, no warm-up, no build/install, no native probe ever
happens here. Passing these controls is not consumer qualification; the
packet's proof routes stay separately GO-armed (packet-r1.md).
"""
import hashlib
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import primary  # noqa: E402
import warmup  # noqa: E402
from common import (  # noqa: E402
    BUNDLE_DECLARATION, BUNDLE_PRE_DIGEST, BUNDLE_PRE_REVISION,
    BUNDLE_TARGET_DIGEST, BUNDLE_TARGET_REVISION_PREFIX,
    CANDIDATE_BINARY_SHA256, CANDIDATE_VERSION_MARKER,
    EXPECTED_PRIMARY_COUNTS, GO_PRIMARY_TOKENS, GO_WARMUP_TOKENS,
    MANIFEST_INCLUDES, PROOF_COMMIT, PROOF_INPUTS, PROOF_SELECTOR,
    PROOF_TASK_ID, PROOF_TASK_REFERENCE, PREREQUISITE_MAX_BYTES,
    PREREQUISITE_SCHEMA, QUEUE_SHAPE_TOTAL_LEN, RELEVANT_INPUT_BLOBS,
    Refused, assert_no_scoped_containers, assert_no_warmup_scope,
    assert_packet_dir, capture_inventory, classify_bundle_state,
    container_state, ensure_owned_dir, exact_ps_ids,
    judge_teardown_inventory, label_length_only, parse_bundle_pin,
    parse_manifest_includes, parse_network_rows, parse_volume_names,
    queue_shape_components, read_owner_marker, require_mariadb_unchanged,
    scoped_container_map, session_identity, validate_attach_record,
    verify_544_proof_receipt, verify_bundle_transition_after,
    verify_bundle_transition_before, verify_consumer_pins,
    verify_go_receipt, verify_installed_identity, verify_packet_bundle,
    volume_mount_identity, write_owner_marker,
)
from primary import compare_counts, observation_plan  # noqa: E402

PASS = []
FAIL = []


class FakeConsole:
    """A console sink that keeps nothing (boundary tests only)."""

    def write(self, data):
        pass

    def flush(self):
        pass

    def close(self):
        pass



def check(name, fn):
    try:
        fn()
        PASS.append(name)
        print('ok   %s' % name)
    except Refused as e:
        FAIL.append('%s: refused unexpectedly: %s' % (name, e))
        print('FAIL %s: unexpected refusal: %s' % (name, e))
    except AssertionError as e:
        FAIL.append('%s: %s' % (name, e))
        print('FAIL %s: %s' % (name, e))
    except Exception as e:  # noqa: BLE001 — the battery reports any failure
        FAIL.append('%s: %s: %s' % (name, type(e).__name__, e))
        print('FAIL %s: %s: %s' % (name, type(e).__name__, e))


def expect_refused(fn, fragment=None):
    try:
        fn()
    except Refused as e:
        if fragment is not None and fragment not in str(e):
            raise AssertionError('refusal text %r lacks %r' % (str(e), fragment)) from None
        return
    raise AssertionError('expected a fail-closed refusal, got none')


def tmpfile(root, name, data, mode=0o600):
    p = root / name
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0), mode)
    with os.fdopen(fd, 'wb') as f:
        f.write(data if isinstance(data, bytes) else data.encode())
    os.chmod(p, mode)
    return p


def sha(data):
    return hashlib.sha256(data if isinstance(data, bytes) else data.encode()).hexdigest()


PACKET_SHA = None
FAKE_RESERVATION = 'acow-reservation-2026-1103-a1'


def warmup_go_text(packet_sha, **over):
    parts = [
        'packet ' + over.get('packet', packet_sha),
        'GO',
        'Supervisor',
        'Tom',
        'colima idle',
        'reservation ' + FAKE_RESERVATION,
        'queue unpaused',
        'qa fence null',
        '544 receipt ' + over.get('r544', 'b' * 64),
        'version ' + over.get('version', 'effigy v0.14.1+local.5f46691'),
        'binary ' + over.get('binary', '/Volumes/Builds/scratch/effigy/540/fake-install/effigy'),
        CANDIDATE_BINARY_SHA256,
    ]
    return '\n'.join(parts) + '\n'


def primary_go_text(packet_sha, **over):
    return '\n'.join([
        'packet ' + over.get('packet', packet_sha),
        'GO',
        'Tom',
        'primary-data',
        'reservation ' + FAKE_RESERVATION,
        'queue unpaused',
        'qa fence null',
    ]) + '\n'


# --- packet self-integrity -------------------------------------------------

def t_packet_bundle(tmp):
    bundle = verify_packet_bundle()
    assert len(bundle['packet_sha256']) == 64
    assert bundle['manifest']['schema'] == 'effigy.qualification.acow-ordinary-compose.r1'


def t_manifest_helpers_match_disk(tmp):
    obj = json.loads((Path(common.DIR) / 'manifest-r1.json').read_text())
    for name, want in obj['helpers'].items():
        got = hashlib.sha256((Path(common.DIR) / name).read_bytes()).hexdigest()
        assert got == want, 'manifest helper hash drift: ' + name
    assert 'boundaries.py' in obj['helpers']


# --- GO receipts -----------------------------------------------------------

def t_go_missing(tmp):
    expect_refused(lambda: verify_go_receipt(tmp / 'nope.txt', PACKET_SHA,
                                             GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'missing')


def t_go_wrong_path(tmp):
    nested = tmp / 'nested'
    nested.mkdir()
    p = tmpfile(nested, 'go.txt', warmup_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'pinned scratch path')


def t_go_wrong_packet(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA, packet='9' * 64))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'packet SHA')


def t_go_mode(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA), mode=0o644)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'mode/owner/type')


def t_go_symlink(tmp):
    target = tmpfile(tmp, 'target.txt', warmup_go_text(PACKET_SHA))
    link = tmp / 'go.txt'
    os.symlink(target, link)
    expect_refused(lambda: verify_go_receipt(link, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'pinned scratch path')


def t_go_missing_tokens(tmp):
    for i, token in enumerate(('GO', 'Supervisor', 'Tom', 'colima idle',
                               'queue unpaused', 'qa fence null',
                               CANDIDATE_BINARY_SHA256)):
        text = warmup_go_text(PACKET_SHA).replace(token, 'REDACTED-TOKEN')
        p = tmpfile(tmp, 'go-token-%d.txt' % i, text)
        expect_refused(lambda p=p, text=text: verify_go_receipt(
            p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup', scratch_dir=tmp),
            token)


def t_go_missing_reservation(tmp):
    text = warmup_go_text(PACKET_SHA).replace('reservation ' + FAKE_RESERVATION, 'reservation')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'reservation')


def t_go_missing_544_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace('544 receipt ' + 'b' * 64, '')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   '544 receipt')


def t_go_stale_539_token(tmp):
    # The old bare #539 prerequisite is dead: a fresh GO that still quotes it
    # refuses instead of silently carrying dead provenance.
    text = warmup_go_text(PACKET_SHA).replace(
        '544 receipt ' + 'b' * 64,
        '539 receipt ' + 'b' * 64 + '\n544 receipt ' + 'b' * 64)
    p = tmpfile(tmp, 'go-stale.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'stale superseded #539')


def t_go_missing_version_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace(
        'version effigy v0.14.1+local.5f46691', 'installed build present')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'version')


def t_go_missing_binary_line(tmp):
    text = warmup_go_text(PACKET_SHA).replace(
        'binary /Volumes/Builds/scratch/effigy/540/fake-install/effigy',
        'binary-at /Volumes/Builds/scratch/effigy/540/fake-install/effigy')
    p = tmpfile(tmp, 'go.txt', text)
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'binary')


def t_go_warmup_valid(tmp):
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA))
    rec = verify_go_receipt(p, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup', scratch_dir=tmp)
    assert rec['reservation_id'] == FAKE_RESERVATION
    assert rec['receipt544_sha256'] == 'b' * 64


def t_route_separation(tmp):
    # A valid warm-up GO verified as the primary route refuses: the routes are
    # separately armed; a warm-up receipt never arms counts observation.
    p = tmpfile(tmp, 'go.txt', warmup_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p, PACKET_SHA, GO_PRIMARY_TOKENS, 'primary',
                                             scratch_dir=tmp),
                   'primary-data')
    # A valid primary GO verified as the warm-up route refuses: it carries no
    # Supervisor/544/version/binary provenance.
    p2 = tmpfile(tmp, 'go2.txt', primary_go_text(PACKET_SHA))
    expect_refused(lambda: verify_go_receipt(p2, PACKET_SHA, GO_WARMUP_TOKENS, 'warmup',
                                             scratch_dir=tmp),
                   'Supervisor')


def t_primary_go_valid(tmp):
    p = tmpfile(tmp, 'go.txt', primary_go_text(PACKET_SHA))
    rec = verify_go_receipt(p, PACKET_SHA, GO_PRIMARY_TOKENS, 'primary', scratch_dir=tmp)
    assert rec['route'] == 'primary' and rec['reservation_id'] == FAKE_RESERVATION


# --- effigy#544 r18 prerequisite receipt ------------------------------------

EXEC_RUN_ID = 'aaaaaaaa-0000-4c11-8e22-333333333333'
REVIEWER_RUN_ID = 'bbbbbbbb-0000-4d22-8f33-444444444444'
PROOF_AGENT_ID = 'cccccccc-0000-4e33-8044-555555555555'


def r544_execution_record(**over):
    """A faithful task.proofExecutions record shape (settled pass)."""
    rec = {
        'runId': EXEC_RUN_ID,
        'agentId': PROOF_AGENT_ID,
        'workspaceId': 'wks_544proofexecutionfixture',
        'profile': None,
        'operationId': 'proof:a648d634-82cf-4b3c-8903-a009eb427c12',
        'selector': PROOF_SELECTOR,
        'inputs': list(PROOF_INPUTS),
        'repository': 'inflatable-cookie/effigy',
        'prNumber': None,
        'branch': 'main',
        'head': PROOF_COMMIT,
        'briefVersion': 1,
        'briefDigest': 'sha256:' + 'd' * 64,
        'authorizedBy': {'role': 'operator', 'caller': 'tom',
                         'note': 'consumer qualification GO'},
        'activatedAt': '2026-11-02T12:00:00.000Z',
        'settledAt': '2026-11-02T12:34:56.789Z',
        'status': 'passed',
        'historical': False,
        'exitCode': 0,
        'evidence': 'r18 ordinary-Compose sweep passed; exit 0',
    }
    rec.update(over)
    return rec


def r544_fixture(**over):
    """The exact-Queue-shaped prerequisite receipt (all checks pass)."""
    execution = over.pop('execution', r544_execution_record())
    exec_run_id = (execution or {}).get('runId') or EXEC_RUN_ID
    obj = {
        'schema': PREREQUISITE_SCHEMA,
        'task': {'reference': PROOF_TASK_REFERENCE, 'id': PROOF_TASK_ID},
        'proofOnly': {'commit': PROOF_COMMIT},
        'proofExecution': {'selector': PROOF_SELECTOR,
                           'inputs': list(PROOF_INPUTS), 'role': 'author'},
        'execution': execution,
        'result': {
            'status': 'passed',
            'commit': PROOF_COMMIT,
            'selector': PROOF_SELECTOR,
            'executionRunId': exec_run_id,
            'reviewerRunId': REVIEWER_RUN_ID,
            'evidence': 'Qualification passed: %s at %s; executor run %s. '
                        'Reviewer verdict ready_to_merge.'
                        % (PROOF_SELECTOR, PROOF_COMMIT, exec_run_id),
        },
        'review': {
            'runId': REVIEWER_RUN_ID,
            'agentId': PROOF_AGENT_ID,
            'head': PROOF_COMMIT,
            'status': 'ready_to_merge',
            'reviewId': None,
            'reviewCommentId': None,
            'at': '2026-11-02T13:00:00.000Z',
            'verdicts': [],
        },
        'outcome': {
            'taskId': PROOF_TASK_ID,
            'reference': PROOF_TASK_REFERENCE,
            'outcome': 'done',
            'reason': 'Proof-only qualification passed at %s; receipt %s was '
                      'accepted by retained reviewer %s'
                      % (PROOF_COMMIT, exec_run_id, REVIEWER_RUN_ID),
            'pr': None,
            'review': {'status': 'ready_to_merge',
                       'summary': 'accepted the r18 receipt',
                       'commentId': None, 'reviewId': None},
        },
    }
    obj.update(over)
    return obj


def r544_text(**over):
    return json.dumps(r544_fixture(**over), indent=2, sort_keys=True) + '\n'


def t_544_old_539_receipt_is_not_prerequisite(tmp):
    # Failing-first control: the OLD bare green-#539 receipt (the entire old
    # machine contract) is not the real prerequisite. The old contract asked
    # only for task/status/40-hex-head/non-empty-selectors; #539 is superseded
    # with zero execution, so such a receipt could never be legitimate
    # qualification and the new contract refuses it on shape alone.
    old = json.dumps({'task': 'effigy#539', 'status': 'green',
                      'head': 'a' * 40,
                      'selectors': ['proof:some:selector']}) + '\n'
    p = tmpfile(tmp, '539-green-receipt.json', old)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(old)),
                   'top-level shape')


def t_544_missing(tmp):
    expect_refused(lambda: verify_544_proof_receipt(tmp / 'none.json',
                                                    sha(r544_text())))


def t_544_hash_drift(tmp):
    p = tmpfile(tmp, '544-proof-receipt.json', r544_text())
    expect_refused(lambda: verify_544_proof_receipt(p, 'c' * 64), 'differs')


def t_544_tampered_body(tmp):
    # A body edited after the GO quoted its SHA refuses on the quoted hash.
    text = r544_text()
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    mutated = text.replace('"status": "passed"', '"status": "passed!"', 1)
    p.write_text(mutated)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'differs')


def t_544_not_json(tmp):
    p = tmpfile(tmp, '544-proof-receipt.json', 'not json at all')
    expect_refused(lambda: verify_544_proof_receipt(p, sha('not json at all')),
                   'JSON')


def t_544_truncated(tmp):
    text = r544_text()
    cut = text[:len(text) // 2]
    p = tmpfile(tmp, '544-proof-receipt.json', cut)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(cut)), 'JSON')


def t_544_oversize(tmp):
    text = r544_text(
        execution=r544_execution_record(evidence='Q' * (PREREQUISITE_MAX_BYTES + 1)))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'exceeds %d bytes' % PREREQUISITE_MAX_BYTES)


def t_544_mode(tmp):
    text = r544_text()
    p = tmpfile(tmp, '544-proof-receipt.json', text, mode=0o644)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'mode/owner/type')


def t_544_not_object(tmp):
    text = json.dumps([r544_fixture()])
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'JSON object')


def t_544_unknown_top_level_key(tmp):
    text = r544_text(extra='field')
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'top-level shape')


def t_544_wrong_schema(tmp):
    text = r544_text(schema='effigy.qualification.acow-ordinary-compose.r1')
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'schema mismatch')


def t_544_wrong_task_reference(tmp):
    text = r544_text(task={'reference': 'effigy#539', 'id': PROOF_TASK_ID})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'actual prerequisite task')


def t_544_wrong_task_id(tmp):
    text = r544_text(task={'reference': PROOF_TASK_REFERENCE, 'id': 'e' * 8})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'task id')


def t_544_wrong_commit(tmp):
    text = r544_text(proofOnly={'commit': 'f' * 40})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'pinned r18 merge')


def t_544_wrong_selector(tmp):
    text = r544_text(proofExecution={'selector': 'python3 other.py',
                                     'inputs': list(PROOF_INPUTS),
                                     'role': 'author'})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'pinned r18 selector')


def t_544_wrong_inputs(tmp):
    for i, mutate in enumerate((
            lambda v: ['sha256:' + '0' * 64] + v[1:],
            lambda v: v[:1] + ['sha256:' + '1' * 64] + v[2:],
            lambda v: v[:2] + ['sha256:' + '2' * 64],
            lambda v: v[:3] + ['sha256:' + '3' * 64],
            lambda v: [v[1], v[0], v[2]],
    )):
        inputs = mutate(list(PROOF_INPUTS))
        text = r544_text(proofExecution={'selector': PROOF_SELECTOR,
                                         'inputs': inputs, 'role': 'author'})
        p = tmpfile(tmp, '544-inputs-%d.json' % i, text)
        expect_refused(lambda p=p, text=text: verify_544_proof_receipt(p, sha(text)),
                       'pinned r18 input hashes')


def t_544_candidate_drift(tmp):
    # The binary input IS the frozen candidate; any other binary identity is
    # a mismatching candidate and refuses.
    inputs = list(PROOF_INPUTS)
    inputs[2] = 'sha256:' + '9' * 64
    text = r544_text(proofExecution={'selector': PROOF_SELECTOR,
                                     'inputs': inputs, 'role': 'author'})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'pinned r18 input hashes')


def t_544_not_author_proof(tmp):
    text = r544_text(proofExecution={'selector': PROOF_SELECTOR,
                                     'inputs': list(PROOF_INPUTS),
                                     'role': 'reviewer'})
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'author-side proof')


def t_544_no_execution(tmp):
    text = r544_text(execution=None)
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'JSON object')


def t_544_source_only(tmp):
    # A proofOnly source pin with NO settled execution record is not
    # qualification: the result block alone never carries the pass.
    fixture = r544_fixture()
    del fixture['execution']
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'top-level shape')


def t_544_execution_failed(tmp):
    text = r544_text(execution=r544_execution_record(status='failed', exitCode=1))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'settled pass')


def t_544_execution_pending(tmp):
    text = r544_text(execution=r544_execution_record(status='running',
                                                     exitCode=None,
                                                     settledAt=None))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'settled pass')


def t_544_execution_unknown_status(tmp):
    text = r544_text(execution=r544_execution_record(status='unknown'))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'settled pass')


def t_544_execution_refused_before_selector(tmp):
    text = r544_text(execution=r544_execution_record(
        status='unknown', refusedBeforeSelector=True, exitCode=None,
        settledAt=None))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'refused before its selector')


def t_544_execution_historical(tmp):
    text = r544_text(execution=r544_execution_record(historical=True))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'historical')


def t_544_execution_exit_nonzero(tmp):
    text = r544_text(execution=r544_execution_record(exitCode=2))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'exit code')


def t_544_execution_exit_missing_or_null(tmp):
    for i, over in enumerate(({}, {'exitCode': None})):
        execution = r544_execution_record()
        execution.pop('exitCode', None)
        execution.update(over)
        text = r544_text(execution=execution)
        p = tmpfile(tmp, '544-exit-%d.json' % i, text)
        expect_refused(lambda p=p, text=text: verify_544_proof_receipt(p, sha(text)),
                       'exit code')


def t_544_execution_head_mismatch(tmp):
    text = r544_text(execution=r544_execution_record(head='f' * 40))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'different head')


def t_544_execution_selector_mismatch(tmp):
    text = r544_text(execution=r544_execution_record(selector='python3 other.py'))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'selector differs from the pinned')


def t_544_execution_inputs_mismatch(tmp):
    inputs = list(PROOF_INPUTS)
    inputs[0] = 'sha256:' + '4' * 64
    text = r544_text(execution=r544_execution_record(inputs=inputs))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'inputs differ from the pinned')


def t_544_execution_unsettled(tmp):
    text = r544_text(execution=r544_execution_record(settledAt=''))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'settledAt')


def t_544_execution_bad_run_id(tmp):
    for i, run in enumerate(('', 'not-a-uuid', 'ZZZZZZZZ-0000-4c11-8e22-333333333333')):
        text = r544_text(execution=r544_execution_record(runId=run))
        p = tmpfile(tmp, '544-run-%d.json' % i, text)
        expect_refused(lambda p=p, text=text: verify_544_proof_receipt(p, sha(text)),
                       'runId')


def t_544_execution_unknown_field(tmp):
    text = r544_text(execution=r544_execution_record(surprise='x'))
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'faithful Queue proofExecutions record')


def t_544_result_missing(tmp):
    fixture = r544_fixture()
    del fixture['result']
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'top-level shape')


def t_544_result_failed(tmp):
    fixture = r544_fixture()
    fixture['result'] = dict(fixture['result'], status='failed')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'not passed')


def t_544_result_binds_other_execution(tmp):
    fixture = r544_fixture()
    fixture['result'] = dict(fixture['result'],
                             executionRunId='dddddddd-0000-4f44-8155-666666666666')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'binds a different execution run')


def t_544_result_self_reviewer(tmp):
    fixture = r544_fixture()
    fixture['result'] = dict(fixture['result'], reviewerRunId=EXEC_RUN_ID)
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'its own reviewer')


def t_544_result_bad_reviewer_id(tmp):
    fixture = r544_fixture()
    fixture['result'] = dict(fixture['result'], reviewerRunId='nope')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'reviewer run id')


def t_544_result_empty_evidence(tmp):
    fixture = r544_fixture()
    fixture['result'] = dict(fixture['result'], evidence='')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'evidence missing')


def t_544_missing_reviewer(tmp):
    fixture = r544_fixture()
    del fixture['review']
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'top-level shape')


def t_544_review_of_other_run(tmp):
    fixture = r544_fixture()
    fixture['review'] = dict(fixture['review'],
                             runId='dddddddd-0000-4f44-8155-666666666666')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'not the retained reviewer run')


def t_544_review_head_mismatch(tmp):
    fixture = r544_fixture()
    fixture['review'] = dict(fixture['review'], head='f' * 40)
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'different head')


def t_544_review_changes_required(tmp):
    fixture = r544_fixture()
    fixture['review'] = dict(fixture['review'], status='changes_required')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'not accepted')


def t_544_review_with_verdicts(tmp):
    fixture = r544_fixture()
    fixture['review'] = dict(fixture['review'], verdicts=[{'id': 'x'}])
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'faithful retained Queue review')


def t_544_outcome_not_done(tmp):
    for i, state in enumerate(('cancelled', 'superseded')):
        fixture = r544_fixture()
        fixture['outcome'] = dict(fixture['outcome'], outcome=state)
        text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
        p = tmpfile(tmp, '544-outcome-%d.json' % i, text)
        expect_refused(lambda p=p, text=text: verify_544_proof_receipt(p, sha(text)),
                       'done outcome')


def t_544_outcome_other_task(tmp):
    fixture = r544_fixture()
    fixture['outcome'] = dict(fixture['outcome'], taskId='e' * 32)
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'different task')


def t_544_outcome_with_pr(tmp):
    fixture = r544_fixture()
    fixture['outcome'] = dict(fixture['outcome'],
                              pr={'url': None, 'number': 680, 'head': PROOF_COMMIT})
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)), 'carries a PR')


def t_544_outcome_review_not_accepted(tmp):
    fixture = r544_fixture()
    fixture['outcome']['review'] = dict(fixture['outcome']['review'],
                                        status='changes_required')
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'durable outcome review')


def t_544_outcome_review_ids_linked(tmp):
    fixture = r544_fixture()
    fixture['review'].update(reviewId=123, reviewCommentId=456)
    fixture['outcome']['review'].update(reviewId=123, commentId=456)
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    verify_544_proof_receipt(p, sha(text))


def t_544_outcome_review_ids_mismatch(tmp):
    for key, retained_key in (('reviewId', 'reviewId'),
                              ('commentId', 'reviewCommentId')):
        fixture = r544_fixture()
        fixture['review'].update(reviewId=123, reviewCommentId=456)
        fixture['outcome']['review'].update(reviewId=123, commentId=456)
        fixture['outcome']['review'][key] = 999
        text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
        p = tmpfile(tmp, key + '-mismatch.json', text)
        expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                       'differs from retained review')
        del fixture['review'][retained_key]
        fixture['outcome']['review'][key] = None
        text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
        p = tmpfile(tmp, key + '-missing.json', text)
        expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                       'differs from retained review')


def t_544_outcome_wrong_shape(tmp):
    fixture = r544_fixture()
    fixture['outcome'] = {'taskId': PROOF_TASK_ID, 'outcome': 'done'}
    text = json.dumps(fixture, indent=2, sort_keys=True) + '\n'
    p = tmpfile(tmp, '544-proof-receipt.json', text)
    expect_refused(lambda: verify_544_proof_receipt(p, sha(text)),
                   'queue.outcome extraction')


def t_544_valid(tmp):
    p = tmpfile(tmp, '544-proof-receipt.json', r544_text())
    rec = verify_544_proof_receipt(p, sha(r544_text()))
    assert rec['task'] == 'effigy#544' and rec['task_id'] == PROOF_TASK_ID
    assert rec['commit'] == PROOF_COMMIT and rec['selector'] == PROOF_SELECTOR
    assert rec['inputs'] == list(PROOF_INPUTS)
    assert rec['execution_run_id'] == EXEC_RUN_ID
    assert rec['reviewer_run_id'] == REVIEWER_RUN_ID
    assert rec['review_status'] == 'ready_to_merge' and rec['outcome'] == 'done'
    assert rec['sha256'] == sha(r544_text())


# --- installed identity ----------------------------------------------------

class FakeBin:
    def __init__(self, root, payload):
        self.path = root / 'fake-effigy'
        fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.write(fd, payload)
        os.close(fd)

    def __str__(self):
        return str(self.path)


def t_identity_which_none(tmp):
    expect_refused(lambda: verify_installed_identity(
        lambda: None, lambda p: 'x', '/nonexistent', lambda p: 'h', 'v'),
        'PATH')


def t_identity_version_mismatch(tmp):
    binp = FakeBin(tmp, b'fake-payload')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v1.0+local.other',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691'),
        'version')


def t_identity_marker_missing(tmp):
    binp = FakeBin(tmp, b'fake-payload')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v9.9.9+local.abcdef0',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v9.9.9+local.abcdef0'),
        'marker')


def t_identity_hash_mismatch(tmp):
    binp = FakeBin(tmp, b'not-the-candidate')
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(binp), lambda p: 'd' * 64,
        'effigy v0.14.1+local.5f46691'),
        'binary hash')


def t_identity_symlink_binary(tmp):
    real = FakeBin(tmp, b'x')
    link = tmp / 'link-effigy'
    os.symlink(real.path, link)
    expect_refused(lambda: verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(link), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691'),
        'regular file')


def t_identity_valid(tmp):
    binp = FakeBin(tmp, b'candidate-payload')
    rec = verify_installed_identity(
        lambda: '/usr/bin/true', lambda p: 'effigy v0.14.1+local.5f46691',
        str(binp), lambda p: CANDIDATE_BINARY_SHA256,
        'effigy v0.14.1+local.5f46691')
    assert rec['binary_sha256'] == CANDIDATE_BINARY_SHA256
    assert CANDIDATE_VERSION_MARKER in rec['version_line']


# --- fixture path shape ----------------------------------------------------

def t_shape_length(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    real = '/Volumes/Builds/queue/workspaces/%s/ns-%s' % (
        '0a95477a55a47e8a77ef32eee19e29ecdb90c123737506dca62d66413a2c7b74',
        common.QUEUE_SHAPE_UUID)
    assert shape['length'] == len(real) == QUEUE_SHAPE_TOTAL_LEN == 137
    assert shape['path'].endswith('/ns-' + common.QUEUE_SHAPE_UUID)


def t_shape_deterministic(tmp):
    a = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    b = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    assert a['path'] == b['path'] and len(a['hex']) == 64


def t_shape_bad_parent_length(tmp):
    expect_refused(lambda: queue_shape_components(str(tmp), PACKET_SHA),
                   'Queue QA shape')


def t_shape_foreign_prefix(tmp):
    expect_refused(lambda: queue_shape_components('/tmp/' + 'x' * 27, PACKET_SHA),
                   'scratch root')


# --- inventory parsing and judgment ----------------------------------------

def t_ps_ids(tmp):
    good = 'a' * 64 + '\n' + 'b' * 64 + '\n'
    assert exact_ps_ids(good) == sorted(['a' * 64, 'b' * 64])
    expect_refused(lambda: exact_ps_ids('zz\n'), 'malformed')
    expect_refused(lambda: exact_ps_ids('a' * 64 + '\n' + 'a' * 64 + '\n'), 'duplicate')


def t_volumes(tmp):
    assert parse_volume_names('vol-a\nvol.b-1\n') == ['vol-a', 'vol.b-1']
    expect_refused(lambda: parse_volume_names('bad name\n'), 'malformed')
    expect_refused(lambda: parse_volume_names('v\nv\n'), 'duplicate')


def t_networks(tmp):
    listed = parse_network_rows('net1\taaaaaaaaaaaa\nnet2\tbbbbbbbbbbbb\n')
    assert listed['net1'] == 'aaaaaaaaaaaa'
    expect_refused(lambda: parse_network_rows('onlyname\n'), 'ambiguous')
    expect_refused(lambda: parse_network_rows('n\ta\nn\tb\n'), 'ambiguous')


def fake_inventory(containers=(), volumes=(), networks=None):
    return {'containers': sorted(containers), 'volumes': sorted(volumes),
            'networks': networks if networks is not None else {'host': '', 'none': ''}}


def t_capture_inventory(tmp):
    calls = []

    def runner(args):
        calls.append(tuple(args))
        if args[0] == 'ps':
            return 0, 'b' * 64 + '\n' + 'a' * 64 + '\n', ''
        if args[0] == 'volume':
            return 0, 'vol-b\nvol-a\n', ''
        return 0, 'net1\t1234567890ab\nhost\t\nnone\t\n', ''

    inv = capture_inventory(runner)
    assert inv['containers'] == sorted(['a' * 64, 'b' * 64])
    assert inv['volumes'] == ['vol-a', 'vol-b']
    assert len(calls) == 3

    def bad_runner(args):
        return 1, '', 'boom'

    expect_refused(lambda: capture_inventory(bad_runner), 'read failed')


def t_mariadb_guard(tmp):
    def ok_runner(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' running\n', ''

    rec = require_mariadb_unchanged(ok_runner)
    assert rec['container'] == common.SHARED_MARIADB_CONTAINER

    def gone(args):
        return 1, '', 'no such'

    expect_refused(lambda: require_mariadb_unchanged(gone), 'MariaDB')

    def exited(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' exited\n', ''

    expect_refused(lambda: require_mariadb_unchanged(exited), 'identity or state')

    def two(args):
        return 0, common.SHARED_MARIADB_CONTAINER + ' running\nother running\n', ''

    expect_refused(lambda: require_mariadb_unchanged(two), 'ambiguous')


def t_volume_identity(tmp):
    def ok_runner(args):
        assert args[0] == 'volume' and args[1] == 'inspect'
        return 0, 'effigy-shared-mariadb-0721de3e-mysql-data /var/lib/nerdctl/volumes/1\n', ''

    mount = volume_mount_identity(ok_runner, 'effigy-shared-mariadb-0721de3e-mysql-data')
    assert mount.startswith('/var/lib/nerdctl/')

    def missing(args):
        return 1, '', 'none'

    expect_refused(lambda: volume_mount_identity(missing, 'v'), 'not inspectable')

    def odd(args):
        return 0, 'v /etc/passwd\n', ''

    expect_refused(lambda: volume_mount_identity(odd, 'v'), 'identity unusable')


def t_no_warmup_scope(tmp):
    assert_no_warmup_scope(['bridge', 'host'])
    expect_refused(lambda: assert_no_warmup_scope(
        ['bridge', 'acowtancy-dev-wt-ns-x_default']), 'already active')


def t_no_scoped_containers(tmp):
    def clean(args):
        return 0, '', ''

    assert assert_no_scoped_containers(clean, 'acowtancy-dev-wt-x') == []

    def leftover(args):
        return 0, 'c' * 64 + '\n', ''

    expect_refused(lambda: assert_no_scoped_containers(leftover, 'p'), 'remain')


def t_judge_teardown(tmp):
    base = fake_inventory(containers=['a' * 64], volumes=['keep-vol'],
                          networks={'bridge': 'x1', 'host': '', 'none': ''})
    scoped = 'acowtancy-dev-wt-ns-x'
    live_ok = fake_inventory(containers=['a' * 64],
                             volumes=['keep-vol', scoped + '-postgres-data',
                                      scoped + '-workspace-data'],
                             networks={'bridge': 'x1', 'host': '', 'none': ''})
    protected = {common.SHARED_MARIADB_VOLUME: '/var/lib/nerdctl/volumes/9'}
    preserved = judge_teardown_inventory(live_ok, base, scoped, protected)
    assert preserved == sorted([scoped + '-postgres-data', scoped + '-workspace-data'])
    # Foreign new volume refuses.
    live_foreign = fake_inventory(containers=['a' * 64], volumes=['keep-vol', 'other-vol'],
                                  networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_foreign, base, scoped, protected),
                   'not guessing')
    # Baseline container disappearance refuses.
    live_lost = fake_inventory(containers=[], volumes=['keep-vol'], networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_lost, base, scoped, protected),
                   'removed or replaced')
    # Lingering scoped network refuses.
    live_net = fake_inventory(containers=['a' * 64], volumes=['keep-vol'],
                              networks={'bridge': 'x1', 'host': '', 'none': '',
                                        scoped + '_default': 'n1'})
    expect_refused(lambda: judge_teardown_inventory(live_net, base, scoped, protected),
                   'still present')
    # Baseline volume removal refuses.
    live_vol = fake_inventory(containers=['a' * 64], volumes=[], networks=base['networks'])
    expect_refused(lambda: judge_teardown_inventory(live_vol, base, scoped, protected),
                   'volume identity removed')
    # Protected identity drift refuses through the mount_of callback.
    expect_refused(lambda: judge_teardown_inventory(
        live_ok, base, scoped, protected,
        mount_of=lambda name: '/var/lib/nerdctl/volumes/CHANGED'),
        'protected volume identity changed')


def t_scoped_map(tmp):
    ws, pg = 'd' * 64, 'e' * 64
    calls = {'inspect': 0}

    def runner(args):
        if args[0] == 'ps':
            return 0, ws + '\n' + pg + '\n', ''
        calls['inspect'] += 1
        return 0, 'workspace\n' if calls['inspect'] == 1 else 'postgres\n', ''

    services = scoped_container_map(runner, 'acowtancy-dev-wt-ns-x')
    assert services == {'workspace': ws, 'postgres': pg}

    def dup(args):
        if args[0] == 'ps':
            return 0, ws + '\n' + pg + '\n', ''
        return 0, 'workspace\n', ''

    expect_refused(lambda: scoped_container_map(dup, 'p'), 'ambiguous')


def t_container_state(tmp):
    assert container_state(lambda a: (0, 'running\n', ''), 'f' * 64) == 'running'
    expect_refused(lambda: container_state(lambda a: (0, 'zombie\n', ''), 'f' * 64),
                   'closed vocabulary')
    expect_refused(lambda: container_state(lambda a: (1, '', 'x'), 'f' * 64), 'read failed')


def t_label_length_only(tmp):
    label = 'x' * 100
    assert label_length_only(lambda a: (0, label + '\n', ''), 'g' * 64,
                             common.MOUNTS_LABEL) == 100
    multi = 'é' * 10
    assert label_length_only(lambda a: (0, multi, ''), 'g' * 64,
                             common.MOUNTS_LABEL) == 20  # utf-8 bytes
    expect_refused(lambda: label_length_only(lambda a: (1, '', ''), 'g' * 64,
                                             common.MOUNTS_LABEL), 'read failed')


# --- session identity and records ------------------------------------------

def t_session_identity(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    ident = session_identity(shape['path'])
    canonical = str(Path(shape['path']))
    want = 'wt-ns-13f69c1b-9a5e-455e-8004-af30ece9685f-' + \
        hashlib.sha256(canonical.encode()).hexdigest()[:8]
    assert ident['session_id'] == want
    assert ident['project'] == 'acowtancy-dev-' + want


def t_attach_record(tmp):
    shape = queue_shape_components(common.WARMUP_PARENT, PACKET_SHA)
    ident = session_identity(shape['path'])
    good = json.dumps({'session_id': ident['session_id'], 'root_repo': shape['path'],
                       'repos': [shape['path']], 'active': True})
    p = tmpfile(tmp, 'record.json', good)
    assert validate_attach_record(p, ident, shape['path'])['active'] is True
    bad = json.dumps({'session_id': 'wt-other', 'root_repo': shape['path'],
                      'repos': [shape['path']], 'active': True})
    p2 = tmpfile(tmp, 'record2.json', bad)
    expect_refused(lambda: validate_attach_record(p2, ident, shape['path']), 'session id')
    p3 = tmpfile(tmp, 'record3.json', '{nope')
    expect_refused(lambda: validate_attach_record(p3, ident, shape['path']), 'JSON')
    expect_refused(lambda: validate_attach_record(tmp / 'missing.json', ident,
                                                  shape['path']), 'missing')


def t_owner_marker(tmp):
    wt = tmp / 'wt'
    wt.mkdir(mode=0o700)
    payload = {'task': 'effigy#540', 'route': 'warmup', 'k': 'v'}
    write_owner_marker(wt, payload)
    assert read_owner_marker(wt, payload) == payload
    expect_refused(lambda: read_owner_marker(wt, {'task': 'other'}), 'mismatch')
    wt2 = tmp / 'wt2'
    wt2.mkdir(mode=0o700)
    expect_refused(lambda: read_owner_marker(wt2, payload), 'missing')


# --- authorized command surface --------------------------------------------

def t_authorized_commands(tmp):
    assert ('container', 'up', '--detach') in warmup.AUTHORIZED_COMMANDS
    for cmd in warmup.AUTHORIZED_COMMANDS:
        assert '--system' not in cmd, '--system must never be authorized: %r' % (cmd,)
    assert ('container', 'up', '--system') not in warmup.AUTHORIZED_COMMANDS
    assert ('worktree:container:attach',) in warmup.AUTHORIZED_COMMANDS
    assert ('bundle', 'sync') in warmup.AUTHORIZED_COMMANDS
    assert ('container', 'down') in warmup.AUTHORIZED_COMMANDS
    assert warmup.PREFLIGHT_INSPECT[:2] == ('bundle', 'inspect')
    assert '--repo' in warmup.PREFLIGHT_INSPECT
    assert '--json' in warmup.PREFLIGHT_INSPECT


def t_runner_refuses_outside_allowlist(tmp):
    class Log:
        def write(self, data):
            pass

        def flush(self):
            pass

    run = warmup.make_effigy_runner(tmp, Log(), '/nonexistent-effigy')
    expect_refused(lambda: run(['container', 'up', '--system']), 'authorized sequence')
    expect_refused(lambda: run(['exec', 'bash']), 'authorized sequence')
    # The allowlist refusal happens before any process spawn: a nonexistent
    # effigy path never gets executed for a refused command.


# --- primary route observation guards --------------------------------------

def t_observation_plan(tmp):
    plans = primary.observation_plan('1234' * 16)
    assert set(plans) == set(EXPECTED_PRIMARY_COUNTS)
    for table, args in plans.items():
        assert args[0] == 'exec' and args[1] == '1234' * 16 and args[2] == 'psql'
        assert tuple(args[3:11]) == primary.PSQL_ALLOWLIST
        assert args[11] == 'SELECT count(*) FROM %s' % table
        assert len(args) == 12
        assert 'password' not in ' '.join(args).lower()


def t_observe_counts_and_console(tmp):
    from types import SimpleNamespace
    original_run = primary.run_quiet
    original_writer = common.receipt_writer
    calls = []
    def fake_run(args, **kwargs):
        calls.append(args)
        return SimpleNamespace(returncode=0, stderr='', stdout='5\n')
    def fake_writer(name):
        path = tmp / name
        return path, os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    primary.run_quiet = fake_run
    common.receipt_writer = fake_writer
    try:
        log = common.ConsoleLog('console')
        try:
            observed = primary.observe_counts(None, 'a' * 64, log)
            log.flush()
        finally:
            log.close()
        assert observed == {table: 5 for table in EXPECTED_PRIMARY_COUNTS}
        assert len(calls) == len(EXPECTED_PRIMARY_COUNTS)
        assert '[exit 0]' in (tmp / 'console').read_text()
    finally:
        primary.run_quiet = original_run
        common.receipt_writer = original_writer


def t_compare_counts(tmp):
    observed = {t: EXPECTED_PRIMARY_COUNTS[t][0] for t in EXPECTED_PRIMARY_COUNTS}
    assert compare_counts(observed) == {}
    observed['learning.pathway'] = 6
    mm = compare_counts(observed)
    assert mm == {'learning.pathway': {'expected': 5, 'observed': 6}}


def t_pick_postgres(tmp):
    pg = '9' * 64
    other = '8' * 64

    def runner(args):
        if args[0] == 'ps':
            return 0, pg + '\n' + other + '\n', ''
        return 0, 'postgres\n' if args[-1] == pg else 'dbgate\n', ''

    assert primary.pick_primary_postgres(runner) == pg

    def none_runner(args):
        if args[0] == 'ps':
            return 0, other + '\n', ''
        return 0, 'dbgate\n', ''

    expect_refused(lambda: primary.pick_primary_postgres(none_runner), 'not up')

    def two_runner(args):
        if args[0] == 'ps':
            return 0, pg + '\n' + other + '\n', ''
        return 0, 'postgres\n', ''

    expect_refused(lambda: primary.pick_primary_postgres(two_runner), 'ambiguous')


def t_dir_guard(tmp):
    assert_packet_dir(Path(common.DIR))
    expect_refused(lambda: assert_packet_dir(tmp), 'packet directory')


# --- bundle source-identity transition -------------------------------------

TARGET_FULL = BUNDLE_TARGET_REVISION_PREFIX + 'c' * 33


def bundle_pin(revision=BUNDLE_PRE_REVISION, digest=BUNDLE_PRE_DIGEST,
               declaration=None, version=None,
               object_='cache/sources/aaaaaaaaacfwlxw6dli6apgpfy/'
                       '1e6ed65d77d6364eeaed5a745ba5c4985ae2b700dd85d7cf7f027bdf294a33fc'):
    pin = {'declaration': declaration if declaration is not None else BUNDLE_DECLARATION,
           'object': object_, 'version': version if version is not None else digest}
    if revision is not None:
        pin['revision'] = revision
    return pin


def inspect_result(hint, stale=True, source_type='git', source_path=None):
    return {'ok': True, 'schema': 'effigy.bundle.inspect.v1',
            'source': {'source_type': source_type,
                       'source_path': source_path or common.BUNDLE_SOURCE,
                       'version_hint': hint, 'stale': stale}}


def lock_file(tmp, name, pin):
    return tmpfile(tmp, name, json.dumps(
        {'version': 1, 'sources': {'bundle': pin}}) + '\n')


def t_bundle_pre_pending(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin())
    state = verify_bundle_transition_before(
        lambda: inspect_result(BUNDLE_PRE_REVISION, stale=True), p)
    assert state['mode'] == 'approved-transition-pending'
    assert state['revision'] == BUNDLE_PRE_REVISION
    assert state['digest'] == BUNDLE_PRE_DIGEST
    assert state['stale'] is True  # the approved transition ahead is exactly this staleness


def t_bundle_pre_at_target(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST))
    state = verify_bundle_transition_before(
        lambda: inspect_result(TARGET_FULL, stale=False), p)
    assert state['mode'] == 'already-at-approved-target'


def t_bundle_pre_moved(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision='f' * 40))
    expect_refused(lambda: verify_bundle_transition_before(
        lambda: inspect_result('f' * 40), p), 're-pin')


def t_bundle_pre_digest_drift(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(digest='sha256:' + 'd' * 64))
    expect_refused(lambda: verify_bundle_transition_before(
        lambda: inspect_result(BUNDLE_PRE_REVISION), p), 're-pin')


def t_bundle_hint_pin_disagreement(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin())
    expect_refused(lambda: verify_bundle_transition_before(
        lambda: inspect_result('e' * 40), p), 'disagrees')


def t_bundle_untyped_identities(tmp):
    # A named ref is not a typed revision and can never match the locked pin.
    p = lock_file(tmp, 'lock.json', bundle_pin())
    expect_refused(lambda: verify_bundle_transition_before(
        lambda: inspect_result('main'), p), 'disagrees')
    # An untyped revision inside the lock refuses at parse time.
    p2 = lock_file(tmp, 'lock2.json', bundle_pin(revision='release-2'))
    expect_refused(lambda: parse_bundle_pin(p2), '40-hex')
    # A digest-shaped version_hint is not silently interchangeable with a
    # revision: the hint must equal the pin's own typed identity.
    p3 = lock_file(tmp, 'lock3.json', bundle_pin())
    expect_refused(lambda: verify_bundle_transition_before(
        lambda: inspect_result(BUNDLE_PRE_DIGEST), p3), 'disagrees')


def t_bundle_post_ok(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST))
    state = verify_bundle_transition_after(
        lambda: inspect_result(TARGET_FULL, stale=False), p)
    assert state['revision'] == TARGET_FULL  # full resolved revision, preserved for the receipt
    assert state['digest'] == BUNDLE_TARGET_DIGEST
    assert state['mode'] == 'already-at-approved-target'


def t_bundle_post_noop(tmp):
    # already settled at the approved target before the window: the no-op
    # sync reaches the same approved target and passes
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST))
    before = verify_bundle_transition_before(lambda: inspect_result(TARGET_FULL, stale=False), p)
    after = verify_bundle_transition_after(lambda: inspect_result(TARGET_FULL, stale=False), p)
    assert before['mode'] == after['mode'] == 'already-at-approved-target'


def t_bundle_post_unapproved_newer(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest='sha256:' + '9' * 64))
    expect_refused(lambda: verify_bundle_transition_after(
        lambda: inspect_result(TARGET_FULL, stale=False), p), 're-pin')


def t_bundle_post_missing_revision(tmp):
    # a digest-only post-state is not the approved target: a missing identity
    # can never become a pass
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=None, digest=BUNDLE_TARGET_DIGEST))
    expect_refused(lambda: verify_bundle_transition_after(
        lambda: inspect_result(BUNDLE_TARGET_DIGEST, stale=False), p), 'unapproved identity')


def t_bundle_post_missing_digest(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(version='not-a-digest'))
    expect_refused(lambda: parse_bundle_pin(p), 'digest missing or untyped')


def t_bundle_post_stale(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST))
    expect_refused(lambda: verify_bundle_transition_after(
        lambda: inspect_result(TARGET_FULL, stale=True), p), 'stale')


def t_bundle_post_source_origin(tmp):
    p = lock_file(tmp, 'lock.json', bundle_pin(revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST))
    expect_refused(lambda: verify_bundle_transition_after(
        lambda: inspect_result(TARGET_FULL, stale=False,
                               source_path='git@example.com:other.git'), p),
        'source identity')
    p2 = lock_file(tmp, 'lock2.json', bundle_pin(
        revision=TARGET_FULL, digest=BUNDLE_TARGET_DIGEST,
        declaration={'type': 'git', 'url': 'git@example.com:other.git', 'ref': None}))
    expect_refused(lambda: verify_bundle_transition_after(
        lambda: inspect_result(TARGET_FULL, stale=False), p2), 'declaration')


def t_worktree_lock_before(tmp):
    lock = tmp / common.BUNDLE_LOCK_REL
    assert warmup.worktree_lock_before(tmp) == {'mode': 'absent', 'path': str(lock)}
    lock.parent.mkdir(parents=True)
    lock.write_text('{}')
    expect_refused(lambda: warmup.worktree_lock_before(tmp), 'unexpectedly exists')
    lock.unlink()
    lock.symlink_to('missing')
    expect_refused(lambda: warmup.worktree_lock_before(tmp), 'unexpectedly exists')


def t_bundle_lock_malformed(tmp):
    expect_refused(lambda: parse_bundle_pin(tmpfile(tmp, 'bad.json', '{nope')), 'JSON')
    expect_refused(lambda: parse_bundle_pin(
        tmpfile(tmp, 'v2.json', json.dumps({'version': 2, 'sources': {}}))), 'version')
    expect_refused(lambda: parse_bundle_pin(
        tmpfile(tmp, 'nobundle.json', json.dumps({'version': 1, 'sources': {}}))),
        'no bundle source pin')
    expect_refused(lambda: parse_bundle_pin(tmpfile(
        tmp, 'escape.json',
        json.dumps({'version': 1, 'sources': {'bundle': bundle_pin(object_='cache/sources/../evil')}}))),
        'escapes')
    for index, value in enumerate((None, 42)):
        pin = bundle_pin()
        pin['object'] = value
        expect_refused(lambda: parse_bundle_pin(lock_file(tmp, 'bad-object-%s.json' % index, pin)), 'object')
    expect_refused(lambda: parse_bundle_pin(tmp / 'missing-lock.json'), 'missing')


def t_bundle_source_identity(tmp):
    expect_refused(lambda: classify_bundle_state(
        inspect_result(BUNDLE_PRE_REVISION, source_path='git@example.com:x.git'),
        bundle_pin()), 'source identity')
    expect_refused(lambda: classify_bundle_state(
        inspect_result(BUNDLE_PRE_REVISION, source_type='path'), bundle_pin()),
        'source identity')
    expect_refused(lambda: classify_bundle_state({'ok': False}, bundle_pin()),
                   'did not succeed')
    expect_refused(lambda: classify_bundle_state(
        {'ok': True, 'source': {'source_type': 'git',
                                'source_path': common.BUNDLE_SOURCE,
                                'version_hint': BUNDLE_PRE_REVISION}},
        bundle_pin()), 'stale flag')


def t_manifest_include_parser(tmp):
    assert parse_manifest_includes(
        '[manifest]\nminimum_effigy_version = "0.13.0"\ninclude = [\n%s,\n]\n'
        % ',\n'.join('  "%s"' % p for p in MANIFEST_INCLUDES)) == MANIFEST_INCLUDES
    reordered = list(reversed(MANIFEST_INCLUDES))
    assert parse_manifest_includes(
        '[manifest]\ninclude = [%s]\n'
        % ', '.join('"%s"' % p for p in reordered)) == reordered
    expect_refused(lambda: parse_manifest_includes('[bundle]\nbase = 1\n'), 'section')
    expect_refused(lambda: parse_manifest_includes('[manifest]\nother = 1\n'), 'include array')
    expect_refused(lambda: parse_manifest_includes(
        '[manifest]\ninclude = [ # note\n "infra/x.toml"]\n'), 'unreadable')
    expect_refused(lambda: parse_manifest_includes('[manifest]\ninclude = [""]\n'), 'unreadable')


# --- consumer pins: selected current head (fake git backend) ----------------

FAKE_HEAD = '4' * 40
FAKE_TREE = '5' * 40


def manifest_toml_text(includes=None):
    inc = MANIFEST_INCLUDES if includes is None else includes
    return '[manifest]\nminimum_effigy_version = "0.13.0"\ninclude = [\n%s,\n]\n' \
        % ',\n'.join('  "%s"' % p for p in inc)


def git_fake(responses):
    def git_fn(args):
        key = ' '.join(args)
        if key not in responses:
            raise AssertionError('unexpected git call: ' + key)
        rc, out = responses[key]
        return rc, out, ''
    return git_fn


def pins_responses(selected=None, tree=None, blobs=None, status='', ancestor=0,
                   anchor_tree=None, head_rc=0, show=None, show_rc=0):
    selected = selected or FAKE_HEAD
    tree = tree or FAKE_TREE
    responses = {
        'rev-parse ' + common.ACOW_PIN_COMMIT + '^{' + 'commit}':
            (0, common.ACOW_PIN_COMMIT + '\n'),
        'rev-parse ' + common.ACOW_PIN_COMMIT + '^{' + 'tree}':
            (0, (anchor_tree or common.ACOW_PIN_TREE) + '\n'),
        'rev-parse HEAD': (head_rc, '' if head_rc else selected + '\n'),
        'rev-parse HEAD^{' + 'tree}': (0, tree + '\n'),
        'merge-base --is-ancestor %s HEAD' % common.ACOW_PIN_COMMIT: (ancestor, ''),
        'status --porcelain=v1': (0, status),
        'show HEAD:effigy.toml':
            (show_rc, '' if show_rc else (show if show is not None else manifest_toml_text())),
    }
    for rel, blob in (blobs if blobs is not None else RELEVANT_INPUT_BLOBS).items():
        responses['rev-parse HEAD:%s' % rel] = (0, blob + '\n')
    return responses


def t_pins_valid_current_head(tmp):
    rec = verify_consumer_pins(git_fake(pins_responses()), primary_path=tmp / 'fake-primary')
    assert rec['selected_commit'] == FAKE_HEAD
    assert rec['selected_commit'] != common.ACOW_PIN_COMMIT  # the CURRENT head, not the anchor
    assert rec['pin_commit'] == common.ACOW_PIN_COMMIT
    assert rec['pin_tree'] == common.ACOW_PIN_TREE
    assert sorted(rec['relevant_input_blobs']) == sorted(RELEVANT_INPUT_BLOBS)
    assert rec['manifest_includes'] == MANIFEST_INCLUDES
    assert rec['primary_clean'] is True


def t_pins_worktree_add_uses_selected_head(tmp):
    worktree = tmp / 'wt'
    record = {'packet_sha256': 'a' * 64,
              'consumer_pins': {'selected_commit': FAKE_HEAD, 'selected_tree': FAKE_TREE}}
    calls = []

    def run_git(args, timeout=120, cwd=None):
        calls.append((tuple(args), cwd))
        if args[:3] == ['worktree', 'add', '--detach']:
            assert args[3] == str(worktree)
            assert args[4] == FAKE_HEAD, 'worktree add must use the SELECTED current head'
            worktree.mkdir(parents=True)
            class R:
                returncode = 0
                stdout = ''
                stderr = ''
            return R()
        if args == ['rev-parse', 'HEAD']:
            assert cwd == str(worktree)
            class R:
                returncode = 0
                stdout = FAKE_HEAD + '\n'
                stderr = ''
            return R()
        raise AssertionError('unexpected git effect: %r' % (args,))

    state = {}
    fake_read = git_fake(pins_responses())
    def reader(args, **kwargs):
        rc, out, err = fake_read(args)
        return type('Result', (), {'returncode': rc, 'stdout': out, 'stderr': err})()
    original = warmup.primary_git_runner
    warmup.primary_git_runner = lambda: reader
    try:
        wt, ident = warmup.create_warmup_worktree(
            record, {'path': str(worktree)}, FakeConsole(), state, run_git=run_git)
    finally:
        warmup.primary_git_runner = original
    assert calls[0][0][-1] == FAKE_HEAD  # the SAME selected head in worktree add
    assert record['activation']['worktree_head'] == FAKE_HEAD  # and in the receipt identities
    assert record['consumer_pins_reverified']['selected_commit'] == FAKE_HEAD
    marker = json.loads((worktree / common.WORKTREE_MARKER).read_text())
    assert marker['selected_commit'] == FAKE_HEAD


def t_pins_worktree_add_refuses_moved_head(tmp):
    record = {'packet_sha256': 'a' * 64,
              'consumer_pins': {'selected_commit': '6' * 40, 'selected_tree': FAKE_TREE}}

    def run_git(args, timeout=120, cwd=None):
        raise AssertionError('no effect may happen on a moved head')

    expect_refused(lambda: warmup.create_warmup_worktree(
        record, {'path': str(tmp / 'wt2')}, FakeConsole(), {},
        run_git=run_git, git_fn=git_fake(pins_responses())),
        'moved between preflight')


def t_pins_head_unreadable(tmp):
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(head_rc=1)), primary_path=tmp / 'p'), 'head unreadable')


def t_pins_anchor_tree_drift(tmp):
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(anchor_tree='e' * 40)), primary_path=tmp / 'p'),
        'anchor tree drifted')


def t_pins_dirty(tmp):
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(status=' M apps/x\n')), primary_path=tmp / 'p'), 'dirty')


def t_pins_not_ancestor(tmp):
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(ancestor=1)), primary_path=tmp / 'p'), 'replaced')


def t_pins_relevant_input_drift(tmp):
    blobs = dict(RELEVANT_INPUT_BLOBS)
    blobs['infra/tasks.toml'] = 'd' * 40
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(blobs=blobs)), primary_path=tmp / 'p'),
        'infra/tasks.toml')
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(blobs=blobs)), primary_path=tmp / 'p'),
        'expected')


def t_pins_relevant_input_missing(tmp):
    r = pins_responses()
    r['rev-parse HEAD:infra/bootstrap.toml'] = (1, '')
    expect_refused(lambda: verify_consumer_pins(
        git_fake(r), primary_path=tmp / 'p'), 'missing at the selected head')


def t_pins_new_include_shape(tmp):
    r = pins_responses(show=manifest_toml_text(MANIFEST_INCLUDES + ['infra/extra.toml']))
    expect_refused(lambda: verify_consumer_pins(
        git_fake(r), primary_path=tmp / 'p'), 'reviewed re-pin')


def t_pins_include_reordered(tmp):
    # include order is significant (later includes can override earlier ones)
    r = pins_responses(show=manifest_toml_text(list(reversed(MANIFEST_INCLUDES))))
    expect_refused(lambda: verify_consumer_pins(
        git_fake(r), primary_path=tmp / 'p'), 'reviewed re-pin')


def t_pins_include_unreadable(tmp):
    expect_refused(lambda: verify_consumer_pins(
        git_fake(pins_responses(show_rc=1)), primary_path=tmp / 'p'), 'unreadable')


def t_pins_linked_worktree(tmp):
    fake_primary = tmp / 'linked'
    fake_primary.mkdir()
    (fake_primary / '.git').write_text('gitdir: /elsewhere\n')
    expect_refused(lambda: verify_consumer_pins(git_fake(pins_responses()),
                                                primary_path=fake_primary),
                   'linked worktree')


# --- owned-dir hygiene ------------------------------------------------------

def t_owned_dir(tmp):
    d = tmp / 'ok'
    d.mkdir(mode=0o700)
    assert ensure_owned_dir(d).is_dir()
    loose = tmp / 'loose'
    loose.mkdir(mode=0o755)
    expect_refused(lambda: ensure_owned_dir(loose), 'private')
    filey = tmpfile(tmp, 'afile', 'x')
    expect_refused(lambda: ensure_owned_dir(filey), 'real directory')
    link = tmp / 'linkdir'
    os.symlink(d, link)
    expect_refused(lambda: ensure_owned_dir(link), 'symlink')


def main():
    global PACKET_SHA
    PACKET_SHA = verify_packet_bundle()['packet_sha256']
    common.SCRATCH.mkdir(parents=True, exist_ok=True, mode=0o700)
    root = common.boundaries_root()
    cases = [(name, fn) for name, fn in sorted(globals().items())
             if name.startswith('t_') and callable(fn)]
    try:
        for name, fn in cases:
            sub = root / name
            sub.mkdir(mode=0o700)
            check(name[len('t_'):], lambda fn=fn, s=sub: fn(s))
    finally:
        for path in sorted(root.rglob('*'), reverse=True):
            try:
                if path.is_symlink() or path.is_file():
                    path.unlink()
                elif path.is_dir():
                    path.rmdir()
            except OSError:
                pass
        try:
            root.rmdir()
        except OSError:
            pass
    print('\n%d passed, %d failed' % (len(PASS), len(FAIL)))
    if FAIL:
        for row in FAIL:
            print('FAILED: ' + row)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
