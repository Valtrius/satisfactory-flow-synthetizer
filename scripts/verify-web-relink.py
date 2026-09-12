#!/usr/bin/env python3
"""Relink the public cvc5 bundle and compare it with the shipped browser module."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ("cvc5.mjs", "cvc5.wasm")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def compare_records(expected: dict, actual: dict, directory: Path) -> None:
    for name in ARTIFACTS:
        wanted = expected["artifacts"][name]
        got = actual["artifacts"][name]
        path = directory / name
        observed = {"bytes": path.stat().st_size, "sha256": digest(path)}
        if got != wanted or observed != wanted:
            raise RuntimeError(f"Relinked {name} does not match the distributed browser module")


def main() -> None:
    if os.name == "nt":
        subprocess.run(["wsl", "-d", "Ubuntu", "--", "python3", "scripts/verify-web-relink.py"], cwd=ROOT, check=True)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/satisfactory-flow-synthetizer-web")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        raise SystemExit("Verify relinking on Linux x64 or Ubuntu WSL")
    archive = ROOT / "target/web-backends/relink/cvc5-relink.zip"
    if not archive.is_file():
        raise RuntimeError("Prepare the public relink bundle first: python scripts/package-web-relink.py")
    compiler = args.cache.expanduser().resolve() / "emsdk/upstream/emscripten/em++"
    if not compiler.is_file():
        raise RuntimeError("The pinned Emscripten compiler is missing from the web build cache")
    expected = json.loads((ROOT / "target/web-backends/cvc5/build.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="sfs-relink-") as temporary:
        directory = Path(temporary)
        with zipfile.ZipFile(archive) as package:
            package.extractall(directory)
        required = ["relink.py", "build.json", "inputs.json", "session.o", *[f"libraries/{name}" for name in ("libcvc5parser.a", "libcvc5.a", "libcadical.a", "libgmp.a")]]
        missing = [name for name in required if not (directory / name).is_file()]
        if missing:
            raise RuntimeError("Relink bundle is incomplete: " + ", ".join(missing))
        output = directory / "relinked"
        subprocess.run([sys.executable, str(directory / "relink.py"), "--emxx", str(compiler), "--output", str(output)], check=True)
        actual = json.loads((output / "relink-record.json").read_text(encoding="utf-8"))
        compare_records(expected, actual, output)
    print("Verified public cvc5 relink bundle reproduces the shipped JavaScript and Wasm hashes.")


if __name__ == "__main__":
    main()
