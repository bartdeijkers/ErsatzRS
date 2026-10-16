#!/bin/sh
# Manual Linux test-decoder recipe; no production dependency or client gate.
# Use a fresh /out, read-only input and the existing image without any network:
# docker run --rm --pull=never --network none --read-only --tmpfs /tmp:exec,size=1536m \
#   -v "$PROOF_INPUT:/proof:ro" -v "$PROOF_OUTPUT:/out" --entrypoint /bin/sh \
#   sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e \
#   /proof/scripts/teletext-zvbi-sparse-build.sh
# /proof contains source.tar.gz and both checked-in decoder build recipes under
# scripts/. The original recipe runs unchanged before the separate third build.
# The image identifier is requested by the caller, not self-attested here.
set -eu
python3 - <<'VERIFY_RECIPE'
import hashlib
from pathlib import Path

recipe = Path('/proof/scripts/teletext-zvbi-x27-build.sh')
assert recipe.is_file() and not recipe.is_symlink(), 'Expected the frozen original recipe'
assert hashlib.sha256(recipe.read_bytes()).hexdigest() == '8d101d8dad5d4ad7172cbde9a4e7d3bd1775d3808cb1b3badd560c50722eb7e2', 'Original recipe hash mismatch'
assert not Path('/tmp/sparse').exists(), 'Expected a fresh third-variant directory'
VERIFY_RECIPE
# This retains the original/corrected outputs and proof-build.json meanings.
/bin/sh /proof/scripts/teletext-zvbi-x27-build.sh
python3 - <<'PREPARE_SPARSE'
import hashlib
import json
import shutil
from pathlib import Path


def patch_formatter(old):
    assert hashlib.sha256(old).hexdigest() == 'c0920093077fe80924e483bd6a61c76418b7c20f0418dcfc6b148b1986fae4b8', 'Original formatter hash mismatch'
    # Only the normal DRCS branch. The public-object POP link[25] stays intact.
    before = (b'function = PAGE_FUNCTION_DRCS;\n'
              b'\t\t\t\t\t\tpgno = vtp->data.lop.link[25].pgno;')
    after = before.replace(b'link[25]', b'link[27]')
    assert old.count(before) == 1, 'Normal DRCS formatter context mismatch'
    new = old.replace(before, after)
    assert old.count(b'link[25]') == 2 and new.count(b'link[25]') == 1
    assert sum(a != b for a, b in zip(old.splitlines(), new.splitlines())) == 1
    assert len(old) == len(new)
    assert hashlib.sha256(new).hexdigest() == '973860e2ea47515503f40b2fe7531cf29394be180000d6cd188334125d9e4398', 'Patched formatter hash mismatch'
    return new


base = json.loads(Path('/out/proof-build.json').read_bytes())
assert base['schema'] == 1
assert base['source']['sha256'] == '4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c'
assert base['source']['commit'] == '3785481f51f41a49e28f2b7f6fd5bd9187d24ae1'
source = Path('/out/source')
formatter = (source / 'src/teletext.c').read_bytes()
assert formatter == Path('/tmp/corrected/src/teletext.c').read_bytes()
packet = Path('/tmp/corrected/src/packet.c').read_bytes()
assert hashlib.sha256(packet).hexdigest() == '9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd'
shutil.copytree(source, '/tmp/sparse')
Path('/tmp/sparse/src/packet.c').write_bytes(packet)
Path('/tmp/sparse/src/teletext.c').write_bytes(patch_formatter(formatter))
PREPARE_SPARSE
for tool in python3 gcc make autoconf automake libtoolize gettextize pkg-config; do
    command -v "$tool" >/dev/null
    "$tool" --version >> /out/sparse-tool-versions.log 2>&1
done
cd /tmp/sparse
mkdir /out/sparse
cp src/packet.c src/teletext.c COPYING.md /out/sparse/
NOCONFIGURE=1 ./autogen.sh > /out/sparse/bootstrap.log 2>&1
LIBS=-lm CPPFLAGS="${CPPFLAGS-} -D_GNU_SOURCE" ./configure \
    --enable-shared --disable-static --without-x --disable-v4l \
    --disable-dvb --disable-bktr --disable-proxy --disable-nls \
    --without-doxygen > /out/sparse/configure.log 2>&1
cp config.log /out/sparse/config-details.log
make -C src -j2 > /out/sparse/build.log 2>&1
cp -a src/.libs/libzvbi.so* /out/sparse/
python3 - <<'SPARSE_RECEIPT'
import hashlib
import json
import os
from pathlib import Path


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


base = json.loads(Path('/out/proof-build.json').read_bytes())
configuration = {
    'arguments': ['--enable-shared', '--disable-static', '--without-x', '--disable-v4l',
                  '--disable-dvb', '--disable-bktr', '--disable-proxy', '--disable-nls', '--without-doxygen'],
    'LIBS': '-lm',
    'CPPFLAGS': os.environ.get('CPPFLAGS', '') + ' -D_GNU_SOURCE',
}
assert configuration == base['configuration'], 'Third variant configuration differs'
assert digest('/out/sparse/COPYING.md') == base['source']['copying_sha256']
source = dict(base['source'], tag='v0.2.42')
receipt = {
    'schema': 1,
    'source': source,
    'requested_image': base['requested_image'],
    'base_receipt': {'path': 'proof-build.json', 'sha256': digest('/out/proof-build.json')},
    'recipes': {
        'base': {'path': 'scripts/teletext-zvbi-x27-build.sh',
                 'sha256': digest('/proof/scripts/teletext-zvbi-x27-build.sh')},
        'sparse': {'path': 'scripts/teletext-zvbi-sparse-build.sh',
                   'sha256': digest('/proof/scripts/teletext-zvbi-sparse-build.sh')},
    },
    'original_formatter_sha256': digest('/out/source/src/teletext.c'),
    'configuration': configuration,
    'base_tool_versions_sha256': base['tool_versions_sha256'],
    'tool_versions_sha256': digest('/out/sparse-tool-versions.log'),
    'variants': {
        'sparse': {
            'label': 'test-only-x27-and-normal-drcs-corrected',
            'packet_sha256': digest('/out/sparse/packet.c'),
            'formatter_sha256': digest('/out/sparse/teletext.c'),
            'library': 'sparse/libzvbi.so.0.13.2',
            'library_sha256': digest('/out/sparse/libzvbi.so.0.13.2'),
            'configure_log_sha256': digest('/out/sparse/config-details.log'),
            'bootstrap_log_sha256': digest('/out/sparse/bootstrap.log'),
            'build_log_sha256': digest('/out/sparse/build.log'),
        },
    },
    'limitations': {
        'test_only': True,
        'production_dependency': False,
        'independent_main_columns': 41,
        'side_panels_proven': False,
        'full_client_acceptance': False,
        'full_level25_acceptance': False,
    },
}
with Path('/out/proof-sparse-build.json').open('x', encoding='utf-8') as output:
    output.write(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
SPARSE_RECEIPT
