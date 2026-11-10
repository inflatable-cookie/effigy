"""One launch request for C2–C6; unchanged stage windows, claims and receipts."""
import os
import time

# Imported by the proof entry only after its normal module initialisation.
import macos_stage_executor as executor
import macos_stage_proof as proof

STAGES = ('C2', 'C3', 'C4', 'C5', 'C6')
SCHEMA = 'macos-colima-chain-window-v1'


def recipe(bindings, options):
    if set(options) != {'activation_id', 'evidence_root', 'prior_receipt'}:
        raise executor.Refusal('unsupported chain options')
    ident = options['activation_id']
    if not isinstance(ident, str) or len(ident) > 60:
        raise executor.Refusal('chain activation id must leave room for stage suffixes')
    manifest = executor.load_manifest()
    first = proof.window_recipe('C2', bindings, dict(options, activation_id=ident + '-c2',
                                duration_seconds=executor.stage_of(manifest, 'C2')['seconds']))
    if os.path.lexists(bindings['colima_input_dir']):
        raise executor.Refusal('C4 input destination is not absent')
    budgets = {stage: executor.stage_of(manifest, stage)['seconds'] for stage in STAGES}
    for stage in STAGES:
        if executor.resolve_stage(stage, bindings, manifest)['static_admission']:
            raise executor.Refusal('chain stage unadmitted: ' + stage)
    return dict(schema=SCHEMA, activation_id=ident, first=first, budgets=budgets,
                duration_seconds=sum(budgets.values()))


def execute(request, request_sha):
    if (not isinstance(request, dict) or set(request) != {
            'schema', 'activation_id', 'first', 'budgets', 'duration_seconds'}
            or not isinstance(request['first'], dict)):
        raise executor.Refusal('unsupported chain request')
    first = request['first']
    options = dict(activation_id=request['activation_id'], evidence_root=first['evidence_root'],
                   prior_receipt=first['prior_receipt'])
    if request != recipe(first['bindings'], options) or any(v is None for v in first['roots'].values()):
        raise executor.Refusal('chain request, inputs or root identity differs')
    root = first['evidence_root']
    name = 'chain-' + request['activation_id']
    if os.path.lexists(root + '/' + name + '.json'):
        raise executor.Refusal('chain destination already exists')
    pins = executor.Pins()
    summary = dict(schema='macos-colima-chain-receipt-v1', request_sha256=request_sha,
                   closure='unknown', receipts=[])
    claimed = False
    try:
        for path, identity in first['roots'].items():
            pins.hold(path)
            if list(pins.items[path]) != identity:
                raise executor.Refusal('chain root identity changed')
        executor.claim_activation(name, request_sha)
        claimed = True
        deadline = time.monotonic() + request['duration_seconds']
        prior = first['prior_receipt']
        for stage in STAGES:
            pins.recheck()
            options = dict(activation_id=request['activation_id'] + '-' + stage.lower(),
                           duration_seconds=request['budgets'][stage], evidence_root=root, prior_receipt=prior)
            window = proof.window_recipe(stage, first['bindings'], options)
            # Do not silently rebind a changed source or machine input between stages.
            for key in ('inputs', 'inputs_sha256', 'manifest_sha256', 'source_sha256'):
                if window[key] != first[key]:
                    raise executor.Refusal('chain input drift: ' + key)
            receipt = proof.execute_window(window, executor.canon_sha(window), executor.load_manifest(),
                                           first['inputs_sha256'], outer_deadline=deadline)
            path = root + '/' + stage + '-' + options['activation_id'] + '/stage-receipt.json'
            prior = dict(stage=stage, path=path, sha256=executor.sha_file(path))
            summary['receipts'].append(prior)
            if receipt['closure'] != 'closed':
                summary['closure'] = receipt['closure']
                break
            proof.checked_prior(prior, first['bindings'], executor.load_manifest(), stage)
        else:
            summary['closure'] = 'closed'
    except BaseException as exc:
        summary['error'] = str(exc)
        raise
    finally:
        pins.close()
        # A failed claim must not overwrite an earlier occurrence's summary.
        if claimed and not os.path.lexists(root + '/' + name + '.json'):
            executor.write_json_new(root, name + '.json', summary)
    return summary
