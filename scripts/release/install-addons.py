#!/usr/bin/env python3
"""Install release addon assets beside Focus Editor's logs; preserve addon state."""
import argparse
import fcntl
import json
import platform
import tempfile
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
        target_platform = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu", ("Linux", "aarch64"): "aarch64-unknown-linux-gnu", ("Darwin", "arm64"): "aarch64-apple-darwin"}.get((platform.system(), platform.machine()))
        if manifest.get("target") and manifest["target"] != target_platform:
            raise ValueError("Addon package targets a different platform")
        addon_root = Path(destination) / "addons"
        addon_root.mkdir(parents=True, exist_ok=True)
        locks = addon_root / ".locks"
        locks.mkdir(exist_ok=True)
        if locks.is_symlink():
            raise ValueError("Invalid addon locks directory")
        lock_path = locks / f"{addon_id}.lock"
        if lock_path.is_symlink():
            raise ValueError("Invalid addon lock path")
        with lock_path.open("a+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            receipts = Path(destination) / "addon-receipts"
            receipts.mkdir(parents=True, exist_ok=True)
            transactions = addon_root / ".transactions"
            transactions.mkdir(exist_ok=True)
            if any(p.is_symlink() for p in (receipts, transactions, target)):
                raise ValueError("Installation directory contains a symlink")
            journal_path = transactions / f"{addon_id}.json"
            if journal_path.exists():
                raise ValueError("Interrupted installation exists; open the editor to recover it first")
            stage = Path(tempfile.mkdtemp(prefix=".install-", dir=addon_root))
            try:
                payload = stage / "payload"
                payload.mkdir()
                for relative, path in validated:
                    output = payload / relative
                    output.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, output)
                receipt = receipts / f"{addon_id}.json"
                previous_receipt = stage / "receipt.previous.json"
                if receipt.exists():
                    shutil.copy2(receipt, previous_receipt)
                journal = {"id": addon_id, "stage": stage.name, "had_previous": target.exists(), "had_receipt": receipt.exists()}
                with tempfile.NamedTemporaryFile(mode="w", dir=transactions, delete=False) as journal_file:
                    try:
                        json.dump(journal, journal_file)
                        journal_file.flush()
                        os.fsync(journal_file.fileno())
                        os.replace(journal_file.name, journal_path)
                    finally:
                        Path(journal_file.name).unlink(missing_ok=True)
                try:
                    if target.exists():
                        target.rename(stage / "previous")
                    payload.rename(target)
                    temporary_receipt = receipts / f".{addon_id}.tmp"
                    temporary_receipt.write_text(json.dumps({"source": "local", "release": None, "package": None}))
                    temporary_receipt.replace(receipt)
                    journal_path.unlink()
                except Exception:
                    if (stage / "previous").exists():
                        if target.exists():
                            shutil.rmtree(target)
                        (stage / "previous").rename(target)
                    elif not journal["had_previous"] and target.exists():
                        shutil.rmtree(target)
                    if previous_receipt.exists():
                        shutil.copy2(previous_receipt, receipt)
                    elif not journal["had_receipt"]:
                        receipt.unlink(missing_ok=True)
                    journal_path.unlink(missing_ok=True)
                    raise
            finally:
                # A failed rollback leaves the journal and its backup for host recovery.
                if not journal_path.exists():
                    shutil.rmtree(stage)
        installed.append(target)
    return installed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    here = Path(__file__).resolve().parent
    source = here / "addons"
    if not source.exists():
        source = here / "Focus Editor.app/Contents/Resources/addons"
    parser.add_argument("--addon", action="append", help="Install only this addon ID; repeat for several")
    parser.add_argument("--source", type=Path, default=source)
    parser.add_argument("--data-root", type=Path, default=data_root())
    args = parser.parse_args()
    for path in install(args.source, args.data_root, args.addon):
        print(f"Installed {path}")


if __name__ == "__main__":
    main()
