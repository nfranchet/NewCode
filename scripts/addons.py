#!/usr/bin/env python3
"""Build addons independently, stage release assets, and optionally install them."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import json
import tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts/release"))
from addon_release import manifests, assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--target")
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--data-root", type=Path, help="Override application data root for isolated installation tests")
    parser.add_argument("--test", action="store_true")
    parser.add_argument("--addon", action="append", choices=[m["id"] for _, m in manifests(ROOT)])
    args = parser.parse_args()
    if args.test and args.install:
        parser.error("--test and --install cannot be combined")
    selected = [(p,m) for p,m in manifests(ROOT) if not args.addon or m["id"] in args.addon]
    projects = [p.parent / "Cargo.toml" for p, _ in selected]
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
        return
    target = args.target or next(line.split(": ", 1)[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines() if line.startswith("host: "))
    output = ROOT / "target"
    if args.target:
        output /= args.target
    output /= "release" if args.release else "debug"
    for path, manifest in selected:
        if tomllib.loads((path.parent / "Cargo.toml").read_text())["package"]["version"] != manifest["version"]:
            raise ValueError("Addon manifest and Cargo versions differ")
        stage = output / "addons" / manifest["id"]
        stage.mkdir(parents=True, exist_ok=True)
        text = path.read_text()
        if "target" not in manifest:
            text = f"target = {json.dumps(target)}\n" + text
        (stage / "addon.toml").write_text(text)
        sources = {manifest["executable"]: output / Path(manifest["executable"]).name,
                   manifest["icon"]: path.parent / manifest["icon"],
                   "LICENSE": path.parent / "LICENSE" if (path.parent / "LICENSE").is_file() else ROOT / "LICENSE",
                   "THIRD_PARTY_NOTICES.txt": path.parent / "THIRD_PARTY_NOTICES.txt" if (path.parent / "THIRD_PARTY_NOTICES.txt").is_file() else ROOT / "THIRD_PARTY_NOTICES.txt"}
        for relative in assets(manifest):
            if relative == "addon.toml":
                continue
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(sources[relative], destination)
        print(f"Staged {stage}", flush=True)
    if args.install:
        script = ROOT / "scripts/release/install-addons.py"
        command = [sys.executable, str(script), "--source", str(output / "addons")]
        for _, manifest in selected:
            command += ["--addon", manifest["id"]]
        if args.data_root:
            command += ["--data-root", str(args.data_root)]
        subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
