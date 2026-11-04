#!/usr/bin/env python3
"""Require both Linux release assets before generating the upload manifest."""
import hashlib
from pathlib import Path
import sys
from package import RELEASE_TARGETS, asset_name


def main():
    directory, tag = Path(sys.argv[1]), sys.argv[2]
    expected = {asset_name(tag, target) for target in RELEASE_TARGETS}
    actual = {p.name for p in directory.iterdir()}
    if actual - {"SHA256SUMS"} != expected:
        raise RuntimeError(f"Unexpected or missing release assets: {sorted(actual)}; expected {sorted(expected)}")
    records = []
    for name in sorted(expected):
        path = directory / name
        if not path.is_file() or path.is_symlink() or path.stat().st_size == 0:
            raise RuntimeError(f"Invalid release asset: {name}")
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        records.append(f"{digest.hexdigest()}  {name}\n")
    (directory / "SHA256SUMS").write_text("".join(records), encoding="utf-8")


if __name__ == "__main__":
    main()
