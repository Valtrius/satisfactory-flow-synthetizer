#!/usr/bin/env python3
"""Reuse a verified cvc5 browser build, or prepare it with the pinned builder."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def current(root: Path = ROOT) -> bool:
    """Check recipes, source pins, shipped notices and both runtime artifact hashes."""
    try:
        directory = root / "target/web-backends/cvc5"
        manifest = json.loads((directory / "build.json").read_text(encoding="utf-8"))
        builder = runpy.run_path(str(root / "scripts/build-cvc5-wasm.py"))
        if manifest["schema"] != 1 or manifest["sources"] != json.loads((root / "web/cvc5/toolchain.json").read_text(encoding="utf-8")):
            return False
        if manifest["configure"] != builder["CONFIGURE"] or manifest["linkFlags"] != builder["LINK_FLAGS"]:
            return False
        if manifest["wrapperSha256"] != hashlib.sha256((root / "web/cvc5/session.cpp").read_bytes()).hexdigest():
            return False
        for name in ("cvc5.mjs", "cvc5.wasm"):
            content = (directory / name).read_bytes()
            if manifest["artifacts"][name] != {"bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()}:
                return False
        return all(any((directory / "licenses" / name).iterdir()) for name in ("cvc5", "GMP-EP", "CaDiCaL-EP", "SymFPU-EP"))
    except (OSError, ValueError, KeyError, TypeError):
        return False


def main() -> None:
    if not current():
        subprocess.run([sys.executable, "scripts/build-cvc5-wasm.py"], cwd=ROOT, check=True)
        if not current():
            raise RuntimeError("The browser cvc5 build failed its artifact checks")
    print("Verified browser cvc5 artifacts are ready.")


if __name__ == "__main__":
    main()
