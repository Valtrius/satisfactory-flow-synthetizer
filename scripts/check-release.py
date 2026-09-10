"""Check local package versions and require reviewed notes for a release tag."""
import argparse
import json
import re
import tomllib
from pathlib import Path


def check_release(root: Path, tag: str | None = None) -> str:
    def json_file(name: str) -> dict:
        return json.loads((root / name).read_text(encoding="utf-8"))

    version = json_file("package.json")["version"]
    versions = {"package.json": version}
    for name in ("frontend/package.json", "src-tauri/tauri.conf.json"):
        versions[name] = json_file(name)["version"]
    for name in ("package-lock.json", "frontend/package-lock.json"):
        lock = json_file(name)
        versions[name] = lock["version"]
        versions[f'{name} packages[""]'] = lock["packages"][""]["version"]
        if name == "package-lock.json":
            versions[f'{name} frontend'] = lock["packages"]["frontend"]["version"]
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    workspace_version = workspace["package"]["version"]
    versions["Cargo.toml workspace"] = workspace_version
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    for member in workspace["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        declared = package["version"]
        versions[f"{member}/Cargo.toml"] = workspace_version if declared == {"workspace": True} else declared
        locked = [item for item in lock if item["name"] == package["name"] and "source" not in item]
        if len(locked) != 1:
            raise ValueError(f"Expected one workspace package {package['name']} in Cargo.lock")
        versions[f"Cargo.lock {package['name']}"] = locked[0]["version"]
    badge = re.findall(r"badge/version-([^/\s]+)-blue\.svg", (root / "README.md").read_text(encoding="utf-8"))
    if len(badge) != 1:
        raise ValueError("Expected one README version badge")
    versions["README badge"] = badge[0]
    mismatches = [f"{name}: {value!r}" for name, value in versions.items() if value != version]
    if mismatches:
        raise ValueError(f"Expected version {version}: " + "; ".join(mismatches))
    if tag is not None:
        if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
            raise ValueError("Release tags must use a stable X.Y.Z version")
        if tag != version:
            raise ValueError(f"Tag {tag} does not match package version {version}")
        notes = root / "docs" / "release-notes" / f"{tag}.md"
        if not notes.is_file() or not notes.read_text(encoding="utf-8").strip():
            raise ValueError(f"Missing nonempty release notes: {notes}")
    return version


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    args = parser.parse_args()
    try:
        print(f"Release metadata OK: {check_release(Path(__file__).resolve().parents[1], args.tag)}")
    except (ValueError, KeyError, OSError) as error:
        parser.exit(1, f"Release metadata failed: {error}\n")
