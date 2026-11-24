#!/usr/bin/env python3
"""Split a redacted aggregate CodeMagic log into GitHub-friendly chunks."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

DEFAULT_CHUNK_BYTES = 3 * 1024 * 1024
MAX_CHUNK_BYTES = 4 * 1024 * 1024


def split_log(data: bytes, output_dir: Path, chunk_bytes: int = DEFAULT_CHUNK_BYTES) -> dict:
    if not isinstance(data, bytes):
        raise TypeError("data must be bytes")
    if not 1 <= chunk_bytes <= MAX_CHUNK_BYTES:
        raise ValueError(f"chunk_bytes must be between 1 and {MAX_CHUNK_BYTES}")
    output_dir.mkdir(parents=True, exist_ok=True)

    chunks = [data[offset:offset + chunk_bytes] for offset in range(0, len(data), chunk_bytes)]
    if not chunks:
        chunks = [b""]

    parts = []
    for index, chunk in enumerate(chunks, start=1):
        filename = "latest" if index == 1 else f"latest.part{index:03d}"
        path = output_dir / filename
        path.write_bytes(chunk)
        parts.append({
            "path": filename,
            "bytes": len(chunk),
            "sha256": hashlib.sha256(chunk).hexdigest(),
        })

    metadata = {
        "schema_version": 1,
        "aggregate_bytes": len(data),
        "aggregate_lines": data.count(b"\n") + (1 if data and not data.endswith(b"\n") else 0),
        "aggregate_sha256": hashlib.sha256(data).hexdigest(),
        "max_part_bytes": chunk_bytes,
        "part_count": len(parts),
        "log_parts": parts,
    }
    (output_dir / "latest.metadata.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )
    return metadata


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="redacted aggregate log file")
    parser.add_argument("output_dir", type=Path, help="directory for latest and numbered parts")
    parser.add_argument("--chunk-bytes", type=int, default=DEFAULT_CHUNK_BYTES)
    args = parser.parse_args()

    metadata = split_log(args.input.read_bytes(), args.output_dir, args.chunk_bytes)
    print(
        f"Split {metadata['aggregate_bytes']} bytes into "
        f"{metadata['part_count']} part(s), max {metadata['max_part_bytes']} bytes each."
    )


if __name__ == "__main__":
    main()
