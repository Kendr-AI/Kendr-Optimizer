#!/usr/bin/env python3
"""Verify that the committed whitepaper PDF identifies the current Markdown."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCE = ROOT / "docs" / "whitepaper.md"
DEFAULT_PDF = (
    ROOT
    / "output"
    / "pdf"
    / "kendr-optimizer-verification-gated-token-reduction-whitepaper.pdf"
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--pdf", type=Path, default=DEFAULT_PDF)
    args = parser.parse_args()

    source = args.source.resolve().read_bytes()
    pdf = args.pdf.resolve().read_bytes()
    digest = hashlib.sha256(source).hexdigest().encode("ascii")

    if not pdf.startswith(b"%PDF-"):
        raise SystemExit(f"not a PDF: {args.pdf}")
    if digest not in pdf:
        raise SystemExit(
            "committed whitepaper PDF is stale: current Markdown SHA-256 "
            f"{digest.decode('ascii')} is absent"
        )

    print(f"whitepaper PDF source digest verified: {digest.decode('ascii')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
