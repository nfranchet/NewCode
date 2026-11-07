#!/usr/bin/env python3
"""Build addons independently, stage release assets, and optionally install them."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--addon", action="append", choices=[p.name for p in (ROOT / "addons").iterdir() if (p / "Cargo.toml").exists()], help="Build/install only a selected addon; repeat to select several")
    parser.add_argument("--test", action="store_true", help="Run SDK, shared UI, and addon tests")
    parser.add_argument("--target")
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--data-root", type=Path, help="Override application data root for isolated installation tests")
    args = parser.parse_args()
    if args.test and args.install:
        parser.error("--test and --install cannot be combined")
    projects = sorted((ROOT / "addons").glob("*/Cargo.toml"))
    if args.addon:
        projects = [p for p in projects if p.parent.name in args.addon]
    if args.test:
        projects = [ROOT / "crates/addon-sdk/Cargo.toml", ROOT / "crates/addon-support/Cargo.toml"] + projects
    for project in projects:
        if args.test:
            subprocess.run(["cargo", "fmt", "--check", "--manifest-path", str(project)], cwd=ROOT, check=True)
        command = ["cargo", "test" if args.test else "build", "--locked", "--manifest-path", str(project), "--target-dir", str(ROOT / "target")]
        if args.release:
            command.append("--release")
        if args.target:
            command += ["--target", args.target]
        subprocess.run(command, cwd=ROOT, check=True)
        if args.test:
            continue
        manifest = tomllib.loads((project.parent / "addon.toml").read_text())
        output = ROOT / "target"
        if args.target:
            output /= args.target
        output /= "release" if args.release else "debug"
        stage = output / "addons" / manifest["id"]
        stage.mkdir(parents=True, exist_ok=True)
        for source in (output / manifest["executable"], project.parent / "addon.toml", project.parent / manifest["icon"], ROOT / "LICENSE"):
            shutil.copy2(source, stage / source.name)
        print(f"Staged {stage}", flush=True)
    if args.install:
        script = ROOT / "scripts/release/install-addons.py"
        command = [sys.executable, str(script), "--source", str(output / "addons")]
        for addon in args.addon or []:
            command += ["--addon", addon]
        if args.data_root:
            command += ["--data-root", str(args.data_root)]
        subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
