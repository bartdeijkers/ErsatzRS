"""Pure warm-client protocol tests; these start no client, GUI or native actor.

Run: python3 -m unittest discover -s scripts/e2e -p test_teletext_live_update_client_trial.py
"""

import copy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
from types import ModuleType
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).with_name("teletext-live-update-client-trial.py")
HELPER = ModuleType("warm_client_contract")
HELPER.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), HELPER.__dict__)


def readiness(directory):
    # These two packets test framing only and do not allege playable video.
    body = b"\x47\x01\x00\x10" + bytes(184) + b"\x47\x01\x01\x10" + bytes(184)
    (directory / "source.ts").write_bytes(body)
    channels = []
    expiry = (datetime.now(timezone.utc) + timedelta(minutes=5)).isoformat()
    for index, theme in enumerate(["day", "night"]):
        channel = 41 + index
        channels.append({
            "channel_id": channel, "theme": theme,
            "url": f"http://127.0.0.1:8550/iptv/hls-direct/{channel}.ts?teletext_test_viewer={theme}",
            "addon_id": "org.example.teletext.client", "profile_id": 101 + index, "profile_revision": 1,
            "theme_rgb4": [15, 0, 0] if theme == "day" else [0, 0, 15],
            "generation_id": 301 + index, "request_id": 401 + index,
            "design_id": 201 + index, "design_revision": 2 if theme == "day" else 1,
            "scoped_role": 2, "original_design_id": 201 + index, "original_revision": 1,
            "original_digest": "a" * 64, "root_receipt_id": 501 + index, "root_receipt_digest": "b" * 64,
            "original_valid_until": expiry, "expected_pages": [200, 201], "subcode": 0,
            "expected_navigation": [200, 201, 200], "edited_q_page": 200 if theme == "day" else None,
            "edited_r_page": None, "edited_q_cell": {"row": 2, "column": 8}, "edited_r_cell": {"row": 3, "column": 8},
        })
    return {"probe_version": 2, "protocol": "warm-scoped-live-ordinary-v2", "completion_timeout_seconds": 180,
            "source": {"file": "source.ts", "bytes": len(body), "sha256": HELPER.digest(body), "duration_seconds": 60},
            "channels": channels, "expected_pages": [200, 201], "subcode": 0,
            "stages": ["q", "cumulative_qr", "lua_off"], "stage_files": "stage-N-ready.json and stage-N-captured.json, N from 1 through 3",
            "final_completion": {"probe_version": 2, "client_name": "VLC", "client_version": "3.0.20",
                                 "decoder_version": "0.2.42", "proof_suffix": "warm-scoped-live-ordinary"},
            "limitations": "Structural test fixture, no rendering certification"}


def stage(channels, number, previous=None):
    result = {"probe_version": 2, "stage": number, "state": HELPER.STATES[number - 1],
              "scope": "Wire-observed, no visual certification", "expected_pages": [200, 201], "subcode": 0,
              "wire_offsets": [188, 376, 564]}
    for theme, initial in channels.items():
        keys = HELPER.STAGE_CHANNEL_KEYS - {"design_digest", "valid_until", "teletext_visible", "edited_q_present", "edited_r_present"}
        item = {key: copy.deepcopy(initial[key]) for key in keys}
        item.update(design_digest="c" * 64, valid_until=initial["original_valid_until"], teletext_visible=True,
                    edited_q_present=theme == "day", edited_r_present=False)
        if theme == "night" and previous:
            item = copy.deepcopy(previous[theme])
        elif theme == "day" and number == 2:
            item.update(design_revision=3, design_digest="d" * 64, generation_id=601, request_id=701,
                        edited_r_present=True)
        elif theme == "day" and number == 3:
            item.update(design_id=801, design_revision=1, design_digest="e" * 64, generation_id=901, request_id=1001,
                        original_design_id=801, original_revision=1, original_digest="e" * 64,
                        root_receipt_id=None, root_receipt_digest=None, teletext_visible=False,
                        edited_q_present=False, edited_r_present=False)
        result[theme] = item
    return result


class WarmClientContractTests(unittest.TestCase):
    def test_pinned_cold_helper_matches_the_actual_checked_in_source(self):
        self.assertEqual(HELPER.digest(HELPER.HELPERS.read_bytes()), HELPER.COLD_HELPER_SHA256)

    def test_replacement_reselects_existing_day_decoder_without_activating_off(self):
        clients = [{"viewer": "day"}, {"viewer": "night"}]
        cases = [
            (2, 1, clients), (2, 2, [clients[0]]), (2, 3, []),
            (1, 1, [clients[1]]), (1, 2, [clients[0]]),
            (1, 3, [clients[0]]), (1, 4, []), (1, 5, [clients[0]]),
        ]
        for version, stage, expected in cases:
            with self.subTest(version=version, stage=stage), patch.object(HELPER, "activate") as activate:
                HELPER.activate_stage(clients, version, stage, 123)
                self.assertEqual([call.args[0] for call in activate.call_args_list], expected)
                self.assertTrue(all(call.args[1] == 123 for call in activate.call_args_list))

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.ready = readiness(self.directory)
        self.channels = HELPER.scoped_ready(self.ready, self.directory)
        self.first = stage(self.channels, 1)
        self.second = stage(self.channels, 2, self.first)
        self.off = stage(self.channels, 3, self.second)

    def test_actual_ids_three_epochs_cumulative_root_and_genuine_absence(self):
        first = HELPER.validate_stage(self.first, 1, self.channels, None)
        second = HELPER.validate_stage(self.second, 2, self.channels, first)
        absent = HELPER.validate_stage(self.off, 3, self.channels, second)
        self.assertEqual(second["day"]["original_valid_until"], first["day"]["original_valid_until"])
        self.assertEqual(second["night"], first["night"])
        self.assertEqual(absent["day"]["original_design_id"], 801)
        self.assertIsNone(absent["day"]["root_receipt_id"])
        self.assertFalse(absent["day"]["teletext_visible"])
        self.assertEqual([self.channels[t]["channel_id"] for t in ["day", "night"]], [41, 42])

    def test_legacy_five_stage_url_contract_remains_available(self):
        ready = {"probe_version": 1, "channels": [
            {"channel_id": index, "theme": theme,
             "url": f"http://127.0.0.1:8550/iptv/hls-direct/{index}.ts?teletext_test_viewer={theme}"}
            for index, theme in [(1, "day"), (2, "night")]]}
        self.assertEqual(set(HELPER.urls(ready)), {"day", "night"})
        path = self.directory / "legacy.json"
        path.write_text(json.dumps(ready))
        self.assertEqual(HELPER.load(path)[1], ready)

    def test_closed_numeric_pages_roles_and_duplicate_json_reject(self):
        for key, value in [("channel_id", True), ("scoped_role", 2.0), ("subcode", False),
                           ("expected_pages", [200.0, 201]), ("theme_rgb4", [15, 0, False]),
                           ("expected_navigation", [200, 201, 201]), ("design_revision", 3)]:
            with self.subTest(key=key):
                changed = copy.deepcopy(self.ready)
                changed["channels"][0][key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.scoped_ready(changed, self.directory)
        changed = copy.deepcopy(self.ready)
        changed["final_completion"]["probe_version"] = 2.0
        with self.assertRaises(RuntimeError):
            HELPER.scoped_ready(changed, self.directory)
        path = self.directory / "duplicate.json"
        path.write_text(json.dumps(self.ready).replace('"probe_version": 2', '"probe_version": 2, "probe_version": 2', 1))
        with self.assertRaises(RuntimeError):
            HELPER.load(path)
        changed = copy.deepcopy(self.ready)
        changed["channels"][0]["base_binding_revision"] = 1
        with self.assertRaises(RuntimeError):
            HELPER.scoped_ready(changed, self.directory)

    def test_original_expiry_profile_isolation_and_root_identity_cannot_drift(self):
        for key, value in [("original_design_id", 999), ("original_digest", "A" * 64),
                           ("original_valid_until", "2020-01-01T00:00:00Z"),
                           ("original_valid_until", "2099-01-01T00:00:00"), ("profile_id", 102)]:
            with self.subTest(key=key, value=value):
                changed = copy.deepcopy(self.ready)
                changed["channels"][0][key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.scoped_ready(changed, self.directory)

    def test_queried_ordinary_origin_is_exact_and_same_host(self):
        for url in ["http://127.0.0.1:8550/synthetic.ts?teletext_test_viewer=day",
                    "http://127.0.0.1:8550/iptv/hls-direct/42.ts?teletext_test_viewer=day",
                    "http://user@127.0.0.1:8550/iptv/hls-direct/41.ts?teletext_test_viewer=day",
                    "http://127.0.0.1:08550/iptv/hls-direct/41.ts?teletext_test_viewer=day",
                    "http://127.0.0.1:8550/iptv/hls-direct/41.ts?teletext_test_viewer=night",
                    "http://127.0.0.1:8550/iptv/hls-direct/41.ts?teletext_test_viewer=day&x=1"]:
            with self.subTest(url=url):
                changed = copy.deepcopy(self.ready)
                changed["channels"][0]["url"] = url
                with self.assertRaises(RuntimeError):
                    HELPER.scoped_ready(changed, self.directory)
        changed = copy.deepcopy(self.ready)
        changed["channels"][1]["url"] = changed["channels"][1]["url"].replace(":8550/", ":8551/")
        with self.assertRaises(RuntimeError):
            HELPER.scoped_ready(changed, self.directory)

    def test_source_cap_hash_duration_framing_and_authored_pid_fail(self):
        for key, value in [("bytes", HELPER.H["MAX_LIVE_SOURCE"] + 1), ("sha256", "0" * 64),
                           ("duration_seconds", 60.0), ("file", "../source.ts")]:
            with self.subTest(key=key):
                changed = copy.deepcopy(self.ready)
                changed["source"][key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.scoped_ready(changed, self.directory)
        body = bytearray((self.directory / "source.ts").read_bytes())
        body[1], body[2] = 2, 0
        (self.directory / "source.ts").write_bytes(body)
        changed = copy.deepcopy(self.ready)
        changed["source"]["sha256"] = HELPER.digest(body)
        with self.assertRaises(RuntimeError):
            HELPER.scoped_ready(changed, self.directory)

    def test_stage_epoch_content_and_current_generation_are_fenced(self):
        for key, value in [("stage", True), ("state", "q"), ("subcode", 32), ("wire_offsets", [1, False, 3])]:
            with self.subTest(key=key):
                changed = copy.deepcopy(self.second)
                changed[key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.validate_stage(changed, 2, self.channels, self.first)
        with self.assertRaises(RuntimeError):
            HELPER.validate_stage(self.second, 2, self.channels, None)
        for key, value in [("root_receipt_id", 999), ("original_valid_until", "2099-01-01T00:00:00Z"),
                           ("generation_id", self.first["day"]["generation_id"]), ("edited_r_present", False),
                           ("teletext_visible", 1)]:
            with self.subTest(key=key):
                changed = copy.deepcopy(self.second)
                changed["day"][key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.validate_stage(changed, 2, self.channels, self.first)
        changed = copy.deepcopy(self.second)
        changed["night"]["generation_id"] += 1
        with self.assertRaises(RuntimeError):
            HELPER.validate_stage(changed, 2, self.channels, self.first)

    def test_off_cannot_impersonate_previous_root_or_keep_authored_pages(self):
        for key, value in [("root_receipt_id", 501), ("edited_q_present", True), ("teletext_visible", True),
                           ("original_design_id", 201), ("design_id", 201), ("generation_id", 601)]:
            with self.subTest(key=key):
                changed = copy.deepcopy(self.off)
                changed["day"][key] = value
                with self.assertRaises(RuntimeError):
                    HELPER.validate_stage(changed, 3, self.channels, self.second)

    def test_one_original_deadline_is_capped_without_stage_renewal(self):
        with patch.object(HELPER.time, "monotonic", return_value=110):
            self.assertEqual(HELPER.scoped_deadline(self.ready, self.channels, 100), 280)
        channels = copy.deepcopy(self.channels)
        expiry = (datetime.now(timezone.utc) + timedelta(seconds=10)).isoformat()
        channels["day"]["original_valid_until"] = expiry
        with patch.object(HELPER.time, "monotonic", return_value=110):
            deadline = HELPER.scoped_deadline(self.ready, channels, 100)
        self.assertGreater(deadline, 119)
        self.assertLessEqual(deadline, 120)

    def test_pid_window_fence_prevents_foreign_observation(self):
        client = {"window": 42, "process": Mock(pid=123)}
        with patch.object(HELPER, "owned_window", side_effect=RuntimeError("Wrong owned PID")), \
             patch.object(HELPER, "command") as observation:
            with self.assertRaises(RuntimeError):
                HELPER.origin(client, 100)
            observation.assert_not_called()

    def test_invalid_initial_route_never_starts_client_or_creates_evidence(self):
        changed = copy.deepcopy(self.ready)
        changed["channels"][0]["url"] = "http://127.0.0.1:8550/synthetic.ts"
        path = self.directory / "ready.json"
        path.write_text(json.dumps(changed))
        with patch.dict(HELPER.os.environ, {"DISPLAY": ":0"}), \
             patch.dict(HELPER.H, {"RECEIPTED": {}}), \
             patch.object(HELPER.sys, "argv", [str(SCRIPT), str(path)]), \
             patch.object(HELPER.subprocess, "Popen") as start:
            with self.assertRaises(RuntimeError):
                HELPER.main()
            start.assert_not_called()
        self.assertFalse((self.directory / "headed-warm-trial.ndjson").exists())



def drcs_readiness(directory):
    source = readiness(directory)["source"]
    channels = []
    for index, theme in enumerate(["day", "night"]):
        channels.append({"channel_id":index+1,"theme":theme,
            "url":f"http://127.0.0.1:8550/iptv/hls-direct/{index+1}.ts?teletext_test_viewer={theme}",
            "addon_id":"org.example.teletext.client","profile_id":101+index,"profile_revision":1,
            "theme_rgb4":[15,0,0] if theme=="day" else [0,0,15],"design_id":201+index,
            "design_revision":1,"design_digest":("a" if index==0 else "b")*64,
            "generation_id":301+index,"request_id":401+index})
    return {"probe_version":3,"protocol":"warm-drcs-live-ordinary-v1","completion_timeout_seconds":55,
        "source":source,"channels":channels,"editor_original":{"design_id":201,"revision":1,
            "digest":"a"*64,"receipt_id":501,"receipt_digest":"c"*64},
        "resource":{"kind":"drcs","magazine":1,"page":192,"logical_subpage":0,"slot":0,
            "original_first_bitmap_row":2049,"edited_first_bitmap_row":2051},
        "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
        "stages":["original_bitmap","edited_bitmap"],
        "stage_files":"stage-N-ready.json and stage-N-captured.json, N from 1 through 2",
        "client_stop_file":"clients-stopped.json","limitations":"MAIN only; root review remains required"}


def drcs_stage(channels, number):
    day = copy.deepcopy(channels["day"])
    if number==2:
        day.update({"design_revision":2,"design_digest":"d"*64,"generation_id":601,"request_id":701})
    return {"probe_version":3,"stage":number,"state":HELPER.DRCS_STATES[number-1],
        "scope":"Actual source units observed; visual review remains pending","initial_ready_sha256":"1"*64,
        "day":day,"night":copy.deepcopy(channels["night"]),"day_first_bitmap_row":2049 if number==1 else 2051,
        "night_first_bitmap_row":2049,"day_s2":number,"night_s2":1,"expected_pages":[124,125],
        "subcode":32,"expected_navigation":[124,125,124],"wire_offsets":[100*number,200*number,300*number]}


class DrcsWarmClientContractTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory=Path(self.temporary.name)
        self.ready=drcs_readiness(self.directory)
        self.channels=HELPER.drcs_ready(self.ready,self.directory)
        self.first=drcs_stage(self.channels,1)
        self.second=drcs_stage(self.channels,2)

    def validate(self, value, number=2, previous=None):
        return HELPER.validate_drcs_stage(value,number,self.channels,
            self.first if previous is None else previous,"1"*64)

    def test_closed_two_stage_drcs_original_and_new_publication(self):
        self.assertEqual(HELPER.drcs_ready(self.ready,self.directory),self.channels)
        self.assertEqual(HELPER.validate_drcs_stage(self.first,1,self.channels,None,"1"*64),self.first)
        self.assertEqual(self.validate(self.second),self.second)

    def test_drcs_ready_rejects_wrong_modes_resources_geometry_and_deadline(self):
        for key,value in [("probe_version",True),("protocol","warm-scoped-live-ordinary-v2"),
                ("completion_timeout_seconds",56),("expected_navigation",[124,124,125]),("subcode",0)]:
            with self.subTest(key=key):
                changed=copy.deepcopy(self.ready);changed[key]=value
                with self.assertRaises(RuntimeError):HELPER.drcs_ready(changed,self.directory)
        for key,value in [("kind","gdrcs"),("logical_subpage",True),("slot",1),("edited_first_bitmap_row",2049)]:
            with self.subTest(resource=key):
                changed=copy.deepcopy(self.ready);changed["resource"][key]=value
                with self.assertRaises(RuntimeError):HELPER.drcs_ready(changed,self.directory)
        changed=copy.deepcopy(self.ready);changed["extra"]=True
        with self.assertRaises(RuntimeError):HELPER.drcs_ready(changed,self.directory)

    def test_drcs_ready_rejects_receipt_cross_channel_and_profile_drift(self):
        for section,key,value in [("editor_original","design_id",202),("editor_original","digest","b"*64),
                ("editor_original","receipt_digest","A"*64),("source","sha256","0"*64)]:
            changed=copy.deepcopy(self.ready);changed[section][key]=value
            with self.subTest(section=section,key=key),self.assertRaises(RuntimeError):
                HELPER.drcs_ready(changed,self.directory)
        for key,value in [("profile_id",102),("theme_rgb4",[0,0,15]),
                ("url","http://127.0.0.1:8550/iptv/hls-direct/2.ts?teletext_test_viewer=day")]:
            changed=copy.deepcopy(self.ready);changed["channels"][0][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):HELPER.drcs_ready(changed,self.directory)

    def test_drcs_stage_rejects_stale_epoch_counter_bitmap_and_foreign_readiness(self):
        for key,value in [("stage",True),("state","original_bitmap"),("initial_ready_sha256","2"*64),
                ("day_s2",1),("night_s2",2),("day_first_bitmap_row",2049),("night_first_bitmap_row",2051),
                ("wire_offsets",self.first["wire_offsets"]),("expected_navigation",[124,125])]:
            changed=copy.deepcopy(self.second);changed[key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):self.validate(changed)
        with self.assertRaises(RuntimeError):
            HELPER.validate_drcs_stage(self.second,2,self.channels,None,"1"*64)

    def test_drcs_stage_rejects_reused_or_cross_channel_publication_and_scope(self):
        for theme,key,value in [("day","generation_id",301),("day","request_id",401),
                ("day","generation_id",302),("day","design_digest","a"*64),
                ("day","profile_id",102),("day","channel_id",True),("day","theme_rgb4",[15,False,0]),
                ("night","design_revision",2),("night","generation_id",602)]:
            changed=copy.deepcopy(self.second);changed[theme][key]=value
            with self.subTest(theme=theme,key=key),self.assertRaises(RuntimeError):self.validate(changed)
        changed=copy.deepcopy(self.first);changed["day"]["design_revision"]=True
        with self.assertRaises(RuntimeError):HELPER.validate_drcs_stage(changed,1,self.channels,None,"1"*64)

    def decoder_client(self, text):
        file=self.directory/"decoder.log";file.write_text(text)
        process=Mock(pid=123);process.poll.return_value=None
        log=Mock();log.name=str(file)
        return {"process":process,"log":log},file

    def test_drcs_decoder_retains_one_actual_module_identity(self):
        line='[00007abc1234] main decoder debug: using spu decoder module "zvbi"\n'
        client,file=self.decoder_client(line)
        self.assertEqual(HELPER.drcs_decoder_identity(client),"00007abc1234")
        self.assertEqual(HELPER.drcs_decoder_identity(client),"00007abc1234")
        for suffix in [line,'[00007fffffff] main decoder debug: using spu decoder module "zvbi"\n',
                '[00007abc1234] main decoder debug: removing module "zvbi"\n']:
            file.write_text(line+suffix)
            with self.subTest(suffix=suffix),self.assertRaises(RuntimeError):HELPER.drcs_decoder_identity(client)
        file.write_text(line);client["drcs_decoder_identity"]="ffffffff"
        with self.assertRaises(RuntimeError):HELPER.drcs_decoder_identity(client)
        client["process"].poll.return_value=0
        with self.assertRaises(RuntimeError):HELPER.drcs_decoder_identity(client)

    def test_drcs_second_stage_never_reactivates_decoder(self):
        clients=[Mock(),Mock()]
        with patch.object(HELPER,"activate") as activate,patch.object(HELPER,"drcs_decoder_identity") as retained:
            HELPER.activate_stage(clients,3,2,123)
            activate.assert_not_called()
            self.assertEqual(retained.call_count,2)
        with patch.object(HELPER,"activate") as activate,patch.object(HELPER,"drcs_decoder_identity",side_effect=RuntimeError("reset")):
            with self.assertRaises(RuntimeError):HELPER.activate_stage(clients,3,2,123)
            activate.assert_not_called()



def presentation_readiness(directory):
    ready = drcs_readiness(directory)
    ready.update(probe_version=4, protocol="warm-page-presentation-bank-v1",
                 stages=["original_m29", "published_page_x28_bank"], bank_logical_subpages=[0, 1])
    return ready


def presentation_stage(channels, number):
    value = drcs_stage(channels, number)
    value.update(probe_version=4, state=HELPER.PAGE_PRESENTATION_STATES[number - 1],
                 editor_original=None, day_presentation_source="magazine" if number == 1 else "page",
                 night_presentation_source="magazine")
    if number == 2:
        value["day"]["design_id"] = 801
        value["editor_original"] = {"design_id": 801, "revision": 1, "digest": "e" * 64,
                                    "receipt_id": 901, "receipt_digest": "f" * 64}
    return value


class PagePresentationWarmClientTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.ready = presentation_readiness(self.directory)
        self.channels = HELPER.page_presentation_ready(self.ready, self.directory)
        self.first = presentation_stage(self.channels, 1)
        self.second = presentation_stage(self.channels, 2)

    def validate(self, value, number=2, previous=None):
        return HELPER.validate_page_presentation_stage(value, number, self.channels,
            self.first if previous is None else previous, "1" * 64, self.ready["editor_original"])

    def test_new_original_receipt_and_concept_publish_without_changing_night(self):
        self.assertEqual(HELPER.page_presentation_ready(self.ready, self.directory), self.channels)
        self.assertEqual(HELPER.validate_page_presentation_stage(self.first, 1, self.channels, None,
            "1" * 64, self.ready["editor_original"]), self.first)
        self.assertEqual(self.validate(self.second), self.second)
        self.assertNotEqual(self.second["day"]["design_id"], self.first["day"]["design_id"])
        self.assertEqual(self.second["night"], self.first["night"])
        with self.assertRaises(RuntimeError):
            HELPER.drcs_ready(self.ready, self.directory)
        with self.assertRaises(RuntimeError):
            HELPER.page_presentation_ready(drcs_readiness(self.directory), self.directory)

    def test_explicit_mode_banks_resources_and_closed_ready_are_required(self):
        changes = [("probe_version", True), ("probe_version", 4.0), ("protocol", "warm-drcs-live-ordinary-v1"),
                   ("stages", HELPER.DRCS_STATES), ("completion_timeout_seconds", 56),
                   ("bank_logical_subpages", [False, 1]), ("bank_logical_subpages", [0, 1.0]),
                   ("bank_logical_subpages", [1, 0]), ("bank_logical_subpages", [0]),
                   ("expected_navigation", [124, 124, 125])]
        for key, value in changes:
            changed = copy.deepcopy(self.ready); changed[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                HELPER.page_presentation_ready(changed, self.directory)
        for key, value in [("logical_subpage", True), ("page", 0x1c0), ("slot", 1), ("edited_first_bitmap_row", 0x801)]:
            changed = copy.deepcopy(self.ready); changed["resource"][key] = value
            with self.subTest(resource=key), self.assertRaises(RuntimeError):
                HELPER.page_presentation_ready(changed, self.directory)
        changed = copy.deepcopy(self.ready); changed["unexpected"] = True
        with self.assertRaises(RuntimeError): HELPER.page_presentation_ready(changed, self.directory)
        path = self.directory / "ready4.json"; path.write_text(json.dumps(self.ready))
        self.assertEqual(HELPER.load(path)[1], self.ready)

    def test_new_original_and_receipt_cannot_reuse_stale_or_foreign_identity(self):
        changes = [("design_id", 201), ("design_id", 202), ("revision", True), ("revision", 2),
                   ("digest", "a" * 64), ("digest", "b" * 64), ("digest", "d" * 64),
                   ("digest", "E" * 64), ("receipt_id", 501), ("receipt_id", True),
                   ("receipt_digest", "c" * 64), ("receipt_digest", "F" * 64)]
        for key, value in changes:
            changed = copy.deepcopy(self.second); changed["editor_original"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError): self.validate(changed)
        changed = copy.deepcopy(self.second); changed["editor_original"]["extra"] = False
        with self.assertRaises(RuntimeError): self.validate(changed)
        changed = copy.deepcopy(self.first); changed["editor_original"] = self.ready["editor_original"]
        with self.assertRaises(RuntimeError): self.validate(changed, 1)

    def test_publication_palette_profile_and_every_wire_offset_remain_isolated(self):
        changes = [("day", "design_id", 201), ("day", "design_id", 202), ("day", "design_revision", True),
                   ("day", "generation_id", 301), ("day", "generation_id", 302), ("day", "request_id", 401),
                   ("day", "request_id", 402), ("day", "design_digest", "a" * 64), ("day", "profile_id", 102),
                   ("day", "url", self.channels["night"]["url"]), ("day", "theme_rgb4", [15, False, 0]),
                   ("night", "design_id", 801), ("night", "generation_id", 902)]
        for theme, key, value in changes:
            changed = copy.deepcopy(self.second); changed[theme][key] = value
            with self.subTest(theme=theme, key=key), self.assertRaises(RuntimeError): self.validate(changed)
        for index in range(3):
            changed = copy.deepcopy(self.second); changed["wire_offsets"][index] = self.first["wire_offsets"][index]
            with self.subTest(offset=index), self.assertRaises(RuntimeError): self.validate(changed)
        changed = copy.deepcopy(self.second); changed["wire_offsets"][2] = True
        with self.assertRaises(RuntimeError): self.validate(changed)

    def test_stage_presentation_epoch_and_original_capture_cannot_drift(self):
        for key, value in [("probe_version", 3), ("stage", True), ("state", "edited_bitmap"),
                           ("day_presentation_source", "magazine"), ("night_presentation_source", "page"),
                           ("initial_ready_sha256", "2" * 64), ("day_s2", True), ("night_s2", 2),
                           ("day_first_bitmap_row", 0x801), ("night_first_bitmap_row", 0x803)]:
            changed = copy.deepcopy(self.second); changed[key] = value
            with self.subTest(key=key), self.assertRaises(RuntimeError): self.validate(changed)
        changed = copy.deepcopy(self.second); changed["extra"] = None
        with self.assertRaises(RuntimeError): self.validate(changed)
        for previous in (None, {**self.first, "stage": True}, {**self.first, "day_presentation_source": "page"}):
            with self.subTest(previous=previous), self.assertRaises(RuntimeError):
                HELPER.validate_page_presentation_stage(self.second, 2, self.channels, previous,
                    "1" * 64, self.ready["editor_original"])

    def test_version4_keeps_existing_one_decoder_activation_safeguards(self):
        clients = [Mock(), Mock()]
        for stage in (1, 2):
            with patch.object(HELPER, "activate") as activate, patch.object(HELPER, "drcs_decoder_identity") as identity:
                HELPER.activate_stage(clients, 4, stage, 123)
                self.assertEqual(activate.call_count, 2 if stage == 1 else 0)
                self.assertEqual(identity.call_count, 2)
        with patch.object(HELPER, "activate") as activate, patch.object(HELPER, "drcs_decoder_identity", side_effect=RuntimeError("reset")):
            with self.assertRaisesRegex(RuntimeError, "reset"): HELPER.activate_stage(clients, 4, 2, 123)
            activate.assert_not_called()

    def test_stock_mapping_uses_exact_owned_pid_and_receipted_files(self):
        verify = Mock(return_value={"verified": True})
        client = {"process": Mock(pid=123)}
        self.assertEqual(HELPER.stock_decoder_mapping(client, {"verify_process": verify}), {"verified": True})
        pid, decoder, plugin, plugin_sha = verify.call_args.args
        self.assertEqual(pid, 123)
        self.assertEqual(decoder, {"path": "/usr/lib/x86_64-linux-gnu/libzvbi.so.0.13.2",
            "sha256": "134705a7ef4d982eb57bf71a9378f3eb0d95237c8dcb686d684e35471ae834b0",
            "label": "Ubuntu libzvbi 0.2.42-2", "test_only": False})
        self.assertEqual(str(plugin), "/usr/lib/x86_64-linux-gnu/vlc/plugins/codec/libzvbi_plugin.so")
        self.assertEqual(plugin_sha, "b87f42fb841588fe68c0845f4cd56dd1062b287eb92faa5f9a086e5ce5fefd00")
        verify.side_effect = RuntimeError("wrong owned mapping")
        with self.assertRaisesRegex(RuntimeError, "wrong owned mapping"):
            HELPER.stock_decoder_mapping(client, {"verify_process": verify})

    def test_qualification_authenticates_source_before_loading(self):
        sources = {}
        qualification = HELPER.stock_mapping_qualification(sources)
        self.assertIn("verify_process", qualification)
        self.assertEqual(sources, {"scripts/e2e/teletext-client-decoder-candidate.py":
            "f1acfc1ab867e67b2ca3e1eecb26e1289e0192c62b9f78b383adec6832a0d4ec"})
        with patch.object(HELPER, "bounded_bytes", return_value=b"raise AssertionError('must not execute')"):
            with self.assertRaisesRegex(RuntimeError, "helper source changed"):
                HELPER.stock_mapping_qualification({})

    def test_actual_mapper_rejects_alternate_duplicate_or_replaced_files(self):
        verify = HELPER.stock_mapping_qualification({})["verify_process"]
        library = self.directory / "libzvbi.so.0.13.2"
        plugin = self.directory / "libzvbi_plugin.so"
        alternate = self.directory / "alternate" / library.name
        alternate.parent.mkdir()
        library.write_bytes(b"stock library"); alternate.write_bytes(b"stock library")
        plugin.write_bytes(b"stock plugin")
        proc = self.directory / "proc"; (proc / "123").mkdir(parents=True)
        def row(path):
            stat = path.stat()
            return f"1000-2000 r-xp 0000 {HELPER.os.major(stat.st_dev):x}:{HELPER.os.minor(stat.st_dev):x} {stat.st_ino} {path}\n"
        expected = {"path": str(library), "sha256": HELPER.digest(library.read_bytes())}
        plugin_sha = HELPER.digest(plugin.read_bytes())
        good = row(library) + row(plugin)
        maps = proc / "123" / "maps"; maps.write_text(good)
        report = verify(123, expected, plugin, plugin_sha, proc)
        self.assertTrue(report["exact_mapped_paths_and_file_identities_verified"])
        self.assertEqual(report["client_pid"], 123)
        bad_identity = good.replace(str(library.stat().st_ino), "0", 1)
        for body in (row(alternate) + row(plugin), good + row(alternate), row(library), bad_identity):
            maps.write_text(body)
            with self.subTest(body=body), self.assertRaises(ValueError):
                verify(123, expected, plugin, plugin_sha, proc)
        maps.write_text(good); library.write_bytes(b"replacement")
        with self.assertRaisesRegex(ValueError, "decoder content changed"):
            verify(123, expected, plugin, plugin_sha, proc)
        library.write_bytes(b"stock library"); plugin.write_bytes(b"replacement")
        with self.assertRaisesRegex(ValueError, "plugin content changed"):
            verify(123, expected, plugin, plugin_sha, proc)
        with self.assertRaisesRegex(ValueError, "positive client PID"):
            verify(True, expected, plugin, plugin_sha, proc)

    def test_main_rejects_bad_mapping_or_replaced_instance_before_any_capture(self):
        for failure in ("mapping", "instance"):
            directory = self.directory / failure; directory.mkdir()
            ready = presentation_readiness(directory)
            path = directory / "ready4.json"; raw = json.dumps(ready).encode(); path.write_bytes(raw)
            channels = HELPER.page_presentation_ready(ready, directory)
            first = presentation_stage(channels, 1); first["initial_ready_sha256"] = HELPER.digest(raw)
            clients = []
            def client(viewer, url, _directory, index, _deadline, first_page):
                process = Mock(pid=100 + index); process.poll.return_value = None
                process.terminate.side_effect = lambda: setattr(process.poll, "return_value", -HELPER.signal.SIGTERM)
                value = {"viewer": viewer, "url": url, "window": 1000 + index, "process": process, "log": Mock()}
                clients.append(value)
                return value
            with self.subTest(failure=failure), \
                    patch.dict(HELPER.os.environ, {"DISPLAY": ":0"}), patch.dict(HELPER.H, {"RECEIPTED": {}}), \
                    patch.object(HELPER.sys, "argv", [str(SCRIPT), str(path)]), \
                    patch.object(HELPER, "wait_stage", return_value=(json.dumps(first).encode(), first)), \
                    patch.object(HELPER, "start_client", side_effect=client), patch.object(HELPER, "activate"), \
                    patch.object(HELPER, "drcs_decoder_identity", side_effect=RuntimeError("replaced instance") if failure == "instance" else None), \
                    patch.object(HELPER, "stock_decoder_mapping", side_effect=ValueError("wrong mapping")) as mapped, \
                    patch.object(HELPER, "capture") as capture, \
                    patch.object(HELPER.sys, "stdout", __import__("io").StringIO()):
                with self.assertRaisesRegex((RuntimeError, ValueError), "replaced instance|wrong mapping"):
                    HELPER.main()
            capture.assert_not_called()
            self.assertEqual(mapped.call_count, 0 if failure == "instance" else 1)
            self.assertFalse((directory / "stage-1-captured.json").exists())
            self.assertFalse((directory / "clients-stopped.json").exists())
            self.assertTrue(all(item["process"].terminate.call_count == 1 for item in clients))
            self.assertTrue(all(item["log"].close.call_count == 1 for item in clients))

    def test_main_version4_keeps_two_stages_navigation_and_owned_stop_markers(self):
        path = self.directory / "ready4.json"
        raw = json.dumps(self.ready).encode(); path.write_bytes(raw)
        for stage in (self.first, self.second): stage["initial_ready_sha256"] = HELPER.digest(raw)
        clients, calls = [], []
        def client(viewer, url, _directory, index, _deadline, first_page):
            process = Mock(pid=100 + index); process.poll.return_value = None
            process.wait.return_value = -HELPER.signal.SIGTERM
            def terminate():
                self.assertEqual(calls[-1], ("mapping", viewer))
                calls.append(("stop", viewer))
                process.poll.return_value = -HELPER.signal.SIGTERM
            process.terminate.side_effect = terminate
            value = {"viewer": viewer, "url": url, "window": 1000 + index, "process": process, "log": Mock()}
            clients.append(value)
            return value
        def image(client, _directory, stage, page, visit, _deadline):
            self.assertEqual(calls[-1], ("mapping", client["viewer"]))
            calls.append(("capture", client["viewer"]))
            return {"viewer": client["viewer"], "page": page, "file": f"stage-{stage}-{client['viewer']}-{visit}.png", "sha256": "a" * 64}
        def mapping(client, qualification):
            self.assertIn("verify_process", qualification)
            calls.append(("mapping", client["viewer"]))
            return {"client_pid": client["process"].pid, "exact_mapped_paths_and_file_identities_verified": True}
        stages = [(json.dumps(value).encode(), value) for value in (self.first, self.second)]
        with patch.dict(HELPER.os.environ, {"DISPLAY": ":0"}), patch.dict(HELPER.H, {"RECEIPTED": {}}), \
                patch.object(HELPER.sys, "argv", [str(SCRIPT), str(path)]), \
                patch.object(HELPER, "wait_stage", side_effect=stages), patch.object(HELPER, "start_client", side_effect=client), \
                patch.object(HELPER, "settle"), patch.object(HELPER, "activate") as activate, \
                patch.object(HELPER, "drcs_decoder_identity", return_value="retained-decoder") as retained, \
                patch.object(HELPER, "stock_decoder_mapping", side_effect=mapping) as mapped, \
                patch.object(HELPER, "owned_window"), patch.object(HELPER, "origin", return_value={"x": 0, "y": 0}), \
                patch.object(HELPER, "gui") as gui, patch.object(HELPER, "capture", side_effect=image), \
                patch.object(HELPER.sys, "stdout", __import__("io").StringIO()):
            HELPER.main()
        self.assertEqual(activate.call_count, 2)
        self.assertEqual(retained.call_count, 18)
        self.assertEqual(mapped.call_count, 18)
        self.assertEqual(gui.call_count, 8)
        events = [json.loads(line) for line in (self.directory / "headed-warm-trial.ndjson").read_text().splitlines()]
        mappings = [event for event in events if event["event"] == "decoder_mapping"]
        self.assertEqual(len(mappings), 18)
        self.assertEqual([event["phase"] for event in mappings].count("activated"), 4)
        self.assertEqual([event["phase"] for event in mappings].count("before_capture"), 12)
        self.assertEqual([event["phase"] for event in mappings].count("before_stop"), 2)
        self.assertTrue(all(event["mapping"]["client_pid"] == (100 if event["viewer"] == "day" else 101) for event in mappings))
        for number in (1, 2):
            marker = json.loads((self.directory / f"stage-{number}-captured.json").read_bytes())
            self.assertEqual(marker["probe_version"], 4)
            self.assertTrue(all(set(image) == {"viewer", "page", "file", "sha256"} for image in marker["screenshots"]))
            self.assertEqual([image["page"] for image in marker["screenshots"]], [124, 124, 125, 125, 124, 124])
        stopped = json.loads((self.directory / "clients-stopped.json").read_bytes())
        self.assertEqual(stopped["probe_version"], 4)
        self.assertTrue(all(item["decoder_starts"] == 1 and item["decoder_stops_before_shutdown"] == 0 for item in stopped["clients"]))
        self.assertFalse((self.directory / "client-complete.json").exists())
        self.assertTrue(all(item["process"].terminate.call_count == 1 for item in clients))

if __name__ == "__main__":
    unittest.main()
