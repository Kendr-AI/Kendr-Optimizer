#!/usr/bin/env python3
"""Reject non-public development artifacts from the repository.

In a Git checkout, the tracked-file list is authoritative: ignored local build
output is not a publication defect, while a tracked cache is. Source archives
without Git metadata use an ignore-aware filesystem walk instead.

Three assistant-shaped paths are distributable target-harness packages, not
instructions for developing this repository. They are deliberately allowlisted
at their exact locations:

* .claude-plugin/marketplace.json
* integrations/claude-code/.claude-plugin/plugin.json
* integrations/nanoclaw/skill/SKILL.md
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path, PurePosixPath


# Keep blocking the retired private-control namespace without presenting it as
# project identity in live source or documentation.
LEGACY_CONTROL_STEM = "".join(map(chr, (99, 111, 100, 101, 120)))
RETIRED_IDENTITY = LEGACY_CONTROL_STEM.encode("ascii")
# Exact checksum-bound third-party registry metadata in the benchmark record.
IMMUTABLE_IDENTITY_EXCEPTIONS = {
    PurePosixPath(
        "releases/v0.1.0-benchmark.5/evidence/bootstrap/"
        "headroom-0.34.0/pypi.json"
    )
}

FORBIDDEN_CONTROL_DIRECTORIES = {
    ".aider",
    ".agents",
    ".claude",
    ".cline",
    ".continue",
    f".{LEGACY_CONTROL_STEM}",
    ".cursor",
    ".gemini",
    ".roo",
    ".windsurf",
}
FORBIDDEN_CONTROL_FILES = {
    ".aider.conf.yml",
    ".aiderignore",
    ".clinerules",
    ".cursorrules",
    ".windsurfrules",
    "agents.md",
    "claude.md",
    f"{LEGACY_CONTROL_STEM}.md",
    "copilot-instructions.md",
    "gemini.md",
}
FORBIDDEN_CONTROL_SUFFIXES = (".agent.md", ".chatmode.md", ".instructions.md", ".prompt.md")

CLAUDE_PLUGIN_DIRECTORY = PurePosixPath(
    "integrations/claude-code/.claude-plugin"
)
CLAUDE_PLUGIN_MANIFEST = CLAUDE_PLUGIN_DIRECTORY / "plugin.json"
CLAUDE_MARKETPLACE_DIRECTORY = PurePosixPath(".claude-plugin")
CLAUDE_MARKETPLACE_MANIFEST = CLAUDE_MARKETPLACE_DIRECTORY / "marketplace.json"
NANOCLAW_SKILL = PurePosixPath("integrations/nanoclaw/skill/SKILL.md")
REQUIRED_DISTRIBUTION_FILES = {
    PurePosixPath("THIRD_PARTY_LICENSES.html"),
    PurePosixPath("RUST_STDLIB_LICENSES.html"),
    PurePosixPath("install/kendr-opt-installer.ps1"),
    PurePosixPath("install/kendr-opt-installer.sh"),
    CLAUDE_PLUGIN_MANIFEST,
    NANOCLAW_SKILL,
    PurePosixPath("integrations/nanoclaw/skill/REMOVE.md"),
    PurePosixPath("integrations/nanoclaw/skill/LICENSE"),
    PurePosixPath("integrations/nanoclaw/skill/NOTICE"),
    PurePosixPath("integrations/nanoclaw/skill/assets/apply-patch.mjs"),
    PurePosixPath("integrations/nanoclaw/skill/assets/kendr-optimizer.test.ts"),
    PurePosixPath("integrations/nanoclaw/skill/assets/kendr-optimizer.ts"),
    PurePosixPath("integrations/claude-channels/LICENSE"),
    PurePosixPath("integrations/claude-channels/NOTICE"),
    PurePosixPath("integrations/claude-code/LICENSE"),
    PurePosixPath("integrations/claude-code/NOTICE"),
    PurePosixPath("integrations/hermes-agent/LICENSE"),
    PurePosixPath("integrations/hermes-agent/NOTICE"),
    PurePosixPath("integrations/openclaw/LICENSE"),
    PurePosixPath("integrations/openclaw/NOTICE"),
    PurePosixPath("integrations/opencode/LICENSE"),
    PurePosixPath("integrations/opencode/NOTICE"),
    PurePosixPath("integrations/pi-agent/LICENSE"),
    PurePosixPath("integrations/pi-agent/NOTICE"),
    PurePosixPath("crates/kendr-optimizer-contracts/NOTICE"),
    PurePosixPath("crates/kendr-optimizer-core/NOTICE"),
    PurePosixPath("crates/kendr-optimizer-cli/NOTICE"),
}

CACHE_DIRECTORIES = {
    ".cache",
    ".mypy_cache",
    ".nox",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    "__pycache__",
    "build",
    "node_modules",
    "target",
    "tmp",
}
BYTECODE_SUFFIXES = {".class", ".pyc", ".pyo", ".tsbuildinfo"}
GENERATED_STATE_FILES = {".coverage", ".ds_store", ".eslintcache", "coverage.xml"}

# Written as a regular expression rather than a sample path so this checker
# does not contain the private-path shape it rejects.
WINDOWS_USER_PROFILE = re.compile(
    rb"(?i)(?<![a-z0-9])c(?::|%3a)(?:[\\/]+|%(?:5c|2f))+"
    rb"users(?:[\\/]+|%(?:5c|2f))+"
)

def normalize_path(path: str | Path | PurePosixPath) -> PurePosixPath:
    """Return a repository-relative path with stable POSIX separators."""

    value = str(path).replace("\\", "/")
    while value.startswith("./"):
        value = value[2:]
    return PurePosixPath(value)


def git_tracked_paths(root: Path) -> list[PurePosixPath] | None:
    """Return tracked paths, or None when root is not a Git work tree."""

    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z", "--cached"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except OSError:
        return None

    if result.returncode != 0:
        return None

    return [
        normalize_path(os.fsdecode(raw_path))
        for raw_path in result.stdout.split(b"\0")
        if raw_path
    ]


def is_local_output_directory(name: str) -> bool:
    """Identify directories excluded from archive-mode publication scans."""

    folded = name.casefold()
    return (
        folded == ".git"
        or folded in CACHE_DIRECTORIES
        or folded.startswith(".venv")
        or folded.endswith(".egg-info")
    )


def archive_paths(root: Path) -> list[PurePosixPath]:
    """Enumerate publishable paths when Git metadata is unavailable."""

    paths: list[PurePosixPath] = []
    for current, directory_names, file_names in os.walk(root):
        current_path = Path(current)
        relative_directory = current_path.relative_to(root)

        retained_directories: list[str] = []
        for name in directory_names:
            relative = normalize_path(relative_directory / name)
            if name.casefold() in FORBIDDEN_CONTROL_DIRECTORIES:
                # Record the directory itself, then avoid reading private local
                # contents that can never be a public repository artifact.
                paths.append(relative)
            elif is_local_output_directory(name):
                continue
            else:
                retained_directories.append(name)
        directory_names[:] = retained_directories

        paths.extend(
            normalize_path(relative_directory / name) for name in file_names
        )

    return paths


def path_violations(path: PurePosixPath) -> list[str]:
    """Return all hygiene failures encoded by a repository-relative path."""

    violations: list[str] = []
    folded_parts = tuple(part.casefold() for part in path.parts)
    folded_name = path.name.casefold()

    if any(part in FORBIDDEN_CONTROL_DIRECTORIES for part in folded_parts):
        violations.append("repository-development assistant control directory")

    if folded_name in FORBIDDEN_CONTROL_FILES:
        violations.append("repository-development assistant instruction file")

    if folded_name.endswith(FORBIDDEN_CONTROL_SUFFIXES):
        violations.append("repository-development assistant instruction file")

    if any(part in CACHE_DIRECTORIES for part in folded_parts):
        violations.append("generated dependency, build, or interpreter cache")

    if any(part.startswith(".venv") for part in folded_parts):
        violations.append("generated Python virtual environment")

    if any(part.endswith(".egg-info") for part in folded_parts):
        violations.append("generated Python package metadata")

    if path.suffix.casefold() in BYTECODE_SUFFIXES:
        violations.append("generated bytecode")

    if folded_name in GENERATED_STATE_FILES:
        violations.append("generated coverage state")

    if ".claude-plugin" in folded_parts:
        allowed_plugin_paths = {
            normalize_path(CLAUDE_MARKETPLACE_DIRECTORY),
            normalize_path(CLAUDE_MARKETPLACE_MANIFEST),
            normalize_path(CLAUDE_PLUGIN_DIRECTORY),
            normalize_path(CLAUDE_PLUGIN_MANIFEST),
        }
        if path not in allowed_plugin_paths:
            violations.append(
                "Claude plugin metadata outside the audited integration package"
            )

    if folded_name == "skill.md" and path != NANOCLAW_SKILL:
        violations.append(
            "distributable assistant skill outside the audited NanoClaw package"
        )

    return violations


def contains_windows_user_profile(path: Path) -> bool:
    """Scan a file for an absolute Windows user-profile path."""

    overlap = b""
    try:
        with path.open("rb") as stream:
            byte_order_mark = stream.read(2)
            stream.seek(0)
            if byte_order_mark in {b"\xff\xfe", b"\xfe\xff"}:
                decoded = stream.read().decode("utf-16", errors="replace")
                return WINDOWS_USER_PROFILE.search(decoded.encode("utf-8")) is not None

            while chunk := stream.read(1024 * 1024):
                candidate = overlap + chunk
                if WINDOWS_USER_PROFILE.search(candidate):
                    return True
                overlap = candidate[-64:]
    except OSError as error:
        raise RuntimeError(f"cannot read {path}: {error}") from error
    return False


def contains_retired_identity(path: Path) -> bool:
    """Scan a file for a retired assistant identity, case-insensitively."""

    overlap = b""
    try:
        with path.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                candidate = overlap + chunk
                if RETIRED_IDENTITY in candidate.lower():
                    return True
                overlap = candidate[-(len(RETIRED_IDENTITY) - 1) :]
    except OSError as error:
        raise RuntimeError(f"cannot read {path}: {error}") from error
    return False


def inspect(root: Path, paths: Iterable[PurePosixPath]) -> list[tuple[str, str]]:
    """Inspect repository paths and return sorted (path, reason) pairs."""

    findings: set[tuple[str, str]] = set()
    for relative in paths:
        relative = normalize_path(relative)
        display_path = relative.as_posix()

        for reason in path_violations(relative):
            findings.add((display_path, reason))

        absolute = root.joinpath(*relative.parts)
        if absolute.is_file() and contains_windows_user_profile(absolute):
            findings.add(
                (
                    display_path,
                    "absolute Windows user-profile path in public content",
                )
            )
        if (
            absolute.is_file()
            and relative not in IMMUTABLE_IDENTITY_EXCEPTIONS
            and contains_retired_identity(absolute)
        ):
            findings.add(
                (
                    display_path,
                    "retired assistant identity in live public content",
                )
            )

    return sorted(findings)


def missing_distribution_files(
    paths: Iterable[PurePosixPath],
) -> list[PurePosixPath]:
    """Return required package/legal files absent from the public file set."""

    present = {normalize_path(path) for path in paths}
    return sorted(
        REQUIRED_DISTRIBUTION_FILES - present,
        key=lambda path: path.as_posix(),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to the parent of scripts/)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    if not (root / "Cargo.toml").is_file():
        print(f"error: {root} does not look like the repository root", file=sys.stderr)
        return 2

    tracked = git_tracked_paths(root)
    if tracked is None:
        paths = archive_paths(root)
        source = "archive-mode public files"
    else:
        paths = tracked
        source = "Git-tracked files"

    try:
        findings = inspect(root, paths)
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    findings.extend(
        (
            path.as_posix(),
            "required distribution package or legal file is missing",
        )
        for path in missing_distribution_files(paths)
    )
    findings.sort()

    if findings:
        print("repository hygiene check failed:", file=sys.stderr)
        for path, reason in findings:
            print(f"  - {path}: {reason}", file=sys.stderr)
        return 1

    print(f"repository hygiene check passed ({len(paths)} {source})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
