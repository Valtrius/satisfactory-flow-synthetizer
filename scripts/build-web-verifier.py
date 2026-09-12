#!/usr/bin/env python3
"""Build the verifier without native solver dependencies and emit ES-module bindings."""
from __future__ import annotations

import hashlib
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.2.127"
WINDOWS_ARCHIVE_SHA256 = "fd52ef9896cbb0f4f59ef115bdf2f4664a76d716b6201c35e5936018ad680cfc"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument('--qualification', action='store_true', help='Build the separate portable-search qualification module, not application assets')
    selection.add_argument('--browser', action='store_true', help='Build the production browser solver, separately from the small share verifier')
    selection.add_argument('--verifier-qualification', action='store_true', help='Expose low-level verification APIs in a separate qualification distribution')
    arguments = parser.parse_args()
    package = 'solver-portable-tests' if arguments.qualification else 'solver-browser' if arguments.browser else 'solver-web'
    directory = 'portable' if arguments.qualification else 'solver' if arguments.browser else 'verifier-qualification' if arguments.verifier_qualification else 'verifier'
    module = package.replace('-', '_')
    tools = ROOT / "target/web-tools"
    tools.mkdir(parents=True, exist_ok=True)
    executable = shutil.which("wasm-bindgen")
    if executable and subprocess.check_output([executable, "--version"], text=True).strip() != f"wasm-bindgen {VERSION}":
        executable = None
    if executable is None and os.name == "nt":
        name = f"wasm-bindgen-{VERSION}-x86_64-pc-windows-msvc"
        archive = tools / f"{name}.tar.gz"
        executable = str(tools / name / "wasm-bindgen.exe")
        if not archive.is_file():
            url = f"https://github.com/wasm-bindgen/wasm-bindgen/releases/download/{VERSION}/{name}.tar.gz"
            with urllib.request.urlopen(url, timeout=120) as response:
                archive.write_bytes(response.read())
        if hashlib.sha256(archive.read_bytes()).hexdigest() != WINDOWS_ARCHIVE_SHA256:
            raise RuntimeError("wasm-bindgen archive checksum mismatch")
        with tarfile.open(archive) as distribution:
            distribution.extractall(tools, filter="data")
    elif executable is None:
        executable = str(tools / "bin/wasm-bindgen")
        if not Path(executable).is_file():
            subprocess.run(["cargo", "install", "wasm-bindgen-cli", "--version", VERSION, "--locked", "--root", str(tools), "-j", "2"], check=True, cwd=ROOT)
    if subprocess.check_output([executable, "--version"], text=True).strip() != f"wasm-bindgen {VERSION}":
        raise RuntimeError(f"wasm-bindgen {VERSION} is required")
    features = ['--features', 'qualification'] if arguments.verifier_qualification else []
    subprocess.run(["cargo", "build", "-p", package, *features, "--target", "wasm32-unknown-unknown", "--release", "--locked", "-j", "2"], check=True, cwd=ROOT)
    output = ROOT / 'target/web-backends' / directory
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run([executable, "--target", "web", "--out-dir", str(output), str(ROOT / 'target/wasm32-unknown-unknown/release' / f'{module}.wasm')], check=True, cwd=ROOT)
    print(f"{package} bindings: {output}")


if __name__ == "__main__":
    main()
