import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from codemagic_log_chunks import MAX_CHUNK_BYTES, split_log


class CodeMagicLogChunkTests(unittest.TestCase):
    def test_small_log_keeps_legacy_latest_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = b"build started\nbuild passed\n"
            metadata = split_log(data, root)
            self.assertEqual((root / "latest").read_bytes(), data)
            self.assertEqual(metadata["part_count"], 1)
            self.assertFalse((root / "latest.part002").exists())

    def test_large_log_splits_and_reconstructs_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = bytes(range(256)) * 40000
            metadata = split_log(data, root, chunk_bytes=1024 * 1024)
            rebuilt = b"".join((root / part["path"]).read_bytes() for part in metadata["log_parts"])
            self.assertEqual(rebuilt, data)
            self.assertGreater(metadata["part_count"], 1)
            self.assertTrue(all(part["bytes"] <= 1024 * 1024 for part in metadata["log_parts"]))
            self.assertEqual(metadata["aggregate_sha256"], hashlib.sha256(data).hexdigest())
            saved = json.loads((root / "latest.metadata.json").read_text())
            self.assertEqual(saved, metadata)

    def test_empty_log_still_has_one_part(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            metadata = split_log(b"", root)
            self.assertEqual((root / "latest").read_bytes(), b"")
            self.assertEqual(metadata["part_count"], 1)
            self.assertEqual(metadata["log_parts"][0]["bytes"], 0)

    def test_chunk_size_cannot_exceed_four_mib(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                split_log(b"x", Path(directory), MAX_CHUNK_BYTES + 1)


if __name__ == "__main__":
    unittest.main()
