#!/usr/bin/env python3
"""Replay the fixed X27/MOT oracle with one receipted Linux test decoder.

Run four separate processes (original/corrected x root/nested), each with
LD_LIBRARY_PATH=/out/<variant>. This records an upstream defect and a narrowly
corrected independent C test decoder, not full-client or publication acceptance.
Fixtures remain producer-owned captured JSON; this script never changes them.
"""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import runpy
import sys

SOURCE = {
    'version': '0.2.42',
    'commit': '3785481f51f41a49e28f2b7f6fd5bd9187d24ae1',
    'url': 'https://codeload.github.com/zapping-vbi/zvbi/tar.gz/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1',
    'bytes': 1267984,
    'members': 267,
    'sha256': '4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c',
    'copying_sha256': 'ae5ad395d972abe37981d782637940665462028daa67335ae4fb462592e2fba9',
}
PARSERS = {
    'original': 'e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18',
    'corrected': '9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd',
}
FIXTURES = {
    'root': '2759c8a3b45f0df2fe6beab77a39b5af432369b4f89ebdf2b93a72add762c9cd',
    'nested': '98d8416f14be48c760473a39e73f48b5934fcb91e8478a3ea6478fd7b7262db2',
}
HELPERS = {
    'teletext-object-decoder.py': 'd8d7001f6df374614b95c59a413b07f60af4038089bed1811c12abbe2cf6c6f1',
    'teletext-resource-decoder.py': '615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d',
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded(path, maximum):
    require(path.is_file() and path.stat().st_size <= maximum, 'Invalid bounded file: ' + str(path))
    with path.open('rb') as stream:
        data = stream.read(maximum + 1)
    require(len(data) <= maximum, 'File grew past limit')
    return data


def digest(path, maximum=16 * 1024 * 1024):
    return hashlib.sha256(bounded(path, maximum)).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate JSON member')
        result[key] = value
    return result


def main():
    require(len(sys.argv) == 3, 'Usage: teletext-zvbi-x27-proof.py original|corrected root|nested')
    variant, fixture = sys.argv[1:]
    require(variant in PARSERS and fixture in FIXTURES, 'Unknown variant or fixture')
    output, proof = Path('/out'), Path('/proof')
    receipt_raw = bounded(output / 'proof-build.json', 65536)
    receipt = json.loads(receipt_raw, object_pairs_hook=unique_object)
    require(receipt['schema'] == 1 and receipt['source'] == SOURCE, 'Source receipt mismatch')
    require(digest(output / 'source.tar.gz') == SOURCE['sha256'], 'Source archive changed')
    require(digest(output / 'source/COPYING.md') == SOURCE['copying_sha256'], 'Source notice changed')
    require(set(receipt['variants']) == set(PARSERS), 'Variant receipt mismatch')
    for name, parser in PARSERS.items():
        entry = receipt['variants'][name]
        label = 'upstream-unmodified' if name == 'original' else 'test-only-x27-corrected'
        require(entry['label'] == label and entry['packet_sha256'] == parser, 'Parser receipt mismatch')
        require(entry['library'] == name + '/libzvbi.so.0.13.2', 'Unexpected library path')
        require(digest(output / name / 'packet.c') == parser, 'Parser changed')
        require(digest(output / name / 'COPYING.md') == SOURCE['copying_sha256'], 'Variant notice changed')
        require(digest(output / entry['library']) == entry['library_sha256'], 'Built library changed')
        require(digest(output / name / 'config-details.log') == entry['configure_log_sha256'], 'Configuration log changed')
    require(digest(output / 'tool-versions.log') == receipt['tool_versions_sha256'], 'Tool receipt changed')
    for name, wanted in HELPERS.items():
        require(digest(proof / 'scripts' / name) == wanted, 'Frozen helper changed')
    request = bounded(proof / (fixture + '.json'), 16 * 1024 * 1024)
    require(hashlib.sha256(request).hexdigest() == FIXTURES[fixture], 'Producer fixture changed')
    # The unchanged helper performs closed input/ABI/version validation before decoding.
    saved_stdin = sys.stdin
    stdout, stderr = io.StringIO(), io.StringIO()
    status = 0
    try:
        sys.stdin = io.TextIOWrapper(io.BytesIO(request), encoding='utf-8')
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                runpy.run_path(str(proof / 'scripts/teletext-object-decoder.py'), run_name='__main__')
            except SystemExit as error:
                status = error.code
    finally:
        sys.stdin = saved_stdin
    mappings = bounded(Path('/proc/self/maps'), 4 * 1024 * 1024).decode().splitlines()
    libraries = {line.split()[-1] for line in mappings if '/libzvbi.so.' in line}
    require(len(libraries) == 1, 'Expected exactly one mapped libzvbi')
    library = Path(libraries.pop())
    entry = receipt['variants'][variant]
    require(library == output / entry['library'], 'Unexpected mapped decoder path')
    loaded_hash = digest(library)
    require(loaded_hash == entry['library_sha256'], 'Mapped decoder hash mismatch')
    wanted_status = 1 if variant == 'original' and fixture == 'nested' else 0
    require(status == wanted_status, 'Unexpected native outcome: ' + stdout.getvalue() + stderr.getvalue())
    if status == 0:
        report = json.loads(stdout.getvalue(), object_pairs_hook=unique_object)
        require(report == {'cases_passed': 2, 'decoder_version': '0.2.42'} and not stderr.getvalue(), 'Success oracle mismatch')
    else:
        report = json.loads(stderr.getvalue(), object_pairs_hook=unique_object)
        require(report == {'error': 'native_foreign_mot_default_nested_selected_page: cell 1,8 differs: actual (84, 7, 0), expected (65, 1, 4)'} and not stdout.getvalue(), 'Upstream failure oracle changed')
    print(json.dumps({
        'schema': 1, 'variant': variant, 'decoder_label': entry['label'], 'fixture': fixture,
        'conformance_passed': status == 0, 'exit_code': status,
        'request_sha256': FIXTURES[fixture], 'loaded_library_sha256': loaded_hash,
        'build_receipt_sha256': hashlib.sha256(receipt_raw).hexdigest(), 'report': report,
    }, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RecursionError) as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        sys.exit(1)
