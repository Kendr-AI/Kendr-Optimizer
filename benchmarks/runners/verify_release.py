#!/usr/bin/env python3
"""Verify a benchmark release's integrity and publication-path policy."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from public_artifacts import (
    PublicArtifactSanitizer,
    default_aliases,
    parse_alias,
    verify_release,
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--project-root", type=Path)
    parser.add_argument(
        "--redact-path",
        action="append",
        default=[],
        metavar="LABEL=PATH",
        help="additional private path that must not occur in the bundle",
    )
    parser.add_argument(
        "--require-complete-attempts",
        action="store_true",
        help="also require every execution-ledger attempt to have exited successfully",
    )
    args = parser.parse_args()

    release = args.release.resolve()
    project = (args.project_root or Path.cwd()).resolve()
    aliases = default_aliases(project, release)
    aliases.extend(parse_alias(item) for item in args.redact_path)
    result = verify_release(
        release,
        sanitizer=PublicArtifactSanitizer(aliases),
        require_complete_attempts=args.require_complete_attempts,
    )
    print(
        json.dumps(
            {
                "release": release.name,
                "files": result.files,
                "manifest_entries": result.manifest_entries,
                "sha256_entries": result.sha256_entries,
                "complete_attempts": result.complete_attempts,
                "private_markers": 0,
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
