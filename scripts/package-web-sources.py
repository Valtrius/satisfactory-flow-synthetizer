#!/usr/bin/env python3
"""Create browser distribution notices and source downloads from this checkout.

No .git data, agent memories, environment files, history databases or generated
application outputs enter the source snapshot. Artifacts stay under target/.
"""
from __future__ import annotations

import hashlib
from html import escape
import json
from pathlib import Path
import runpy
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
HELPERS = runpy.run_path(str(ROOT / "scripts/package-web-relink.py"))
digest, fetch, write_zip = (HELPERS[name] for name in ("digest", "fetch", "write_zip"))
ROOT_FILES = {"Cargo.toml", "Cargo.lock", "package.json", "package-lock.json", "LICENSE", "README.md", ".gitignore", ".gitattributes", "prettier.config.mjs", "rustfmt.toml"}
SOURCE_ROOTS = {"crates", "vendor", "frontend", "web", "scripts", "src-tauri", ".github"}
EXCLUDED = {"node_modules", "target", "dist", ".git", ".tmp", "__pycache__"}


def source_files(root: Path = ROOT) -> dict[str, Path]:
    listed = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root).decode("utf-8").split("\0")
    result = {}
    for name in listed:
        if not name:
            continue
        path = Path(name)
        if name not in ROOT_FILES and path.parts[0] not in SOURCE_ROOTS and name not in {"docs/web-backends.md", "docs/web-release.md"}:
            continue
        if any(part in EXCLUDED or part.startswith(".env") for part in path.parts):
            continue
        source = root / path
        if source.is_symlink() or not source.resolve().is_relative_to(root.resolve()):
            raise RuntimeError(f"Source snapshot cannot include an external link: {name}")
        if source.is_file():
            result[name] = source
    return result


def notices(directory: Path) -> list[Path]:
    return sorted(path for path in directory.iterdir() if path.is_file() and path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "AUTHORS")))


def notice_documents(package: dict) -> list[Path]:
    directory = Path(package["manifest_path"]).parent
    files = notices(directory)
    if files:
        return files
    # Some npm tarballs declare a standard license but omit a standalone
    # license file. Keep the exact package metadata and its shipped README in
    # the visible notices instead of inventing a copyright notice.
    if package.get("license") == "MIT" and Path(package["manifest_path"]).name == "package.json":
        readme = next((path for path in directory.iterdir() if path.is_file() and path.name.lower().startswith("readme")), None)
        return [Path(package["manifest_path"]), *([readme] if readme else [])]
    raise RuntimeError(f"Missing license notices for dependency {package['name']}")


def rust_packages() -> list[dict]:
    data = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "wasm32-unknown-unknown"], cwd=ROOT))
    packages = {package["id"]: package for package in data["packages"]}
    nodes = {node["id"]: node for node in data["resolve"]["nodes"]}
    pending = [key for key, value in packages.items() if value["name"] in {"solver-browser", "solver-web"}]
    visited = set()
    while pending:
        key = pending.pop()
        if key in visited:
            continue
        visited.add(key)
        pending.extend(nodes[key]["dependencies"])
    return [packages[key] for key in sorted(visited) if not Path(packages[key]["manifest_path"]).resolve().is_relative_to(ROOT)]


def npm_packages() -> list[dict]:
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    result = []
    for path, metadata in lock["packages"].items():
        if not path.startswith("node_modules/") or metadata.get("dev") or metadata.get("link"):
            continue
        manifest = ROOT / path / "package.json"
        if not manifest.is_file():
            if metadata.get("optional"):
                continue
            raise RuntimeError(f"Install the locked production dependency: {path}")
        package = json.loads(manifest.read_text(encoding="utf-8"))
        if package["version"] != metadata["version"]:
            raise RuntimeError(f"Installed dependency does not match package-lock.json: {path}")
        result.append({**package, "manifest_path": str(manifest)})
    return result


def main() -> None:
    subprocess.run([sys.executable, "scripts/package-web-relink.py"], cwd=ROOT, check=True)
    output = ROOT / "target/web-backends/distribution"
    downloads = output / "sources"
    downloads.mkdir(parents=True, exist_ok=True)
    entries = []
    inventory = {}

    def publish(name: str, source: Path) -> str:
        checksum = digest(source)
        relative = f"sources/{name}-{checksum[:16]}{''.join(source.suffixes)}"
        destination = output / relative
        import shutil
        shutil.copyfile(source, destination)
        inventory[relative] = {"bytes": destination.stat().st_size, "sha256": checksum}
        return relative

    relink = publish("cvc5-relink", ROOT / "target/web-backends/relink/cvc5-relink.zip")
    inputs = source_files()
    source_checksums = {name: digest(path) for name, path in inputs.items()}
    fingerprint = hashlib.sha256(json.dumps(source_checksums, sort_keys=True).encode()).hexdigest()
    snapshot = ROOT / "target/web-backends/application-source.zip"
    write_zip(snapshot, {**inputs, "SOURCE-INVENTORY.json": (json.dumps(source_checksums, indent=2) + "\n").encode()})
    application = publish("application", snapshot)

    pins = json.loads((ROOT / "web/distribution/sources.json").read_text(encoding="utf-8"))
    elk_version = json.loads((ROOT / "node_modules/elkjs/package.json").read_text(encoding="utf-8"))["version"]
    if elk_version != pins["elkjs"]["version"]:
        raise RuntimeError("Update and qualify the pinned ELK source archives for the installed elkjs version")
    upstream = {}
    for name, pin in pins.items():
        archive = fetch(ROOT / f"target/web-backends/source-cache/{name}.tar.gz", pin)
        upstream[name] = publish(name, archive)

    texts = [("Satisfactory Flow Synthetizer", "MIT", [("LICENSE", (ROOT / "LICENSE").read_text(encoding="utf-8"))])]
    dependency_sources: dict[str, Path | bytes] = {}
    for ecosystem, packages in [("Rust", rust_packages()), ("JavaScript", npm_packages())]:
        for package in packages:
            directory = Path(package["manifest_path"]).parent
            files = notice_documents(package)
            label = f"{package['name']} {package['version']}"
            license_name = package.get("license") or "See included notices"
            texts.append((label, license_name, [(file.name, file.read_text(encoding="utf-8", errors="replace")) for file in files]))
            entries.append({"ecosystem": ecosystem, "name": package["name"], "version": package["version"], "license": license_name})
            prefix = f"{ecosystem.lower()}/{package['name']}-{package['version']}"
            for file in directory.rglob("*"):
                relative = file.relative_to(directory)
                if any(part in EXCLUDED for part in relative.parts) or file.is_symlink():
                    continue
                if file.is_file():
                    dependency_sources[f"{prefix}/{relative.as_posix()}"] = file
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()) / "share/doc/rust"
    runtime = sysroot / "COPYRIGHT-library.html"
    if not runtime.is_file():
        raise RuntimeError("Install Rust's runtime copyright notices before packaging")
    texts.append(("Rust standard library", "MIT OR Apache-2.0 and component notices", [(runtime.name, runtime.read_text(encoding="utf-8"))]))
    for path in (sysroot / "licenses").glob("*"):
        if path.is_file():
            dependency_sources["rust-runtime/licenses/" + path.name] = path
    dependency_sources["rust-runtime/COPYRIGHT-library.html"] = runtime
    third_party_zip = ROOT / "target/web-backends/dependency-sources.zip"
    write_zip(third_party_zip, dependency_sources)
    dependencies = publish("dependency-sources", third_party_zip)
    for path in sorted((ROOT / "target/web-backends/cvc5/licenses").rglob("*")):
        if path.is_file():
            texts.append(("cvc5 bundle / " + path.parent.name, "See component license", [(path.name, path.read_text(encoding="utf-8", errors="replace"))]))
    sections = []
    for label, license_name, documents in texts:
        details = ''.join(f'<details><summary>{escape(name)}</summary><pre>{escape(text)}</pre></details>' for name, text in documents)
        sections.append(f'<section><h2>{escape(label)}</h2><p>{escape(str(license_name))}</p>{details}</section>')
    links = ''.join(f'<li><a href="{escape(path)}">{escape(label)}</a></li>' for label, path in [("Application source and build scripts", application), ("cvc5/GMP source, static libraries and relink recipe", relink), ("Installed Rust/JavaScript dependency sources and notices", dependencies), ("ELK Java source, EPL-2.0", upstream["elk"]), ("elkjs source, EPL-2.0", upstream["elkjs"])])
    html = f'''<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Licenses and source | Satisfactory Flow Synthetizer</title><style>:root{{color-scheme:light dark}}body{{font:16px/1.6 system-ui,sans-serif;max-width:80rem;margin:auto;padding:1.5rem}}a{{color:LinkText}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font:0.85rem/1.5 monospace}}section{{border-top:1px solid GrayText;margin-top:2rem}}summary{{cursor:pointer;padding:.5rem 0}}</style></head><body><main><h1>Licenses and source</h1><p><a href="./">Return to the application</a></p><p>The application uses the MIT license. Third-party components retain their own notices. These files carry no warranty.</p><h2>Source downloads for this build</h2><ul>{links}</ul><p>GMP is used under LGPL-3.0-or-later. Its source, the application wrapper object, static libraries and a standalone relink script are included above. Modifications and reverse engineering to debug those modifications are permitted. ELK and elkjs source is available under EPL-2.0 in the linked archives.</p><p>Source downloads are not included in the offline runtime cache. Save them separately while online. Exact archive SHA-256 values are in <a href="source-manifest.json">the source manifest</a>.</p>{''.join(sections)}</main></body></html>'''
    (output / "licenses.html").write_text(html, encoding="utf-8")
    inventory["licenses.html"] = {"bytes": (output / "licenses.html").stat().st_size, "sha256": digest(output / "licenses.html")}
    record = {"schema": 1, "sourceFingerprint": fingerprint, "sourceFiles": source_checksums, "sourceCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "dependencies": entries, "upstream": pins, "files": inventory, "cvc5Artifacts": json.loads((ROOT / "target/web-backends/cvc5/build.json").read_text())["artifacts"]}
    (output / "source-manifest.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"Prepared source downloads and notices for {len(entries)} dependency packages. Fingerprint: {fingerprint}")


if __name__ == "__main__":
    main()
