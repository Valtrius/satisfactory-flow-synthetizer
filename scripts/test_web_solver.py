"""Production browser asset preparation without network or compiler side effects."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("solver_assets", Path(__file__).with_name("prepare-web-solver.py"))
ASSETS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSETS)


class BrowserAssetTests(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[Path, dict]:
        output = root / "target/web-backends/cvc5"
        output.mkdir(parents=True)
        (root / "scripts").mkdir()
        (root / "scripts/build-cvc5-wasm.py").write_text("CONFIGURE = ['production']\nLINK_FLAGS = ['-O1']\n", encoding="utf-8")
        (root / "web/cvc5").mkdir(parents=True)
        (root / "web/cvc5/toolchain.json").write_text('{"version":"pinned"}', encoding="utf-8")
        (root / "web/cvc5/session.cpp").write_bytes(b"wrapper")
        artifacts = {}
        for name in ("cvc5.mjs", "cvc5.wasm"):
            content = name.encode("utf-8")
            (output / name).write_bytes(content)
            artifacts[name] = {"bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()}
        for name in ("cvc5", "GMP-EP", "CaDiCaL-EP", "SymFPU-EP"):
            notice = output / "licenses" / name / "LICENSE"
            notice.parent.mkdir(parents=True)
            notice.write_text("notice", encoding="utf-8")
        manifest = {"schema": 1, "sources": {"version": "pinned"}, "configure": ["production"], "linkFlags": ["-O1"], "wrapperSha256": hashlib.sha256(b"wrapper").hexdigest(), "artifacts": artifacts}
        (output / "build.json").write_text(json.dumps(manifest), encoding="utf-8")
        return output, manifest

    def test_verified_unchanged_artifacts_are_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            self.assertTrue(ASSETS.current(root))

    def test_pin_recipe_and_wrapper_mismatches_require_preparation(self):
        for field in ("schema", "sources", "configure", "linkFlags", "wrapperSha256"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output, manifest = self.fixture(root)
                manifest[field] = "stale"
                (output / "build.json").write_text(json.dumps(manifest), encoding="utf-8")
                self.assertFalse(ASSETS.current(root))

    def test_corrupt_missing_and_incomplete_artifacts_are_not_reused(self):
        for failure in ("js", "wasm", "wrapper", "notice", "manifest", "missing"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output, _ = self.fixture(root)
                if failure == "notice":
                    (output / "licenses/GMP-EP/LICENSE").unlink()
                elif failure == "missing":
                    (output / "cvc5.wasm").unlink()
                elif failure == "wrapper":
                    (root / "web/cvc5/session.cpp").write_bytes(b"changed")
                else:
                    name = {"js": "cvc5.mjs", "wasm": "cvc5.wasm", "manifest": "build.json"}[failure]
                    (output / name).write_bytes(b"corrupt")
                self.assertFalse(ASSETS.current(root))

    def test_ready_assets_do_not_launch_the_toolchain(self):
        with patch.object(ASSETS, "current", return_value=True), patch.object(ASSETS.subprocess, "run") as run:
            ASSETS.main()
            run.assert_not_called()

    def test_preparation_checks_the_result_and_propagates_failure(self):
        for success in (True, False):
            with self.subTest(success=success), patch.object(ASSETS, "current", side_effect=[False, success]), patch.object(ASSETS.subprocess, "run") as run:
                if success:
                    ASSETS.main()
                else:
                    with self.assertRaisesRegex(RuntimeError, "artifact checks"):
                        ASSETS.main()
                run.assert_called_once_with([ASSETS.sys.executable, "scripts/build-cvc5-wasm.py"], cwd=ASSETS.ROOT, check=True)


if __name__ == "__main__":
    unittest.main()
