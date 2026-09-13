#!/usr/bin/env python3
"""Package pinned cvc5/GMP source, static objects and a standalone relink recipe."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fetch(path: Path, pin: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file():
        partial = path.with_suffix(".download")
        try:
            with urllib.request.urlopen(pin["url"], timeout=120) as response, partial.open("wb") as output:
                shutil.copyfileobj(response, output)
            if digest(partial) != pin["sha256"]:
                raise RuntimeError(f"Source checksum mismatch: {path.name}")
            partial.replace(path)
        finally:
            partial.unlink(missing_ok=True)
    if digest(path) != pin["sha256"]:
        raise RuntimeError(f"Cached source checksum mismatch: {path.name}")
    return path


def write_zip(path: Path, files: dict[str, Path | bytes]) -> None:
    partial = path.with_suffix(".partial")
    try:
        with zipfile.ZipFile(partial, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for name, source in sorted(files.items()):
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                with archive.open(info, "w") as destination:
                    if isinstance(source, bytes):
                        destination.write(source)
                    else:
                        with source.open("rb") as stream:
                            shutil.copyfileobj(stream, destination)
        partial.replace(path)
    finally:
        partial.unlink(missing_ok=True)


def main() -> None:
    if os.name == "nt":
        subprocess.run(["wsl", "-d", "Ubuntu", "--", "python3", "scripts/package-web-relink.py"], cwd=ROOT, check=True)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/satisfactory-flow-synthetizer-web")
    args = parser.parse_args()
    cache = args.cache.resolve()
    builder = runpy.run_path(str(ROOT / "scripts/build-cvc5-wasm.py"))
    prepare = runpy.run_path(str(ROOT / "scripts/prepare-web-solver.py"))
    if not prepare["current"](ROOT):
        raise RuntimeError("Build and verify the browser solver before packaging its relink material")
    pins = json.loads((ROOT / "web/cvc5/toolchain.json").read_text())
    build = cache / "cvc5/build-web"
    builder["verify_dependency_pins"](build, pins["dependencies"])
    recipe = hashlib.sha256(json.dumps([pins, builder["CONFIGURE"]], sort_keys=True).encode()).hexdigest()
    if (cache / "cvc5.configure-sha256").read_text().strip() != recipe:
        raise RuntimeError("The static libraries do not match the configured build")
    files: dict[str, Path | bytes] = {}
    for name in ("cvc5", "emsdk"):
        files[f"sources/{name}.tar.gz"] = fetch(cache / f"{name}.tar.gz", pins[name])
    for name, pin in pins["dependencies"].items():
        files[f"sources/{name}.archive"] = fetch(cache / f"distribution/{name}.archive", pin)
    for relative in ("src/parser/libcvc5parser.a", "src/libcvc5.a", "deps/lib/libcadical.a", "deps/lib/libgmp.a"):
        path = build / relative
        if not path.is_file():
            raise RuntimeError(f"Missing relink library: {path.name}")
        files[f"libraries/{path.name}"] = path
    backend = ROOT / "target/web-backends/cvc5"
    record = json.loads((backend / "build.json").read_text())
    if digest(backend / "session.o") != record["artifacts"]["session.o"]["sha256"]:
        raise RuntimeError("The application object does not match the shipped cvc5 module")
    files["session.o"] = backend / "session.o"
    files["session.cpp"] = ROOT / "web/cvc5/session.cpp"
    files["session.mjs"] = ROOT / "web/cvc5/session.mjs"
    files["build.json"] = backend / "build.json"
    files["relink.py"] = ROOT / "scripts/relink-web-cvc5.py"
    files["build-cvc5-wasm.py"] = ROOT / "scripts/build-cvc5-wasm.py"
    for path in (backend / "licenses").rglob("*"):
        if path.is_file():
            files["licenses/" + path.relative_to(backend / "licenses").as_posix()] = path
    files["licenses/emscripten/LICENSE"] = cache / "emsdk/upstream/emscripten/LICENSE"
    files["inputs.json"] = (json.dumps({name: digest(path) for name, path in files.items() if name.startswith("libraries/") or name == "session.o"}, indent=2) + "\n").encode()
    files["README.txt"] = (ROOT / "web/distribution/relink-readme.txt").read_bytes()
    destination = ROOT / "target/web-backends/relink"
    destination.mkdir(parents=True, exist_ok=True)
    write_zip(destination / "cvc5-relink.zip", files)
    print(f"Packaged cvc5 source and relink material: {digest(destination / 'cvc5-relink.zip')}")


if __name__ == "__main__":
    main()
