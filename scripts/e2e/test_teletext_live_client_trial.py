"""Source-only manifest regressions; no GUI/client/native decoder is started.

Run with: python3 -m unittest discover -s scripts/e2e -p test_teletext_live_client_trial.py
"""

import copy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import tempfile
from types import ModuleType
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-live-client-trial.py")
HELPER = ModuleType("teletext_live_client_contract")
HELPER.__file__ = str(SCRIPT)
# Execute the inspected source definitions, never an unrelated cached pyc.
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), HELPER.__dict__)


def effective_request(directory):
    # Structurally framed bytes only, never alleged playable source media.
    body = b"\x47\x01\x00\x10" + bytes(184) + b"\x47\x01\x01\x10" + bytes(184)
    (directory / "source.ts").write_bytes(body)
    channels = []
    for index, theme in enumerate(["day", "night"]):
        channel = 41 + index
        channels.append({
            "channel_id": channel, "theme": theme,
            "url": f"http://127.0.0.1:8550/iptv/hls-direct/{channel}.ts",
            "addon_id": "org.example.teletext.client", "profile_id": 101 + index,
            "profile_revision": 1, "theme_rgb4": [15, 0, 0] if theme == "day" else [0, 0, 15],
            "generation_id": 301 + index, "request_id": 401 + index,
            "design_id": 201 + index, "design_revision": 3 if theme == "day" else 1,
            "scoped_role": 2, "original_design_id": 201 + index, "original_revision": 1,
            "original_digest": "a" * 64, "root_receipt_id": 501 + index,
            "root_receipt_digest": "b" * 64,
            "original_valid_until": (datetime.now(timezone.utc) + timedelta(minutes=5)).isoformat(),
            "expected_pages": [200, 201], "subcode": 0, "expected_navigation": [200, 201, 200],
            "edited_q_page": 200 if theme == "day" else None,
            "edited_r_page": 200 if theme == "day" else None,
            "edited_q_cell": {"row": 2, "column": 8}, "edited_r_cell": {"row": 3, "column": 8},
        })
    return {
        "probe_version": 2, "scope": "Structural test input only",
        "source": {"file": "source.ts", "bytes": len(body), "sha256": HELPER.digest(body), "duration_seconds": 60},
        "channels": channels, "active_transcode_limit": 4,
        "completion_file": "client-complete.json", "completion_timeout_seconds": 180,
        "expected_completion": {"probe_version": 2, "client_name": "VLC", "client_version": "3.0.20",
                                "decoder_version": "0.2.42", "proof_suffix": "cumulative-effective-live-ordinary"},
        "client_actions": "Owned client actions require independent inspection", "limitations": "No rendering proof",
    }


def legacy_wrapper_request(direct):
    ready = copy.deepcopy(direct)
    ready["probe_version"] = 3
    ready["expected_completion"].update(probe_version=3, proof_suffix="cumulative-effective-legacy-ordinary",
                                        termination="owned-sigterm")
    for channel in ready["channels"]:
        channel["url"] = f"http://127.0.0.1:8550/iptv/channel/{channel['channel_id']}.ts?mode=ts-legacy"
    return ready


class LiveClientManifestTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.ready = effective_request(self.directory)

    def validate(self, request, replay=False):
        return HELPER.validate_readiness(json.dumps(request).encode(), self.directory, replay)

    def test_effective_scope_keeps_actual_ids_and_complete_navigation(self):
        ready, channels, first, next_page, subcode = self.validate(self.ready)
        self.assertEqual((first, next_page, subcode), ("200", "201", 0))
        self.assertEqual([channels[theme]["channel_id"] for theme in ["day", "night"]], [41, 42])
        self.assertEqual(ready["expected_completion"]["proof_suffix"], "cumulative-effective-live-ordinary")
        self.assertEqual(channels["day"]["edited_r_page"], 200)
        self.assertIsNone(channels["night"]["edited_r_page"])

    def test_legacy_live_pages_and_channel_identity_remain_separate(self):
        request = {"probe_version": 1, "channels": [{
            "channel_id": index, "theme": theme,
            "url": f"http://127.0.0.1:8550/iptv/hls-direct/{index}.ts",
            "expected_pages": [124, 125], "subcode": 32, "expected_navigation": [124, 125, 124],
        } for index, theme in [(1, "day"), (2, "night")]]}
        _, channels, first, next_page, subcode = self.validate(request)
        self.assertEqual((first, next_page, subcode), ("124", "125", 32))
        self.assertEqual(channels["night"]["channel_id"], 2)
        request["channels"][0]["expected_pages"] = [200, 201]
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_captured_replay_remains_explicit_and_cannot_accept_live_scope(self):
        source = self.ready["source"]
        request = {"probe_version": 1, "mode": "captured-ordinary-replay", "expected_pages": [200, 201],
                   "subcode": 0, "viewers": [{"viewer": theme,
                   "url": f"http://127.0.0.1:{8560 + index}/synthetic.ts", "capture": {
                       "file": source["file"], "bytes": source["bytes"], "sha256": source["sha256"]}}
                   for index, theme in enumerate(["day", "night"])]}
        _, _, first, next_page, subcode = self.validate(request, replay=True)
        self.assertEqual((first, next_page, subcode), ("200", "201", 0))
        with self.assertRaises(RuntimeError):
            self.validate(self.ready, replay=True)
        with self.assertRaises(RuntimeError):
            self.validate(request, replay=False)

    def test_numeric_vectors_and_completion_cannot_coerce_floats_or_bools(self):
        changes = [
            ("expected_pages", [200.0, 201]), ("expected_navigation", [200, 201.0, 200]),
            ("theme_rgb4", [15, False, 0]), ("scoped_role", 2.0),
            ("edited_q_page", 200.0), ("edited_q_cell", {"row": 2, "column": 8.0}),
            ("channel_id", True), ("profile_revision", 1.0),
        ]
        for key, value in changes:
            with self.subTest(key=key):
                request = copy.deepcopy(self.ready)
                request["channels"][0][key] = value
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = copy.deepcopy(self.ready)
        request["expected_completion"]["probe_version"] = 2.0
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_foreign_routes_and_split_hosts_fail_before_client_start(self):
        for url in ["http://example.test/iptv/hls-direct/41.ts", "http://127.0.0.1:8550/synthetic.ts",
                    "http://127.0.0.1:8550/iptv/hls-direct/42.ts", "http://127.0.0.1:8550/iptv/hls-direct/41.ts?x=1",
                    "http://user@127.0.0.1:8550/iptv/hls-direct/41.ts", "http://127.0.0.1:08550/iptv/hls-direct/41.ts",
                    "http://127.0.0.1:99999/iptv/hls-direct/41.ts", "http://127.0.0.1:8550/iptv/hls-direct/41.ts#page"]:
            with self.subTest(url=url):
                request = copy.deepcopy(self.ready)
                request["channels"][0]["url"] = url
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = copy.deepcopy(self.ready)
        request["channels"][1]["url"] = "http://127.0.0.1:8551/iptv/hls-direct/42.ts"
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_mixed_roles_root_identity_and_page_edit_ownership_reject(self):
        changes = [("theme", "night"), ("scoped_role", 1), ("design_revision", 2),
                   ("original_design_id", 999), ("original_revision", 2), ("addon_id", "bad_id"),
                   ("original_digest", "A" * 64), ("expected_navigation", [200, 200, 200])]
        for key, value in changes:
            with self.subTest(key=key):
                request = copy.deepcopy(self.ready)
                request["channels"][0][key] = value
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = copy.deepcopy(self.ready)
        request["channels"][1]["edited_r_page"] = 200
        with self.assertRaises(RuntimeError):
            self.validate(request)
        request = copy.deepcopy(self.ready)
        request["channels"][1]["profile_id"] = request["channels"][0]["profile_id"]
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_original_expiry_is_not_refreshed_and_naive_or_invalid_dates_reject(self):
        for expiry in ["2020-01-01T12:00:00Z", "2026-99-01T12:00:00Z", "2099-01-01T12:00:00",
                       "2099-01-01T12:00:00+01:00", "2099-01-01T12:00:00.1234567890Z"]:
            with self.subTest(expiry=expiry):
                request = copy.deepcopy(self.ready)
                request["channels"][0]["original_valid_until"] = expiry
                with self.assertRaises(RuntimeError):
                    self.validate(request)

    def test_source_hash_framing_bounds_and_authored_pid_are_verified(self):
        request = copy.deepcopy(self.ready)
        request["source"]["sha256"] = "0" * 64
        with self.assertRaises(RuntimeError):
            self.validate(request)
        request = copy.deepcopy(self.ready)
        request["source"]["bytes"] = HELPER.MAX_LIVE_SOURCE + 1
        with self.assertRaises(RuntimeError):
            self.validate(request)
        body = bytearray((self.directory / "source.ts").read_bytes())
        body[1], body[2] = 2, 0  # Authored PID512 in the purported plain source.
        (self.directory / "source.ts").write_bytes(body)
        request = copy.deepcopy(self.ready)
        request["source"]["sha256"] = HELPER.digest(body)
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_duplicate_and_unknown_readiness_fields_are_closed(self):
        raw = json.dumps(self.ready).replace('"probe_version": 2', '"probe_version": 2, "probe_version": 2', 1)
        with self.assertRaises(RuntimeError):
            HELPER.validate_readiness(raw.encode(), self.directory)
        request = copy.deepcopy(self.ready)
        request["channels"][0]["base_binding_revision"] = 1
        with self.assertRaises(RuntimeError):
            self.validate(request)
        request = copy.deepcopy(self.ready)
        request["expected_completion"]["auto_release"] = True
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_invalid_main_input_never_launches_a_client_or_creates_output(self):
        request = copy.deepcopy(self.ready)
        request["channels"][0]["url"] = "http://127.0.0.1:8550/synthetic.ts"
        ready = self.directory / "ready.json"
        ready.write_text(json.dumps(request))
        output = self.directory / "new-client-evidence"
        with patch.dict(HELPER.os.environ, {"DISPLAY": ":0"}), \
             patch.object(HELPER, "RECEIPTED", {}), \
             patch.object(HELPER.shutil, "which", return_value="unused-existing-tool"), \
             patch.object(HELPER.sys, "argv", [str(SCRIPT), str(ready), str(output)]), \
             patch.object(HELPER.subprocess, "Popen") as client:
            with self.assertRaises(RuntimeError):
                HELPER.main()
            client.assert_not_called()
        self.assertFalse(output.exists())

    def test_wrapper_version_is_distinct_with_same_scope_source_and_navigation(self):
        request = legacy_wrapper_request(self.ready)
        ready, channels, first, next_page, subcode = self.validate(request)
        self.assertEqual((ready["probe_version"], first, next_page, subcode), (3, "200", "201", 0))
        self.assertEqual(ready["expected_completion"]["termination"], "owned-sigterm")
        self.assertEqual(ready["active_transcode_limit"], 4)
        for theme, original in zip(["day", "night"], self.ready["channels"]):
            expected = copy.deepcopy(original)
            expected["url"] = channels[theme]["url"]
            self.assertEqual(channels[theme], expected)
        self.assertEqual(ready["source"], self.ready["source"])
        self.assertEqual(self.validate(self.ready)[0]["probe_version"], 2)

    def test_wrapper_route_accepts_only_generated_legacy_ordinary_owner(self):
        for url in ["http://127.0.0.1:8550/iptv/hls-direct/41.ts",
                    "http://127.0.0.1:8550/iptv/channel/41.ts",
                    "http://127.0.0.1:8550/iptv/channel/41.ts?mode=ts",
                    "http://127.0.0.1:8550/iptv/channel/41.ts?mode=hybrid",
                    "http://127.0.0.1:8550/iptv/channel/41.ts?mode=ts-legacy&viewer=day",
                    "http://127.0.0.1:8550/iptv/channel/42.ts?mode=ts-legacy",
                    "http://127.0.0.1:8550/iptv/channel/41.ts?mode=ts-legacy#page",
                    "http://127.0.0.1:08550/iptv/channel/41.ts?mode=ts-legacy",
                    "http://example.test:8550/iptv/channel/41.ts?mode=ts-legacy"]:
            with self.subTest(url=url):
                request = legacy_wrapper_request(self.ready)
                request["channels"][0]["url"] = url
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = legacy_wrapper_request(self.ready)
        request["channels"][1]["url"] = request["channels"][1]["url"].replace(":8550/", ":8551/")
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_wrapper_completion_is_closed_typed_and_requires_owned_sigterm(self):
        for key, value in [("probe_version", 3.0), ("probe_version", 2), ("termination", "sigterm"),
                           ("termination", "natural-eof"), ("termination", "owned-qt-quit"), ("proof_suffix", "cumulative-effective-live-ordinary"),
                           ("auto_approve", True)]:
            with self.subTest(key=key, value=value):
                request = legacy_wrapper_request(self.ready)
                request["expected_completion"][key] = value
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = legacy_wrapper_request(self.ready)
        del request["expected_completion"]["termination"]
        with self.assertRaises(RuntimeError):
            self.validate(request)
        request = legacy_wrapper_request(self.ready)
        request["active_transcode_limit"] = 2
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_wrapper_cannot_enter_direct_or_replay_contracts_or_bypass_scope_checks(self):
        request = legacy_wrapper_request(self.ready)
        with self.assertRaises(RuntimeError):
            self.validate(request, replay=True)
        request["probe_version"] = 2
        request["expected_completion"] = copy.deepcopy(self.ready["expected_completion"])
        with self.assertRaises(RuntimeError):
            self.validate(request)
        for key, value in [("scoped_role", 1), ("original_revision", 2), ("theme_rgb4", [0, 0, 15]),
                           ("expected_pages", [124, 125]), ("original_valid_until", "2020-01-01T00:00:00Z")]:
            with self.subTest(key=key):
                request = legacy_wrapper_request(self.ready)
                request["channels"][0][key] = value
                with self.assertRaises(RuntimeError):
                    self.validate(request)
        request = legacy_wrapper_request(self.ready)
        request["source"]["sha256"] = "0" * 64
        with self.assertRaises(RuntimeError):
            self.validate(request)

    def test_owned_stop_checks_pid_and_targets_only_its_popen_owner(self):
        for status in [0, -HELPER.signal.SIGTERM]:
            with self.subTest(status=status):
                client = Mock(pid=123)
                client.poll.return_value = None
                client.wait.return_value = status
                owned = {"client": client, "window": 42}
                with patch.object(HELPER, "assert_owned_window") as ownership, \
                     patch.object(HELPER, "remaining", return_value=10) as bound, \
                     patch.object(HELPER, "gui") as gui:
                    self.assertEqual(HELPER.stop_owned_wrapper_client(owned, 100), status)
                    ownership.assert_called_once_with(42, 123, 100)
                    client.terminate.assert_called_once_with()
                    client.wait.assert_called_once_with(timeout=10)
                    bound.assert_called_once_with(100, 10)
                    gui.assert_not_called()
        client = Mock(pid=123)
        client.poll.return_value = None
        owned = {"client": client, "window": 42}
        with patch.object(HELPER, "assert_owned_window", side_effect=RuntimeError("Foreign PID")):
            with self.assertRaises(RuntimeError):
                HELPER.stop_owned_wrapper_client(owned, 100)
        client.terminate.assert_not_called()
        client.poll.return_value = 0
        with patch.object(HELPER, "assert_owned_window") as ownership:
            with self.assertRaises(RuntimeError):
                HELPER.stop_owned_wrapper_client(owned, 100)
            ownership.assert_not_called()
        client.terminate.assert_not_called()

    def test_owned_stop_rejects_unexpected_exit_and_timeout_without_successful_teardown(self):
        for outcome in [-HELPER.signal.SIGKILL, HELPER.subprocess.TimeoutExpired("owned client", 10)]:
            with self.subTest(outcome=outcome):
                client = Mock(pid=123)
                client.poll.return_value = None
                if isinstance(outcome, Exception):
                    client.wait.side_effect = outcome
                else:
                    client.wait.return_value = outcome
                owned = {"client": client, "window": 42}
                with patch.object(HELPER, "assert_owned_window"), \
                     patch.object(HELPER, "remaining", return_value=10), \
                     patch.object(HELPER, "gui") as gui:
                    with self.assertRaises((RuntimeError, HELPER.subprocess.TimeoutExpired)):
                        HELPER.stop_owned_wrapper_client(owned, 100)
                    client.terminate.assert_called_once_with()
                    client.kill.assert_not_called()
                    gui.assert_not_called()


if __name__ == "__main__":
    unittest.main()
