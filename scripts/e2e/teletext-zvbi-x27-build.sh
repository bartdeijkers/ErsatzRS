#!/bin/sh
# Manual Linux test-decoder recipe; no production dependency or client gate.
# Run in the already available image, offline, with fresh /out and read-only input:
# docker run --rm --pull=never --network none --read-only --tmpfs /tmp:exec,size=1536m \
#   -v "$PROOF_INPUT:/proof:ro" -v "$PROOF_OUTPUT:/out" \
#   --entrypoint /bin/sh \
#   sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e \
#   /proof/scripts/teletext-zvbi-x27-build.sh
# The image identifier is the requested outer invocation, not self-attestation.
set -eu
[ -d /out ] && [ ! -L /out ] || { echo 'Expected an existing /out directory' >&2; exit 1; }
[ -z "$(find /out -mindepth 1 -maxdepth 1 -print -quit)" ] || {
    echo 'Refusing nonempty /out' >&2; exit 1;
}
for directory in /tmp/build /tmp/original /tmp/corrected /tmp/phase9-cache; do
    [ ! -e "$directory" ] || { echo 'Expected fresh container temporary directories' >&2; exit 1; }
done
mkdir /tmp/build /tmp/phase9-cache
export XDG_CACHE_HOME=/tmp/phase9-cache
python3 - <<'PREPARE'
import hashlib
import shutil
import tarfile
from pathlib import Path

archive = Path('/proof/source.tar.gz')
assert archive.is_file() and archive.stat().st_size == 1267984, 'Archive size mismatch'
raw = archive.read_bytes()
assert hashlib.sha256(raw).hexdigest() == '4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c', 'Archive hash mismatch'
with tarfile.open(archive, 'r:gz') as source:
    assert len(source.getmembers()) == 267, 'Archive member count mismatch'
    source.extractall('/tmp/build', filter='data')
original_source = Path('/tmp/build/zvbi-3785481f51f41a49e28f2b7f6fd5bd9187d24ae1')
assert hashlib.sha256((original_source / 'COPYING.md').read_bytes()).hexdigest() == 'ae5ad395d972abe37981d782637940665462028daa67335ae4fb462592e2fba9'
shutil.copyfile(archive, '/out/source.tar.gz')
shutil.copytree(original_source, '/out/source')
for variant in ('original', 'corrected'):
    shutil.copytree(original_source, '/tmp/' + variant)
packet = Path('/tmp/corrected/src/packet.c')
old = packet.read_bytes()
assert hashlib.sha256(old).hexdigest() == 'e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18'
# EN 300 706 Table 15: magazine bits 12..14, page units 7..10,
# page tens 15..18 and subcode mask 3..18 (one-based bit numbers).
start = old.index(b'case 4:', old.index(b'parse_27'))
start = old.index(b'case 5:', start)
start = old.index(b'cvtp->data.unknown.link[designation * 6 + i].pgno', start)
end = old.index(b'if(0)', start)
region = old[start:end]
corrected = region.replace(b't1 >> 12', b't1 >> 11').replace(
    b't1 >> 11) & 0x0F0', b't1 >> 10) & 0x0F0').replace(
    b't1 >> 7', b't1 >> 6').replace(b't2 >> 3', b't2 >> 2')
new = old[:start] + corrected + old[end:]
assert sum(a != b for a, b in zip(old.splitlines(), new.splitlines())) == 3
assert hashlib.sha256(new).hexdigest() == '9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd'
packet.write_bytes(new)
PREPARE
for tool in python3 gcc make autoconf automake libtoolize gettextize pkg-config; do
    command -v "$tool" >/dev/null
    "$tool" --version >> /out/tool-versions.log 2>&1
done
for variant in original corrected; do
    cd "/tmp/$variant"
    mkdir "/out/$variant"
    cp src/packet.c COPYING.md "/out/$variant/"
    NOCONFIGURE=1 ./autogen.sh > "/out/$variant/bootstrap.log" 2>&1
    LIBS=-lm CPPFLAGS="${CPPFLAGS-} -D_GNU_SOURCE" ./configure \
        --enable-shared --disable-static --without-x --disable-v4l \
        --disable-dvb --disable-bktr --disable-proxy --disable-nls \
        --without-doxygen > "/out/$variant/configure.log" 2>&1
    cp config.log "/out/$variant/config-details.log"
    make -C src -j2 > "/out/$variant/build.log" 2>&1
    cp -a src/.libs/libzvbi.so* "/out/$variant/"
done
python3 - <<'RECEIPT'
import hashlib
import json
import os
from pathlib import Path

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

receipt = {
    'schema': 1,
    'source': {
        'version': '0.2.42',
        'commit': '3785481f51f41a49e28f2b7f6fd5bd9187d24ae1',
        'url': 'https://codeload.github.com/zapping-vbi/zvbi/tar.gz/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1',
        'bytes': 1267984,
        'members': 267,
        'sha256': digest('/out/source.tar.gz'),
        'copying_sha256': digest('/out/source/COPYING.md'),
    },
    'requested_image': 'sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e',
    'configuration': {
        'arguments': ['--enable-shared', '--disable-static', '--without-x', '--disable-v4l',
                      '--disable-dvb', '--disable-bktr', '--disable-proxy', '--disable-nls', '--without-doxygen'],
        'LIBS': '-lm',
        'CPPFLAGS': os.environ.get('CPPFLAGS', '') + ' -D_GNU_SOURCE',
    },
    'tool_versions_sha256': digest('/out/tool-versions.log'),
    'variants': {},
}
for variant, label in [('original', 'upstream-unmodified'), ('corrected', 'test-only-x27-corrected')]:
    directory = Path('/out') / variant
    receipt['variants'][variant] = {
        'label': label,
        'packet_sha256': digest(directory / 'packet.c'),
        'library': variant + '/libzvbi.so.0.13.2',
        'library_sha256': digest(directory / 'libzvbi.so.0.13.2'),
        'configure_log_sha256': digest(directory / 'config-details.log'),
    }
Path('/out/proof-build.json').write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
RECEIPT
