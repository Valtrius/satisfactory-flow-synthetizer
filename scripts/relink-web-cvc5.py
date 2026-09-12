#!/usr/bin/env python3
"""Relink the distributed cvc5 objects, optionally against a modified GMP library.

Run with Emscripten 3.1.70 activated. This script is also copied into the source
archive as relink.py and needs no copy of the application's working directory.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emxx", default=os.environ.get("EMXX", shutil.which("em++")))
    parser.add_argument("--gmp-library", type=Path, help="use a modified, Emscripten-built libgmp.a")
    parser.add_argument("--output", type=Path, default=Path("relinked"))
    args = parser.parse_args()
    if not args.emxx:
        parser.error("Activate Emscripten 3.1.70 or pass --emxx /path/to/em++")
    root = Path(__file__).resolve().parent
    record = json.loads((root / "build.json").read_text(encoding="utf-8"))
    version = subprocess.check_output([args.emxx, "--version"], text=True).splitlines()[0]
    if record["sources"]["emsdk"]["version"] not in version:
        raise RuntimeError("Use the Emscripten version recorded in build.json")
    inventory = json.loads((root / "inputs.json").read_text(encoding="utf-8"))
    for name, expected in inventory.items():
        if sha256(root / name) != expected:
            raise RuntimeError(f"Original relink input changed: {name}. Use --gmp-library for a replacement.")
    libraries = [root / "libraries" / name for name in ("libcvc5parser.a", "libcvc5.a", "libcadical.a", "libgmp.a")]
    if args.gmp_library:
        libraries[-1] = args.gmp_library.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    environment = {**os.environ, "EMCC_CORES": "2", "BINARYEN_CORES": "2"}
    subprocess.run([args.emxx, *record["linkFlags"], str(root / "session.o"), *map(str, libraries), "-o", str(output / "cvc5.mjs")], env=environment, check=True)
    result = {"compiler": version, "modifiedGmp": bool(args.gmp_library), "artifacts": {
        name: {"bytes": (output / name).stat().st_size, "sha256": sha256(output / name)} for name in ("cvc5.mjs", "cvc5.wasm")}}
    (output / "relink-record.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
