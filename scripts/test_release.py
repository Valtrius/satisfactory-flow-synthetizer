"""Exercise release checks and the real version updater in an isolated copy."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("check_release", ROOT / "scripts/check-release.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        files = ["package.json", "package-lock.json", "frontend/package.json", "frontend/package-lock.json",
                 "src-tauri/tauri.conf.json", "Cargo.toml", "Cargo.lock", "README.md", "scripts/release-version.mjs"]
        files += [str(path.relative_to(ROOT)) for path in ROOT.glob("crates/*/Cargo.toml")]
        files += ["src-tauri/Cargo.toml"]
        for name in files:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)

    def test_updater_synchronizes_all_versions_and_is_idempotent(self):
        command = ["node", str(self.root / "scripts/release-version.mjs"), "1.0.0"]
        subprocess.run(command, check=True, capture_output=True)
        self.assertEqual(CHECK.check_release(self.root), "1.0.0")
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        subprocess.run(command, check=True, capture_output=True)
        self.assertEqual(before, {p: p.read_bytes() for p in before})

    def test_mismatched_lock_version_is_rejected(self):
        path = self.root / "package-lock.json"
        lock = json.loads(path.read_text())
        lock["packages"]["frontend"]["version"] = "99.0.0"
        path.write_text(json.dumps(lock))
        with self.assertRaisesRegex(ValueError, "frontend"):
            CHECK.check_release(self.root)

    def test_tag_requires_matching_version_and_nonempty_notes(self):
        version = CHECK.check_release(self.root)
        with self.assertRaisesRegex(ValueError, "does not match"):
            CHECK.check_release(self.root, "99.0.0")
        with self.assertRaisesRegex(ValueError, "Missing nonempty"):
            CHECK.check_release(self.root, version)
        notes = self.root / "docs/release-notes" / f"{version}.md"
        notes.parent.mkdir(parents=True)
        notes.write_text("Reviewed release highlights.")
        self.assertEqual(CHECK.check_release(self.root, version), version)

    def test_invalid_version_does_not_write_any_files(self):
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        result = subprocess.run(["node", str(self.root / "scripts/release-version.mjs"), "1.02.0"], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, {p: p.read_bytes() for p in before})
