"""Freeze already-built Custom benchmark executables with exact source provenance."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def freeze(destination):
    repository = Path(__file__).resolve().parent.parent
    destination = Path(destination).resolve()
    binaries = repository / "target/release/examples"
    examples = ["profile_obligation", "profile_case", "profile_witness"]
    for name in examples:
        if not (binaries / (name + ".exe")).is_file():
            raise ValueError(f"Build {name} before freezing")
    destination.mkdir(parents=True, exist_ok=False)
    shutil.copytree(repository / "crates", destination / "crates")
    for name in ("Cargo.toml", "Cargo.lock"):
        shutil.copy2(repository / name, destination / name)
    (destination / "bin").mkdir()
    for name in examples:
        shutil.copy2(binaries / (name + ".exe"), destination / "bin")
    for name, arguments in [
        ("revision.txt", ["rev-parse", "HEAD"]),
        ("source.diff", ["diff", "--binary", "--", "crates"]),
        ("source-status.txt", ["status", "--short", "--", "crates"]),
    ]:
        (destination / name).write_bytes(subprocess.check_output(["git", "-C", str(repository), *arguments]))
    hashes = {p.relative_to(destination).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in destination.rglob("*") if p.is_file()}
    (destination / "hashes.json").write_text(json.dumps(hashes, indent=2) + "\n", encoding="utf-8")
    print(f"Frozen {len(hashes)} files at {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    freeze(parser.parse_args().destination)
