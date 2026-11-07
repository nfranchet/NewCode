#!/usr/bin/env python3
"""Install release addon assets beside Focus Editor's logs; preserve addon state."""
import argparse
import os
from pathlib import Path
import re
import shutil
import sys
import tomllib


def data_root():
    if sys.platform == "darwin":
        return Path.home() / "Library/Application Support/focus-editor"
    value = os.environ.get("XDG_DATA_HOME", "")
    base = Path(value) if value and Path(value).is_absolute() else Path.home() / ".local/share"
    return base / "focus-editor"


def install(source, destination, ids=None):
    source = Path(source).resolve(strict=True)
    installed = []
    for directory in sorted(source.iterdir()):
        if not directory.is_dir() or (ids and directory.name not in ids):
            continue
        manifest = tomllib.loads((directory / "addon.toml").read_text())
        addon_id = manifest["id"]
        if not re.fullmatch(r"[A-Za-z0-9_-]+", addon_id) or directory.name != addon_id:
            raise ValueError("Invalid addon ID/directory")
        if manifest["protocol"] != 1:
            raise ValueError("Unsupported addon protocol")
        assets = ["addon.toml", manifest["executable"], manifest["icon"]]
        assets += [name for name in ("LICENSE", "THIRD_PARTY_NOTICES.txt") if (directory / name).is_file()]
        target = Path(destination) / "addons" / addon_id
        if not target.resolve().is_relative_to(Path(destination).resolve()):
            raise ValueError("Installation destination escapes application data directory")
        resolved_directory = directory.resolve(strict=True)
        validated = []
        for asset in assets:
            relative = Path(asset)
            if relative.is_absolute() or ".." in relative.parts or not relative.parts:
                raise ValueError("Addon asset must be a relative path")
            path = (directory / relative).resolve(strict=True)
            if not path.is_relative_to(resolved_directory) or not path.is_file():
                raise ValueError("Addon asset escapes its directory")
            validated.append((relative, path))
        # Manifest last: discovery sees the installed version only once assets exist.
        validated.sort(key=lambda asset: asset[0] == Path("addon.toml"))
        for relative, path in validated:
            output = target / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            if output.is_symlink() or not output.resolve().is_relative_to(Path(destination).resolve()):
                raise ValueError("Installation destination escapes application data directory")
            temporary = output.with_name(output.name + ".installing")
            if temporary.is_symlink():
                raise ValueError("Invalid installation temporary file")
            shutil.copy2(path, temporary)
            temporary.replace(output)
        installed.append(target)
    return installed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    here = Path(__file__).resolve().parent
    source = here / "addons"
    if not source.exists():
        source = here / "Focus Editor.app/Contents/Resources/addons"
    parser.add_argument("--addon", action="append", help="Install only this addon ID; repeat to select several")
    parser.add_argument("--source", type=Path, default=source)
    parser.add_argument("--data-root", type=Path, default=data_root())
    args = parser.parse_args()
    for path in install(args.source, args.data_root, args.addon):
        print(f"Installed {path}")


if __name__ == "__main__":
    main()
