import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts.capture_corpus import capture


class CaptureTests(unittest.TestCase):
    def test_missing_pattern_fails_instead_of_capturing_empty_coverage(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "No files matched"):
                capture(Path(directory), ["missing/*.wav"])

    def test_parent_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "relative to game root"):
                capture(Path(directory), ["../*.wav"])

    def test_comparison_detects_edits_and_preserves_reference(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "input.wav").write_bytes(b"input")
            (root / "output.sabl").write_bytes(b"output")
            (root / "tool.exe").write_bytes(b"binary")
            (root / "manifest.json").write_text(json.dumps({
                "name": "test", "invocation": ["pc", "sound_zone"],
                "inputs": ["input.wav"], "outputs": ["output.sabl"],
            }))
            command = [sys.executable, "scripts/capture_corpus.py", "--root", directory,
                       "--manifest", str(root / "manifest.json"), "--exe", str(root / "tool.exe")]
            reference = root / "reference.json"
            def run(*args):
                return subprocess.run(command + list(args), capture_output=True, text=True, timeout=10)
            first = run("--out", str(reference))
            self.assertEqual(first.returncode, 0, first.stderr)
            previous = reference.read_bytes()
            candidate = root / "candidate.json"
            same = run("--out", str(candidate), "--compare", str(reference))
            self.assertEqual(same.returncode, 0, same.stderr)
            (root / "output.sabl").write_bytes(b"changed")
            different = run("--out", str(candidate), "--compare", str(reference))
            self.assertEqual(different.returncode, 1, different.stderr)
            self.assertEqual(json.loads(candidate.read_text())["comparison"]["outputs"], ["output.sabl"])
            refused = run("--out", str(reference), "--compare", str(reference))
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual(reference.read_bytes(), previous)
            refused = run("--out", str(root / "input.wav"))
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual((root / "input.wav").read_bytes(), b"input")


if __name__ == "__main__":
    unittest.main()
