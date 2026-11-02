#!/usr/bin/env python3
"""Exercise the actual optional failure-capture function without a host or browsers."""

import json
import os
from pathlib import Path
import subprocess
import struct
import tempfile
import unittest
import zipfile


WRAPPER = Path(__file__).with_name("playwright-e2e.sh").read_text()
FUNCTION = WRAPPER.split("capture_troubleshooting_failure() (", 1)[1].split(
    "\nWINDOWS_POSIX=0", 1
)[0]
FUNCTION = "capture_troubleshooting_failure() (" + FUNCTION
CLOSEOUT = WRAPPER.rsplit("STATUS=$?\n", 1)[1]


class FailureCapture(unittest.TestCase):
    def run_capture(self, status, http="200", curl_exit=0, valid_zip=True, oversize=None):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = root / "bin"
            binary.mkdir()
            payload = root / "payload"
            if valid_zip:
                with zipfile.ZipFile(payload, "w") as archive:
                    archive.writestr("next/ffreport.log", "Synthetic diagnostic fixture")
                    archive.writestr("next/channel_config.json", "{}")
            else:
                payload.write_text("Not an archive")
            if oversize == "uncompressed":
                # Declare an excessive expanded member size without allocating it.
                data = bytearray(payload.read_bytes())
                header = data.index(b"PK\x01\x02")
                struct.pack_into("<I", data, header + 24, 134217729)
                payload.write_bytes(data)
            curl = binary / "curl"
            curl.write_text("""#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
with pathlib.Path(os.environ['CURL_CALLS']).open('a') as calls:
    calls.write(json.dumps(args) + '\\n')
if os.environ['CURL_SPARSE_BYTES']:
    with pathlib.Path(args[args.index('--output') + 1]).open('wb') as output:
        output.truncate(int(os.environ['CURL_SPARSE_BYTES']))
else:
    shutil.copyfile(os.environ['CURL_PAYLOAD'], args[args.index('--output') + 1])
print(os.environ['CURL_HTTP'], end='')
raise SystemExit(int(os.environ['CURL_EXIT']))
""")
            curl.chmod(0o755)
            results = root / "result"
            results.mkdir()
            env = os.environ.copy()
            env.update(PATH=str(binary) + os.pathsep + env["PATH"],
                       RESULT_DIR=str(results), BASE_URL="http://127.0.0.1:12345",
                       RESULT_JSON="synthetic-result.json", SERVER_LOG="synthetic-host.log",
                       CURL_CALLS=str(root / "calls"), CURL_PAYLOAD=str(payload),
                       CURL_HTTP=http, CURL_EXIT=str(curl_exit),
                       CURL_SPARSE_BYTES="33554433" if oversize == "compressed" else "")
            command = ("set -euo pipefail\nlog() { :; }\nerr() { printf '%s\\n' \"$*\" >&2; }\n"
                       + FUNCTION + f"\nSTATUS={status}\n" + CLOSEOUT)
            result = subprocess.run(["bash", "-c", command], env=env,
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, status, result.stderr)
            if status == 0:
                self.assertFalse((root / "calls").exists())
                self.assertEqual(list(results.iterdir()), [])
                return
            requests = (root / "calls").read_text().splitlines()
            self.assertEqual(len(requests), 1, "Optional capture must not retry")
            calls = json.loads(requests[0])
            self.assertEqual(calls[calls.index("--max-time") + 1], "5")
            self.assertEqual(calls[calls.index("--max-filesize") + 1], "33554432")
            self.assertEqual(calls[-1], env["BASE_URL"] + "/api/troubleshoot/playback/archive")
            receipts = list(results.glob("*.json"))
            self.assertEqual(len(receipts), 1)
            receipt = json.loads(receipts[0].read_text())
            self.assertEqual(receipt["original_test_exit_code"], status)
            self.assertEqual(receipt["http_status"], http)
            self.assertEqual(receipt["curl_exit_code"], curl_exit)
            self.assertFalse(receipt["exact_failed_preview_identity_verified"])
            self.assertEqual(receipt["max_compressed_bytes"], 33554432)
            self.assertEqual(receipt["max_uncompressed_bytes"], 134217728)
            retained = curl_exit == 0 and http == "200" and valid_zip and oversize is None
            self.assertEqual(receipt["archive_retained"], retained)
            self.assertFalse(list(results.glob("*.partial")))
            archives = list(results.glob("*.zip"))
            self.assertEqual(len(archives), int(retained))
            if retained:
                with zipfile.ZipFile(archives[0]) as archive:
                    self.assertIsNone(archive.testzip())
            else:
                self.assertIn("capture_error", receipt)
                if oversize:
                    self.assertIn(oversize + "-byte cap", receipt["capture_error"])

    def test_success_does_not_request_diagnostics(self):
        self.run_capture(0)

    def test_valid_failure_archive_preserves_original_status(self):
        self.run_capture(37)

    def test_failed_requests_and_invalid_zip_never_mask_failure(self):
        for http, exit_code, valid in (("404", 0, True), ("000", 7, True), ("200", 0, False)):
            with self.subTest(http=http, curl_exit=exit_code, valid_zip=valid):
                self.run_capture(37, http, exit_code, valid)

    def test_oversized_archives_are_rejected_before_crc_and_preserve_status(self):
        for oversize in ("compressed", "uncompressed"):
            with self.subTest(oversize=oversize):
                self.run_capture(37, oversize=oversize)


if __name__ == "__main__":
    unittest.main()
