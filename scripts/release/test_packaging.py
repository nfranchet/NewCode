"""Release safety checks, independent of the host's available CPU architectures."""
import hashlib
import shutil
import json
from addon_release import package_addons, manifests, assets as addon_files, index_name
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import fcntl

from package import RELEASE_TARGETS, ADDON_ASSETS, ADDON_EXECUTABLES, asset_name, package_linux
import importlib.util

spec = importlib.util.spec_from_file_location("install_addons", Path(__file__).with_name("install-addons.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class PackagingTests(unittest.TestCase):
    def test_linux_archive_keeps_executable_permissions_and_excludes_other_data(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "built-binary"
            binary.write_bytes(b"fixture executable")
            binary.chmod(0o755)
            stage = root / "stage"
            stage.mkdir()
            (stage / "auth.json").write_text("must not ship")
            output = root / "release.tar.gz"
            addon = root / "addon"
            addon.mkdir()
            for name in ADDON_ASSETS:
                (addon / name).parent.mkdir(parents=True, exist_ok=True)
                (addon / name).write_bytes(b"fixture addon")
                if name in ADDON_EXECUTABLES:
                    (addon / name).chmod(0o755)
            package_linux(binary, output, stage, addon)
            with tarfile.open(output) as archive:
                self.assertEqual(set(archive.getnames()), {"focus-editor", "INSTALL.txt", "LICENSE", "THIRD_PARTY_NOTICES.txt", "install-addons.py"} | set(ADDON_ASSETS))
                self.assertEqual(archive.extractfile("LICENSE").read(), Path("LICENSE").read_bytes())
                self.assertTrue(archive.getmember("focus-editor").mode & 0o111)
                self.assertEqual(archive.extractfile("focus-editor").read(), binary.read_bytes())
                for name in ADDON_EXECUTABLES:
                    self.assertTrue(archive.getmember(name).mode & 0o111)

    def test_addon_installer_places_assets_beside_logs_and_preserves_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "payload/github"
            source.mkdir(parents=True)
            (source / "addon.toml").write_text("id='github'\nprotocol=1\nexecutable='program'\nicon='icon.png'\n")
            (source / "program").write_bytes(b"executable")
            (source / "program").chmod(0o755)
            (source / "icon.png").write_bytes(b"icon")
            destination = root / "app"
            (destination / "logs").mkdir(parents=True)
            state = destination / "workspaces/project/addon-data/github/state.json"
            state.parent.mkdir(parents=True)
            state.write_text("keep non-secret state")
            installer.install(source.parent, destination)
            self.assertEqual(state.read_text(), "keep non-secret state")
            self.assertFalse((destination / "addon-data").exists())
            self.assertEqual((destination / "addons/github/program").read_bytes(), b"executable")
            self.assertTrue((destination / "addons/github/program").stat().st_mode & 0o111)
            (source / "addon.toml").write_text("id='github'\nprotocol=1\nexecutable='../escape'\nicon='icon.png'\n")
            with self.assertRaises(ValueError):
                installer.install(source.parent, destination)

    def test_installing_one_addon_does_not_install_other_staged_addons(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("first", "second"):
                source = root / "payload" / name
                source.mkdir(parents=True)
                (source / "addon.toml").write_text(f"id='{name}'\nprotocol=1\nexecutable='program'\nicon='icon.png'\n")
                (source / "program").write_bytes(b"executable")
                (source / "icon.png").write_bytes(b"icon")
            installer.install(root / "payload", root / "app", ["first"])
            self.assertTrue((root / "app/addons/first/program").exists())
            self.assertFalse((root / "app/addons/second").exists())

    def test_checksums_require_both_linux_targets_and_reject_macos_assets(self):
        script = Path(__file__).with_name("checksums.py").resolve()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # A release tag containing '/' must still produce safe single filenames.
            names = [asset_name("release/v1.2.3", target) for target in RELEASE_TARGETS]
            self.assertEqual(set(names), {
                "focus-editor-release-v1.2.3-linux-amd64.tar.gz",
                "focus-editor-release-v1.2.3-linux-arm64.tar.gz",
            })
            for name in names:
                self.assertEqual(Path(name).name, name)
                (root / name).write_bytes(name.encode())
            for target in RELEASE_TARGETS:
                build = root / "build" / target
                for path, manifest in manifests():
                    stage = build / "addons" / manifest["id"]
                    stage.mkdir(parents=True)
                    (stage / "addon.toml").write_text(f"target='{target}'\n" + path.read_text())
                    for relative in addon_files(manifest):
                        if relative != "addon.toml":
                            (stage / relative).parent.mkdir(parents=True, exist_ok=True)
                            (stage / relative).write_bytes(b"fixture")
                package_addons(build, root, "release/v1.2.3", target)
            shutil.rmtree(root / "build")

            def generate():
                return subprocess.run([sys.executable, str(script), str(root), "release/v1.2.3"],
                                      capture_output=True, text=True)

            self.assertEqual(generate().returncode, 0)
            manifest = (root / "SHA256SUMS").read_text()
            for name in names:
                digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
                self.assertIn(f"{digest}  {name}\n", manifest)
            # Rerunning is safe, but partial builds must not replace a valid manifest.
            self.assertEqual(generate().returncode, 0)
            (root / names[0]).unlink()
            self.assertNotEqual(generate().returncode, 0)
            self.assertEqual((root / "SHA256SUMS").read_text(), manifest)
            (root / names[0]).write_bytes(b"replacement")
            (root / asset_name("release/v1.2.3", "aarch64-apple-darwin")).write_bytes(b"excluded macOS asset")
            self.assertNotEqual(generate().returncode, 0)

    def test_installer_refuses_live_addons_and_rolls_back_failed_replacements(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "payload/fixture"
            source.mkdir(parents=True)
            (source / "addon.toml").write_text("id='fixture'\nprotocol=1\nexecutable='program'\nicon='icon.png'\n")
            (source / "program").write_bytes(b"old executable")
            (source / "icon.png").write_bytes(b"icon")
            destination = root / "app"
            installer.install(source.parent, destination)
            (source / "program").write_bytes(b"new executable")
            with (destination / "addons/.locks/fixture.lock").open("a+") as lock:
                fcntl.flock(lock, fcntl.LOCK_SH)
                with self.assertRaises(BlockingIOError):
                    installer.install(source.parent, destination)
            receipt = destination / "addon-receipts/fixture.json"
            previous_receipt = receipt.read_bytes()
            replace = installer.os.replace

            def fail_receipt(source, target):
                if Path(target) == receipt:
                    raise OSError("simulated receipt failure")
                return replace(source, target)

            with mock.patch.object(installer.os, "replace", side_effect=fail_receipt):
                with self.assertRaisesRegex(OSError, "simulated"):
                    installer.install(source.parent, destination)
            self.assertEqual((destination / "addons/fixture/program").read_bytes(), b"old executable")
            self.assertEqual(receipt.read_bytes(), previous_receipt)
            self.assertFalse((destination / "addons/.transactions/fixture.json").exists())

    def test_multiple_addons_are_discovered_packaged_and_indexed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text("[package]\nversion='1.0.0'\n")
            build = root / "build"
            for name in ("first", "second"):
                definition = root / "addons" / name
                definition.mkdir(parents=True)
                text = f"id='{name}'\nname='{name}'\nversion='1.0.0'\nprotocol=1\nexecutable='program'\nicon='icon.png'\n"
                (definition / "addon.toml").write_text(text)
                stage = build / "addons" / name
                stage.mkdir(parents=True)
                (stage / "addon.toml").write_text("target='x86_64-unknown-linux-gnu'\n" + text)
                for asset in ("program", "icon.png", "LICENSE", "THIRD_PARTY_NOTICES.txt"):
                    (stage / asset).write_bytes(b"fixture")
                (stage / "state.json").write_text("must not ship")
            output = root / "dist"
            catalog = package_addons(build, output, "v1", RELEASE_TARGETS[0], root)
            self.assertEqual({p["id"] for p in catalog["packages"]}, {"first", "second"})
            for package in catalog["packages"]:
                path = output / package["archive"]
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), package["sha256"])
                with tarfile.open(path) as archive:
                    self.assertNotIn("state.json", archive.getnames())
            self.assertTrue((output / index_name(RELEASE_TARGETS[0])).is_file())


if __name__ == "__main__":
    unittest.main()
