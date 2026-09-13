"""Offline/publication packaging checks without network or compiler work."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("web_sources", ROOT / "scripts/package-web-sources.py")
SOURCES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCES)
VERIFY_SPEC = importlib.util.spec_from_file_location("web_relink", ROOT / "scripts/verify-web-relink.py")
VERIFY = importlib.util.module_from_spec(VERIFY_SPEC)
VERIFY_SPEC.loader.exec_module(VERIFY)


class WebDeliveryTests(unittest.TestCase):
    def test_zip_writer_is_deterministic_and_uses_fixed_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.txt"
            source.write_text("source", encoding="utf-8")
            left = root / "left.zip"
            right = root / "right.zip"
            contents = {"z/source.txt": source, "a/inline.txt": b"inline"}
            SOURCES.write_zip(left, contents)
            SOURCES.write_zip(right, contents)
            self.assertEqual(left.read_bytes(), right.read_bytes())
            with zipfile.ZipFile(left) as archive:
                self.assertEqual(archive.namelist(), ["a/inline.txt", "z/source.txt"])
                self.assertTrue(all(item.date_time == (1980, 1, 1, 0, 0, 0) for item in archive.infolist()))

    def test_npm_mit_metadata_is_retained_when_tarball_has_no_license_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "package.json"
            manifest.write_text(json.dumps({"name": "fixture", "license": "MIT"}), encoding="utf-8")
            readme = root / "README.md"
            readme.write_text("# fixture\n\n## License\nMIT\n", encoding="utf-8")
            self.assertEqual(SOURCES.notice_documents({"name": "fixture", "license": "MIT", "manifest_path": str(manifest)}), [manifest, readme])

    def test_missing_non_mit_notice_fails_instead_of_guessing_terms(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "package.json"
            manifest.write_text(json.dumps({"name": "fixture", "license": "UNKNOWN"}), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "Missing license notices"):
                SOURCES.notice_documents({"name": "fixture", "license": "UNKNOWN", "manifest_path": str(manifest)})

    def test_relink_record_requires_exact_shipped_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = {}
            for name in VERIFY.ARTIFACTS:
                path = root / name
                path.write_bytes(name.encode())
                artifacts[name] = {"bytes": path.stat().st_size, "sha256": VERIFY.digest(path)}
            expected = {"artifacts": artifacts}
            VERIFY.compare_records(expected, {"artifacts": dict(artifacts)}, root)
            wrong = json.loads(json.dumps(expected))
            wrong["artifacts"]["cvc5.wasm"]["sha256"] = "0" * 64
            with self.assertRaisesRegex(RuntimeError, "Relinked cvc5.wasm"):
                VERIFY.compare_records(expected, wrong, root)


if __name__ == "__main__":
    unittest.main()
