#!/usr/bin/env python3
"""Regression proof that HLS boundaries cannot hide unmarked packet loss."""

import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'capture', Path(__file__).with_name('capture-hls-groups.py'))
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


def packets(counters):
    return b''.join(bytes((0x47, 0x01, 0x00, 0x10 | cc)) + b'\xff' * 184
                    for cc in counters)


class CaptureTests(unittest.TestCase):
    def capture_fixture(self, root, marked, second=None):
        marker = '#EXT-X-DISCONTINUITY\n' if marked else ''
        playlist = ('#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:17\n#EXTINF:2,\na.ts\n'
                    + marker + '#EXTINF:2,\nb.ts\n').encode()
        responses = {'http://fixture/list.m3u8': playlist,
                     'http://fixture/a.ts': packets(range(4)),
                     'http://fixture/b.ts': packets(range(4)) if second is None else second}
        with patch.object(capture, 'fetch', side_effect=lambda url, _: responses[url]):
            capture.capture('http://fixture/list.m3u8', 4, root)
        return sorted(root.glob('group-*.ts'))

    def continuity_errors(self, path):
        if not shutil.which('tsp'):
            self.skipTest('TSDuck required for packet-loss regression')
        result = subprocess.run(['tsp', '-I', 'file', str(path), '-P', 'continuity',
                                 '-O', 'drop'], capture_output=True, text=True, check=True)
        return result.stderr

    def test_declared_boundary_splits_original_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / 'capture'
            groups = self.capture_fixture(root, True)
            self.assertEqual(len(groups), 2)
            self.assertEqual((root / 'capture.ts').read_bytes(), packets(range(4)) * 2)
            for group in groups:
                self.assertEqual(self.continuity_errors(group), '')

    def test_unmarked_reset_remains_a_transport_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            groups = self.capture_fixture(Path(tmp) / 'capture', False)
            self.assertEqual(len(groups), 1)
            self.assertIn('missing 12 packets', self.continuity_errors(groups[0]))

    def test_marked_boundary_does_not_hide_loss_inside_group(self):
        with tempfile.TemporaryDirectory() as tmp:
            groups = self.capture_fixture(Path(tmp) / 'capture', True, packets([0, 1, 3]))
            self.assertIn('missing 1 packets', self.continuity_errors(groups[1]))

    def test_unmarked_continuous_segments_stay_together(self):
        with tempfile.TemporaryDirectory() as tmp:
            groups = self.capture_fixture(Path(tmp) / 'capture', False, packets(range(4, 8)))
            self.assertEqual(len(groups), 1)
            self.assertEqual(self.continuity_errors(groups[0]), '')

    def test_rejects_unsupported_or_incomplete_playlists(self):
        for body in ('#EXTM3U\n#EXT-X-KEY:METHOD=AES-128\n',
                     '#EXTM3U\n#EXT-X-MAP:URI=init.mp4\n', '#EXTM3U\na.ts\n'):
            with self.subTest(body=body), self.assertRaises(ValueError):
                capture.segments(body)


if __name__ == '__main__':
    unittest.main()
