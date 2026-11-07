"""Manifest-driven staging and addon release assets. Never package runtime data."""
import hashlib
import json
from pathlib import Path
import re
import tarfile
import tomllib

TARGETS = {"x86_64-unknown-linux-gnu": ("linux-amd64", ".tar.gz"),
           "aarch64-unknown-linux-gnu": ("linux-arm64", ".tar.gz"),
           "aarch64-apple-darwin": ("macos-arm64", ".dmg")}
RELEASE_TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu")


def safe_tag(tag):
    value = re.sub(r"[^A-Za-z0-9._-]", "-", tag)
    if not value:
        raise ValueError("Release tag must not be empty")
    return value


def assets(manifest):
    names = ["addon.toml", manifest["executable"], manifest["icon"], "LICENSE", "THIRD_PARTY_NOTICES.txt"]
    if len(set(names)) != len(names):
        raise ValueError("Addon asset paths overlap")
    for name in names:
        path = Path(name)
        if path.is_absolute() or not path.parts or any(p in ("..", ".") for p in path.parts):
            raise ValueError("Addon assets must stay within the package")
    return names


def manifests(root=Path(".")):
    result = []
    for path in sorted((root / "addons").glob("*/addon.toml")):
        manifest = tomllib.loads(path.read_text())
        if not re.fullmatch(r"[A-Za-z0-9_-]+", manifest["id"]) or manifest["id"] != path.parent.name:
            raise ValueError("Addon directory must match its ID")
        assets(manifest)
        result.append((path, manifest))
    return result


def archive_name(tag, target, addon_id):
    return f"focus-addon-{addon_id}-{safe_tag(tag)}-{TARGETS[target][0]}.tar.gz"


def index_name(target):
    return f"addons-{TARGETS[target][0]}.json"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def package_addons(binary_directory, destination, tag, target, root=Path(".")):
    destination.mkdir(parents=True, exist_ok=True)
    packages = []
    for _, definition in manifests(root):
        source = binary_directory / "addons" / definition["id"]
        manifest = tomllib.loads((source / "addon.toml").read_text())
        if manifest["target"] != target or any(manifest[key] != definition[key] for key in ("id", "version", "protocol", "executable", "icon")):
            raise ValueError("Staged addon identity/platform mismatch")
        name = archive_name(tag, target, manifest["id"])
        path = destination / name
        with tarfile.open(path, "w:gz", format=tarfile.PAX_FORMAT) as archive:
            for relative in assets(manifest):
                asset = source / relative
                if asset.is_symlink() or not asset.is_file() or not asset.resolve().is_relative_to(source.resolve()):
                    raise ValueError("Invalid addon asset")
                archive.add(asset, arcname=relative, recursive=False)
        if path.stat().st_size > 256 * 1024 * 1024:
            raise ValueError("Addon archive exceeds manager size limit")
        packages.append({"id": manifest["id"], "name": manifest["name"],
                         "description": manifest.get("description", ""), "publisher": manifest.get("publisher", ""),
                         "version": manifest["version"], "protocol": manifest["protocol"], "target": target,
                         "archive": name, "size": path.stat().st_size, "sha256": digest(path)})
    catalog = {"schema": 1, "release": tag,
               "editor_version": tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"],
               "packages": packages}
    (destination / index_name(target)).write_text(json.dumps(catalog, indent=2) + "\n")
    return catalog
