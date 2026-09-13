#!/usr/bin/env python3
"""Build the pinned incremental cvc5 module on Linux x64, including WSL.

Tools and upstream objects live in a user cache. Only generated JS/Wasm and
build records are copied to target/web-backends. No shell startup files change.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "web/cvc5/toolchain.json"
CONFIGURE = [
    "production", "--static", "--static-binary", "--auto-download", "--wasm=JS",
    "--no-poly", "--no-cocoa", "--no-editline", "--no-gpl", "--no-ipo",
    "--name=build-web", "-DCMAKE_CXX_FLAGS=-fexceptions",
    "--wasm-flags=-sDISABLE_EXCEPTION_CATCHING=0 -sALLOW_MEMORY_GROWTH=1",
]
# Keep upstream translation units at -O3, but avoid expensive whole-module
# optimization while proving the worker interface. Tune that separately later.
LINK_FLAGS = [
    "-O1", "-fexceptions", "--no-entry", "-sDISABLE_EXCEPTION_CATCHING=0",
    "-sALLOW_MEMORY_GROWTH=1", "-sMODULARIZE=1", "-sEXPORT_ES6=1",
    "-sEXPORT_NAME=createCvc5", "-sENVIRONMENT=web,worker",
    "-sMAXIMUM_MEMORY=1073741824", "-sINITIAL_MEMORY=33554432", "-sSTACK_SIZE=5242880",
    '-sEXPORTED_RUNTIME_METHODS=["cwrap","lengthBytesUTF8","stringToUTF8"]',
    '-sEXPORTED_FUNCTIONS=["_sfs_session_create","_sfs_session_execute","_sfs_session_output","_sfs_session_error","_sfs_session_destroy","_sfs_session_count","_sfs_heap_bytes","_malloc","_free"]',
]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def unpack_verified(cache: Path, name: str, pin: dict) -> Path:
    archive = cache / f"{name}.tar.gz"
    if not archive.is_file():
        partial = archive.with_suffix(".download")
        try:
            print(f"Downloading {name} {pin['version']}", flush=True)
            with urllib.request.urlopen(pin["url"], timeout=120) as response, partial.open("wb") as output:
                shutil.copyfileobj(response, output)
            if digest(partial) != pin["sha256"]:
                raise RuntimeError(f"{name} archive checksum mismatch")
            partial.replace(archive)
        finally:
            partial.unlink(missing_ok=True)
    if digest(archive) != pin["sha256"]:
        raise RuntimeError(f"cached {name} archive checksum mismatch")
    destination = cache / name
    stamp = cache / f"{name}.source-sha256"
    if destination.is_dir():
        if not stamp.is_file() or stamp.read_text().strip() != pin["sha256"]:
            raise RuntimeError(f"unverified extracted {name} directory; select a fresh --cache directory")
        return destination
    with tempfile.TemporaryDirectory(prefix=f".{name}-", dir=cache) as temporary:
        with tarfile.open(archive) as package:
            package.extractall(temporary, filter="data")
        entries = list(Path(temporary).iterdir())
        if len(entries) != 1 or not entries[0].is_dir():
            raise RuntimeError(f"unexpected {name} archive structure")
        entries[0].rename(destination)
    stamp.write_text(pin["sha256"] + "\n")
    return destination


def run(arguments: list[str], *, cwd: Path, env: dict[str, str]) -> None:
    subprocess.run(arguments, cwd=cwd, env=env, check=True)


def verify_dependency_pins(build: Path, pins: dict) -> None:
    for name, pin in pins.items():
        info = build / "deps/src" / f"{name}-stamp" / f"{name}-urlinfo.txt"
        text = info.read_text()
        if f"hash=SHA256={pin['sha256']}" not in text or f"url(s)={pin['url']}" not in text:
            raise RuntimeError(f"upstream build no longer matches the pinned {name} source")


def copy_notices(source: Path, build: Path, output: Path) -> None:
    groups = [("cvc5", source)] + [(name, build / "deps/src" / name) for name in ("GMP-EP", "CaDiCaL-EP", "SymFPU-EP")]
    for name, directory in groups:
        notices = [p for p in directory.iterdir() if p.is_file() and p.name.upper().startswith(("COPYING", "LICENSE", "AUTHORS", "NOTICE"))]
        if not notices:
            raise RuntimeError(f"missing source license notices for {name}")
        destination = output / "licenses" / name
        destination.mkdir(parents=True, exist_ok=True)
        for notice in notices:
            shutil.copyfile(notice, destination / notice.name)


def main() -> None:
    if os.name == "nt":
        import sys
        if len(sys.argv) > 1:
            raise SystemExit("Pass Linux build options through WSL: wsl -d Ubuntu -- python3 scripts/build-cvc5-wasm.py [options]")
        subprocess.run(["wsl", "-d", "Ubuntu", "--", "python3", "scripts/build-cvc5-wasm.py"], cwd=ROOT, check=True)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/satisfactory-flow-synthetizer-web")
    parser.add_argument("--link-only", action="store_true", help="relink an existing matching library build")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        raise SystemExit("Build cvc5 in Linux x64 or WSL: wsl -d Ubuntu -- python3 scripts/build-cvc5-wasm.py")
    pins = json.loads(MANIFEST.read_text())
    cache = args.cache.expanduser().resolve()
    cache.mkdir(parents=True, exist_ok=True)
    source = unpack_verified(cache, "cvc5", pins["cvc5"])
    sdk = unpack_verified(cache, "emsdk", pins["emsdk"])
    cmake = unpack_verified(cache, "cmake", pins["cmake"])
    env = os.environ.copy()
    # Bound compiler concurrency including Emscripten's internal link helpers.
    env.update(EMSDK_QUIET="1", EMCC_CORES="2", BINARYEN_CORES="2", CMAKE_BUILD_PARALLEL_LEVEL="2")
    active = cache / "emsdk.active-version"
    if not active.is_file() or active.read_text().strip() != pins["emsdk"]["version"]:
        run([str(sdk / "emsdk"), "install", pins["emsdk"]["version"]], cwd=sdk, env=env)
        run([str(sdk / "emsdk"), "activate", pins["emsdk"]["version"]], cwd=sdk, env=env)
        active.write_text(pins["emsdk"]["version"] + "\n")
    # emsdk constructs its own environment; transfer it through JSON, not shell parsing.
    environment = subprocess.check_output(["bash", "-c", 'source "$1/emsdk_env.sh"; python3 -c "import os,json; print(json.dumps(dict(os.environ)))"', "emsdk-env", str(sdk)], env=env, text=True)
    env = json.loads(environment)
    env["PATH"] = str(cmake / "bin") + os.pathsep + env["PATH"]
    compiler = str(sdk / "upstream/emscripten/em++")
    version = subprocess.check_output([compiler, "--version"], env=env, text=True).splitlines()[0]
    if pins["emsdk"]["version"] not in version:
        raise RuntimeError("Emscripten version mismatch")
    build = source / "build-web"
    recipe = hashlib.sha256(json.dumps([pins, CONFIGURE], sort_keys=True).encode()).hexdigest()
    configured = cache / "cvc5.configure-sha256"
    matches = configured.is_file() and configured.read_text().strip() == recipe and (build / "CMakeCache.txt").is_file()
    if args.link_only and not matches:
        raise RuntimeError("--link-only requires a matching configured library build")
    if not matches:
        run(["./configure.sh", *CONFIGURE], cwd=source, env=env)
        verify_dependency_pins(build, pins["dependencies"])
        configured.write_text(recipe + "\n")
    verify_dependency_pins(build, pins["dependencies"])
    if not args.link_only:
        run([str(cmake / "bin/cmake"), "--build", str(build), "--target", "cvc5parser", "--parallel", "2"], cwd=source, env=env)
    libraries = [build / "src/parser/libcvc5parser.a", build / "src/libcvc5.a", build / "deps/lib/libcadical.a", build / "deps/lib/libgmp.a"]
    if not all(library.is_file() for library in libraries):
        raise RuntimeError("the cvc5 static library build is incomplete")
    output = ROOT / "target/web-backends/cvc5"
    output.mkdir(parents=True, exist_ok=True)
    object_file = output / "session.o"
    run([compiler, "-std=c++17", "-O3", "-fexceptions", "-Wall", "-Wextra", "-Werror", "-I" + str(source / "include"), "-I" + str(build / "include"), "-c", str(ROOT / "web/cvc5/session.cpp"), "-o", str(object_file)], cwd=source, env=env)
    run([compiler, *LINK_FLAGS, str(object_file), *map(str, libraries), "-o", str(output / "cvc5.mjs")], cwd=source, env=env)
    copy_notices(source, build, output)
    metadata = {"schema": 1, "sources": pins, "compiler": version, "configure": CONFIGURE, "linkFlags": LINK_FLAGS, "wrapperSha256": digest(ROOT / "web/cvc5/session.cpp"), "artifacts": {name: {"bytes": (output / name).stat().st_size, "sha256": digest(output / name)} for name in ("cvc5.mjs", "cvc5.wasm", "session.o")}}
    (output / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"cvc5 worker module: {output}", flush=True)


if __name__ == "__main__":
    main()
