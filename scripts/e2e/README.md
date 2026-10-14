# TSDuck playback gate (E2E)

## Protected DRCS editor dependency trial

Use the same pinned QTM1.0/cac2167 binary, image, Qt6.10.2 SDK and visible X11
setup below. Compile the maintained widget driver after source changes. Its
optional `ERSATZRS_QTM_NORMAL_DRCS_BASENAME` is a flat TTI filename in `/work`,
valid with `single-page-zone-q` or `single-page-save`. Select the V4 `drcs_tti_set`
from the authenticated sidecar, verify its hash and all linked original DVB
assets, and pass that unchanged file. See the canonical
[indexed set contract](../../docs/reference/teletext-design-interchange.md#protected-drcs-editor-dependencies)
for explicit null editor slots and retained V3 singleton recipes.

The actual View/Normal DRCS/Load file action uses its owned Open dialog.
`ERSATZRS_QTM_NORMAL_DRCS_SUBPAGE` defaults to0; the fixed synthetic pixel oracle
also accepts explicit1, checking the distinct mirrored bitmap at all120 actual
Qt glyph/day-palette pixels. This option chooses the expected oracle; the editor
still selects its actual bank from authored X26. It does not alter any file or
QTM decoding. `single-page-save` opens/loads/checks/saves without adding Q.
Six captures per page retain before/after load, optional edit and actual Save As.

Ignored native selectors in `ersatzrs-web` are
`test(pinned_external_editor_drcs_companion)` (singleton),
`test(pinned_external_editor_indexed_drcs)` (real S1=0+1), and
`test(pinned_external_editor_sparse_drcs)` (only real S1=1 with null editor slot0).
Use the same FFmpeg9/ffprobe/TSDuck environment and create-new interchange
directory as the existing probe below. Do not issue a completion marker until
the actual editor exits0 and saved files/captures are checked. Existing markerV1
imports `edited.tti`; closed test markerV2 additionally requires
`saved_page_125: "edited-125.tti"`. Open/load/save page125 in the actual editor,
without another Q, before importing both actual saved pages. Protected source
sets/sidecars remain byte-exact. Native proof covers Free-row2 Q, shared preview,
separate publication, two reopens and both ordinary channel routes. Regular
`test(drcs_companion)` cases cover missing/modified dependencies, null-slot PTU
rejection, native resource absence, channel isolation and retained V2/V3 receipts.

Ordered captured units are `channel-1-drcs-units.json` and
`channel-2-drcs-units.json`. Run
`teletext-editor-drcs-decoder.py <units> day|night [indexed|sparse]`
with installed pinned libzvbi0.2.42. Omit the third argument for the original
singleton oracle. It reuses the frozen ABI helper and checks literal S1=0/1
bitmaps, palettes and all120 rendered MAIN pixels on pages124/125:0020.
Record actual library/helper/script/input hashes alongside TSDuck, exact-model
and source-AV proof. Client navigation remains a separate headed trial of the
exact served captures. MAIN41-column proof does not establish56-column panels;
attached-resource editing and full Level2.5/editor acceptance remain open in
P9A-04. The sparse ignored stream probe currently fails with the installed
FFmpeg/zvbi candidate's audited normative X27 field/normal-DRCS slot defects;
retain that negative evidence and correct native bytes. QTM loading and native
sparse preview/publication do not waive its independent/client proof. No fixture
grants package edits or automatic publication.

## QTeletextMaker headed synthetic calibration

This opt-in Linux/X11 probe uses an unchanged external editor and disposable
synthetic files. It proves one editor edit/navigation/save trial; it is not the
ErsatzRS export/import/preview/publication gate. See the canonical
[source audit and recorded identities](../../docs/reference/teletext-design-interchange.md#actual-synthetic-editsave-calibration).
The editor's ordinal subcode rewriting and unknown-record loss are deliberate
observations, not accepted host data loss. Do not point the probe at production
designs or an existing output file.

Prerequisites: the pinned QTeletextMaker checkout, a visible X11 display and a
local Linux amd64 Qt tool image. The recorded image derives from build-base
`sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e`
with `qt6-base-dev`, `fonts-dejavu-core`, `xdotool`, `x11-utils` and `imagemagick`
installed using `apt-get install --no-install-recommends`. The recorded Qt package
is 6.10.2+dfsg-7. Pin/record actual packages and image identity on replay; a moving
package repository cannot recreate the recorded image identity by assumption.
Keep dependencies and artifacts outside the Rust target directory.

```bash
qtm_src=/path/to/pinned-qteletextmaker-checkout
qtm_tools=/path/to/disposable-tool-output
qtm_work=/path/to/new-disposable-audit-workspace
qtm_image=ersatzrs-phase9-qtm-tools:local
qtm_xsocket=/path/to/visible-x11-socket-directory
test "$(git -C "$qtm_src" rev-parse HEAD)" = cac2167bac933a6160cca45a45d5d4ee95355898
test -z "$(git -C "$qtm_src" status --porcelain)"
mkdir -p "$qtm_tools" "$qtm_work"
python3 scripts/e2e/teletext-qtm-calibration.py fixture "$qtm_work/synthetic-editor-audit.tti"
docker run --rm --network none --cpus 2 --memory 1536m --tmpfs /tmp:rw,size=768m \
  --mount "type=bind,src=$qtm_src,dst=/src,readonly" \
  --mount "type=bind,src=$qtm_tools,dst=/out" "$qtm_image" sh -eu -c '
  CC=/usr/bin/cc CXX=/usr/bin/c++ cmake -S /src -B /tmp/build -DCMAKE_BUILD_TYPE=Release
  cmake --build /tmp/build --parallel 2
  cp /tmp/build/qteletextmaker /out/qteletextmaker
  sha256sum /out/qteletextmaker
  QT_QPA_PLATFORM=offscreen /out/qteletextmaker --version'
docker run --rm --network none --cpus 2 --memory 768m \
  --mount "type=bind,src=$PWD/scripts/e2e,dst=/proof,readonly" \
  --mount "type=bind,src=$qtm_tools,dst=/out" "$qtm_image" sh -eu -c '
  export PKG_CONFIG_LIBDIR=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig PKG_CONFIG_SYSROOT_DIR=/
  /usr/bin/c++ -std=c++17 -O2 -fPIC -shared /proof/teletext-qtm-widget-driver.cpp \
    -o /out/maintained-gui-driver.so $(/usr/bin/pkg-config --cflags --libs Qt6Widgets Qt6Test) -ldl
  sha256sum /out/maintained-gui-driver.so'
docker run --rm --network none --cpus 1 --memory 768m --tmpfs /tmp:rw,size=128m \
  --mount "type=bind,src=$qtm_tools,dst=/tool,readonly" \
  --mount "type=bind,src=$qtm_work,dst=/work" \
  --mount "type=bind,src=$qtm_xsocket,dst=/tmp/.X11-unix,readonly" \
  -e DISPLAY -e QT_QPA_PLATFORM=xcb -e QT_X11_NO_MITSHM=1 -e LANG=C.UTF-8 \
  -e XDG_CONFIG_HOME=/tmp/qtm-config "$qtm_image" sh -eu -c '
  install -d -m 700 /tmp/qtm-runtime
  export XDG_RUNTIME_DIR=/tmp/qtm-runtime LD_PRELOAD=/tool/maintained-gui-driver.so
  exec /tool/qteletextmaker /work/synthetic-editor-audit.tti'
python3 scripts/e2e/teletext-qtm-calibration.py verify \
  "$qtm_work/synthetic-editor-audit.tti" "$qtm_work/edited-editor-audit.tti"
```

The native compiler/pkg-config overrides are necessary when the base image has
FFmpeg cross-build defaults. Offscreen `--version` is only identity evidence.
The actual probe requires `xcb` and exposed windows; it uses QtTest input rather
than OS-wide keyboard injection. Retain and visually inspect all six captures,
including both subpages and the actual Save As dialog. Its completion requires
the saved document path and cleared modified state; the Python verifier checks
the bounded literal bytes and reports `engine_roundtrip_proven: false`.

Separately start the same binary against `/work/edited-editor-audit.tti` with the
same visible-display mounts, a read-only work mount and **without LD_PRELOAD**.
Inspect that owned window to prove reopening retains the text and selected
Level 2.5 enhancement. This does not prove every subpage enhancement or resource
function. Stop only the owned test containers after retaining evidence. Shared
X11 access is a local manual-test exception and is never a server dependency.

### Replay the pinned editor against a native export

This opt-in Linux/X11 probe connects the existing native synthetic fixture to an
unchanged external editor through a bounded file handshake. Run the test and
editor in separate terminals. `ready.json` means the original exports exist;
`editor-saved.json` records the separately observed editor completion. A successful
native flow requires the selected nextest process to exit zero and produce
`success.json`. Neither readiness nor a completion marker alone proves acceptance.
P9A-04 records the representative successful charset-zero trial and a separate
nonzero charset failure. Replays require their own final receipt.

The prepared source is QTeletextMaker 1.0 at
`cac2167bac933a6160cca45a45d5d4ee95355898`, with all 124 source blobs checked.
The current prepared executable SHA256 is
`af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a`.
The local Qt tool image remains
`sha256:6064a1cfba3673ce1530dd5ea4c98249a5540666e3feea3160f43c122fd81c01`.
Keep the source, executable, driver, build logs and their hashes in the evidence
receipt; a version check is identity evidence, while the exposed `xcb` window and
captures establish the editor interaction. The editor executable remains
unchanged. The test-only driver sends QtTest input through actual widgets and
Save As; it does not generate the saved TTI bytes.

Choose a new proof directory under the ignored workspace. The test itself creates
the directory with `create_dir` and refuses an existing destination, including
previous attempts. Keep the selected Cargo target directory unchanged.

```bash
qtm_probe_root="$PWD/tmp/phase9-qtm-native-flow/editor-run-01"
test ! -e "$qtm_probe_root"
test ! -L "$qtm_probe_root"
ERSATZRS_TELETEXT_QTM_INTERCHANGE_DIR="$qtm_probe_root" \
  ERSATZRS_TELETEXT_TEST_FFMPEG=/path/to/verified-ffmpeg-9/ffmpeg \
  FFMPEG_PATH=/path/to/verified-ffmpeg-9/ffmpeg \
  FFPROBE_PATH=/path/to/verified-ffmpeg-9/ffprobe \
  ERSATZRS_TELETEXT_TEST_TSANALYZE=/path/to/tsanalyze \
  ./run.sh test -p ersatzrs-web --test teletext_profile_preparation \
    --run-ignored ignored-only \
    -E 'test(external_editor_probe::pinned_external_editor_files_import_preview_publication_and_reopen)'
```

The test prepares the same installed synthetic addon on two channels, with
distinct profiles and day/night themes. Before capturing configuration or original
receipts, it prepares disposable playback and the existing bounded output policy.
It creates immutable original designs,
export receipts and an existing accepted output for each channel. It writes:

| File | Meaning |
| --- | --- |
| `ready.json` | Schema-1 handshake with both channel/profile/theme identities, immutable original/receipt references, exact input filenames and hashes, and the requested edit |
| `channel-1/project.ersatzrs.json` | Protected sidecar for the first original export |
| `channel-2/project.ersatzrs.json` | Protected sidecar for the second original export |
| `channel-1/<exported-file>` and `channel-2/<exported-file>` | Exact exported TTI files and protected binary assets, with lengths/hashes listed in readiness |

The requested edit is channel 1, physical row 2, zero-based column 8, code 81
(`Q`). Each channel exports pages 124 and 125 with reciprocal navigation and raw
subcode 32. Stable per-page mappings preserve these identities while the editor's
PN/SC ordinal is zero. Only the first mapped TTI is opened/edited/saved; page 125
and all other assets remain exact. Header and enhanced charset use zero in this
separate positive fixture; the prior nonzero editor loss remains open. This does
not prove editing all pages/subpages or editor resource dependency loading. Leave both channel directories and their
sidecars/assets byte-identical. Preserve their original names when returning the
edited bytes to the importer.

Once readiness exists, copy its first channel's exact `editor_input` to
`native-input.tti` in the proof root. Do not generate, pad, trim or rewrite the
copy. This script verifies the length/hash before copying, uses create-new output
and verifies the copied hash. In the second terminal, set `qtm_probe_root` to the
same new directory selected for the running test:

```bash
qtm_probe_root="$PWD/tmp/phase9-qtm-native-flow/editor-run-01"
python3 - "$qtm_probe_root" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
ready = json.loads((root / "ready.json").read_bytes())
assert ready["probe_version"] == 1
assert [channel["channel_id"] for channel in ready["channels"]] == [1, 2]
assert ready["edit"] == {"channel_id": 1, "row": 2, "column": 8, "code": 81}
channel = ready["channels"][0]
source = root / channel["editor_input"]
assert source.is_file() and not source.is_symlink()
entry = next(item for item in channel["files"] if item["name"] == source.name)
data = source.read_bytes()
assert len(data) == entry["bytes"]
assert hashlib.sha256(data).hexdigest() == entry["sha256"]
destination = root / "native-input.tti"
with destination.open("xb") as output:
    output.write(data)
assert hashlib.sha256(destination.read_bytes()).hexdigest() == entry["sha256"]
print(json.dumps({"native_input_sha256": entry["sha256"], "bytes": len(data)}))
PY
```

Select the staged tool directory containing `qteletextmaker` and the current
`native-gui-driver.so`, and the visible X11 socket directory. These are explicit
local tool paths; they are not server runtime dependencies. Verify the executable
hash against the prepared identity before launching it. Set the tool directory
to an absolute path accepted by Docker's bind mount syntax.

```bash
qtm_tools=/path/to/staged-native-editor-tools
qtm_xsocket=/path/to/visible-x11-socket-directory
qtm_image=sha256:6064a1cfba3673ce1530dd5ea4c98249a5540666e3feea3160f43c122fd81c01
sha256sum "$qtm_tools/qteletextmaker" "$qtm_tools/native-gui-driver.so"
read -r qtm_left qtm_right < <(python3 - "$qtm_probe_root" <<'PYGEOMETRY'
import json
from pathlib import Path
import sys
geometry = json.loads((Path(sys.argv[1]) / "ready.json").read_bytes())["channels"][0]["editor_geometry"]
print(geometry["left_columns"], geometry["right_columns"])
PYGEOMETRY
)
test ! -e "$qtm_probe_root/edited.tti"
test ! -L "$qtm_probe_root/edited.tti"
docker run --rm --pull=never --network none --read-only \
  --cpus 1 --memory 768m --tmpfs /tmp:rw,size=128m \
  --mount "type=bind,src=$qtm_tools,dst=/tool,readonly" \
  --mount "type=bind,src=$qtm_probe_root,dst=/work" \
  --mount "type=bind,src=$qtm_xsocket,dst=/tmp/.X11-unix,readonly" \
  -e DISPLAY -e QT_QPA_PLATFORM=xcb -e QT_X11_NO_MITSHM=1 \
  -e LANG=C.UTF-8 -e XDG_CONFIG_HOME=/tmp/qtm-config \
  -e ERSATZRS_QTM_DRIVER_MODE=single-page-zone-q \
  -e ERSATZRS_QTM_INPUT_BASENAME=native-input.tti \
  -e ERSATZRS_QTM_OUTPUT_BASENAME=edited.tti \
  -e "ERSATZRS_QTM_LEFT_PANEL_COLUMNS=$qtm_left" \
  -e "ERSATZRS_QTM_RIGHT_PANEL_COLUMNS=$qtm_right" \
  "$qtm_image" sh -eu -c '
    install -d -m 700 /tmp/qtm-runtime
    export XDG_RUNTIME_DIR=/tmp/qtm-runtime LD_PRELOAD=/tool/native-gui-driver.so
    exec /tool/qteletextmaker /work/native-input.tti'
```

The driver requires an exposed X11 window, selects the Level 2.5 radio button and
uses the original compiler-owned left/right panel widths when targeting main row
2/column 8. The recorded geometry has four left and twelve right columns. It
enters `Q`, invokes the actual Save As dialog and saves `edited.tti`. It refuses an
existing/symlink output, verifies the saved document path and cleared modified
state, captures the actual window/dialog and exits. Retain its terminal result,
completion line and these four captures, then inspect the captures:

- `edited-qtm-level25.png`
- `edited-qtm-edited.png`
- `edited-qtm-save-dialog.png`
- `edited-qtm-saved.png`

After the editor process exits zero, its actual save completes and the independent
identity/window evidence is retained, write the bounded schema-1 completion
marker. Use the actual verified executable digest, not a remembered digest or a
different staged file. The marker has exactly these fields; the probe rejects
unknown fields, wrong version/identity, unsupported source pins and saved names:

```json
{
  "probe_version": 1,
  "editor_name": "QTeletextMaker",
  "editor_version": "1.0",
  "editor_source_commit": "cac2167bac933a6160cca45a45d5d4ee95355898",
  "editor_binary_sha256": "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a",
  "saved_file": "edited.tti"
}
```

Create the marker only after those observed steps. It does not authenticate the
edited contents or the executable by itself:

```bash
python3 - "$qtm_probe_root" "$qtm_tools/qteletextmaker" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import sys

root = Path(sys.argv[1])
executable = Path(sys.argv[2])
saved = root / "edited.tti"
assert saved.is_file() and not saved.is_symlink() and 0 < saved.stat().st_size <= 65536
binary_hash = hashlib.sha256(executable.read_bytes()).hexdigest()
assert binary_hash == "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"
marker = {"probe_version": 1, "editor_name": "QTeletextMaker", "editor_version": "1.0",
          "editor_source_commit": "cac2167bac933a6160cca45a45d5d4ee95355898",
          "editor_binary_sha256": binary_hash, "saved_file": "edited.tti"}
temporary = root / "editor-saved.ready"
with temporary.open("x", encoding="utf-8") as output:
    json.dump(marker, output, indent=2)
    output.write("\n")
os.link(temporary, root / "editor-saved.json")
temporary.unlink()
PY
```

The native test waits at most 600 seconds for the completion marker. It then
checks the exact protected sidecar and every original file on both channels,
replaces only the first mapped TTI file with the editor's bytes and calls the
production native importer. Receipt/original/head validation and edit boundaries
must succeed before the concept is saved. The test checks the unchanged original
pending/accepted pointers, compiles the shared preview, reopens the file-backed
database and compares the recovered preview. Only its separate explicit request
and native acceptance publish the edited revision. A second reopen compares the
accepted edited output and the untouched second channel, with originals and
receipts retained. File/import refusals record `failed.json` and assert the old
pointers remain intact; saving the editor file does not itself publish anything.
After the second reopen, the test captures both channels concurrently through the
ordinary `/iptv/hls-direct/{channel}.ts` Router route. It checks TSDuck/FFprobe,
source audio/video hashes/timestamps, the complete encoded model and independent
FFmpeg/libzvbi decoding of both pages. `Q` must occur only on channel 1/page 124.
It retains `source.ts`, `channel-1.ts`, `channel-2.ts`, four page-specific SRT files
and `stream-proof.json`. A separate headed client trial must replay these exact
served bytes and inspect actual headers, glyphs/themes and navigation; native
Router capture does not establish a client attached to a bound live server.

Retain the nextest run identity and terminal zero result, `success.json`, edited
TTI hash, unchanged original/sidecar/asset hashes, tool/source/image/driver
identities, actual headed editor log and inspected captures in the final receipt.
`success.json` records the native scope, edited revision/digest, preview dimensions,
diagnostics/raster hash, accepted generation/request and second-channel
isolation. It deliberately requires headed editor/client evidence independently.

This representative probe covers native application export → actual editor
open/edit/save → validated concept import → shared preview → explicit acceptance
→ database reopen → ordinary stream-route output and independent decoding/TSDuck.
Appropriate-client viewing/navigation requires separately inspected headed proof.
Full authored Level 2.5/fallback, complete resource loading/editing, all supported
page-set/subpage workflows, localized headed
admin, Lua behavior and six-platform acceptance retain their existing separate
requirements. The production TV-guide addon still follows independent engine
acceptance; no milestone completion follows from this probe alone.

### Replay a Scripted original through the headed admin

Use [the effective authoring spec](../../tests/e2e/teletext-effective-authoring-flow.spec.ts)
to connect actual Lua scheduling to admin export, external editing, concept
import, preview, explicit publication and ordinary stream capture. The optional
GUI handshake runs only in Chromium with JavaScript enabled and light mode. It
uses the same pinned QTeletextMaker 1.0 source, executable, offline Qt image and
unchanged widget driver as the preceding recipe. Verify their identities against
the retained tool receipt before setting the two SHA256 variables below.

Choose a **nonexistent** child directory; create only its parent. The spec creates
the child itself and refuses a previous attempt. The wrapper owns the disposable
database and matched SSR/WASM build. Point all FFmpeg/FFprobe overrides at the same
verified FFmpeg9 candidate:

```bash
qtm_probe_root="$PWD/tmp/phase9-qtm-effective-admin/editor-run-01"
qtm_tools=/path/to/verified-native-editor-tools
mkdir -p "$(dirname "$qtm_probe_root")"
test ! -e "$qtm_probe_root"
test ! -L "$qtm_probe_root"
qtm_binary_hash="$(sha256sum "$qtm_tools/qteletextmaker" | cut -d ' ' -f1)"
qtm_driver_hash="$(sha256sum "$qtm_tools/native-gui-driver.so" | cut -d ' ' -f1)"
ERSATZRS_TELETEXT_EFFECTIVE_QTM_REPLAY_DIR="$qtm_probe_root" \
  ERSATZRS_TELETEXT_EFFECTIVE_QTM_BINARY_SHA256="$qtm_binary_hash" \
  ERSATZRS_TELETEXT_EFFECTIVE_QTM_DRIVER_SHA256="$qtm_driver_hash" \
  ERSATZRS_TELETEXT_EFFECTIVE_ADMIN_CAPTURE="$PWD/tmp/phase9-qtm-effective-admin/capture-01" \
  ERSATZRS_E2E_RESULT_DIR="$PWD/tmp/phase9-qtm-effective-admin/results-01" \
  ERSATZRS_E2E_KEEP_TEMP=1 \
  ERSATZRS_E2E_PORT=8539 \
  FFMPEG_PATH=/path/to/verified-ffmpeg-9/ffmpeg \
  FFPROBE_PATH=/path/to/verified-ffmpeg-9/ffprobe \
  ErsatzRS_FFMPEG__PATH=/path/to/verified-ffmpeg-9/ffmpeg \
  ErsatzRS_FFPROBE__PATH=/path/to/verified-ffmpeg-9/ffprobe \
  scripts/e2e/playwright-e2e.sh tests/e2e/teletext-effective-authoring-flow.spec.ts \
    --project chromium --headed --workers 1 \
    --grep 'JavaScript true, dark false' \
    --output tmp/phase9-qtm-effective-admin/browser-01
```

Use a free test port and distinct result/output directories for each run. Reuse
a matched build only after verifying its server and hydration bundle identities.
Set `ERSATZRS_TELETEXT_EFFECTIVE_REPEAT_EDITOR=1` to exercise two separate
export/import/publication rounds. The first imports free-row `Q`; the second
exports that actual concept, edits custom-zone row `R`, imports cumulative `Q+R`
and publishes explicitly again. The second round retains distinct ZIP, file and
preview artifacts and checks one immutable original, unchanged scope/expiry and
the opposite channel. Without the GUI handshake both edits are programmatic;
with it, only the first edit is observed in QTeletextMaker. This flag does not
claim a second external-editor GUI trial or complete interchange acceptance.
Selected ordinary captures occur after the second publication and therefore
require `Q+R` only on day200, with page201 and the night channel unchanged.

In the second terminal, set `qtm_probe_root` and `qtm_tools` to the same paths
and wait for the actual `ready.json`. The spec has already written the downloaded
page200/0000 bytes to `native-input.tti`, with the unchanged
`project.ersatzrs.json` sidecar. Readiness records the original role2 scope/expiry,
receipt, requested row2/column8 `Q`, tool identities and file hashes. The marker
wait is bounded to **90 seconds from readiness**; each retry needs a new directory.

Reuse the preceding offline Docker launch with this `qtm_probe_root` and the same
verified tools/image, `single-page-zone-q`, input `native-input.tti` and output
`edited.tti`. Set `qtm_left=0` and `qtm_right=0`: this synthetic page has zero side
panels. Omit the preceding first-channel copy and geometry-extraction scripts;
this readiness has `editor_geometry.left_panel_columns` and
`editor_geometry.right_panel_columns`, both zero. Retain the editor exit status,
completion log and all four actual widget/Save As captures listed above. Inspect
the loaded page, inserted `Q`, selected saved path and cleared modified state.
Do not rewrite the editor's saved bytes or append the programmatic `R` edit.

Only after the editor exits zero and the actual result is independently inspected,
create this **exact ten-field** completion marker. The six-field marker used by
the preceding native probe is not accepted here. This script computes the actual
input/output/tool hashes, checks them against readiness and releases the marker
atomically without overwriting a prior completion:

```bash
python3 - "$qtm_probe_root" "$qtm_tools" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import sys

root, tools = map(Path, sys.argv[1:])
ready = json.loads((root / "ready.json").read_bytes())
saved = root / "edited.tti"
assert saved.is_file() and not saved.is_symlink() and 0 < saved.stat().st_size <= 65536
hash_file = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
binary_hash = hash_file(tools / "qteletextmaker")
driver_hash = hash_file(tools / "native-gui-driver.so")
input_hash = hash_file(root / "native-input.tti")
assert binary_hash == ready["editor"]["binary_sha256"]
assert driver_hash == ready["editor"]["driver_sha256"]
assert input_hash == ready["input_sha256"]
assert hash_file(root / "project.ersatzrs.json") == ready["sidecar_sha256"]
marker = {
    "probe_version": 1,
    "editor_name": "QTeletextMaker",
    "editor_version": "1.0",
    "editor_source_commit": "cac2167bac933a6160cca45a45d5d4ee95355898",
    "editor_binary_sha256": binary_hash,
    "editor_driver_sha256": driver_hash,
    "input_sha256": input_hash,
    "saved_file": "edited.tti",
    "saved_sha256": hash_file(saved),
    "saved_bytes": saved.stat().st_size,
}
temporary = root / "editor-saved.ready"
with temporary.open("x", encoding="utf-8") as output:
    json.dump(marker, output, indent=2)
    output.write("\n")
os.link(temporary, root / "editor-saved.json")
temporary.unlink()
PY
```

The spec rejects unknown marker fields, wrong identities, nonregular/oversized
files and inconsistent hashes or byte counts. This is an **evidence handshake**,
never import or publication authority. The native application authenticates the
unchanged sidecar against its retained original/export receipt and checks the
role2 scope, original expiry and edit boundaries. Other exported files/assets
remain byte-identical. Import saves a concept; the separate explicit request and
publication actions select it for output. Selected status is not an output-applied
acknowledgement.

Retain the terminal Playwright result, independently inspected editor evidence
and exact stream captures. Standard identities without the GUI opt-in edit `Q`
in the free area and `R` in a custom zone programmatically; they prove the browser
connection and do not claim actual QTeletextMaker use. Canonical results and
remaining editor, client and engine limits belong to
[P9A-04](../../docs/progress/milestones.toml), queried with
`cargo xtask progress milestones --id p9a-04 --full`.

## Next copy-mode transport acceptance

Use the existing calibration playout with the accepted Next executable and
FFmpeg runtime staged locally:

```bash
ETV_NEXT_FOLDER=/path/to/staged-next \
ERSATZRS_FFMPEG__PATH=/path/to/ffmpeg \
ERSATZRS_FFPROBE__PATH=/path/to/ffprobe \
ERSATZRS_SMOKE_PORT=18409 \
scripts/e2e/tsduck-playback-gate.sh gate --next-copy --keep
```

This selects copy for both video and audio and captures Next Hybrid MPEG-TS and
HLS across eight-second programme boundaries. It retains the existing
continuity, clock, PSI, audio/video structure and audio-pip checks. Next does
not expose the native legacy/direct transports, so those remain in the default
native gate. The server uses an isolated database and application data/state
directories. The gate refuses an occupied port and leaves existing servers
alone. Select a free `ERSATZRS_SMOKE_PORT` when the default 8409 is in use.

Next HLS captures retain the served playlist and unchanged segment bytes,
grouped only at explicit `EXT-X-DISCONTINUITY` markers. Each group runs the same
zero-error TSDuck checks, including across every unmarked segment boundary.
This follows [RFC 8216 section 3](https://www.rfc-editor.org/rfc/rfc8216.html#section-3):
counter resets are allowed at a declared discontinuity. Raw concatenation loses
that information and reports valid programme transitions as packet loss.
The joined capture still checks decoded audio-pip cadence across transitions.
Run `python3 scripts/e2e/test-capture-hls-groups.py` with TSDuck installed to
prove that unmarked resets and packet loss inside a declared group still fail.

## Next RTSP input fixture

The separate Next CI job stages a pinned, checksum-verified MediaMTX v1.21.1
Linux x64 binary using `stage-rtsp-test-server.py`. It is a temporary RTSP source
for input-protocol acceptance, not a server dependency shipped with ErsatzRS.
The test starts its own loopback listener with synthetic media and stops it on
completion. TSDuck remains the transport-stream output validator; its HTTP/HLS
and UDP/RTP output plugins do not supply an RTSP control server.

For the opt-in Next runtime tests, supply the staged binary as
`ERSATZRS_NEXT_TEST_RTSP_SERVER` alongside `ERSATZRS_NEXT_TEST_BIN`,
`ERSATZRS_NEXT_TEST_FFMPEG` and `ERSATZRS_NEXT_TEST_FFPROBE`. Local tests on other
platforms can point at a compatible official MediaMTX binary; this Linux x64
staging helper serves the existing Linux x64 Next CI job only. The archive hash,
binary hash and upstream license are retained in the staging directory, and CI
uploads the staging provenance with its Next diagnostics. Tests require no
camera, external stream, personal library or permanent service.

References: [MediaMTX v1.21.1 configuration](https://github.com/bluenviron/mediamtx/blob/v1.21.1/mediamtx.yml)
and [TSDuck plugins](https://tsduck.io/commands/).

## Real SponsorBlock publication and transport probe

`tests/e2e/addon-sponsorblock-streaming.spec.ts` is opt-in and never runs in
default CI. It installs the actual yt-dlp add-on through the admin trust flow,
imports the required public fixture, prepares interval metadata, publishes,
and schedules that scanner-owned media for serial MPEG-TS and HLS captures.
It requires explicit FFmpeg 9 paths and does not use browser cookies.

Run against an isolated host/database with an existing matched SSR/WASM build
and installed Chromium. Keep its log available for the test; do not point these
settings at a production database or browser profile:

```bash
E2E_SPONSORBLOCK_ADDON_CHECKOUT=/path/to/addons-checkout \
ERSATZRS_BASE_URL=http://127.0.0.1:18409 \
E2E_DB_PATH=/path/to/isolated/ErsatzRS.db \
E2E_APP_STATE_DIR=/path/to/isolated/state \
E2E_SPONSORBLOCK_HOST_LOG=/path/to/isolated/host.log \
ErsatzRS_FFMPEG__PATH=/path/to/ffmpeg-9/bin/ffmpeg \
ErsatzRS_FFPROBE__PATH=/path/to/ffmpeg-9/bin/ffprobe \
npx playwright test tests/e2e/addon-sponsorblock-streaming.spec.ts \
  --project=chromium --workers=1
```

Captures and timing evidence are retained even when a transport fails. This
probe establishes actual publication and transport. Its H.264/AAC browser HLS
check records first presented frame, post-start stalls/seeks, frame gaps, buffer
samples and conservative temporal cut coverage while capturing those same
segments. MPEG-TS feeds the same HTTP response to one bounded FFmpeg decoder and
records first decoded audio/video, delivery gaps and decoded spans. A bounded
host-log window rejects concealed inner playback failures; only a timestamped
normal cancellation at or after the deliberate TS cutoff is exempt. Only static
failure categories enter the evidence, never raw log lines. Neither check establishes
exact source-content removal, audible continuity or injected network jitter.
Run without concurrent builds or playback checks. For transport diagnosis,
explicitly restart the retained
isolated host/database and pass `E2E_SPONSORBLOCK_PUBLISHED_LIST_ID` with its
existing E2E environment directly to Playwright; the fixture must already have
a completed publication. Configure the isolated host with `ERSATZRS_APP_STATE_DIR`
and `ERSATZRS_APP_DATA_DIR`; do not override `HOME` to isolate this probe.

## SponsorBlock streaming feasibility experiment

`sponsorblock-streaming-spike.mjs` is an isolated diagnostic, not a product or
player acceptance gate. It generates a synthetic source, serves paced HTTP
ranges with delayed opens, and runs one FFmpeg encoder into HLS. Its simulated
consumer measures completed-segment reserve without pausing its playback clock
to hide deficits. No real provider, credentials, application database or Rust
build is involved. All temporary evidence is retained.

```bash
FFMPEG_PATH=/path/to/ffmpeg-9/bin/ffmpeg \
FFPROBE_PATH=/path/to/ffmpeg-9/bin/ffprobe \
node scripts/e2e/sponsorblock-streaming-spike.mjs
```

The approved startup budget is ten seconds target, fifteen seconds maximum.
Other defaults remain experimental: eight seconds of startup reserve, 250 ms
open delay plus repeating 0/150/300 ms jitter, and 128 KiB/s per-response body
pacing. The earlier five-second cap and its results remain historical. The
script header lists overrides. Set both
`OPEN_DELAY_MS=0 OPEN_JITTER_MS=0` for the control. A missed bound exits nonzero;
do not increase bounds merely to obtain a pass. A simulation-only pass proves neither correct
removed content, A/V continuity, cancellation, nor uninterrupted real-player
playback. Set `BROWSER_PROOF=1` to additionally run the existing Chromium and
vendored HLS player against the growing output, checking startup and post-start
video/buffer continuity. This does not boot ErsatzRS or prove audible continuity
or actual-host integration. No browser or dependency is installed by the script.
Run serially, without another build or playback gate.

## Transport playback gate

A **required manual gate for confirming correct playback** — *not* part of CI.

CI proves the code compiles and the units pass; it does **not** prove that the
MPEG‑TS ErsatzRS actually serves is intact for a real player. This harness does:
it captures the served stream and uses [TSDuck](https://tsduck.io/) to prove the
*container* is correct — no packet loss, valid timing, the right structure —
across all streaming pipelines and across playout‑item boundaries (the concat
seams where ErsatzTV‑style stitching is most likely to break).

It is deliberately kept out of CI because it needs FFmpeg 9.0, real‑time
encoding, and tens of seconds per pipeline — too heavy and too timing‑sensitive
for a hosted runner. Run it locally before claiming playback‑sensitive work is
done, alongside `cargo xtask smoke`.

## Pairing with the calibration clips

This gate proves the **container**; the calibration clips from
[`scripts/calibration`](../calibration) prove the **content**. Each clip carries
a millisecond timer, a per‑frame counter (`f/60` stepping 0→59 every second),
and a sweep bar, so a human watching the stream confirms *order* and *smoothness*
by eye while TSDuck confirms the stream around it is sound.

The clips are **gitignored and never committed** — they are regenerated on
demand. `baseline` and `gate` auto-run the generator when the clips are missing,
so a fresh clone of `main` works with no manual setup (the first run spends a
few minutes encoding; later runs reuse the local files). To (re)generate
explicitly:

```bash
scripts/calibration/gen-calibration-clips.sh    # -> calibration-clips/cal_{1,2,3}.{mkv,ts}
```

HDR and tone-mapping calibration clips are opt-in so the default TSDuck gate
stays fast and compatible with its legacy `cal_{1,2,3}` inputs:

```bash
CAL_HDR_MODES="hdr10-pq hlg" \
CAL_MAKE_SDR_REFS=1 \
CAL_TONEMAP_ALGORITHMS="hable mobius reinhard" \
scripts/calibration/gen-calibration-clips.sh
```

The HDR masters are written as `cal_hdr10-pq_{1,2,3}.mkv` and
`cal_hlg_{1,2,3}.mkv`. SDR reference renders are written as
`cal_<mode>_to_sdr_<algorithm>_<N>.mkv`. Every generated video labels its own
color mode and embeds the inspection helpers: peak-to-black chips, saturated
RGBYCM chips, dark grid lines, white timer boxes, the centered geometry square,
and the sweep bar.

## HDR / Tone-Mapping Calibration Spec

TSDuck can automatically verify the **transport** part of HDR playback: TS
sync, continuity counters, PCR/PTS/DTS continuity, PAT/PMT/service structure,
video and audio PID presence, HLS playlist structure, and whether the stream
keeps flowing. It cannot decide whether HDR -> SDR tone mapping is visually
correct, because that requires decoded pixels and color-management semantics.
Use this split:

| Layer | Tool | Gate |
|---|---|---|
| HDR source signaling | `ffprobe` | HDR masters must be 10-bit BT.2020 with `smpte2084` for HDR10 PQ or `arib-std-b67` for HLG. Treat mastering-display and MaxCLL/MaxFALL side data as an additional libx265 capability check when it is present in the generated file. |
| Static TS integrity | TSDuck | Optional HDR `.ts` muxes must pass the same baseline checks as SDR: no TEI/sync loss, no continuity errors, PCR present, video/audio PIDs present, sane bitrate, and no timestamp leaps. |
| Live stream integrity | TSDuck | Captured ErsatzRS HDR-source playback must pass the existing URL/gate checks for every transport mode under test. |
| SDR output signaling | `ffprobe` | A tone-mapped output must be `bt709` primaries, `bt709` transfer, `bt709` matrix, 8-bit `yuv420p` unless the profile intentionally requests HDR passthrough. |
| SDR tone-map pixels | `ffmpeg` frame crops + metrics | Extract matching frames from the live stream and the generated `*_to_sdr_<algorithm>_*.mkv` reference; compare the embedded helper regions, not full-frame compression noise. |

Recommended automated pixel checks:

- Crop the right-side helper panel and compare it against the selected SDR
  reference with `ssim` or `psnr`. Use strict thresholds for software paths
  using the same algorithm, looser thresholds for hardware encoders.
- Run `signalstats` on the peak-to-black strip. It should retain multiple
  distinct luma bands after tone mapping; all-white peak chips, all-black dark
  chips, or collapsed adjacent bands are failures.
- Run `signalstats` on the RGBYCM strip. The saturated chips should remain
  chroma-distinct; a gray or swapped-hue strip is a failure.
- Crop the timer boxes and centered square. They should stay readable and
  geometrically stable relative to the SDR reference; unreadable labels indicate
  highlight clipping or range mistakes, while a distorted square indicates
  scaling/pixel-aspect regression.
- Keep the existing audio pip cadence and frame-order review. Tone mapping must
  not mask playback-order, A/V sync, or dropped-frame regressions.

Minimum manual review when pixel metrics are not wired yet:

1. Generate HDR masters and SDR references with the command above.
2. Ingest the HDR masters into ErsatzRS and stream through the target FFmpeg
   profile with SDR output enabled.
3. Capture the live output with the `url` mode and keep frame evidence:

   ```bash
   scripts/e2e/tsduck-playback-gate.sh --frames url \
     'http://127.0.0.1:8409/iptv/channel/1.ts' 20
   ```

4. Compare captured frames to the matching `*_to_sdr_<algorithm>_*.mkv`
   reference. Fail for clipped peak chips, crushed dark grid lines, hue shifts,
   unreadable timer text, geometry changes, or timing/order regressions.

The automated live content gate is:

```bash
scripts/e2e/hdr-tone-map-proof.sh
```

It generates short HDR10 PQ and HLG calibration masters plus SDR references,
boots `ersatzrs-web` on a throwaway SQLite database, seeds each HDR master as
the active channel source, captures the live MPEG-TS output, asserts 8-bit
BT.709 SDR signaling with `ffprobe`, and compares the decoded static helper-chip
crop against the generated SDR reference with PSNR and SSIM. Set
`HDR_PROOF_KEEP=1` to keep the throwaway DB, server log, live captures, and
metric files. The same gate honors `HDR_PROOF_MODES`, `HDR_PROOF_CLIP_SECONDS`,
`HDR_PROOF_CAPTURE_SECONDS`, `HDR_PROOF_SIZE`, `HDR_PROOF_MIN_PSNR`, and
`HDR_PROOF_MIN_SSIM` for local proof tuning.

## NVENC Encoder-Only Filter Gate

The automated live proof for ADR 0014's NVENC encoder-only path is:

```bash
NVENC_PROOF_FFMPEG_DIR=/path/to/ffmpeg-package/ffmpeg \
  scripts/e2e/nvenc-filter-gate-proof.sh
```

This gate is intentionally narrower than the normal playback gates. It only
runs when the selected FFmpeg build reports `cuda` hwaccel and `h264_nvenc`
while omitting `scale_cuda`, `yadif_cuda`, and `overlay_cuda`. A full
CUDA-filter build is rejected before boot because it cannot prove the
encoder-only software-graph demotion path. On a matching build, the script
boots `ersatzrs-web` on a throwaway SQLite database, seeds a short SDR
calibration clip, forces the channel profile to NVENC H.264, captures the live
MPEG-TS stream, verifies H.264 BT.709 output with `ffprobe`, and scans the
server log for CUDA-filter and playback failure signatures.

Set `NVENC_PROOF_KEEP=1` to keep the throwaway DB, server log, and capture.
The same gate honors `NVENC_PROOF_CLIP_SECONDS`,
`NVENC_PROOF_CAPTURE_SECONDS`, `NVENC_PROOF_SIZE`, and `NVENC_PROOF_FPS`.

## Scale And Pad Profile Proof

The live Scale and Pad proof reproduces synthetic 1920x800 scope-ratio source
geometry without using private library media:

```bash
scripts/e2e/scale-pad-profile-proof.sh
```

The gate generates a calibration source with
`CAL_SIZE_PRESET=cinema-1920x800` (`1920x800`, SAR `1:1`, DAR `12:5`), boots
`ersatzrs-web` on a throwaway SQLite database, seeds that source as active
channel content, forces a software H.264 `1920x1080` Scale and Pad profile,
captures the live MPEG-TS stream, and samples decoded pixels to prove the
output is `1920x1080` with black top/bottom padding and visible source
content. Set `SCALE_PAD_PROOF_KEEP=1` to keep the throwaway DB, logs, and
capture.

## Requirements

`tsduck` (`tsanalyze`, `tsp`), `jq`, `curl`, and FFmpeg 9 on `PATH`
(the gate auto‑exports `LD_LIBRARY_PATH` for the local `~/.local/ffmpeg-9.0`
build, which fails silently without it). `sqlite3` and `cargo` are needed for
`gate` mode only.

## Modes

```bash
# 1. Baseline — validate the static calibration *.ts (no server). Fast self-test
#    of both the checks and the source quality.
scripts/e2e/tsduck-playback-gate.sh baseline

# 2. Live URL — capture a channel you already have running and validate it.
#    Works on the MPEG-TS endpoint or any HLS playlist.
scripts/e2e/tsduck-playback-gate.sh url 'http://127.0.0.1:8409/iptv/channel/1.ts?mode=ts-legacy' 20
scripts/e2e/tsduck-playback-gate.sh url 'http://127.0.0.1:8409/iptv/session/1/hls.m3u8' 20

# 3. Gate — the full E2E proof. Builds + boots ersatzrs-web on a throwaway
#    SQLite DB, seeds the 3 calibration clips as a short 3-item playout, then
#    captures and validates every pipeline and scans the server log.
scripts/e2e/tsduck-playback-gate.sh gate
```

The `url` mode also compares first-to-last PCR media duration with the requested
wall-clock capture window. `MIN_DELIVERY_RATIO_PERCENT` defaults to `95`, and
`MAX_STREAM_STARTUP_SECONDS` defaults to `30`. This makes a structurally clean
stream fail when it cannot deliver media fast enough for a realtime player.
The calculation deliberately does not divide packet count by PCR-derived
bitrate because that assumes constant bitrate and misclassifies VBR HLS.
TSDuck's rolling `bitrate_monitor` uses 50 percent of the capture's wall-clock
average as a severe-stall floor by default; input query strings are omitted
from gate output because they can contain access tokens.

Run the deterministic delivery-classification regression without a server or
provider request:

```bash
scripts/e2e/test-tsduck-delivery.sh
```

Common flags: `--seconds N` (per‑pipeline capture window), `--item-seconds N`
(seconds of each clip the gate playout uses; total playout = 3×N, so a capture
longer than one item crosses the concat seams), `--frames` (extract PNGs for
visual order evidence), `--keep` (keep the work dir: captures, per‑stream
`*.analyze.json`, server log), `-h`.

**Exit status is the gate**: `0` only if every check passed.

The `gate` server port is hardcoded to **8409**; the gate refuses to start if
something is already serving there (it will not clobber another running server —
e.g. one started by another agent). In that case stop it, or use `url` mode.

## What gets checked (per stream)

| Check | Why it matters |
|---|---|
| packet count / TS bitrate ≥ min | the stream actually flowed |
| transport‑error (TEI) / sync losses = 0 | no corruption in delivery |
| PAT / PMT / service / SDT present | a player can find the program |
| ≥1 video PID, ≥1 audio PID | A/V actually present |
| PCR samples present | the stream carries a clock |
| **continuity‑counter errors = 0** | **real packet loss / skips** (primary signal) |
| **PTS / DTS / PCR leaps = 0** | **no visible skips, no clock breaks** |
| scrambled / invalid‑scrambling = 0 | content is clear |
| `continuity` processor cross‑check | independent confirmation of the CC check |
| HLS: `#EXTM3U`, segment count, target duration, discontinuity markers | playlist is well‑formed |
| PCR media duration / wall time ≥ configured ratio | content keeps pace without assuming CBR |
| rolling bitrate above the capture-relative severe-stall floor | no empty/starved delivery window |
| `pcrverify` jitter | informational (see below) |

`continuity-counter errors` and `pts/dts/pcr leaps` are the playback‑correctness
core: a dropped or duplicated packet, or an unexpected timestamp jump, is exactly
what shows up as a stutter, skip, or A/V‑sync break in a player.

### A note on PCR jitter

`pcrverify` measures each PCR against the average‑bitrate byte‑position model, so
it is only a meaningful pass/fail for **constant‑bitrate** streams. FFmpeg VBR
output legitimately reports large jitter here, so it is **informational by
default**. Set `STRICT_PCR_JITTER=1` to gate on it (CBR channels only). Real PCR
clock breaks are already gated by the `pcr-leap` check.

## Thresholds (environment overrides)

| Var | Default | Meaning |
|---|---|---|
| `MAX_CC_ERRORS` | `0` | allowed continuity‑counter errors |
| `MAX_PTS_LEAPS` / `MAX_DTS_LEAPS` / `MAX_PCR_LEAPS` | `0` | allowed timestamp leaps |
| `MIN_BITRATE_BPS` | `100000` | minimum TS bitrate |
| `MIN_PACKETS` | `2000` | minimum captured packets |
| `EXPECT_VIDEO_PIDS` / `EXPECT_AUDIO_PIDS` | `1` | required ES counts |
| `MIN_HLS_SEGMENTS` | `1` | minimum `#EXTINF` entries |
| `MIN_DELIVERY_RATIO_PERCENT` | `95` | minimum PCR media-time / wall-time percentage |
| `MAX_STREAM_STARTUP_SECONDS` | `30` | maximum input startup latency |
| `MIN_ROLLING_DELIVERY_RATIO_PERCENT` | `50` | interval floor relative to capture average |
| `MAX_LOW_DELIVERY_INTERVALS` | `0` | intervals allowed below the rolling floor |
| `STRICT_PCR_JITTER` | `0` | `1` = gate on `pcrverify` jitter (CBR only) |
| `MAX_PCR_JITTER_US` | `8000` | jitter ceiling (µs) when strict |
| `CAPTURE_SECONDS` | `20` | per‑pipeline capture window |
| `ITEM_SECONDS` | `8` | gate playout: seconds per clip |

## Ad‑hoc TSDuck

```bash
tsanalyze calibration-clips/cal_1.ts                      # full structural report
tsanalyze --json cal_1.ts | jq '.pids[]|{id,description,cc:.packets.discontinuities}'
tsp -I file cal_1.ts -P continuity -P pcrverify -O drop   # CC + PCR in one pass
tsp -I http 'http://127.0.0.1:8409/iptv/channel/1.ts' -P continuity -O drop  # live
```

---

# Island hydration smoke (E2E)

A **required manual gate for confirming the admin UI is interactive** — *not*
part of CI. Where the TSDuck gate proves the served container is sound, this gate
proves every Leptos `#[island]` actually hydrates, so the click handlers that
SSR alone cannot exercise (stop/delete on `/channels`, refresh and row-select on
`/playouts`, every dialog and form) really attach in the browser.

It exists because we keep regressing one failure class: a change to a
shell-level island, a shared UI primitive, the router, or a Cargo dependency
aborts island hydration and silently disables the interactive controls on
otherwise-unrelated pages. Plain `<a>` links keep working, so it is easy to ship
without noticing.

```bash
scripts/e2e/hydration-smoke.sh
```

The runner is self-contained: it builds the Leptos hydration assets
(`cargo leptos build`), boots `ersatzrs-web` on a throwaway SQLite database with
an isolated `HOME` (never touches `~/.ersatzrs`), waits for `/healthz`, runs
`tests/e2e/island-hydration.spec.ts` against every island route, then tears the
server and temp files down. Requires `cargo-leptos` (`cargo install cargo-leptos
--locked`) and `npx`. An FFmpeg override at `~/.local/ffmpeg-9.0` (like
`run.sh`) is used when present so startup skips the managed runtime download.

| Env var | Default | Effect |
| --- | --- | --- |
| `ERSATZRS_SKIP_LEPTOS_BUILD` | `0` | `1` = reuse already-built hydration assets |
| `ERSATZRS_E2E_KEEP_TEMP` | `0` | `1` = keep the temp DB/log dir for debugging |
| `ERSATZRS_E2E_PORT` | `8409` | server port |
| `ERSATZRS_FFMPEG_DIR` | `~/.local/ffmpeg-9.0` | FFmpeg override root |
| `ERSATZRS_SKIP_PW_INSTALL` | `0` | `1` = skip `playwright install chromium` |

Per-route pass criteria: `window.__ersatzrs_hydrated === true` (set right after
`hydrate_islands()` in `crates/ersatzrs-web/src/lib.rs`), no uncaught page
errors, no hydration-signature console errors, and no fall-through to the 404
catch-all. Add new island routes to the `ISLAND_ROUTES` table in the spec.
Detail/editor routes that need a seeded `:id` are out of scope here — cover them
in dedicated seeded specs.

## Teletext resource decoder calibration

Run the explicitly ignored native diagnostic on Linux x64 with Python 3 and
installed libzvbi 0.2.42. Other platforms/versions require their own calibrated
oracle; this helper is not a runtime dependency or a platform acceptance gate.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_RESOURCE_FIXTURE="$fixture_dir/cases.json" \
  ./run.sh test -p ersatzrs-ffmpeg resource_runtime_tests --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-resource-decoder.py < "$fixture_dir/cases.json"
```

`ERSATZRS_TELETEXT_TEST_PYTHON` optionally selects Python through the existing
locator. The optional export contains only synthetic encoded units and expected
pixels; replay returns compact JSON with the actual decoder version and passed
case count. Inputs are bounded and strictly validated. Missing/mismatched native
libraries or failed cases exit nonzero. The existing runner enforces process
and output limits during the test.

Nine MOT-based cases check exact global/normal DRCS identities, all glyph pixels,
native PAL8 cell rendering, defined blank versus fallback, and independently
isolated missing-row, SPACE-half, parity and undefined-mode failures. They also
retain separate base-text checks. Resource units and local display-invocation
bodies come from production encoders; the nine-case fixture remains byte-identical
to the earlier synthetic calibration. The declared MOT extent is fully transmitted,
including empty companion subtables and correct counts.
Own-magazine closing headers avoid a version-specific 0.2.42 cache behavior.

The [interchange reference](../../docs/reference/teletext-design-interchange.md#pinned-independent-resource-decoding)
records source pins, coverage and the separately reproduced X27 limitation.
This diagnostic does not establish client display/navigation, transport integrity,
complete Level 2.5 compilation or the design/editor/publication workflow.

## Teletext warm-cache DRCS update calibration

The additive Linux x64 diagnostic keeps one receipted original libzvbi 0.2.42
decoder across four generations and two complete production-carousel cycles
each. It verifies Changed A at counter7, identical Unchanged A, Changed B after
wrap to counter0 and identical Unchanged B. Actual MOT/DRCS/display packets,
fixed independent glyph/font pixels and PAL8 rendering are checked without
extra closing headers or decoder resets.

Use the original maintained proof-build receipt and run:

```bash
LD_LIBRARY_PATH="$proof_dir/original" \
ERSATZRS_TELETEXT_TEST_RESOURCE_UPDATE_FIXTURE="$fixture_dir/updates.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  consecutive_complete_carousel_drcs_updates_decode_with_one_warm_cache \
  --lib --run-ignored ignored-only --success-output immediate
LD_LIBRARY_PATH="$proof_dir/original" \
  python3 scripts/e2e/teletext-resource-update-decoder.py < "$fixture_dir/updates.json"
python3 scripts/e2e/teletext-resource-update-decoder_test.py
```

The [canonical reference](../../docs/reference/teletext-design-interchange.md#warm-cache-drcs-update-diagnostic)
owns exact hashes, ABI/library/source guards, closed input limits, independent
pixel oracles and proof boundaries. The helper executes only its hash-verified
ABI source bytes, ignoring cached bytecode. Runtime/playback/client navigation,
restart, other resources and full Level 2.5 acceptance remain separate gates.

## Teletext warm-cache magazine-default diagnostic

For the separate pinned M29 cache-persistence trial, run
`teletext-magazine-cache-decoder.py` against the closed archived-unit request
and its `_test.py` contract suite. The
[canonical reference](../../docs/reference/teletext-design-interchange.md#warm-magazine-default-cache-diagnostic)
owns the reconstruction, hashes, three-checkpoint replay and proof boundaries.
A passing trial confirms stale magazine defaults after omission; it does not
prove safe removal or complete replacement/client conformance.

## Teletext object decoder calibration

The separate explicitly ignored diagnostic uses the same Linux x64, Python 3
and pinned libzvbi 0.2.42 public ABI. It imports the frozen resource helper's
version/layout guards, without changing the DRCS oracle.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_OBJECT_FIXTURE="$fixture_dir/objects.json" \
  ./run.sh test -p ersatzrs-ffmpeg object_runtime_tests --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/objects.json"
```

Eight synthetic cases use complete production POP/GPOP pages, exact allocated
invocations and MOT declarations, including legal empty companions for S1=15.
They cover public/global active/adaptive/passive objects and two nested chains.
The fresh-decoder oracle checks exact glyphs, foreground/background colours,
relative origins, active propagation versus adaptive/passive containment and
unchanged base cells. It does not compare font pixels.

The helper bounds input to 2 MiB, twelve cases, 256 units and sixteen checked
cells per case. It rejects unknown/duplicate members and invalid dimensions,
numbers and bytes before native decoding. Replay emits compact version/count
JSON; the existing test runner bounds process time, stdout and stderr.

The [object-decoding reference](../../docs/reference/teletext-design-interchange.md#pinned-independent-object-decoding)
records exact fixture/helper hashes and coverage. Native proof covers the first
pointer bank only; byte tests cover all pointer banks and capacities. Higher-bank
external decoding, local objects, complete inheritance/service composition,
actual client display/navigation, transport and editor/publication remain open.
This helper is neither a production dependency nor platform acceptance.

## Local-object decoder limitation diagnostic

The explicitly ignored Linux x64 diagnostic records an actual libzvbi 0.2.42
limitation for correctly assigned local definition markers. It uses the unchanged
object helper and complete production local-program bodies.

```bash
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_FIXTURE="$fixture_dir/diagnostic.json" \
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE="$fixture_dir/conformance.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  pinned_libzvbi_0_2_42_local_definition_marker_limitation_is_reproduced --lib \
  --run-ignored ignored-only
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/diagnostic.json"
# Expected to fail on this pinned decoder: correct authored glyph/colour oracle.
python3 scripts/e2e/teletext-object-decoder.py < "$fixture_dir/conformance.json"
```

The diagnostic requires exact base T/white/black fallback for all three local
object types, alongside the independent B calibration. The separate conformance
fixture retains the correct A/red/blue expectations. Their production units are
byte-identical; only expected decoder cells differ. A green diagnostic reproduces
the limitation and cannot establish local-object rendering or Level 2.5 acceptance.
The [local-decoder reference](../../docs/reference/teletext-design-interchange.md#local-object-decoder-limitation)
records hashes, source findings and the still-required decoder/client evidence.

## Teletext X27 test-decoder comparison

This manual Linux-x64 calibration compares official libzvbi 0.2.42 with a
**test-only three-line X27 parser correction**. It does not install a runtime
dependency or replace client acceptance. See the
[scope, exact hashes and recorded outcomes](../../docs/reference/teletext-design-interchange.md#test-corrected-x27-independent-decoder).
The original nested conformance case must still fail; the comparison records that
failure separately from successful corrected decoding.

Use an existing Linux-amd64 build image with Python 3.12+, a C toolchain,
autoconf/automake/libtool/gettext and pkg-config. The pinned image below is the
locally available image used for the recorded proof, not a publicly distributed
image or an automatic download. Another image needs its own recorded build and
outer image identity; do not relabel its receipt as the pinned proof. Save the
official source archive from the receipt URL as `source.tar.gz` in a temporary
input directory. The builder enforces its exact size, SHA256 and member count
before extraction. Capture `root.json` and `nested.json` using the
[production fixture export instructions](../../docs/reference/teletext-design-interchange.md#native-default-display-packets).
Exporting nested on unmodified libzvbi intentionally fails its normative test;
keep that failure and the unchanged emitted request. Do not edit its expected cells.

From the repository root, after setting `proof_input` to that absolute temporary
input directory:

```bash
proof_image=sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e
proof_output=$(mktemp -d)
proof_scripts="$PWD/scripts/e2e"
docker image inspect "$proof_image" --format '{{.Id}} {{.Architecture}} {{.Os}}'
docker run --rm --pull=never --network none --read-only --cpus=2 --memory=2g \
  --tmpfs /tmp:rw,exec,size=1536m \
  --mount "type=bind,src=$proof_input/source.tar.gz,dst=/proof/source.tar.gz,readonly" \
  --mount "type=bind,src=$proof_scripts/teletext-zvbi-x27-build.sh,dst=/proof/build.sh,readonly" \
  --mount "type=bind,src=$proof_output,dst=/out" \
  --entrypoint /bin/sh "$proof_image" /proof/build.sh

docker run --rm --pull=never --network none --read-only --cpus=1 --memory=512m \
  --tmpfs /tmp:rw,exec,size=64m \
  --mount "type=bind,src=$proof_output,dst=/out,readonly" \
  --mount "type=bind,src=$proof_scripts,dst=/proof/scripts,readonly" \
  --mount "type=bind,src=$proof_input/root.json,dst=/proof/root.json,readonly" \
  --mount "type=bind,src=$proof_input/nested.json,dst=/proof/nested.json,readonly" \
  --entrypoint /bin/sh "$proof_image" -c '
    set -eu
    for variant in original corrected; do
      for fixture in root nested; do
        LD_LIBRARY_PATH=/out/$variant python3 \
          /proof/scripts/teletext-zvbi-x27-proof.py "$variant" "$fixture"
      done
    done' > "$proof_input/results.ndjson"
```

`proof_output` must initially be empty. Retain it, the outer image-inspection
result and all four NDJSON records together. `proof-build.json` records source,
configuration, tool and library hashes; the replay verifies unchanged helpers and
fixtures and the actual mapped decoder path/hash. Fresh processes prevent library
selection from leaking between cases. The original/nested record must explicitly
report `conformance_passed: false`; the other three report two passing cases each.
As a negative check, request `corrected root` with
`LD_LIBRARY_PATH=/out/original`: the same replay must exit nonzero with
`Unexpected mapped decoder path`. No Docker socket, host library installation,
network access, source modification or publication occurs inside these containers.

The same receipt and frozen object/resource helpers also support
`authored-root` and `authored-nested`, exported by the complete service compiler.
Capture `authored-root.json` and `authored-nested.json` using the
[complete service candidate instructions](../../docs/reference/teletext-design-interchange.md#authenticated-complete-service-candidates).
Reuse valid `proof_output`; no decoder rebuild is necessary. Bind these producer
fixtures explicitly and replay all four variant/fixture combinations:

```bash
docker run --rm --pull=never --network none --read-only --cpus=1 --memory=512m \
  --tmpfs /tmp:rw,exec,size=64m \
  --mount "type=bind,src=$proof_output,dst=/out,readonly" \
  --mount "type=bind,src=$proof_scripts,dst=/proof/scripts,readonly" \
  --mount "type=bind,src=$proof_input/authored-root.json,dst=/proof/authored-root.json,readonly" \
  --mount "type=bind,src=$proof_input/authored-nested.json,dst=/proof/authored-nested.json,readonly" \
  --entrypoint /bin/sh "$proof_image" -c '
    set -eu
    for variant in original corrected; do
      for fixture in authored-root authored-nested; do
        LD_LIBRARY_PATH=/out/$variant python3 \
          /proof/scripts/teletext-zvbi-x27-proof.py "$variant" "$fixture"
      done
    done' > "$proof_input/authored-results.ndjson"
```

The original authored-nested record must preserve the exact known failure with
`conformance_passed: false`. Both corrected records and original authored-root
must report two passing cases. The complete compiler emits the actual resources,
including the required empty companion, and both native and compiled display
paths. Only closing headers are added for decoder feeding. This does not prove
service-carousel timing, transport, lifecycle history or client navigation.

For actual two-cycle production-carousel packets, export `carousel-root.json`
and `carousel-nested.json` using the
[carousel/output instructions](../../docs/reference/teletext-design-interchange.md#complete-service-carousel-and-output-planning).
Reuse the same receipted libraries and command, binding these two files at
`/proof/carousel-root.json` and `/proof/carousel-nested.json`, and iterate
`carousel-root carousel-nested` instead of the authored fixture names.
Both corrected records and original root pass two unchanged cell oracles;
original nested remains the exact known failure. The separate TSDuck test uses
the complete mixed candidate, production carousel and packetizer with independent
calibration PSI/PCR. It checks continuity damage explicitly. Neither these
probes nor the pure output-epoch tests establish hosted playback, navigation,
editor/publication acceptance or every Level2.5 feature.

## Teletext second local-object decoder calibration

The separate opt-in cell oracle uses unmodified teletextimager 0.2.0 at
`2b0125b778c45ffdfa2df0b196870c093361f317`. Supply an external checkout explicitly;
the helper checks its commit and imported source hashes before executing it.
It does not install packages or fetch sources. Python's standard library suffices
for these cell checks; this workflow does not invoke the Pillow renderer.

```bash
decoder_dir="$(mktemp -d)"
git clone https://github.com/gkthemac/teletextimager.git "$decoder_dir"
git -C "$decoder_dir" checkout --detach 2b0125b778c45ffdfa2df0b196870c093361f317
fixture_dir="$(mktemp -d)"
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE="$fixture_dir/conformance.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  authored_local_objects_decode_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-imager-decoder.py < "$fixture_dir/conformance.json"
```

The original correct conformance request retains SHA256
`11bfc143b8ad25d66433b7e8235cdede3fc39ab4cddae685547eb2c6c09bfe2c`.
Its `expected_version` field identifies its original libzvbi provenance; the
second helper guards and reports its own pinned decoder/version. All three local
types must render A/red/blue, with exact type-specific adjacent T attributes and
unchanged B. The decoder reads original production DVB bytes through its own
Hamming reader, after framing removal and bit reversal only. Wrong expected cells
or modified upstream sources fail rather than relaxing the oracle.

This proves representative direct local-object decoding. It is neither full
Level 2.5 acceptance nor actual interactive client/navigation or bitmap evidence.
Non-Latin adaptive/passive G0/G2, side-panel edges, automatic MOT/X27 selection,
complete local coverage and resource pointer extensions remain separate
proof obligations. See the
[pinned source and acceptance boundaries](../../docs/reference/teletext-design-interchange.md#second-independent-local-object-decoder).

The advanced opt-in case group adds production-encoded local definitions at
designation 8/slot 0 and designation 15/slot 0, plus a full-ID nested
Active-to-Adaptive-to-Passive chain. Reuse the same detached decoder checkout:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_ADVANCED_LOCAL_OBJECT_FIXTURE="$fixture_dir/advanced.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  advanced_local_objects_decode_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-imager-decoder.py < "$fixture_dir/advanced.json"
```

The advanced fixture SHA256 is
`57bf567c2dd894bf884d2435a9932738ca972c7fb3cfd16a77509801eceaece3`.
This is additional independent local formatting/origin proof, with unchanged
original fixtures. It does not certify all object interactions, pixels, transport
or client navigation. The only replaced base X26 in these synthetic ASCII cases
is its known empty sequence; generic base/enhancement composition remains open.

## Teletext ordered-main composition decoder calibration

The opt-in composition oracle reuses the unchanged pinned teletextimager loader
and its eight source guards. It checks literal code/repertoire/diacritic/colour
tuples at both Level 1.5 and 2.5 from production-composed and lowered DVB units.
Reuse the explicit detached `decoder_dir` and temporary `fixture_dir` above:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_MAIN_COMPOSITION_FIXTURE="$fixture_dir/main.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  composed_main_program_decodes_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py < "$fixture_dir/main.json"
```

The fixed cases cover derived pound/acute glyphs, compatible authored replacement,
Level-2.5 G0/G1 with Level-1.5 fallback, and an anchored full-ID local Passive
invocation with its definition pointer shifted by inserted base commands. All
original base X26 bodies are replaced by the one compiled sequence; header,
presentation and fallback bytes remain unchanged. The separate schema reports
the decoder's own 0.2.0 version and rejects duplicate/unknown fields and unbounded
units/cells. It adds no runtime dependency or source download. Non-Latin restore
decoding, Active overlap, header/dual-row lowering, complete display geometry,
transport and actual client navigation remain separate acceptance obligations.

The four-case fixture SHA256 is
`bf4584486120137270f3a5ed62a7830ee5b1af81923445c0b87013b8ae91c603`;
the dedicated helper SHA256 is
`e1cb2ad2b16e78a0f56baaae14434aa6d78f146d7e0173c7ade9f19db97228eb`.
The pages explicitly select German national option one: pound therefore requires
the intended derived G2 replacement, while ordinary ASCII retains repertoire 15
in the pinned decoder. The original local/resource fixture hashes stay unchanged.

## Teletext compatibility-aware main calibration

Two further fixed cases use `compile_teletext_composed_main_program`, which
composes and packs against the same immutable service and original header colour.
They cover a derived body glyph after an anchored Passive invocation and a first
header glyph after explicit screen colour. Reuse the same detached decoder:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_CONTEXT_MAIN_FIXTURE="$fixture_dir/context-main.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  contextual_main_program_decodes_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py \
  < "$fixture_dir/context-main.json"
```

The separate two-case fixture SHA256 is
`a8e677edf49a3141a5f5a3c2e412d1e6dae29db653b41e42a88f621b7dbb3774`.
The helper and original four-case composition fixture are unchanged. The pinned
decoder proves these literal cells at both levels, but its Level 1.5 path does
not model every historical row-address disturbance. Separate literal triplet and
protected-pointer regressions check restoration fields, high local pointers and
exact physical capacity. This is partial wire/decoder evidence; complex header
normalization, original default realization, final geometry/priority and suitable
client navigation remain open. No successful result activates or publishes pages.

The national-overlap fixture exercises actual normalization of an enlarged German
Base `§` over an Active double-size object. The same frozen composition helper
checks both receiver levels, with literal national/G2 identities and adjacent
`A/X/B` cells. Export with `ERSATZRS_TELETEXT_TEST_NATIONAL_OVERLAP_IMAGER_FIXTURE`
and run `national_base_enlarged_overlap_decodes_with_pinned_imager`; see the
[exact command, fixture hash and limits](../../docs/reference/teletext-design-interchange.md#national-option-carrier-conversion).
This selected-cell proof does not establish all national glyph pixels or actual
client navigation. Four national conversion positions remain explicitly pending.
The English Hebrew-G0 carrier has full model equality but fails the unchanged
pinned imager oracle because private object charset designation is omitted.
`english_national_overlap_records_pinned_imager_private_charset_gap` and
`teletext-national-carrier-calibration.py` record this exact negative result with
`conformance:false`; they do not certify Level2.5 decoding. The linked reference
contains both strict-failure and negative-calibration replay commands and hashes.

The complete-display calibration feeds `encode_teletext_normalized_display_page`
output directly to the unchanged decoder, retaining exact owner/subcode, original
fallback and actual metadata presence. Run
`production_display_units_decode_active_override_and_original_fallback` with
`ERSATZRS_TELETEXT_TEST_COMPILED_DISPLAY_IMAGER_FIXTURE` to export the request.
[Complete packet evidence and replay](../../docs/reference/teletext-design-interchange.md#complete-normalized-display-packets)
record its hash, three dual-level selected cells and remaining service/client gates.

## Teletext base display calibration

Four additional synthetic cases use actual page and presentation encoders with
the same pinned teletextimager checkout and unchanged guarded helper:

```bash
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
ERSATZRS_TELETEXT_TEST_BASE_DISPLAY_FIXTURE="$fixture_dir/base-display.json" \
  ./run.sh test -p ersatzrs-ffmpeg \
  actual_base_spacing_cells_decode_with_pinned_independent_teletextimager --lib \
  --run-ignored ignored-only
ERSATZRS_TELETEXT_TEST_IMAGER_DIR="$decoder_dir" \
  python3 scripts/e2e/teletext-main-composition-decoder.py \
  < "$fixture_dir/base-display.json"
```

Literal code/repertoire/diacritic/foreground/background tuples at both levels
cover control spaces, row reset, mosaics, asymmetric foreground/background CLUT
remapping and existing base Unicode replacements. Internal trace tests separately
check authored intent and attribute sources. These selected cells do not prove
object priority, double-height rendering, complete enhanced semantics or actual
client display/navigation. Import, preview and publication remain separate gates.
The four-case fixture SHA256 is
`2100d9bd163221cfa57617a5936ae780d6b174a895b48508c70bf536666e5d30`;
the helper and earlier four/two-case composition fixtures remain unchanged.

### Localized teletext editor transfer flow

`tests/e2e/teletext-editor-flow.spec.ts` uses the runner-owned disposable database
and one v3-only synthetic addon on two channels with day/night profiles. Run it
through `scripts/e2e/playwright-e2e.sh` with `--headed --workers 1`, in Chromium
and Firefox. Four JavaScript/theme combinations each exercise all nine cultures:
profile selection, original validation, complete/selected ZIP download, protected
edit rejection, multipart concept import, shared preview and separate publication.
Its bounded TTI cell edit tests the HTTP/admin connection; the actual pinned
QTeletextMaker and ordinary-stream/client receipts remain separate evidence.

The native forms require the exact unchanged `.ersatzrs.json` sidecar and every
TTI/DVB file from the same export. Imports preserve filenames and cannot publish.
The structured agent-file workflow and protection rules live in
[the interchange reference](../../docs/reference/teletext-design-interchange.md);
P9A-04 owns the current proof/gap map. No representative flow closes full authored
Level 2.5, editor-resource, Lua/restart or platform acceptance.

The versioned carrier probes use the same replay procedure with test filters
`pinned_external_editor_nonzero_equal_nos` and
`pinned_external_editor_independent_nos`. Set **all** fixture/route/decoder paths
(`FFMPEG_PATH`, `FFPROBE_PATH`, `ErsatzRS_FFMPEG__PATH`,
`ErsatzRS_FFPROBE__PATH`, `ERSATZRS_TELETEXT_TEST_FFMPEG`) to the same verified
FFmpeg9 candidate; a decoder-only override leaves the fixture on its old runtime.
Each probe requires a new directory. The driver selects only its exact owned
`AcceptSave`/`Save As` dialog, never a visible Open dialog.

The pinned tool image contains Qt6.10.2 headers/libraries without pkg-config
metadata. To rebuild only the test preload, use its existing SDK with
`g++ -std=c++17 -fPIC -shared`, the Qt Widgets/Test/Gui/Core module defines,
include paths under `/usr/include/x86_64-linux-gnu/qt6` and those four module
subdirectories, and `-lQt6Widgets -lQt6Gui -lQt6Core -lQt6Test -ldl`. Bind the
repository driver read-only and a separate owned output directory writable;
use the same offline image with bounded CPU/memory. Retain the new driver hash,
keep the editor binary unchanged, and record actual runtime/editor/native results.
P9A-04 owns both successful carrier trials and their precise limits.

### Published ordinary-URL teletext client trial

The ignored native selector `published_indexed_originals_serve_live_ordinary_urls`
prepares and publishes two synthetic profiles, reopens durable state and serves
the actual ordinary HTTP routes on a test-owned loopback host. The headed
`scripts/e2e/teletext-live-client-trial.py` uses pinned VLC/libzvbi and exact owned
X11 windows for both themes and124->125->124 navigation. It consumes live URLs,
not replay files. Independent image inspection and client exit0 are required
before explicitly releasing the native completion marker; nextest success also
checks retained state and bounded owned-host closure. Follow the
[workflow, capacity and completion rules](../../docs/reference/teletext-design-interchange.md#published-cold-ordinary-url-client-workflow).
P9A-04 owns receipts, negative trials and remaining criteria. These Linux-x64/X11
helpers are test tooling; neither a representative pass nor four-slot fixture
capacity closes full engine or capacity-wait playback acceptance.
