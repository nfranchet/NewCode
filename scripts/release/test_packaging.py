"""Release safety checks, independent of the host's available CPU architectures."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

from package import TARGETS, asset_name, package_linux


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
            package_linux(binary, output, stage)
            with tarfile.open(output) as archive:
                self.assertEqual(set(archive.getnames()), {"focus-editor", "INSTALL.txt", "LICENSE"})
                self.assertEqual(archive.extractfile("LICENSE").read(), Path("LICENSE").read_bytes())
                self.assertTrue(archive.getmember("focus-editor").mode & 0o111)
                self.assertEqual(archive.extractfile("focus-editor").read(), binary.read_bytes())

    def test_checksums_require_all_targets_and_reject_extra_assets(self):
        script = Path(__file__).with_name("checksums.py").resolve()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # A release tag containing '/' must still produce safe single filenames.
            names = [asset_name("release/v1.2.3", target) for target in TARGETS]
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
            (root / "unexpected.bin").write_bytes(b"unrelated")
            self.assertNotEqual(generate().returncode, 0)


if __name__ == "__main__":
    unittest.main()
