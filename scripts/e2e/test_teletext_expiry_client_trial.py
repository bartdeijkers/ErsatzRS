"""Closed Off-expiry observation guards; no host or client is started.

Run: python3 -m unittest discover -s scripts/e2e -p test_teletext_expiry_client_trial.py
"""

import copy
from pathlib import Path
import runpy
import unittest

H = runpy.run_path(str(Path(__file__).with_name("teletext-expiry-client-trial.py")))
DIGEST = "a" * 64


def fixture():
    channels = {}
    for channel, theme, colour in [(1, "day", [15, 0, 0]), (2, "night", [0, 0, 15])]:
        channels[theme] = {"channel_id": channel, "addon_id": "test.fixture.teletext",
                           "base_generation_id": 10 + channel, "base_request_id": 20 + channel,
                           "base_design_id": 30 + channel, "base_design_revision": 1,
                           "base_digest": DIGEST, "theme_rgb4": colour}

    def base(theme):
        original = channels[theme]
        return {"channel_id": original["channel_id"], "document_version": 1,
                "generation_id": original["base_generation_id"], "request_id": original["base_request_id"],
                "design_id": original["base_design_id"], "design_revision": 1, "design_digest": DIGEST,
                "teletext_visible": True, "edited_q_present": False, "theme_rgb4": original["theme_rgb4"]}

    def stage(number, time):
        return {"probe_version": 2, "stage": number, "state": H["OFF_STATES"][number - 1],
                "scope": "Synthetic validator fixture, no runtime evidence", "snapshot_utc": time,
                "day": base("day"), "night": base("night"), "scoped": None,
                "expected_pages": [200, 201], "subcode": 0, "wire_offsets": [188, 376, 564]}

    first = stage(1, "2026-10-18T10:00:00Z")
    absent = stage(2, "2026-10-18T10:00:05Z")
    absent["scoped"] = {"design_id": 50, "revision": 1, "digest": DIGEST, "generation_id": 60,
                        "request_id": 70, "playout_id": 93, "addon_id": channels["day"]["addon_id"],
                        "start": "2026-10-18T10:00:01Z", "finish": "2026-10-18T10:00:36Z",
                        "state": "off", "producer_count": 0, "owner": None, "has_editorial_overlay": False}
    absent["day"].update({"generation_id": 60, "request_id": 70, "design_id": 50,
                          "document_version": 2, "teletext_visible": False, "theme_rgb4": None,
                          "owner": None, "producer_count": 0})
    final = stage(3, "2026-10-18T10:00:37Z")
    final["scoped"] = copy.deepcopy(absent["scoped"])
    return channels, first, absent, final


class OffExpiryProtocol(unittest.TestCase):
    def test_genuine_absence_shape_and_immutable_base_restoration(self):
        channels, first, absent, final = fixture()
        previous = None
        for number, value in enumerate([first, absent, final], 1):
            previous = H["stage"](value, number, channels, previous, True)

    def test_off_cannot_claim_on_editorial_or_rendered_authority(self):
        channels, first, absent, _ = fixture()
        mutations = [("scoped", "root_receipt_id", 1), ("scoped", "owner", 1),
                     ("scoped", "producer_count", False), ("scoped", "has_editorial_overlay", True),
                     ("scoped", "addon_id", "other.fixture.teletext"),
                     ("day", "owner", 1), ("day", "producer_count", 1),
                     ("day", "teletext_visible", True), ("day", "theme_rgb4", [15, 0, 0]),
                     ("day", "edited_q_present", True), ("day", "design_revision", 2)]
        for group, key, value in mutations:
            with self.subTest(group=group, key=key):
                invalid = copy.deepcopy(absent)
                invalid[group][key] = value
                with self.assertRaises(RuntimeError):
                    H["stage"](invalid, 2, channels, first, True)

    def test_original_scope_cannot_be_renewed_or_restored_early(self):
        channels, _, absent, final = fixture()
        for change in ["early", "renew", "wrong_base", "wrong_version"]:
            with self.subTest(change=change):
                invalid = copy.deepcopy(final)
                if change == "early":
                    invalid["snapshot_utc"] = "2026-10-18T10:00:35Z"
                elif change == "renew":
                    invalid["scoped"]["start"] = "2026-10-18T10:00:02Z"
                    invalid["scoped"]["finish"] = "2026-10-18T10:00:37Z"
                elif change == "wrong_base":
                    invalid["day"]["generation_id"] += 1
                else:
                    invalid["probe_version"] = 1
                with self.assertRaises(RuntimeError):
                    H["stage"](invalid, 3, channels, absent, True)

    def test_off_is_not_accepted_under_existing_on_protocol(self):
        channels, first, absent, _ = fixture()
        first["probe_version"] = 1
        with self.assertRaises(RuntimeError):
            H["stage"](absent, 2, channels, first)


if __name__ == "__main__":
    unittest.main()
