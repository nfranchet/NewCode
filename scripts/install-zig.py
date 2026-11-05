#!/usr/bin/env python3
"""Install the pinned Ghostty build toolchain into an explicit directory."""
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
from urllib.request import urlopen

VERSION = "0.15.2"


def install(destination):
    destination = Path(destination).resolve()
    if (destination / "zig").is_file():
        version = subprocess.check_output([str(destination / "zig"), "version"], text=True).strip()
        if version == VERSION:
            return
        raise RuntimeError(f"{destination} contains Zig {version}, expected {VERSION}")
    if destination.exists() and any(destination.iterdir()):
        raise RuntimeError(f"Refusing to overwrite nonempty directory: {destination}")
    machine = {"AMD64": "x86_64", "arm64": "aarch64"}.get(platform.machine(), platform.machine())
    system = {"Linux": "linux", "Darwin": "macos"}[platform.system()]
    with urlopen("https://ziglang.org/download/index.json", timeout=60) as response:
        archive = json.load(response)[VERSION][f"{machine}-{system}"]
    with tempfile.TemporaryDirectory(prefix="focus-zig-") as temporary:
        package = Path(temporary) / "zig.tar.xz"
        with urlopen(archive["tarball"], timeout=60) as response, package.open("wb") as output:
            shutil.copyfileobj(response, output)
        if hashlib.sha256(package.read_bytes()).hexdigest() != archive["shasum"]:
            raise RuntimeError("Zig archive checksum mismatch")
        extracted = Path(temporary) / "unpacked"
        extracted.mkdir()
        with tarfile.open(package) as tar:
            tar.extractall(extracted, filter="data")
        (toolchain,) = extracted.iterdir()
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(toolchain, destination, dirs_exist_ok=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python3 scripts/install-zig.py DESTINATION")
    install(sys.argv[1])
    print(Path(sys.argv[1]).resolve())
