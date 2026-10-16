#!/usr/bin/env python3
"""Receipt-bound Linux test-decoder trial of unchanged captured sparse DRCS.

Use a separate process per variant with LD_LIBRARY_PATH set to its build
subdirectory. This executes the existing strict oracle unchanged, then checks
its actual mapped library. Corrected libraries are test-only decoder candidates;
this supplies no actual-client, panel, sparse editor publication or engine gate.
"""

import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tarfile

SOURCE = {
    "version": "0.2.42",
    "commit": "3785481f51f41a49e28f2b7f6fd5bd9187d24ae1",
    "url": "https://codeload.github.com/zapping-vbi/zvbi/tar.gz/3785481f51f41a49e28f2b7f6fd5bd9187d24ae1",
    "bytes": 1267984,
    "members": 267,
    "sha256": "4ecc6b129d6b8191db86953efec5f50afb6f1d9db1a706bb903ebde0d489df4c",
    "copying_sha256": "ae5ad395d972abe37981d782637940665462028daa67335ae4fb462592e2fba9",
}
IMAGE = "sha256:c0aee3f2c9c722c2afe67dff563609b63cbbe09f79e97ce8d5debdddbfab302e"
CONFIGURATION = {
    "arguments": ["--enable-shared", "--disable-static", "--without-x", "--disable-v4l",
                  "--disable-dvb", "--disable-bktr", "--disable-proxy", "--disable-nls", "--without-doxygen"],
    "LIBS": "-lm", "CPPFLAGS": " -D_GNU_SOURCE",
}
PARSERS = {
    "original": "e41fa3c60308599ab6a008da21b793f18e86a5011fac32fe6f84befc9dd6db18",
    "corrected": "9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd",
    "sparse": "9547e54ff9f72999289423d5a85a3ee4cceea93e1858034a82b62e10370d70bd",
}
FORMATTERS = {
    "original": "c0920093077fe80924e483bd6a61c76418b7c20f0418dcfc6b148b1986fae4b8",
    "sparse": "973860e2ea47515503f40b2fe7531cf29394be180000d6cd188334125d9e4398",
}
LABELS = {
    "original": "upstream-unmodified",
    "corrected": "test-only-x27-corrected",
    "sparse": "test-only-x27-and-normal-drcs-corrected",
}
ORACLE_SHA256 = "fe250705ddc7665a5012d2bb207d1702cfd3bf765716b0c1d4022265d1959a47"
HELPER_SHA256 = "615bc0ceb7b24738e7cb7e8a6df076c8b58eb9968ecdc7e07d226f76f5d7a76d"
UNITS_SHA256 = "1479080fa6c2db2f635906957eb080a976f4f189c3ea7b97e2dc98e9f0764eff"
RECIPES = {
    "base": {"path": "scripts/teletext-zvbi-x27-build.sh",
             "sha256": "8d101d8dad5d4ad7172cbde9a4e7d3bd1775d3808cb1b3badd560c50722eb7e2"},
    # Filled from the source-frozen candidate recipe, never a receipt assertion.
    "sparse": {"path": "scripts/teletext-zvbi-sparse-build.sh", "sha256": "73cfc7f789d522197d253034910b77863d33188536ba2195df8ba4fa76052f05"},
}
LIMITATIONS = {"test_only": True, "production_dependency": False, "independent_main_columns": 41,
               "side_panels_proven": False, "full_client_acceptance": False, "full_level25_acceptance": False}
MAX_FILE = 16 * 1024 * 1024
MAX_RECEIPT = 65536


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bounded(path, maximum=MAX_FILE):
    require(path.is_file() and not path.is_symlink(), "Expected regular proof input")
    require(path.stat().st_size <= maximum, "Proof input exceeds byte bound")
    with path.open("rb") as stream:
        body = stream.read(maximum + 1)
    require(len(body) <= maximum, "Proof input grew past byte bound")
    return body


def digest(path, maximum=MAX_FILE):
    return hashlib.sha256(bounded(path, maximum)).hexdigest()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON member")
        result[key] = value
    return result


def closed_json(body):
    return json.loads(body, object_pairs_hook=unique_object)


def hash_shape(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def variant_entry(root, name, entry, sparse=False):
    fields = {"label", "packet_sha256", "library", "library_sha256", "configure_log_sha256"}
    if sparse:
        fields |= {"formatter_sha256", "bootstrap_log_sha256", "build_log_sha256"}
    require(type(entry) is dict and set(entry) == fields, "Unexpected variant receipt members")
    require(entry["label"] == LABELS[name] and entry["packet_sha256"] == PARSERS[name], "Parser receipt pin mismatch")
    require(entry["library"] == name + "/libzvbi.so.0.13.2", "Unexpected library receipt path")
    directory = root / name
    require(digest(directory / "packet.c") == PARSERS[name], "Parser source changed")
    require(digest(directory / "COPYING.md") == SOURCE["copying_sha256"], "Variant license changed")
    for relative, key in [(entry["library"], "library_sha256"),
                          (name + "/config-details.log", "configure_log_sha256")]:
        require(hash_shape(entry[key]) and digest(root / relative) == entry[key], "Variant content receipt mismatch")
    if sparse:
        require(entry["formatter_sha256"] == FORMATTERS["sparse"], "Formatter receipt pin mismatch")
        require(digest(directory / "teletext.c") == FORMATTERS["sparse"], "Formatter source changed")
        for filename, key in [("bootstrap.log", "bootstrap_log_sha256"), ("build.log", "build_log_sha256")]:
            require(hash_shape(entry[key]) and digest(directory / filename) == entry[key], "Variant build log changed")


def verify_build(root, proof, expected_receipt_hash):
    base_raw = bounded(root / "proof-build.json", MAX_RECEIPT)
    sparse_raw = bounded(root / "proof-sparse-build.json", MAX_RECEIPT)
    require(hash_shape(expected_receipt_hash) and hashlib.sha256(sparse_raw).hexdigest() == expected_receipt_hash,
            "Externally pinned sparse build receipt mismatch")
    base, sparse = closed_json(base_raw), closed_json(sparse_raw)
    require(type(base) is dict and set(base) == {"schema", "source", "requested_image", "configuration", "tool_versions_sha256", "variants"}, "Unexpected base receipt members")
    require(type(base["schema"]) is int and base["schema"] == 1 and base["source"] == SOURCE, "Base source receipt mismatch")
    require(base["requested_image"] == IMAGE and base["configuration"] == CONFIGURATION, "Base recipe configuration mismatch")
    require(set(base["variants"]) == {"original", "corrected"}, "Expected original and X27-only builds")
    require(digest(root / "tool-versions.log", MAX_RECEIPT) == base["tool_versions_sha256"], "Base tool receipt mismatch")
    archive = bounded(root / "source.tar.gz", 2 * 1024 * 1024)
    require(len(archive) == SOURCE["bytes"] and hashlib.sha256(archive).hexdigest() == SOURCE["sha256"], "Source archive changed")
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as source:
        require(len(source.getmembers()) == SOURCE["members"], "Source archive member count mismatch")
    require(digest(root / "source/COPYING.md") == SOURCE["copying_sha256"], "Source license changed")
    require(digest(root / "source/src/packet.c") == PARSERS["original"], "Original parser source changed")
    require(digest(root / "source/src/teletext.c") == FORMATTERS["original"], "Original formatter source changed")
    for name, entry in base["variants"].items():
        variant_entry(root, name, entry)
    # The sparse recipe extends an exact retained base receipt, not a rebuilt
    # unlabelled substitute for the original and X27-only control libraries.
    require(type(sparse) is dict and set(sparse) == {
        "schema", "source", "requested_image", "base_receipt", "recipes", "original_formatter_sha256",
        "configuration", "base_tool_versions_sha256", "tool_versions_sha256", "variants", "limitations"},
        "Unexpected sparse receipt members")
    require(type(sparse["schema"]) is int and sparse["schema"] == 1 and sparse["base_receipt"] == {
        "path": "proof-build.json", "sha256": hashlib.sha256(base_raw).hexdigest()}, "Base receipt link mismatch")
    require(sparse["source"] == {**SOURCE, "tag": "v0.2.42"}, "Sparse source receipt mismatch")
    require(sparse["original_formatter_sha256"] == FORMATTERS["original"], "Original formatter receipt mismatch")
    require(sparse["base_tool_versions_sha256"] == base["tool_versions_sha256"], "Base tool receipt link mismatch")
    require(sparse["limitations"] == LIMITATIONS, "Test-only candidate boundary changed")
    require(sparse["requested_image"] == IMAGE and sparse["configuration"] == CONFIGURATION, "Sparse recipe configuration mismatch")
    require(sparse["recipes"] == RECIPES, "Frozen recipe receipt mismatch")
    for recipe in RECIPES.values():
        require(digest(proof / recipe["path"], MAX_RECEIPT) == recipe["sha256"], "Frozen build recipe changed")
    require(digest(root / "sparse-tool-versions.log", MAX_RECEIPT) == sparse["tool_versions_sha256"], "Sparse tool receipt mismatch")
    require(set(sparse["variants"]) == {"sparse"}, "Expected one combined candidate build")
    variant_entry(root, "sparse", sparse["variants"]["sparse"], sparse=True)
    return base_raw, sparse_raw, {**base["variants"], **sparse["variants"]}


def execute_oracle(oracle, units):
    body = bounded(oracle, MAX_RECEIPT)
    require(hashlib.sha256(body).hexdigest() == ORACLE_SHA256, "Unchanged strict oracle pin mismatch")
    require(digest(oracle.with_name("teletext-resource-decoder.py"), MAX_RECEIPT) == HELPER_SHA256, "Unchanged ABI helper pin mismatch")
    saved_argv = sys.argv
    stdout, stderr, status = io.StringIO(), io.StringIO(), 0
    try:
        sys.argv = [str(oracle), str(units), "day", "sparse"]
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                # Execute only authenticated source, never cached bytecode or a
                # second read that could detach the pin from executed content.
                exec(compile(body, str(oracle), "exec"), {"__name__": "__main__", "__file__": str(oracle)})
            except SystemExit as error:
                status = error.code
    finally:
        sys.argv = saved_argv
    require(type(status) is int, "Unexpected noninteger oracle exit status")
    require(len(stdout.getvalue().encode()) <= MAX_RECEIPT and len(stderr.getvalue().encode()) <= MAX_RECEIPT, "Oracle output exceeded byte bound")
    return status, stdout.getvalue(), stderr.getvalue()


def mapped_library(root, name, entry, mappings):
    libraries = set()
    for line in mappings.splitlines():
        parts = line.split(maxsplit=5)
        if len(parts) == 6 and "/libzvbi.so." in parts[5]:
            path = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), parts[5])
            libraries.add(path)
    require(len(libraries) == 1, "Expected exactly one actually mapped libzvbi")
    actual = Path(libraries.pop())
    expected = root / entry["library"]
    require(entry["library"] == name + "/libzvbi.so.0.13.2" and actual == expected, "Actual mapped decoder path differs from selected receipt")
    loaded_hash = digest(actual)
    require(loaded_hash == entry["library_sha256"], "Actually mapped decoder hash mismatch")
    return {"path": str(actual), "sha256": loaded_hash}


def expected_report():
    return {"decoder_version": "0.2.42", "units_sha256": UNITS_SHA256,
            "theme": "day", "mode": "sparse", "panels_proven": False,
            "pages": [{"page": f"{number:03X}", "raw_page": number, "subcode": 32,
                       "unicode": 0xF440, "logical_subpage": 1, "pixels": 120,
                       "foreground_rgba": [255, 0, 0, 255]} for number in (0x124, 0x125)]}


def verify_outcome(name, status, stdout, stderr):
    if name in ("original", "corrected"):
        require(status == 1 and not stdout, "Control decoder did not fail as expected")
        report = closed_json(stderr)
        require(report == {"error": "Normal indexed DRCS/slot0 missing"}, "Control failure oracle changed")
    else:
        require(name == "sparse" and status == 0 and not stderr, "Combined candidate did not pass the unchanged oracle")
        report = closed_json(stdout)
        require(report == expected_report(), "Combined sparse literal oracle report changed")
    return report


def write_new(path, body):
    with path.open("xb") as stream:
        stream.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", required=True, type=Path, help="Existing receipted build output directory")
    parser.add_argument("--proof", required=True, type=Path, help="Existing build proof input directory with scripts")
    parser.add_argument("--build-receipt-sha256", required=True, help="External SHA256 pin of proof-sparse-build.json")
    parser.add_argument("--units", required=True, type=Path, help="Unchanged retained sparse captured units")
    parser.add_argument("--receipt-dir", required=True, type=Path, help="New destination for this variant trial")
    parser.add_argument("--variant", required=True, choices=tuple(LABELS))
    arguments = parser.parse_args()
    require(sys.platform == "linux", "The test-decoder map proof requires Linux")
    require(not arguments.build.is_symlink() and not arguments.proof.is_symlink(), "Expected real build/proof directories")
    require(not arguments.units.is_symlink(), "Expected regular captured sparse input")
    root, proof, units = arguments.build.resolve(), arguments.proof.resolve(), arguments.units.absolute()
    require(os.environ.get("LD_LIBRARY_PATH") == str(root / arguments.variant), "LD_LIBRARY_PATH must select exactly the explicit variant directory before process start")
    base_raw, sparse_raw, variants = verify_build(root, proof, arguments.build_receipt_sha256)
    require(digest(units, 2 * 1024 * 1024) == UNITS_SHA256, "Unchanged captured sparse input pin mismatch")
    arguments.receipt_dir.mkdir()
    oracle = Path(__file__).with_name("teletext-editor-drcs-decoder.py")
    status, stdout, stderr = execute_oracle(oracle, units)
    write_new(arguments.receipt_dir / "oracle-stdout.txt", stdout.encode())
    write_new(arguments.receipt_dir / "oracle-stderr.txt", stderr.encode())
    loaded = mapped_library(root, arguments.variant, variants[arguments.variant], bounded(Path("/proc/self/maps"), 4 * 1024 * 1024).decode())
    report = verify_outcome(arguments.variant, status, stdout, stderr)
    require(digest(units, 2 * 1024 * 1024) == UNITS_SHA256, "Captured sparse input changed during trial")
    result = {"schema": 1, "variant": arguments.variant, "decoder_label": LABELS[arguments.variant],
              "source": SOURCE, "recipes": RECIPES, "build_receipt_sha256": hashlib.sha256(base_raw).hexdigest(),
              "sparse_build_receipt_sha256": hashlib.sha256(sparse_raw).hexdigest(),
              "oracle_sha256": ORACLE_SHA256, "helper_sha256": HELPER_SHA256, "units_sha256": UNITS_SHA256,
              "loaded_library": loaded, "exit_code": status, "conformance_passed": status == 0,
              "report": report, "decoder_library_test_only": arguments.variant != "original",
              "actual_client_proven": False, "panels_proven": False, "full_level25_proven": False}
    encoded = (json.dumps(result, sort_keys=True, indent=2) + "\n").encode()
    write_new(arguments.receipt_dir / "receipt.json", encoded)
    print(encoded.decode(), end="")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, AttributeError, RecursionError, RuntimeError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        sys.exit(1)
