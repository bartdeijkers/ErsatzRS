#!/bin/sh
# Manual Linux test-decoder recipe; no production dependency or client gate.
# Use a fresh /out, read-only input and the existing image without any network:
# docker run --rm --pull=never --network none --read-only --tmpfs /tmp:exec,size=1536m \
#   --memory 2g --cpus 2 --pids-limit 256 --cap-drop ALL \
#   --security-opt no-new-privileges --user "$(id -u):$(id -g)" \
#   -v "$PROOF_INPUT:/proof:ro" -v "$PROOF_OUTPUT:/out" --entrypoint /bin/sh \
#   sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e \
#   /proof/scripts/teletext-zvbi-g1-build.sh
# /proof contains source.tar.gz and all three checked-in decoder build recipes under
# scripts/. The original recipe runs unchanged before the separate fourth candidate build.
# The image identifier is requested by the caller, not self-attested here.
set -eu
python3 - <<'VERIFY_RECIPE'
import hashlib
from pathlib import Path

for name, expected in (
    ('teletext-zvbi-x27-build.sh', '8d101d8dad5d4ad7172cbde9a4e7d3bd1775d3808cb1b3badd560c50722eb7e2'),
    ('teletext-zvbi-sparse-build.sh', '73cfc7f789d522197d253034910b77863d33188536ba2195df8ba4fa76052f05'),
):
    recipe = Path('/proof/scripts') / name
    assert recipe.is_file() and not recipe.is_symlink(), 'Expected a frozen recipe'
    assert hashlib.sha256(recipe.read_bytes()).hexdigest() == expected, 'Recipe hash mismatch'
assert not Path('/tmp/g1').exists(), 'Expected a fresh fourth-variant directory'
assert not Path('/tmp/g1-base-manifest.json').exists(), 'Expected a fresh preservation receipt'
VERIFY_RECIPE
# Preserve the original three builds and both prior receipts without modification.
/bin/sh /proof/scripts/teletext-zvbi-sparse-build.sh
python3 - <<'PREPARE_G1'
import hashlib
import json
import shutil
from pathlib import Path


def patch_g1(old):
    assert hashlib.sha256(old).hexdigest() == '973860e2ea47515503f40b2fe7531cf29394be180000d6cd188334125d9e4398', 'Sparse formatter hash mismatch'
    replacements = (
        (b'\tint\t\t\tinvert;\n};\n\nstatic void\nenhance_flush',
         b'\tint\t\t\tinvert;\n\tvbi_bool\t\tbase_separation_restored;\n};\n\nstatic void\nenhance_flush'),
        (b'\t\t\t\telse\n\t\t\t\t\tes->ac.unicode |= 0x20; /* contiguous */',
         b'\t\t\t\telse if (es->mac.unicode || !es->base_separation_restored)\n\t\t\t\t\tes->ac.unicode |= 0x20; /* contiguous */'),
        (b'\t\t\t\t/*\n\t\t\t\t *  Non-spacing underlined/separated display attribute\n\t\t\t\t *  cannot be cancelled by a subsequent spacing attribute.\n\t\t\t\t */',
         b'\t\t\tcase 0x1A:\t\t/* later Base separated mosaics */\n\t\t\t\t/* D5 remains active for enhanced glyphs and underline. */\n\t\t\t\tes->base_separation_restored = TRUE;\n\t\t\t\tbreak;'),
        (b'\tif (es->type != OBJECT_TYPE_PASSIVE)\n\t\tmemset (&es->mac, 0, sizeof (es->mac));\n}',
         b'\tif (es->type != OBJECT_TYPE_PASSIVE)\n\t\tmemset (&es->mac, 0, sizeof (es->mac));\n\tes->base_separation_restored = FALSE;\n}'),
        (b'\tes.invert = 0;\n\n\tif (type == OBJECT_TYPE_PASSIVE)',
         b'\tes.invert = 0;\n\tes.base_separation_restored = FALSE;\n\n\tif (type == OBJECT_TYPE_PASSIVE)'),
        (b'\t\t\t\tes.ac.underline = !!(p->data & 0x20);\n\t\t\t\tes.mac.underline = ~0;',
         b'\t\t\t\tes.ac.underline = !!(p->data & 0x20);\n\t\t\t\tes.mac.underline = ~0;\n\t\t\t\tes.base_separation_restored = FALSE;'),
    )
    new = old
    for before, after in replacements:
        assert new.count(before) == 1, 'G1 formatter context mismatch'
        new = new.replace(before, after)
    assert hashlib.sha256(new).hexdigest() == '9bd5df7a7837ac558f0e14a1d493e8022f76aeb1d3bbd812b3bbd903fef71db7', 'G1 formatter result hash mismatch'
    return new


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


base = json.loads(Path('/out/proof-sparse-build.json').read_bytes())
assert base['schema'] == 1
assert base['source']['commit'] == '3785481f51f41a49e28f2b7f6fd5bd9187d24ae1'
assert base['source']['sha256'] == '4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c'
assert base['requested_image'] == 'sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e'
assert digest(Path('/out/proof-build.json')) == base['base_receipt']['sha256']
formatter = Path('/out/sparse/teletext.c').read_bytes()
packet = Path('/out/sparse/packet.c').read_bytes()
assert hashlib.sha256(packet).hexdigest() == '9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd'
assert digest(Path('/out/source/src/teletext.c')) == 'c0920093077fe80924e483bd6a61c76418b7c20f0418dcfc6b148b1986fae4b8'
manifest = {}
for path in sorted(Path('/out').rglob('*')):
    if path.is_symlink():
        manifest[str(path.relative_to('/out'))] = {'symlink': str(path.readlink())}
    elif path.is_file():
        manifest[str(path.relative_to('/out'))] = {'sha256': digest(path)}
Path('/tmp/g1-base-manifest.json').write_text(json.dumps(manifest, sort_keys=True))
shutil.copytree('/out/source', '/tmp/g1')
Path('/tmp/g1/src/packet.c').write_bytes(packet)
Path('/tmp/g1/src/teletext.c').write_bytes(patch_g1(formatter))
PREPARE_G1
for tool in python3 gcc make autoconf automake libtoolize gettextize pkg-config; do
    command -v "$tool" >/dev/null
    "$tool" --version >> /out/g1-tool-versions.log 2>&1
done
cd /tmp/g1
mkdir /out/g1
cp src/packet.c src/teletext.c COPYING.md /out/g1/
NOCONFIGURE=1 ./autogen.sh > /out/g1/bootstrap.log 2>&1
LIBS=-lm CPPFLAGS="${CPPFLAGS-} -D_GNU_SOURCE" ./configure \
    --enable-shared --disable-static --without-x --disable-v4l \
    --disable-dvb --disable-bktr --disable-proxy --disable-nls \
    --without-doxygen > /out/g1/configure.log 2>&1
cp config.log /out/g1/config-details.log
make -C src -j2 > /out/g1/build.log 2>&1
cp -a src/.libs/libzvbi.so* /out/g1/
python3 - <<'G1_RECEIPT'
import hashlib
import json
import os
from pathlib import Path


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


manifest = json.loads(Path('/tmp/g1-base-manifest.json').read_bytes())
for name, expected in manifest.items():
    path = Path('/out') / name
    actual = ({'symlink': str(path.readlink())} if path.is_symlink()
              else {'sha256': digest(path)})
    assert actual == expected, 'Earlier build output changed: ' + name
base = json.loads(Path('/out/proof-sparse-build.json').read_bytes())
configuration = {
    'arguments': ['--enable-shared', '--disable-static', '--without-x', '--disable-v4l',
                  '--disable-dvb', '--disable-bktr', '--disable-proxy', '--disable-nls', '--without-doxygen'],
    'LIBS': '-lm',
    'CPPFLAGS': os.environ.get('CPPFLAGS', '') + ' -D_GNU_SOURCE',
}
assert configuration == base['configuration'], 'Fourth variant configuration differs'
assert digest('/out/g1/COPYING.md') == base['source']['copying_sha256']
assert digest('/out/g1/packet.c') == base['variants']['sparse']['packet_sha256']
assert digest('/out/g1/teletext.c') == '9bd5df7a7837ac558f0e14a1d493e8022f76aeb1d3bbd812b3bbd903fef71db7'
receipt = {
    'schema': 1,
    'source': base['source'],
    'requested_image': base['requested_image'],
    'base_receipt': {'path': 'proof-build.json', 'sha256': digest('/out/proof-build.json')},
    'sparse_receipt': {'path': 'proof-sparse-build.json', 'sha256': digest('/out/proof-sparse-build.json')},
    'recipes': {
        role: {'path': 'scripts/' + name, 'sha256': digest('/proof/scripts/' + name)}
        for role, name in (
            ('base', 'teletext-zvbi-x27-build.sh'),
            ('sparse', 'teletext-zvbi-sparse-build.sh'),
            ('g1', 'teletext-zvbi-g1-build.sh'),
        )
    },
    'original_formatter_sha256': base['original_formatter_sha256'],
    'sparse_formatter_sha256': base['variants']['sparse']['formatter_sha256'],
    'preserved_outputs': manifest,
    'configuration': configuration,
    'base_tool_versions_sha256': base['base_tool_versions_sha256'],
    'sparse_tool_versions_sha256': base['tool_versions_sha256'],
    'tool_versions_sha256': digest('/out/g1-tool-versions.log'),
    'variants': {'g1': {
        'label': 'test-only-x27-normal-drcs-and-g1-later-spacing-candidate',
        'packet_sha256': digest('/out/g1/packet.c'),
        'formatter_sha256': digest('/out/g1/teletext.c'),
        'library': 'g1/libzvbi.so.0.13.2',
        'library_sha256': digest('/out/g1/libzvbi.so.0.13.2'),
        'configure_log_sha256': digest('/out/g1/config-details.log'),
        'bootstrap_log_sha256': digest('/out/g1/bootstrap.log'),
        'build_log_sha256': digest('/out/g1/build.log'),
    }},
    'limitations': {
        'test_only': True, 'production_dependency': False,
        'independent_main_columns': 41, 'side_panels_proven': False,
        'full_client_acceptance': False, 'full_level25_acceptance': False,
        'g1_correction_conformance_proven': False,
        'prior_object_glyph_provenance_proven': False,
        'required_proof': ['unchanged four G1 vectors', 'later enhanced G1 ignores Base separation',
                          'D5 true survives contiguous spacing', 'explicit D5 false rearms cancellation',
                          'Local and offset Active scope', 'Adaptive extent and Passive private state',
                          'D5 false still clears inherited alphabetic underline', 'row and invocation reset',
                          'prior-object enhanced G1 provenance'],
    },
}
with Path('/out/proof-g1-build.json').open('x', encoding='utf-8') as output:
    output.write(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
G1_RECEIPT
