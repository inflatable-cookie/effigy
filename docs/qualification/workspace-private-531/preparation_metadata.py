"""Derive only the explicitly bound native nextest binary paths."""
import copy
from pathlib import PurePosixPath


class Refusal(ValueError):
    pass


def derive(metadata, version, replacements):
    if version != '0.9.145' or set(metadata) != {'rust-build-meta', 'rust-binaries'}:
        raise Refusal('unsupported nextest version or schema')
    data = copy.deepcopy(metadata)
    try:
        build = data['rust-build-meta']
        binaries = data['rust-binaries']
        host = build['platforms']['host']['platform']['triple']
        if host != 'aarch64-unknown-linux-gnu':
            raise Refusal('metadata is not native Linux aarch64')
        if set(binaries) != {'effigy-kernel', 'effigy-kernel::custody'}:
            raise Refusal('unexpected test executables')
        if set(replacements) != {'kernel_tests', 'custody_tests', 'kernel_fixture'}:
            raise Refusal('incomplete frozen executable mapping')
        destinations = [value[1] for value in replacements.values()]
        if len(set(destinations)) != 3 or any(not PurePosixPath(x).is_absolute() or '..' in PurePosixPath(x).parts for x in destinations):
            raise Refusal('invalid or aliased frozen paths')
        for binary_id, key in [('effigy-kernel', 'kernel_tests'), ('effigy-kernel::custody', 'custody_tests')]:
            item = binaries[binary_id]
            old, new = replacements[key]
            allowed = {'binary-id', 'binary-name', 'package-id', 'kind', 'binary-path', 'build-platform'}
            if set(item) - allowed or item['binary-id'] != binary_id or item['binary-path'] != old or item['build-platform'] != 'target':
                raise Refusal('test executable identity mismatch')
            item['binary-path'] = new
        non_tests = build['non-test-binaries']
        entries = [entry for group in non_tests.values() for entry in group]
        if (len(entries) != 1 or set(entries[0]) != {'name', 'kind', 'path', 'build-platform'}
                or entries[0]['name'] != 'kernel-custody-fixture' or entries[0]['kind'] != 'bin-exe'):
            raise Refusal('unexpected non-test executables')
        old, new = replacements['kernel_fixture']
        if entries[0]['path'] != old or entries[0]['build-platform'] != 'target':
            raise Refusal('non-test executable identity mismatch')
        entries[0]['path'] = new
    except (KeyError, TypeError, ValueError) as exc:
        if isinstance(exc, Refusal):
            raise
        raise Refusal('unsupported nextest metadata schema') from exc
    return data
