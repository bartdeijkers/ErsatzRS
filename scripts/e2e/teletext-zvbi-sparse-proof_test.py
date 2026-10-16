"""Pure receipt/outcome regressions; no native decoding/build/client claim."""

import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("teletext-zvbi-sparse-proof.py")
SPEC = importlib.util.spec_from_file_location("teletext_sparse_proof_contract", SCRIPT)
PROOF = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROOF)


def sha(body):
    return hashlib.sha256(body).hexdigest()


class BuildFixture:
    """Small fake files exercise receipt integrity without external tools."""

    def __init__(self, root):
        self.root, self.proof = root / "build", root / "proof"
        self.root.mkdir()
        self.proof.mkdir()
        self.source = copy.deepcopy(PROOF.SOURCE)
        self.parsers = {name: sha(name.encode()) for name in PROOF.PARSERS}
        self.formatters = {name: sha((name + " formatter").encode()) for name in PROOF.FORMATTERS}
        self.recipes = copy.deepcopy(PROOF.RECIPES)
        for name, recipe in self.recipes.items():
            recipe["sha256"] = self.file(self.proof / recipe["path"], name.encode())
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as output:
            member = tarfile.TarInfo("synthetic.txt")
            member.size = 3
            output.addfile(member, io.BytesIO(b"one"))
        self.source.update(bytes=len(archive.getvalue()), members=1, sha256=sha(archive.getvalue()), copying_sha256=sha(b"license"))
        self.file(self.root / "source.tar.gz", archive.getvalue())
        self.file(self.root / "source/COPYING.md", b"license")
        self.file(self.root / "source/src/packet.c", b"original")
        self.file(self.root / "source/src/teletext.c", b"original formatter")
        tools = self.file(self.root / "tool-versions.log", b"base tools")
        sparse_tools = self.file(self.root / "sparse-tool-versions.log", b"candidate tools")
        variants = {}
        for name in PROOF.LABELS:
            self.file(self.root / name / "packet.c", name.encode())
            self.file(self.root / name / "COPYING.md", b"license")
            variants[name] = {"label": PROOF.LABELS[name], "packet_sha256": self.parsers[name],
                              "library": name + "/libzvbi.so.0.13.2",
                              "library_sha256": self.file(self.root / name / "libzvbi.so.0.13.2", (name + " library").encode()),
                              "configure_log_sha256": self.file(self.root / name / "config-details.log", (name + " configure").encode())}
        variants["sparse"].update(
            formatter_sha256=self.file(self.root / "sparse/teletext.c", b"sparse formatter"),
            bootstrap_log_sha256=self.file(self.root / "sparse/bootstrap.log", b"bootstrap"),
            build_log_sha256=self.file(self.root / "sparse/build.log", b"make"))
        self.base = {"schema": 1, "source": self.source, "requested_image": PROOF.IMAGE,
                     "configuration": PROOF.CONFIGURATION, "tool_versions_sha256": tools,
                     "variants": {name: variants[name] for name in ("original", "corrected")}}
        self.sparse = {"schema": 1, "source": {**self.source, "tag": "v0.2.42"},
                       "requested_image": PROOF.IMAGE, "base_receipt": {"path": "proof-build.json", "sha256": ""},
                       "recipes": self.recipes, "original_formatter_sha256": self.formatters["original"],
                       "configuration": PROOF.CONFIGURATION, "base_tool_versions_sha256": tools,
                       "tool_versions_sha256": sparse_tools, "variants": {"sparse": variants["sparse"]},
                       "limitations": PROOF.LIMITATIONS}
        self.write()

    @staticmethod
    def file(path, body):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
        return sha(body)

    def write(self):
        base_raw = json.dumps(self.base).encode()
        self.file(self.root / "proof-build.json", base_raw)
        self.sparse["base_receipt"]["sha256"] = sha(base_raw)
        self.pin = self.file(self.root / "proof-sparse-build.json", json.dumps(self.sparse).encode())

    def verify(self, pin=None):
        with patch.object(PROOF, "SOURCE", self.source), patch.object(PROOF, "PARSERS", self.parsers), \
                patch.object(PROOF, "FORMATTERS", self.formatters), patch.object(PROOF, "RECIPES", self.recipes):
            return PROOF.verify_build(self.root, self.proof, self.pin if pin is None else pin)


class VerificationTests(unittest.TestCase):
    def test_external_pin_and_all_three_library_receipts_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = BuildFixture(Path(directory))
            base, sparse, entries = fixture.verify()
            self.assertEqual(set(entries), {"original", "corrected", "sparse"})
            self.assertEqual(sha(sparse), fixture.pin)
            self.assertEqual(sha(base), fixture.sparse["base_receipt"]["sha256"])
            for pin in ("0" * 64, "bad", True):
                with self.subTest(pin=pin), self.assertRaisesRegex(ValueError, "Externally pinned"):
                    fixture.verify(pin)
            for name in entries:
                library = fixture.root / entries[name]["library"]
                before = library.read_bytes()
                library.write_bytes(before + b"tamper")
                with self.subTest(variant=name), self.assertRaisesRegex(ValueError, "content receipt"):
                    fixture.verify()
                library.write_bytes(before)

    def test_receipt_roles_sources_recipes_and_candidate_boundary_are_not_self_attested(self):
        mutations = [
            lambda f: f.base["source"].update(commit="other"),
            lambda f: f.base["variants"].update(other=f.base["variants"]["original"]),
            lambda f: f.sparse["variants"]["sparse"].update(library="original/libzvbi.so.0.13.2"),
            lambda f: f.sparse["variants"]["sparse"].update(formatter_sha256="0" * 64),
            lambda f: f.sparse.update(original_formatter_sha256="0" * 64),
            lambda f: f.sparse.update(recipes={}),
            lambda f: f.sparse.update(limitations={"full_client_acceptance": True}),
            lambda f: f.sparse.update(extra=True),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index), tempfile.TemporaryDirectory() as directory:
                fixture = BuildFixture(Path(directory))
                # Copy source so replacing fake receipt claims cannot replace
                # the independently supplied test verifier's source constants.
                source = copy.deepcopy(fixture.source)
                fixture.base["source"] = copy.deepcopy(source)
                mutate(fixture)
                fixture.write()
                with self.assertRaises(ValueError):
                    fixture.verify()

    def test_archive_license_recipe_and_build_logs_remain_exact(self):
        paths = ("source.tar.gz", "source/COPYING.md", "sparse/COPYING.md",
                 "source/src/packet.c", "source/src/teletext.c", "sparse/teletext.c",
                 "sparse/bootstrap.log", "sparse/build.log", "sparse-tool-versions.log")
        for filename in paths:
            with self.subTest(file=filename), tempfile.TemporaryDirectory() as directory:
                fixture = BuildFixture(Path(directory))
                path = fixture.root / filename
                path.write_bytes(path.read_bytes() + b"changed")
                with self.assertRaises(ValueError):
                    fixture.verify()
        with tempfile.TemporaryDirectory() as directory:
            fixture = BuildFixture(Path(directory))
            path = fixture.proof / fixture.recipes["sparse"]["path"]
            path.write_bytes(b"unreceipted recipe")
            with self.assertRaisesRegex(ValueError, "Frozen build recipe"):
                fixture.verify()

    def test_actual_mapping_must_be_one_selected_exact_library_not_a_label(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            library = root / "sparse/libzvbi.so.0.13.2"
            library.parent.mkdir()
            library.write_bytes(b"candidate")
            entry = {"library": "sparse/libzvbi.so.0.13.2", "library_sha256": sha(b"candidate")}
            mapping = f"1000-2000 r-xp 0 00:01 2 {library}\n"
            self.assertEqual(PROOF.mapped_library(root, "sparse", entry, mapping),
                             {"path": str(library), "sha256": sha(b"candidate")})
            for maps in ("", mapping + "2000-3000 r-xp 0 00:02 3 /foreign/libzvbi.so.0.13.2\n",
                         mapping.replace("/sparse/", "/original/"), mapping.replace(".13.2", ".13.2 (deleted)")):
                with self.subTest(maps=maps), self.assertRaises(ValueError):
                    PROOF.mapped_library(root, "sparse", entry, maps)
            library.write_bytes(b"replacement")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                PROOF.mapped_library(root, "sparse", entry, mapping)

    def test_exact_control_failure_and_full_two_page_sparse_success_are_distinct(self):
        failure = json.dumps({"error": "Normal indexed DRCS/slot0 missing"})
        for name in ("original", "corrected"):
            self.assertEqual(PROOF.verify_outcome(name, 1, "", failure), json.loads(failure))
            for status, stdout, stderr in ((0, "", failure), (1, "extra", failure),
                                          (1, "", '{"error":"other"}'), (1, "", failure + failure)):
                with self.subTest(name=name, status=status, stdout=stdout, stderr=stderr), self.assertRaises(ValueError):
                    PROOF.verify_outcome(name, status, stdout, stderr)
        success = PROOF.expected_report()
        self.assertEqual(PROOF.verify_outcome("sparse", 0, json.dumps(success), ""), success)
        for field, value in (("logical_subpage", 0), ("unicode", 0xF400), ("pixels", 119),
                             ("subcode", 0), ("foreground_rgba", [0, 0, 255, 255])):
            changed = copy.deepcopy(success)
            changed["pages"][1][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "literal oracle"):
                PROOF.verify_outcome("sparse", 0, json.dumps(changed), "")
        with self.assertRaisesRegex(ValueError, "Combined candidate"):
            PROOF.verify_outcome("sparse", 1, "", failure)

    def test_duplicate_json_symlinks_and_read_bounds_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "Duplicate JSON"):
            PROOF.closed_json('{"schema":1,"schema":1}')
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "target"
            target.write_bytes(b"x" * 33)
            with self.assertRaisesRegex(ValueError, "byte bound"):
                PROOF.bounded(target, 32)
            link = Path(directory) / "link"
            link.symlink_to(target)
            with self.assertRaisesRegex(ValueError, "regular proof"):
                PROOF.bounded(link, 64)

    def test_pinned_source_execution_preserves_streams_status_and_argv(self):
        source = b'import sys\nprint("exact output")\nprint("exact error", file=sys.stderr)\nsys.exit(1)\n'
        with tempfile.TemporaryDirectory() as directory:
            oracle = Path(directory) / "oracle.py"
            oracle.write_bytes(source)
            helper = oracle.with_name("teletext-resource-decoder.py")
            helper.write_bytes(b"helper")
            before = sys.argv
            with patch.object(PROOF, "ORACLE_SHA256", sha(source)), patch.object(PROOF, "HELPER_SHA256", sha(b"helper")):
                self.assertEqual(PROOF.execute_oracle(oracle, Path("unchanged.json")),
                                 (1, "exact output\n", "exact error\n"))
                self.assertIs(sys.argv, before)
                oracle.write_bytes(source + b"\n")
                with self.assertRaisesRegex(ValueError, "strict oracle pin"):
                    PROOF.execute_oracle(oracle, Path("unchanged.json"))


if __name__ == "__main__":
    unittest.main()
