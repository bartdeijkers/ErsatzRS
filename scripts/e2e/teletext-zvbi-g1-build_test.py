#!/usr/bin/env python3
"""Guarded source-transform checks only; no compilation or decoder acceptance.

Provide ERSATZRS_TELETEXT_TEST_G1_SOURCE as the retained sparse teletext.c.
The source stays external rather than vendoring the GPL decoder into fixtures.
"""
import ast
import hashlib
import os
from pathlib import Path
import unittest
from unittest.mock import Mock


RECIPE = Path(__file__).with_name("teletext-zvbi-g1-build.sh")
BEFORE = "973860e2ea47515503f40b2fe7531cf29394be180000d6cd188334125d9e4398"
AFTER = "9bd5df7a7837ac558f0e14a1d493e8022f76aeb1d3bbd812b3bbd903fef71db7"


def blocks():
    script = RECIPE.read_text()
    return {
        tag: script.split("python3 - <<'" + tag + "'\n", 1)[1].split("\n" + tag + "\n", 1)[0]
        for tag in ("VERIFY_RECIPE", "PREPARE_G1", "G1_RECEIPT")
    }


class G1RecipeGuards(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        source = os.environ.get("ERSATZRS_TELETEXT_TEST_G1_SOURCE")
        if not source:
            raise unittest.SkipTest("Requires the retained, hash-checked sparse formatter source")
        cls.source = Path(source).read_bytes()
        if hashlib.sha256(cls.source).hexdigest() != BEFORE:
            raise ValueError("External sparse formatter source hash mismatch")
        tree = ast.parse(blocks()["PREPARE_G1"])
        function = next(node for node in tree.body
                        if isinstance(node, ast.FunctionDef) and node.name == "patch_g1")
        cls.code = compile(ast.Module(body=[function], type_ignores=[]), str(RECIPE), "exec")

    def patch(self, source, digest_module=hashlib):
        namespace = {"hashlib": digest_module}
        exec(self.code, namespace)
        return namespace["patch_g1"](source)

    def test_exact_sparse_source_produces_exact_candidate(self):
        candidate = self.patch(self.source)
        self.assertEqual(hashlib.sha256(candidate).hexdigest(), AFTER)
        self.assertNotEqual(candidate, self.source)

    def test_changed_source_rejected_before_transform(self):
        with self.assertRaisesRegex(AssertionError, "Sparse formatter hash mismatch"):
            self.patch(self.source + b"\n")

    def test_transform_cannot_be_applied_twice(self):
        with self.assertRaisesRegex(AssertionError, "Sparse formatter hash mismatch"):
            self.patch(self.patch(self.source))

    def test_original_normal_drcs_source_rejected(self):
        original = self.source.replace(b"pgno = vtp->data.lop.link[27].pgno;",
                                       b"pgno = vtp->data.lop.link[25].pgno;")
        self.assertNotEqual(original, self.source)
        with self.assertRaisesRegex(AssertionError, "Sparse formatter hash mismatch"):
            self.patch(original)

    def test_missing_context_rejected_independently_of_source_digest(self):
        fake = Mock()
        fake.sha256.return_value.hexdigest.return_value = BEFORE
        source = self.source.replace(b"\tes.invert = 0;", b"\tes.invert = FALSE;")
        with self.assertRaisesRegex(AssertionError, "G1 formatter context mismatch"):
            self.patch(source, fake)

    def test_duplicate_context_rejected_independently_of_source_digest(self):
        fake = Mock()
        fake.sha256.return_value.hexdigest.return_value = BEFORE
        context = b"\tes.invert = 0;\n\n\tif (type == OBJECT_TYPE_PASSIVE)"
        with self.assertRaisesRegex(AssertionError, "G1 formatter context mismatch"):
            self.patch(self.source + context, fake)

    def test_every_embedded_python_block_compiles_without_running_build(self):
        for tag, source in blocks().items():
            with self.subTest(tag=tag):
                compile(source, str(RECIPE) + ":" + tag, "exec")


if __name__ == "__main__":
    unittest.main()
