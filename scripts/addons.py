#!/usr/bin/env python3
"""Build addons independently, stage release assets, and optionally install them."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--target")
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--data-root", type=Path, help="Override application data root for isolated installation tests")
    args = parser.parse_args()
    command = ["cargo", "build", "--locked", "--manifest-path", str(ROOT / "addons/github/Cargo.toml"), "--target-dir", str(ROOT / "target")]
    if args.release:
        command.append("--release")
    if args.target:
        command += ["--target", args.target]
    subprocess.run(command, cwd=ROOT, check=True)
    output = ROOT / "target"
    if args.target:
        output /= args.target
    output /= "release" if args.release else "debug"
    stage = output / "addons/github"
    stage.mkdir(parents=True, exist_ok=True)
    for source in (output / "focus-addon-github", ROOT / "addons/github/addon.toml", ROOT / "addons/github/icon.png", ROOT / "LICENSE"):
        shutil.copy2(source, stage / source.name)
    print(f"Staged {stage}", flush=True)
    if args.install:
        script = ROOT / "scripts/release/install-addons.py"
        command = [sys.executable, str(script), "--source", str(stage.parent)]
        if args.data_root:
            command += ["--data-root", str(args.data_root)]
        subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
