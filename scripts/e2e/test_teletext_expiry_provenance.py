"""Offline owner/window/PES regressions; no native host or client is started.

Run: python3 -m unittest discover -s scripts/e2e -p test_teletext_expiry_provenance.py
"""

import copy
import json
from pathlib import Path
import tempfile
from types import ModuleType
import unittest
import uuid

SCRIPT = Path(__file__).with_name("teletext-expiry-provenance.py")
H = ModuleType("expiry_provenance")
H.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), H.__dict__)

EXPIRY = "2026-10-14T10:00:01.000000000+00:00"


def utc(nanoseconds):
    return f"2026-10-14T10:00:{nanoseconds // 1_000_000_000:02d}.{nanoseconds % 1_000_000_000:09d}Z"


def unit(row, page=200, c4=False, marker=None):
    address = 2 | row << 3
    result = bytearray([2, 44, 0xe0, 0xe4,
                        H.REVERSE[H.HAM[address & 15]], H.REVERSE[H.HAM[address >> 4]]])
    if row == 0:
        number = 0xff if page is None else page - 200
        subcode = 0
        header = [number & 15, number >> 4, subcode & 15,
                  (subcode >> 4) & 7 | (8 if c4 else 0),
                  (subcode >> 8) & 15, (subcode >> 12) & 3, 0, 1]
        if page is None:
            header = [15, 15, 0, 0, 0, 0, 1, 1]
        result.extend(H.REVERSE[H.HAM[value]] for value in header)
        result.extend([H.REVERSE[0x20]] * 32)
    elif 1 <= row <= 25:
        cells = bytearray(b" " * 40)
        if marker is not None:
            cells[8] = ord(marker)
        result.extend(H.REVERSE[value | (0x80 if value.bit_count() % 2 == 0 else 0)] for value in cells)
    else:
        result.extend(bytes(40))
    return bytes(result)


def pes(units, packet_index, padded=None):
    padded = padded if padded is not None else len(units) + (3 - len(units)) % 4
    payload = b"".join(units) + (b"\xff\x2c" + b"\xff" * 44) * (padded - len(units))
    size = 46 + len(payload)
    require_multiple = size % 184 == 0
    if not require_multiple:
        raise AssertionError("test PES must occupy whole TS payloads")
    header = b"\x00\x00\x01\xbd" + (size - 6).to_bytes(2, "big") + b"\x84\x80\x24\x21\x00\x01\x00\x01" + b"\xff" * 31 + b"\x10"
    body = header + payload
    return b"".join(bytes([0x47, 0x42 if index == 0 else 0x02, 0, 0x10 | ((packet_index + index) % 16)])
                    + body[offset:offset + 184] for index, offset in enumerate(range(0, len(body), 184)))


def capture(scoped, withdraw=False, absent_windows=1):
    output = bytearray()
    for cycle in range(4):
        scoped_cycle = scoped and cycle == 1
        start = len(output) // 188 - (absent_windows if scoped and withdraw and cycle >= 2 else 0)
        batches = [[unit(29)], [unit(0, 200, c4=cycle != 3)],
                   [unit(2, marker="Q" if scoped_cycle else None), unit(3)],
                   [unit(0, 201)], [unit(0, None)]]
        for offset, batch in enumerate(batches):
            output.extend(pes(batch, start + offset))
        if scoped and withdraw and cycle == 1:
            output.extend((b"\x47\x01\x00\x10" + bytes(184)) * absent_windows)
    return bytes(output)


def fixture(directory, withdraw=False, absent_windows=1):
    lines = []
    def emit(event, fields, time=None, span=None):
        payload = {"event": event, **fields}
        if time is not None:
            payload["observed_utc"] = utc(time)
        fields_text = " ".join(f"{key}={json.dumps(value, separators=(',', ':'))}" for key, value in payload.items())
        prefix = f"teletext_native_trial_request{{viewer_index={span[0]} request_trace={span[1]}}}: " if span else ""
        lines.append(f"   1.000000000s DEBUG {prefix}{H.TARGET} synthetic event {fields_text}\n")

    for viewer_index, (viewer, channel) in enumerate(H.VIEWERS):
        request, intent, slot, epoch = (str(uuid.UUID(int=100 * group + viewer_index + 1)) for group in range(1, 5))
        raw = capture(channel == 1, withdraw, absent_windows)
        (directory / f"{viewer}-ordinary.ts").write_bytes(raw)
        emit("ordinary_live_intent", {"intent_id": intent, "channel_id": channel}, span=(viewer_index, request))
        emit("monitor_initial", {"proposal_slot": slot, "channel_id": channel, "generation_id": 2 if channel == 1 else 3}, time=0)
        emit("monitor_owner_link", {"intent_id": intent, "proposal_slot": slot, "channel_id": channel})
        for packet in range(len(raw) // 188):
            if channel == 1 and 5 <= packet < 10:
                sequence = 2
            elif channel == 1 and withdraw and 10 <= packet < 10 + absent_windows:
                sequence = 3
            elif channel == 1 and packet >= 10:
                sequence = 4
            else:
                sequence = 0
            prepared_ns = packet * 10_000_000 if packet < 5 else 990_000_000 if packet == 5 else 1_000_000_000 + (packet - 5) * 10_000_000
            common = {"proposal_slot": slot, "output_epoch": epoch, "active_sequence": sequence, "first_packet": packet}
            emit("sink_window_prepared", {**common, "end_packet": packet + 1, "authored_selected":sequence != 3}, time=prepared_ns)
            emit("sink_window_first_packet_written", {**common, "first_packet_sha256": H.digest(raw[packet * 188:(packet + 1) * 188])}, time=prepared_ns + 1_000_000)
            if channel == 1 and packet == 4:
                emit("monitor_recovered", {"proposal_slot": slot, "channel_id": 1, "generation_id": 4, "original_valid_until": EXPIRY}, time=prepared_ns + 1_100_000)
                emit("monitor_proposal", {"proposal_slot": slot, "channel_id": 1, "sequence": 2, "action": "replace", "generation_id": 4, "design_id": 4, "revision": 2}, time=prepared_ns + 1_200_000)
            if channel == 1 and packet == 9:
                emit("scope_boundary_expired", {"proposal_slot": slot, "generation_id": 4}, time=prepared_ns + 1_100_000)
                emit("monitor_proposal", {"proposal_slot": slot, "channel_id": 1, "sequence": 3 if withdraw else 4,
                                         "action":"withdraw" if withdraw else "replace",
                                         **({} if withdraw else {"generation_id":2})}, time=prepared_ns + 1_200_000)
            if channel == 1 and withdraw and packet == 10 + absent_windows - 1:
                emit("monitor_proposal", {"proposal_slot":slot,"channel_id":1,"sequence":4,"action":"replace","generation_id":2}, time=prepared_ns + 1_200_000)
            logical_packet = packet - absent_windows if channel == 1 and withdraw and packet >= 10 + absent_windows else packet
            absent = channel == 1 and withdraw and 10 <= packet < 10 + absent_windows
            complete = not absent and logical_packet % 5 == 4
            ack = {**common, "end_packet": packet + 1, "accepted_elapsed_ns": prepared_ns + 2_000_000,
                   "cycle_boundary": complete or absent, "complete_cycle": complete,
                   "signalling_announced": absent and packet == 10,"authored_selected":not absent}
            emit("sink_window_flushed", ack, time=prepared_ns + 2_000_000)
            if channel == 1 and (packet in (4, 9) or (absent and packet == 10 + absent_windows - 1)):
                installed = {key: value for key, value in ack.items() if key not in ("active_sequence", "first_packet")}
                installed.update(sequence=2 if packet == 4 else 3 if withdraw and packet == 9 else 4,
                                 action="withdraw" if withdraw and packet == 9 else "replace",
                                 accepted_elapsed_ns=prepared_ns + 2_020_000)
                emit("transition_installed", installed, time=prepared_ns + 3_000_000)
    expiry = {"probe_version": 1, "scoped_generation_id": 4, "scoped_design_id": 4, "scoped_revision": 2,
              "base_generation_ids": [2, 3], "original_valid_until": EXPIRY,
              "observed_utc": utc(1_005_000_000),
              "source_requests_per_viewer": 1, "immutable_scope_and_publication_state_across_expiry": True,
              "expiry_offsets": [6 * 188, 6 * 188, 6 * 188]}
    (directory / "stream-terminal.log").write_text("".join(lines))
    (directory / "expiry.json").write_text(json.dumps(expiry))
    return lines


class ExpiryProvenanceTests(unittest.TestCase):
    def test_explicit_owner_join_accepts_identical_startup_hashes_and_preprepared_scoped_suffix(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture(directory)
            report = H.qualify(directory)
            self.assertEqual([viewer["viewer"] for viewer in report["viewers"]], ["day", "observer", "night"])
            self.assertFalse(report["expiry_grace_added"])
            for viewer in report["viewers"][:2]:
                self.assertEqual(viewer["qualified_ack_cut_bytes"], 10 * 188)
                self.assertEqual(viewer["first_written_matches"], 20)
                self.assertEqual(viewer["restored_complete_cycles"], 2)
                cycle = viewer["scoped_cycles"][0]
                self.assertEqual(cycle["first_pes_packet"], 5)
                self.assertEqual(cycle["first200_packet"], 6)
                self.assertLess(cycle["prepared_utc_ns"], H.utc_ns(EXPIRY))
                self.assertTrue(viewer["first_restored200_c4"])
            self.assertIsNone(report["viewers"][2]["qualified_ack_cut_bytes"])

    def test_actual_withdraw_complete_ack_then_absent_psi_ack_qualifies_base_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture(directory, withdraw=True)
            report = H.qualify(directory)
            for viewer in report["viewers"][:2]:
                self.assertEqual(viewer["ack_path"], "scoped_complete_ack_then_absent_psi_ack_then_base")
                self.assertEqual(viewer["scoped_ack_cut_packet"],10)
                self.assertEqual(viewer["qualified_ack_cut_packet"],11)
                self.assertEqual(viewer["restored_complete_cycles"],2)
            original = (directory / "stream-terminal.log").read_text()
            # A boundary boolean alone does not establish physically absent PSI.
            (directory / "stream-terminal.log").write_text(original.replace('"signalling_announced":true','"signalling_announced":false')
                                                           .replace('signalling_announced=true','signalling_announced=false'))
            with self.assertRaisesRegex(H.ProofError,"preceding PSI ACK"):
                H.qualify(directory)

    def test_base_recovery_after_multiple_absent_windows_uses_preceding_psi_ack(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture(directory, withdraw=True, absent_windows=3)
            report = H.qualify(directory)
            for viewer in report["viewers"][:2]:
                self.assertEqual(viewer["scoped_ack_cut_packet"], 10)
                self.assertEqual(viewer["qualified_ack_cut_packet"], 13)
                self.assertEqual(viewer["ack_path"], "scoped_complete_ack_then_absent_psi_ack_then_base")
            original = (directory / "stream-terminal.log").read_text()
            (directory / "stream-terminal.log").write_text(original.replace('signalling_announced=true', 'signalling_announced=false'))
            with self.assertRaisesRegex(H.ProofError, "preceding PSI ACK"):
                H.qualify(directory)

    def test_terminal_base_prefix_is_retained_hashed_and_never_claimed_flushed_or_complete(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture(directory)
            def append_tail(marker=None):
                tail = pes([unit(29),unit(0,200),unit(2,marker=marker)],20,padded=7)[:188]
                original = capture(True)
                (directory / "day-ordinary.ts").write_bytes(original+tail)
                slot,epoch = str(uuid.UUID(int=301)),str(uuid.UUID(int=401))
                common = {"proposal_slot":slot,"output_epoch":epoch,"active_sequence":4,"first_packet":20}
                extra=[]
                for event,values,time in [
                    ("sink_window_prepared",{**common,"end_packet":22,"authored_selected":True},2_000_000_000),
                    ("sink_window_first_packet_written",{**common,"first_packet_sha256":H.digest(tail)},2_001_000_000),
                ]:
                    fields={"event":event,**values,"observed_utc":utc(time)}
                    extra.append("DEBUG "+H.TARGET+" tail "+" ".join(f"{key}={json.dumps(value)}" for key,value in fields.items())+"\n")
                return extra
            original_log=(directory / "stream-terminal.log").read_text()
            (directory / "stream-terminal.log").write_text(original_log+"".join(append_tail()))
            report=H.qualify(directory)
            day=report["viewers"][0]
            self.assertEqual(day["capture_bytes"],21*188)
            self.assertEqual(day["retainer_tail_bytes"],188)
            self.assertEqual(day["first_written_matches"],21)
            self.assertEqual(day["flushed_windows"],20)
            self.assertIsNotNone(day["partial_pes"])
            self.assertIsNotNone(day["partial_base_cycle"])
            self.assertFalse(day["full_capture_eof_claimed"])
            self.assertFalse(day["terminal_window_flushed_in_log"])
            (directory / "stream-terminal.log").write_text(original_log+"".join(append_tail("Q")))
            with self.assertRaisesRegex(H.ProofError,"old Q/R marker"):
                H.qualify(directory)

    def test_owner_collision_duplicate_request_and_disagreeing_span_fields_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            lines = fixture(directory)
            original = "".join(lines)
            for changed in [original.replace(str(uuid.UUID(int=102)), str(uuid.UUID(int=101))),
                            original + lines[0],
                            original.replace('channel_id=1', 'channel_id=1 viewer_index=2', 1)]:
                (directory / "stream-terminal.log").write_text(changed)
                with self.assertRaises(H.ProofError):
                    H.qualify(directory)

    def test_assigned_capture_hash_wrong_offset_and_foreign_ack_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            lines = fixture(directory)
            path = directory / "day-ordinary.ts"
            raw = path.read_bytes()
            changed = bytearray(raw)
            changed[100] ^= 1
            path.write_bytes(changed)
            with self.assertRaisesRegex(H.ProofError, "exact unadjusted offset"):
                H.qualify(directory)
            path.write_bytes(b"\x47\x01\x00\x10" + bytes(184) + raw)
            with self.assertRaises(H.ProofError):
                H.qualify(directory)
            path.write_bytes(raw)
            mutated = [line.replace('end_packet=10', 'end_packet=11')
                       if '"transition_installed"' in line and 'sequence=4' in line else line for line in lines]
            (directory / "stream-terminal.log").write_text("".join(mutated))
            with self.assertRaises(H.ProofError):
                H.qualify(directory)

    def test_log_cap_partial_terminal_line_duplicate_fields_and_precision_loss_reject(self):
        with self.assertRaises(H.ProofError):
            H.parse_log(b" " * H.MAX_LOG)
        with self.assertRaises(H.ProofError):
            H.parse_log((H.TARGET + ' event="monitor_initial" observed_utc=' + EXPIRY).encode())
        with self.assertRaises(H.ProofError):
            H.parse_log((H.TARGET + ' event="monitor_initial" proposal_slot=' + str(uuid.UUID(int=1))
                         + ' proposal_slot=' + str(uuid.UUID(int=2)) + ' observed_utc=' + EXPIRY + '\n').encode())
        with self.assertRaises(H.ProofError):
            H.utc_ns("2026-10-14T10:00:00.1234567891Z")
        self.assertEqual(H.utc_ns("2026-10-14T10:00:00.123456789Z") + 1,
                         H.utc_ns("2026-10-14T10:00:00.123456790+00:00"))

    def test_expired_new_cycle_and_late_scoped_installation_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            lines = fixture(directory)
            original = "".join(lines)
            changed = original.replace('2026-10-14T10:00:00.990000000Z', '2026-10-14T10:00:01.000000000Z')\
                .replace('2026-10-14T10:00:00.991000000Z','2026-10-14T10:00:01.001000000Z')\
                .replace('2026-10-14T10:00:00.992000000Z','2026-10-14T10:00:01.002000000Z')
            (directory / "stream-terminal.log").write_text(changed)
            with self.assertRaisesRegex(H.ProofError, "new Scoped logical cycle"):
                H.qualify(directory)
            changed = [line.replace('2026-10-14T10:00:00.043000000Z', '2026-10-14T10:00:01.000000000Z')
                       if '"transition_installed"' in line and 'sequence=2' in line else line for line in lines]
            (directory / "stream-terminal.log").write_text("".join(changed))
            with self.assertRaisesRegex(H.ProofError, "installation before immutable expiry"):
                H.qualify(directory)

    def test_strict_pusi_cc_pes_length_and_ack_cut_crossing_reject(self):
        foreign_ff = bytearray(unit(0, None))
        foreign_ff[8] = H.REVERSE[H.HAM[15]]
        with self.assertRaisesRegex(H.ProofError, "resource or foreign page/subpage"):
            H.decode_capture(pes([bytes(foreign_ff)], 0))
        raw = pes([unit(0)], 0, padded=7)
        packets, units, _ = H.decode_capture(raw)
        windows = [{"first": 0, "end": 2, "sequence": 2}]
        transition = {"base_install": {"end_packet": 1}, "scoped_sequence": 2, "base_sequence": 4}
        with self.assertRaisesRegex(H.ProofError, "PES crosses qualified ACK cut"):
            H.cycle_proof({}, windows, packets, units, transition, {"original_valid_until": EXPIRY})
        for position, mask in [(188 + 1, 0x40), (188 + 3, 1), (9, 1)]:
            changed = bytearray(raw)
            changed[position] ^= mask
            with self.assertRaises(H.ProofError):
                H.decode_capture(changed)
        with self.assertRaises(H.ProofError):
            H.decode_capture(raw[:-188])

    def test_post_cut_q_marker_or_missing_first_restored_erase_fence_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fixture(directory)
            raw = (directory / "day-ordinary.ts").read_bytes()
            events = H.parse_log((directory / "stream-terminal.log").read_bytes())
            expiry = json.loads((directory / "expiry.json").read_text())
            owner = H.owners(events, expiry)[0]
            windows, _ = H.qualified_windows(owner, raw)
            transition = H.transitions(owner, windows, expiry)
            packets, units, _ = H.decode_capture(raw)
            for marker in ("Q", "R"):
                changed = copy.deepcopy(units)
                record = next(unit for unit in changed if unit["pes_first"] == 12 and unit["row"] == (2 if marker == "Q" else 3))
                cells = bytearray(record["cells"])
                cells[8] = ord(marker)
                record["cells"] = bytes(cells)
                with self.assertRaisesRegex(H.ProofError, "old Q/R marker"):
                    H.cycle_proof(owner, windows, packets, changed, transition, expiry)
            changed = copy.deepcopy(units)
            next(unit for unit in changed if unit["pes_first"] == 11 and unit["row"] == 0)["c4"] = False
            with self.assertRaisesRegex(H.ProofError, "C4 erasure"):
                H.cycle_proof(owner, windows, packets, changed, transition, expiry)


if __name__ == "__main__":
    unittest.main()
