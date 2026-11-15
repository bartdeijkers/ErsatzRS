"""Offline negative controls for the ordinary guide/PMT boundary qualifier."""

import copy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
from types import ModuleType
import unittest
import uuid

SCRIPT = Path(__file__).with_name("teletext-guide-boundary.py")
G = ModuleType("guide_boundary")
G.__file__ = str(SCRIPT)
exec(compile(SCRIPT.read_bytes(), str(SCRIPT), "exec"), G.__dict__)


def utc(ns):
    instant = datetime(2026, 11, 13, tzinfo=timezone.utc) + timedelta(seconds=ns // 1_000_000_000)
    return instant.strftime("%Y-%m-%dT%H:%M:%S") + f".{ns % 1_000_000_000:09d}Z"


def fixture(withdraw=True, startup=False):
    intent, slot, epoch = [str(uuid.UUID(int=n)) for n in (1, 2, 3)]
    raw = b"".join(bytes([0x47, 0x02 if n in (2, 3, 7, 8) else 0x01, 0, 0x10]) + bytes(184) for n in range(14))
    events = []
    def emit(ns, kind, **fields):
        events.append((ns, {"event": kind, **fields, "observed_utc": utc(ns)}))
    emit(0, "ordinary_live_intent", intent_id=intent, channel_id=1)
    emit(1_000_000, "monitor_owner_link", intent_id=intent, proposal_slot=slot, channel_id=1)
    identity = dict(channel_id=1, generation_id=10, request_id=20, design_id=30, revision=1)
    guide = dict(proposal_slot=slot, **identity, authored_selected=True, original_valid_until=None, guide_seed_id=40)
    emit(2_000_000, "monitor_initial", **guide, guide_runtime_id=None, guide_valid_until=utc(1_000_000_000))
    emit(1_000_000_000, "scope_boundary_expired", proposal_slot=slot, generation_id=10)
    emit(1_000_010_000, "monitor_proposal", proposal_slot=slot, sequence=1, action="withdraw")
    emit(1_001_200_000 if withdraw else 1_000_030_000, "monitor_recovered", **guide, guide_runtime_id=50, guide_valid_until=utc(61_000_000_000))
    emit(1_001_300_000 if withdraw else 1_000_040_000, "monitor_proposal", proposal_slot=slot, **identity, sequence=2, action="replace")
    for n in range(14):
        prepared = (100_000_000 + n * 100_000_000 if n < 3 else 900_000_000 if n == 3 else
                    1_000_300_000 if n == 4 else 1_001_700_000 if n == 5 else 1_010_000_000 + (n - 6) * 10_000_000)
        flushed = 1_000_100_000 if n == 3 else prepared + 200_000
        sequence = 2 if n >= (6 if withdraw else 4) else 1 if withdraw and n >= 4 else 0
        absent = withdraw and n in (4, 5)
        common = dict(proposal_slot=slot, output_epoch=epoch, active_sequence=sequence, first_packet=n)
        flags = dict(cycle_boundary=n in (3, 4, 5, 10), complete_cycle=n in (3, 10), signalling_announced=absent)
        emit(prepared, "sink_window_prepared", **common, end_packet=n + 1)
        emit(prepared + 100_000, "sink_window_first_packet_written", **common,
             first_packet_sha256=G.H.digest(raw[n * 188:(n + 1) * 188]))
        emit(flushed, "sink_window_flushed", **common, end_packet=n + 1,
             accepted_elapsed_ns=flushed, authored_selected=not absent, **flags)
        if n in ((3, 5) if withdraw else (3,)):
            emit(flushed + 100_000, "transition_installed", proposal_slot=slot, output_epoch=epoch,
                 sequence=1 if withdraw and n == 3 else 2, action="withdraw" if withdraw and n == 3 else "replace",
                 end_packet=n + 1, accepted_elapsed_ns=flushed + 100_000, **flags)
        if n in ((4, 10) if withdraw else (10,)):
            emit(flushed + 100_000, "transition_applied", proposal_slot=slot, output_epoch=epoch,
                 sequence=1 if n == 4 else 2, action="withdraw" if n == 4 else "replace",
                 end_packet=n + 1, accepted_elapsed_ns=flushed + 100_000, **flags)
    if startup:
        for _, fields in events:
            if fields["event"] == "monitor_initial":
                fields["event"] = "monitor_recovered"
            if "sequence" in fields:
                fields["sequence"] += 1
            if "active_sequence" in fields and fields["first_packet"] != 0:
                fields["active_sequence"] += 1
            if fields.get("first_packet") == 0 and fields["event"] == "sink_window_flushed":
                fields.update(cycle_boundary=True, complete_cycle=False, signalling_announced=True, authored_selected=False)
            if fields.get("first_packet") == 2 and fields["event"] == "sink_window_flushed":
                fields.update(cycle_boundary=True, complete_cycle=True)
        emit(3_000_000, "monitor_proposal", proposal_slot=slot, **identity, sequence=1, action="replace")
        emit(100_300_000, "transition_installed", proposal_slot=slot, output_epoch=epoch, sequence=1,
             action="replace", end_packet=1, accepted_elapsed_ns=100_300_000,
             cycle_boundary=True, complete_cycle=False, signalling_announced=True)
        emit(300_300_000, "transition_applied", proposal_slot=slot, output_epoch=epoch, sequence=1,
             action="replace", end_packet=3, accepted_elapsed_ns=300_300_000,
             cycle_boundary=True, complete_cycle=True, signalling_announced=False)
    lines = []
    for _, fields in sorted(events, key=lambda row: row[0]):
        encoded = " ".join(key + "=" + ("None" if value is None else json.dumps(value, separators=(',', ':')))
                           for key, value in fields.items())
        lines.append(G.H.TARGET + " " + encoded + "\n")
    log = "".join(lines).encode()
    ready = next(index + 1 for index, line in enumerate(lines) if 'event="sink_window_first_packet_written"' in line)
    request = {"channel_id": 1, "log_start": 0, "log_first_chunk": len("".join(lines[:ready]).encode()), "log_end": len(log)}
    av = [{"#name": "component", "elementary_pid": n, "stream_type": kind} for n, kind in ((256, 27), (257, 15), (258, 6))]
    authored = {"#name": "component", "elementary_pid": 512, "stream_type": 6}
    tables, sections = [], []
    for version, first, present in ([(1, 0, True), (2, 4, False), (3, 6, True)] if withdraw else [(1, 0, True)]):
        if startup and version == 1:
            first = 1
        tables.append({"#name": "PMT", "version": version, "current": True, "service_id": 1, "pcr_pid": 256,
                       "#nodes": [{"#name": "metadata", "pid": 4096, "first_ts_packet": first, "last_ts_packet": first}]
                       + copy.deepcopy(av) + ([copy.deepcopy(authored)] if present else [])})
        sections.append(f"Packet {first} to {first}, PID 0x1000, TID 0x02, TIDext 0x0001, V{version}, Sec 0/0: E1 00\n")
    if startup:
        tables.insert(0, {"#name": "PMT", "version": 0, "current": True, "service_id": 1, "pcr_pid": 256,
                          "#nodes": [{"#name": "metadata", "pid": 4096, "first_ts_packet": 0, "last_ts_packet": 0}] + copy.deepcopy(av)})
        sections.insert(0, "Packet 0 to 0, PID 0x1000, TID 0x02, TIDext 0x0001, V0, Sec 0/0: E1 00\n")
    metadata = [{"channel_id": 1, "seed_id": 40, "generation_id": 10, "runtime_id": runtime, "valid_until": utc(until)}
                for runtime, until in ((None, 1_000_000_000), (50, 61_000_000_000))]
    return [raw, log, request, tables, "".join(sections), metadata]


def terminal_withdrawal(values):
    line = next(line for line in values[1].splitlines(keepends=True)
                if b'event="monitor_proposal"' in line and b'action="withdraw"' in line)
    line = line.replace(b'sequence=1 ', b'sequence=3 ').replace(utc(1_000_010_000).encode(), utc(2_000_000_000).encode())
    values[1] += line
    values[2]["log_end"] = len(values[1])


def repeated_observation(values, runtime=50):
    lines = values[1].splitlines(keepends=True)
    index = next(i for i, line in enumerate(lines) if b'event="monitor_recovered"' in line)
    duplicate = lines[index].replace(b'guide_runtime_id=50 ', f'guide_runtime_id={runtime} '.encode())
    duplicate = duplicate.replace(utc(1_001_200_000).encode(), utc(1_001_210_000).encode())
    lines.insert(index + 1, duplicate)
    values[1] = b"".join(lines)
    values[2]["log_end"] = len(values[1])


def effective_scope(values, until=100_000_000_000):
    previous = values[1]
    old = b'original_valid_until=None'
    new = b'original_valid_until=' + json.dumps(utc(until)).encode()
    values[1] = previous.replace(old, new)
    values[2]["log_first_chunk"] = len(previous[:values[2]["log_first_chunk"]].replace(old, new))
    values[2]["log_end"] = len(values[1])
    for row in values[5]:
        row["original_valid_until"] = utc(until)


class GuideBoundaryTests(unittest.TestCase):
    def test_effective_guide_keeps_exact_original_cap_and_all_boundary_checks(self):
        values = fixture()
        effective_scope(values)
        result = G.qualify(*values)
        self.assertEqual(result["capture_sha256"], G.H.digest(values[0]))
        self.assertEqual(result["original_lua_valid_until"], utc(100_000_000_000))
        self.assertFalse(result["caption_timing_waived"])

    def test_effective_runtime_cannot_extend_original_lua_cap(self):
        values = fixture()
        effective_scope(values, 60_000_000_000)
        with self.assertRaisesRegex(G.H.ProofError, "exceeds its original Lua scope"):
            G.qualify(*values)

    def test_recovered_effective_output_cannot_substitute_a_new_lua_cap(self):
        values = fixture()
        effective_scope(values)
        values[1] = b"".join(line.replace(utc(100_000_000_000).encode(), utc(101_000_000_000).encode())
            if b'event="monitor_recovered"' in line else line for line in values[1].splitlines(keepends=True))
        values[2]["log_end"] = len(values[1])
        with self.assertRaisesRegex(G.H.ProofError, "differs from its saved original"):
            G.qualify(*values)

    def test_effective_cap_requires_saved_metadata_and_base_keeps_absence(self):
        values = fixture()
        effective_scope(values)
        for row in values[5]:
            del row["original_valid_until"]
        with self.assertRaisesRegex(G.H.ProofError, "Base guide unexpectedly"):
            G.qualify(*values)
        values = fixture()
        for row in values[5]:
            row["original_valid_until"] = utc(100_000_000_000)
        with self.assertRaisesRegex(G.H.ProofError, "UTC timestamp is not a string"):
            G.qualify(*values)

    def test_repeated_observations_keep_one_exact_fresh_runtime_identity(self):
        values = fixture()
        repeated_observation(values)
        result = G.qualify(*values)
        self.assertEqual(result["fresh_runtime_observations"], 2)
        self.assertFalse(result["caption_timing_waived"])

    def test_repeated_observations_cannot_hide_a_different_runtime(self):
        values = fixture()
        repeated_observation(values, runtime=51)
        with self.assertRaisesRegex(G.H.ProofError, "one fresh runtime identity"):
            G.qualify(*values)

    def test_terminal_undelivered_withdrawal_is_retained_outside_packet_scope(self):
        values = fixture()
        terminal_withdrawal(values)
        result = G.qualify(*values)
        self.assertEqual(result["capture_sha256"], G.H.digest(values[0]))
        self.assertEqual([event["sequence"] for event in result["terminal_undelivered_proposals"]], [3])
        self.assertFalse(result["caption_timing_waived"])

    def test_terminal_withdrawal_cannot_have_any_delivery_event(self):
        values = fixture()
        terminal_withdrawal(values)
        line = next(line for line in values[1].splitlines(keepends=True) if b'event="transition_installed"' in line)
        values[1] += line.replace(b'sequence=1 ', b'sequence=3 ')
        values[2]["log_end"] = len(values[1])
        with self.assertRaisesRegex(G.H.ProofError, "terminal withdrawal has delivery"):
            G.qualify(*values)

    def test_terminal_proposal_sequences_remain_positive_and_unique(self):
        for sequence in (0, 2):
            with self.subTest(sequence=sequence):
                values = fixture()
                terminal_withdrawal(values)
                lines = values[1].splitlines(keepends=True)
                lines[-1] = lines[-1].replace(b'sequence=3 ', f'sequence={sequence} '.encode())
                values[1] = b"".join(lines)
                with self.assertRaisesRegex(G.H.ProofError, "proposal sequences are ambiguous"):
                    G.qualify(*values)

    def test_terminal_log_order_cannot_hide_a_preflush_utc_proposal(self):
        values = fixture()
        terminal_withdrawal(values)
        values[1] = values[1].replace(utc(2_000_000_000).encode(), utc(1_050_000_000).encode())
        with self.assertRaisesRegex(G.H.ProofError, "unexpected proposals"):
            G.qualify(*values)

    def test_withdrawal_before_the_last_captured_window_is_still_unexpected(self):
        values = fixture()
        terminal_withdrawal(values)
        lines = values[1].splitlines(keepends=True)
        terminal = lines.pop()
        lines.insert(-1, terminal)
        values[1] = b"".join(lines)
        with self.assertRaisesRegex(G.H.ProofError, "unexpected proposals"):
            G.qualify(*values)

    def test_unflushed_partial_tail_cannot_exclude_a_terminal_proposal(self):
        for terminal in (False, True):
            with self.subTest(terminal=terminal):
                values = fixture()
                values[1] = b"".join(line.replace(b'end_packet=14 ', b'end_packet=15 ')
                                     for line in values[1].splitlines(keepends=True)
                                     if not (b'event="sink_window_flushed"' in line and b'first_packet=13 ' in line))
                values[2]["log_end"] = len(values[1])
                if terminal:
                    terminal_withdrawal(values)
                    with self.assertRaisesRegex(G.H.ProofError, "unexpected proposals"):
                        G.qualify(*values)
                else:
                    self.assertEqual(G.qualify(*values)["terminal_undelivered_proposals"], [])

    def test_actual_withdrawal_ack_and_reannouncement(self):
        result = G.qualify(*fixture())
        self.assertEqual(result["recovered_packet"], 6)
        self.assertEqual(result["path"], "expiry_withdrawal_ack_then_reannouncement")
        self.assertFalse(result["caption_timing_waived"])

    def test_expiring_reauthorization_may_withdraw_before_the_loop_boundary_trace(self):
        values = fixture()
        lines = values[1].splitlines(keepends=True)
        boundary = next(i for i, line in enumerate(lines) if b'event="scope_boundary_expired"' in line)
        withdrawal = next(i for i, line in enumerate(lines) if b'event="monitor_proposal"' in line and b'action="withdraw"' in line)
        lines[boundary] = lines[boundary].replace(utc(1_000_000_000).encode(), utc(1_000_020_000).encode())
        lines[boundary], lines[withdrawal] = lines[withdrawal], lines[boundary]
        values[1] = b"".join(lines)
        self.assertEqual(G.qualify(*values)["path"], "expiry_withdrawal_ack_then_reannouncement")

    def test_expiry_diagnostics_cannot_attribute_a_preexpiry_withdrawal(self):
        values = fixture()
        values[1] = b"".join(line.replace(utc(1_000_010_000).encode(), utc(999_990_000).encode())
                              if b'event="monitor_proposal"' in line and b'action="withdraw"' in line else line
                              for line in values[1].splitlines(keepends=True))
        with self.assertRaisesRegex(G.H.ProofError, "saved UTC expiry"):
            G.qualify(*values)

    def test_coalesced_expiry_replacement_preserves_continuous_pmt(self):
        self.assertEqual(G.qualify(*fixture(False))["path"], "confirmed_replacement_without_withdrawal")

    def test_startup_absence_requires_timely_installation_and_actual_delivery(self):
        result = G.qualify(*fixture(startup=True))
        self.assertTrue(result["startup_recovery"])
        self.assertEqual(result["path"], "expiry_withdrawal_ack_then_reannouncement")

    def test_startup_recovery_can_precede_coalesced_expiry(self):
        self.assertTrue(G.qualify(*fixture(False, startup=True))["startup_recovery"])

    def test_initial_absence_cannot_borrow_undelivered_startup(self):
        v = fixture(startup=True)
        v[1] = b"".join(line for line in v[1].splitlines(keepends=True)
                         if not (b'event="transition_applied"' in line and b'sequence=1 ' in line))
        v[2]["log_end"] = len(v[1])
        with self.assertRaisesRegex(G.H.ProofError, "startup guide lacks actual delivery"):
            G.qualify(*v)

    def test_startup_installation_cannot_follow_original_expiry(self):
        v = fixture(startup=True)
        v[1] = b"".join(line.replace(utc(100_300_000).encode(), utc(1_100_300_000).encode())
                         if b'event="transition_installed"' in line and b'sequence=1 ' in line else line
                         for line in v[1].splitlines(keepends=True))
        with self.assertRaisesRegex(G.H.ProofError, "original authority"):
            G.qualify(*v)

    def test_authored_payload_cannot_precede_initial_pmt_announcement(self):
        v = fixture(startup=True)
        changed = bytes([0x47, 0x02]) + v[0][2:]
        old, new = G.H.digest(v[0][:188]).encode(), G.H.digest(changed[:188]).encode()
        v[0] = changed
        v[1] = b"".join(line.replace(old, new) if b'first_packet=0 ' in line else line
                         for line in v[1].splitlines(keepends=True))
        with self.assertRaisesRegex(G.H.ProofError, "no current PMT announcement"):
            G.qualify(*v)

    def rejected(self, mutate):
        values = fixture()
        mutate(values)
        with self.assertRaises(G.H.ProofError):
            G.qualify(*values)

    def test_foreign_request_channel_cannot_borrow_matching_packets(self):
        self.rejected(lambda v: v[2].update(channel_id=2))

    def test_candidate_expiry_must_equal_saved_metadata(self):
        self.rejected(lambda v: v[5][0].update(valid_until=utc(2_000_000_000)))

    def test_metadata_cannot_be_borrowed_from_another_channel(self):
        self.rejected(lambda v: v[5][0].update(channel_id=2))

    def test_original_metadata_cannot_impersonate_a_fresh_runtime(self):
        self.rejected(lambda v: v[5].pop())

    def test_ack_cannot_be_borrowed_from_another_epoch(self):
        def mutate(v):
            v[1] = v[1].replace(b'event="transition_applied" proposal_slot=', b'event="transition_applied" output_epoch="00000000-0000-0000-0000-000000000099" proposal_slot=')
            v[2]["log_end"] = len(v[1])
        self.rejected(mutate)

    def test_packet_hash_mutation_cannot_reassign_ownership(self):
        self.rejected(lambda v: v.__setitem__(0, bytes([0x46]) + v[0][1:]))

    def test_raw_pmt_cannot_reannounce_an_obsolete_version(self):
        self.rejected(lambda v: v.__setitem__(4, v[4] + "Packet 12 to 12, PID 0x1000, TID 0x02, TIDext 0x0001, V1, Sec 0/0: E1 00\n"))

    def test_complete_pmt_extent_must_precede_its_exclusive_ack_end(self):
        v = fixture()
        v[3][1]["#nodes"][0]["last_ts_packet"] = 5
        v[4] = v[4].replace("Packet 4 to 4", "Packet 4 to 5")
        with self.assertRaisesRegex(G.H.ProofError, "actual ACK interval"):
            G.qualify(*v)

    def test_av_and_caption_descriptors_must_survive_withdrawal(self):
        for index in (1, 2, 3):
            with self.subTest(component=index):
                self.rejected(lambda v: v[3][1]["#nodes"][index].update(stream_type=3))

    def test_expiry_cannot_move_before_its_exact_saved_utc(self):
        def mutate(v):
            v[1] = b"".join(line.replace(utc(1_000_000_000).encode(), utc(999_000_000).encode())
                            if b'event="scope_boundary_expired"' in line else line for line in v[1].splitlines(keepends=True))
        self.rejected(mutate)

    def test_authored_payload_cannot_survive_absent_signalling(self):
        v = fixture()
        changed = v[0][:4 * 188] + bytes([0x47, 0x02]) + v[0][4 * 188 + 2:]
        old = G.H.digest(v[0][4 * 188:5 * 188]).encode()
        new = G.H.digest(changed[4 * 188:5 * 188]).encode()
        v[0] = changed
        v[1] = b"".join(line.replace(old, new) if b'first_packet=4 ' in line else line
                         for line in v[1].splitlines(keepends=True))
        with self.assertRaisesRegex(G.H.ProofError, "authored payload"):
            G.qualify(*v)

    def test_window_cannot_keep_an_obsolete_installed_sequence(self):
        def mutate(v):
            v[1] = b"".join(line.replace(b'active_sequence=2', b'active_sequence=0')
                            if b'first_packet=6 ' in line else line for line in v[1].splitlines(keepends=True))
        self.rejected(mutate)

    def test_a_written_prefix_is_not_a_complete_recovered_cycle(self):
        v = fixture()
        v[1] = b"".join(line.replace(b'complete_cycle=true', b'complete_cycle=false')
                         if b'end_packet=11 ' in line else line for line in v[1].splitlines(keepends=True))
        v[2]["log_end"] = len(v[1])
        with self.assertRaisesRegex(G.H.ProofError, "fresh guide cycle"):
            G.qualify(*v)

    def test_withdrawal_requires_actual_absent_signalling_ack(self):
        def mutate(v):
            v[1] = b"".join(line for line in v[1].splitlines(keepends=True) if not (b'event="transition_applied"' in line and b'action="withdraw"' in line))
            v[2]["log_end"] = len(v[1])
        self.rejected(mutate)

    def test_duplicate_same_channel_intents_are_ambiguous(self):
        def mutate(v):
            first = v[1].splitlines(keepends=True)[0]
            v[1] = first + v[1]
            v[2]["log_first_chunk"] += len(first)
            v[2]["log_end"] = len(v[1])
        self.rejected(mutate)


if __name__ == "__main__":
    unittest.main()
