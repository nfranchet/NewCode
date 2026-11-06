"""Release safety checks, independent of the host's available CPU architectures."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

from package import RELEASE_TARGETS, ADDON_ASSETS, asset_name, package_linux
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
                (addon / name).write_bytes(b"fixture addon")
            (addon / "focus-addon-github").chmod(0o755)
            package_linux(binary, output, stage, addon)
            with tarfile.open(output) as archive:
                self.assertEqual(set(archive.getnames()), {"focus-editor", "INSTALL.txt", "LICENSE", "THIRD_PARTY_NOTICES.txt", "install-addons.py"} | {"addons/github/" + name for name in ADDON_ASSETS})
                self.assertEqual(archive.extractfile("LICENSE").read(), Path("LICENSE").read_bytes())
                self.assertTrue(archive.getmember("focus-editor").mode & 0o111)
                self.assertEqual(archive.extractfile("focus-editor").read(), binary.read_bytes())
                self.assertTrue(archive.getmember("addons/github/focus-addon-github").mode & 0o111)

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
            state = destination / "addons/github/data/state.json"
            state.parent.mkdir(parents=True)
            state.write_text("keep non-secret state")
            installer.install(source.parent, destination)
            self.assertEqual(state.read_text(), "keep non-secret state")
            self.assertEqual((destination / "addons/github/program").read_bytes(), b"executable")
            self.assertTrue((destination / "addons/github/program").stat().st_mode & 0o111)
            (source / "addon.toml").write_text("id='github'\nprotocol=1\nexecutable='../escape'\nicon='icon.png'\n")
            with self.assertRaises(ValueError):
                installer.install(source.parent, destination)

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


if __name__ == "__main__":
    unittest.main()
