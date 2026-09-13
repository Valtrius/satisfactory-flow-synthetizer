"""Build-input checks that do not download tools or compile cvc5."""
import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("web_builder", Path(__file__).with_name("build-cvc5-wasm.py"))
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class WebBuildTests(unittest.TestCase):
    def archive(self, directory: Path) -> tuple[Path, dict]:
        archive = directory / "fixture.tar.gz"
        with tarfile.open(archive, "w:gz") as package:
            content = b"source file"
            entry = tarfile.TarInfo("fixture-1/source.txt")
            entry.size = len(content)
            package.addfile(entry, io.BytesIO(content))
        return archive, {"version": "1", "url": "unused", "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}

    def test_verified_archives_extract_once_and_keep_their_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, pin = self.archive(root)
            destination = BUILDER.unpack_verified(root, "fixture", pin)
            self.assertEqual((destination / "source.txt").read_bytes(), b"source file")
            self.assertEqual(BUILDER.unpack_verified(root, "fixture", pin), destination)

    def test_corrupt_archive_and_unverified_directory_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, pin = self.archive(root)
            contents = archive.read_bytes()
            archive.write_bytes(contents + b"corruption")
            with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
                BUILDER.unpack_verified(root, "fixture", pin)
            archive.write_bytes(contents)
            (root / "fixture").mkdir()
            with self.assertRaisesRegex(RuntimeError, "unverified extracted"):
                BUILDER.unpack_verified(root, "fixture", pin)

    def test_dependency_hash_and_url_must_both_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            info = root / "deps/src/example-stamp/example-urlinfo.txt"
            info.parent.mkdir(parents=True)
            pin = {"url": "https://example.invalid/source.tar.gz", "sha256": "a" * 64}
            info.write_text(f"url(s)={pin['url']}\nhash=SHA256={pin['sha256']}\n")
            BUILDER.verify_dependency_pins(root, {"example": pin})
            info.write_text(f"url(s)={pin['url']}\nhash=SHA256={'b' * 64}\n")
            with self.assertRaisesRegex(RuntimeError, "pinned example"):
                BUILDER.verify_dependency_pins(root, {"example": pin})


if __name__ == "__main__":
    unittest.main()
