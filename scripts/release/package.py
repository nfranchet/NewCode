#!/usr/bin/env python3
"""Package a native release build; never include workspace data or credentials."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
from addon_release import TARGETS, RELEASE_TARGETS, safe_tag, manifests, assets as addon_files, package_addons


def asset_name(tag, target):
    # Git tags may contain slashes; asset names must be single safe filenames.
    tag = safe_tag(tag)
    platform, extension = TARGETS[target]
    return f"focus-editor-{tag}-{platform}{extension}"


def run(*args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs).strip()


def verify_binary(binary, target, name="focus-editor"):
    description = run("file", "-b", str(binary))
    print(description)
    architecture = {
        "x86_64-unknown-linux-gnu": "x86-64",
        "aarch64-unknown-linux-gnu": "aarch64",
        "aarch64-apple-darwin": "arm64",
    }[target]
    if architecture not in description:
        raise RuntimeError(f"Wrong executable architecture: expected {architecture}")
    if target.endswith("linux-gnu"):
        libraries = run("ldd", str(binary))
        print(libraries)
        if "libghostty-vt.so" in libraries:
            raise RuntimeError("Ghostty must be statically linked in release binaries")
        if "not found" in libraries:
            raise RuntimeError("Unresolved Linux shared libraries")
    else:
        if run("lipo", "-archs", str(binary)) != "arm64":
            raise RuntimeError("Expected a native arm64 Mach-O executable")
        libraries = run("otool", "-L", str(binary))
        print(libraries)
        # No runner-local Homebrew or build-directory dependencies may ship.
        for line in libraries.splitlines()[1:]:
            dependency = line.strip().split(" (", 1)[0]
            if not dependency.startswith(("/usr/lib/", "/System/Library/")):
                raise RuntimeError(f"Unbundled macOS dependency: {dependency}")
    version = run(str(binary), "--version")
    if not version.startswith(name + " "):
        raise RuntimeError("Executable version smoke test failed")
    print(version)


ADDON_ASSETS = tuple("addons/" + m["id"] + "/" + name for _, m in manifests() for name in addon_files(m))
ADDON_EXECUTABLES = tuple("addons/" + m["id"] + "/" + m["executable"] for _, m in manifests())


def copy_addons(binary, destination, addon_source=None):
    source = Path(addon_source) if addon_source else binary.parent
    for relative in ADDON_ASSETS:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source / relative, target)


def package_linux(binary, output, stage, addon_source=None):
    shutil.copy2(binary, stage / "focus-editor")
    shutil.copy2("scripts/release/INSTALL-linux.txt", stage / "INSTALL.txt")
    shutil.copy2("LICENSE", stage / "LICENSE")
    shutil.copy2("THIRD_PARTY_NOTICES.txt", stage / "THIRD_PARTY_NOTICES.txt")
    shutil.copy2("scripts/release/install-addons.py", stage / "install-addons.py")
    copy_addons(binary, stage, addon_source)
    assets = ("focus-editor", "INSTALL.txt", "LICENSE", "THIRD_PARTY_NOTICES.txt", "install-addons.py") + ADDON_ASSETS
    with tarfile.open(output, "w:gz") as archive:
        for name in assets:
            archive.add(stage / name, arcname=name)
    with tarfile.open(output) as archive:
        if set(archive.getnames()) != set(assets):
            raise RuntimeError("Unexpected Linux archive contents")
        if not archive.getmember("focus-editor").mode & 0o111:
            raise RuntimeError("Packaged executable lost its execute permission")
        for name in ADDON_EXECUTABLES:
            if not archive.getmember(name).mode & 0o111:
                raise RuntimeError("Packaged addon lost its execute permission")


def package_macos(binary, output, stage):
    metadata = json.loads(run("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "focus-editor")
    app = stage / "Focus Editor.app"
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Resources").mkdir()
    executable = contents / "MacOS/focus-editor"
    shutil.copy2(binary, executable)
    shutil.copy2("LICENSE", contents / "Resources/LICENSE")
    shutil.copy2("THIRD_PARTY_NOTICES.txt", contents / "Resources/THIRD_PARTY_NOTICES.txt")
    copy_addons(binary, contents / "Resources")
    # Rust's macOS target supports 11.0 and later; keep bundle and binary aligned.
    with (contents / "Info.plist").open("wb") as stream:
        plistlib.dump({
            "CFBundleIdentifier": "io.github.nfranchet.focus-editor",
            "CFBundleName": "Focus Editor",
            "CFBundleDisplayName": "Focus Editor",
            "CFBundleExecutable": "focus-editor",
            "CFBundlePackageType": "APPL",
            "CFBundleInfoDictionaryVersion": "6.0",
            "CFBundleShortVersionString": version.split("-", 1)[0].split("+", 1)[0],
            "CFBundleVersion": version.split("-", 1)[0].split("+", 1)[0],
            "LSMinimumSystemVersion": "11.0",
            "NSHighResolutionCapable": True,
        }, stream)
    run("plutil", "-lint", str(contents / "Info.plist"))
    for name in ADDON_EXECUTABLES:
        run("codesign", "--force", "--sign", "-", str(contents / "Resources" / name))
    run("codesign", "--force", "--sign", "-", str(app))
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", str(app))
    os.symlink("/Applications", stage / "Applications")
    shutil.copy2("scripts/release/INSTALL-macos.txt", stage / "INSTALL.txt")
    shutil.copy2("LICENSE", stage / "LICENSE")
    shutil.copy2("THIRD_PARTY_NOTICES.txt", stage / "THIRD_PARTY_NOTICES.txt")
    shutil.copy2("scripts/release/install-addons.py", stage / "install-addons.py")
    run("hdiutil", "create", "-volname", "Focus Editor", "-srcfolder", str(stage),
        "-format", "UDZO", "-ov", str(output))
    run("hdiutil", "verify", str(output))
    # Verify the mounted image, including its app signature and drag-and-drop link.
    mount = stage.parent / "mounted"
    mount.mkdir()
    run("hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount), str(output))
    try:
        assert (mount / "Applications").is_symlink()
        assert os.readlink(mount / "Applications") == "/Applications"
        run("codesign", "--verify", "--deep", "--strict", str(mount / "Focus Editor.app"))
        assert run(str(mount / "Focus Editor.app/Contents/MacOS/focus-editor"), "--version").startswith("focus-editor ")
    finally:
        run("hdiutil", "detach", str(mount))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("tag")
    args = parser.parse_args()
    binary = Path("target", args.target, "release/focus-editor").resolve(strict=True)
    verify_binary(binary, args.target)
    for name in ADDON_EXECUTABLES:
        executable = binary.parent / name
        verify_binary(executable, args.target, executable.name)
    destination = Path("dist").resolve()
    destination.mkdir(exist_ok=True)
    output = destination / asset_name(args.tag, args.target)
    with tempfile.TemporaryDirectory(prefix="focus-release-") as temporary:
        stage = Path(temporary) / "payload"
        stage.mkdir()
        if args.target.endswith("apple-darwin"):
            package_macos(binary, output, stage)
        else:
            package_linux(binary, output, stage)
    package_addons(binary.parent, destination, args.tag, args.target)
    print(f"Verified release asset: {output.name}")


if __name__ == "__main__":
    main()
