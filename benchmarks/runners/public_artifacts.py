#!/usr/bin/env python3
"""Sanitize private machine paths and verify published benchmark bundles."""

from __future__ import annotations

import getpass
import hashlib
import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable
from urllib.parse import quote


LABEL_RE = re.compile(r"^[A-Z][A-Z0-9_]*$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
PUBLIC_SERVICE_ACCOUNTS = {"runner"}
LOCAL_FILE_URI_RE = re.compile(
    r"file:///(?:[A-Za-z]:/|/(?:home|Users)/)[^\s\"'<>#]+",
    flags=re.IGNORECASE,
)
LOCAL_FILE_URI_TOKEN = "<LOCAL_FILE_URI>"


@dataclass(frozen=True)
class PublicAlias:
    """A private value and the deterministic token used in public artifacts."""

    label: str
    private: str

    def __post_init__(self) -> None:
        if not LABEL_RE.fullmatch(self.label):
            raise ValueError(f"invalid public alias label: {self.label!r}")
        if not self.private:
            raise ValueError(f"private value for {self.label} must not be empty")

    @property
    def public(self) -> str:
        return f"<{self.label}>"


@dataclass(frozen=True)
class SanitizationReport:
    files_changed: tuple[str, ...]
    replacements: int


@dataclass(frozen=True)
class VerificationReport:
    files: int
    manifest_entries: int
    sha256_entries: int
    complete_attempts: int | None


def parse_alias(specification: str) -> PublicAlias:
    """Parse ``LABEL=private value`` without interpreting the private value."""

    label, separator, private = specification.partition("=")
    if not separator:
        raise ValueError("public alias must use LABEL=private-value syntax")
    return PublicAlias(label=label, private=private)


def default_aliases(project: Path, release: Path) -> list[PublicAlias]:
    """Return machine-specific values that must never enter a release bundle."""

    candidates: list[PublicAlias] = [
        PublicAlias("RELEASE_ROOT", str(release.resolve())),
        PublicAlias("PROJECT_ROOT", str(project.resolve())),
        PublicAlias("PYTHON", str(Path(sys.executable).resolve())),
        PublicAlias("USER_HOME", str(Path.home().resolve())),
    ]
    for label, key in (
        ("USER_PROFILE", "USERPROFILE"),
        ("APP_DATA", "APPDATA"),
        ("LOCAL_APP_DATA", "LOCALAPPDATA"),
        ("TEMP_ROOT", "TEMP"),
        ("TMP_ROOT", "TMP"),
    ):
        value = os.environ.get(key)
        if value:
            candidates.append(PublicAlias(label, value))

    usernames = {
        value.strip()
        for value in (
            getpass.getuser(),
            os.environ.get("USERNAME", ""),
            os.environ.get("USER", ""),
        )
        if value and value.strip()
    }
    # Short account names such as "root" or "user", and public CI identities such
    # as GitHub's hosted "runner" account, occur naturally in source and prose.
    # Their home-directory paths are still covered above; longer personal account
    # names are safe to redact as standalone private literals as well.
    for username in sorted(usernames):
        if len(username) >= 6 and username.casefold() not in PUBLIC_SERVICE_ACCOUNTS:
            candidates.append(PublicAlias("USER_NAME", username))
    return _deduplicate_aliases(candidates)


def _deduplicate_aliases(aliases: Iterable[PublicAlias]) -> list[PublicAlias]:
    result: list[PublicAlias] = []
    seen: set[str] = set()
    for alias in aliases:
        key = alias.private.replace("\\", "/").rstrip("/").casefold()
        if key and key not in seen:
            seen.add(key)
            result.append(alias)
    return result


def _private_variants(value: str) -> set[str]:
    stripped = value.rstrip("/\\") or value
    slash = stripped.replace("\\", "/")
    backslash = stripped.replace("/", "\\")
    variants = {stripped, slash, backslash}
    variants.update(
        quote(item, safe=safe)
        for item in tuple(variants)
        for safe in ("/:\\", ":")
    )
    # JSON encodes each literal backslash as two characters in the artifact.
    variants.update(item.replace("\\", "\\\\") for item in tuple(variants))
    return {item for item in variants if item}


def _decode_artifact(raw: bytes) -> tuple[str, str] | None:
    if raw.startswith((b"\xff\xfe\x00\x00", b"\x00\x00\xfe\xff")):
        return raw.decode("utf-32"), "utf-32"
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")):
        return raw.decode("utf-16"), "utf-16"
    if raw.startswith(b"\xef\xbb\xbf"):
        return raw.decode("utf-8-sig"), "utf-8-sig"
    try:
        return raw.decode("utf-8"), "utf-8"
    except UnicodeDecodeError:
        return None


def _is_machine_metadata(root: Path, path: Path) -> bool:
    relative = path.relative_to(root).parts
    return bool(
        relative
        and (
            relative[0] == "logs"
            or relative[:2] == ("evidence", "bootstrap")
        )
    )


class PublicArtifactSanitizer:
    """Replace known private values while preserving benchmark fixture paths."""

    def __init__(self, aliases: Iterable[PublicAlias]) -> None:
        self.aliases = _deduplicate_aliases(aliases)
        replacements: list[tuple[re.Pattern[str], str, str]] = []
        marker_variants: list[tuple[str, str]] = []
        for alias in self.aliases:
            for variant in _private_variants(alias.private):
                marker_variants.append((variant, alias.label))
                replacements.append(
                    (
                        re.compile(re.escape(variant), flags=re.IGNORECASE),
                        alias.public,
                        alias.label,
                    )
                )
        # A release path is nested under the project path, and tool paths may be
        # nested under the user home. Longest-first guarantees the specific,
        # stable token wins regardless of registration order.
        self._replacements = sorted(
            replacements, key=lambda item: len(item[0].pattern), reverse=True
        )
        self._marker_variants = sorted(
            marker_variants, key=lambda item: len(item[0]), reverse=True
        )

    def sanitize_text(self, value: str) -> tuple[str, int]:
        replacements = 0
        for pattern, public, _label in self._replacements:
            value, count = pattern.subn(lambda _match, token=public: token, value)
            replacements += count
        return value, replacements

    def find_markers(self, value: str) -> set[str]:
        matches: set[str] = set()
        for pattern, _public, label in self._replacements:
            if pattern.search(value):
                matches.add(label)
        return matches

    def find_markers_bytes(self, value: bytes) -> set[str]:
        matches: set[str] = set()
        folded = value.lower()
        for variant, label in self._marker_variants:
            for encoding in ("utf-8", "utf-16-le", "utf-16-be"):
                if variant.encode(encoding).lower() in folded:
                    matches.add(label)
                    break
        return matches

    def sanitize_tree(
        self,
        root: Path,
        *,
        exclude_names: set[str] | None = None,
    ) -> SanitizationReport:
        excluded = exclude_names or set()
        changed: list[str] = []
        total_replacements = 0
        for path in sorted(item for item in root.rglob("*") if item.is_file()):
            if path.name in excluded:
                continue
            raw = path.read_bytes()
            decoded = _decode_artifact(raw)
            if decoded is None:
                # Do not silently publish a binary artifact containing a private
                # marker that this text-only sanitizer cannot safely rewrite.
                markers = self.find_markers_bytes(raw)
                if markers:
                    labels = ", ".join(sorted(markers))
                    raise ValueError(
                        f"private marker(s) {labels} found in non-UTF-8 artifact "
                        f"{path.relative_to(root).as_posix()}"
                    )
                continue
            text, encoding = decoded
            sanitized, count = self.sanitize_text(text)
            if _is_machine_metadata(root, path):
                sanitized, uri_count = LOCAL_FILE_URI_RE.subn(
                    LOCAL_FILE_URI_TOKEN, sanitized
                )
                count += uri_count
            if count:
                path.write_bytes(sanitized.encode(encoding))
                changed.append(path.relative_to(root).as_posix())
                total_replacements += count
        self.assert_tree_safe(root, exclude_names=excluded)
        return SanitizationReport(tuple(changed), total_replacements)

    def assert_tree_safe(
        self,
        root: Path,
        *,
        exclude_names: set[str] | None = None,
    ) -> None:
        excluded = exclude_names or set()
        failures: list[str] = []
        for path in sorted(item for item in root.rglob("*") if item.is_file()):
            if path.name in excluded:
                continue
            raw = path.read_bytes()
            decoded = _decode_artifact(raw)
            markers = (
                self.find_markers(decoded[0])
                if decoded is not None
                else self.find_markers_bytes(raw)
            )
            if (
                decoded is not None
                and _is_machine_metadata(root, path)
                and LOCAL_FILE_URI_RE.search(decoded[0])
            ):
                markers.add("LOCAL_FILE_URI")
            if markers:
                failures.append(
                    f"{path.relative_to(root).as_posix()}: {', '.join(sorted(markers))}"
                )
        if failures:
            raise ValueError(
                "private values remain in public release artifacts:\n"
                + "\n".join(failures)
            )


def file_sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def _safe_release_path(release: Path, relative: str) -> Path:
    logical = PurePosixPath(relative)
    if logical.is_absolute() or not logical.parts or ".." in logical.parts:
        raise ValueError(f"unsafe release artifact path: {relative!r}")
    return release.joinpath(*logical.parts)


def verify_release(
    release: Path,
    *,
    sanitizer: PublicArtifactSanitizer | None = None,
    require_complete_attempts: bool = False,
) -> VerificationReport:
    """Verify manifest/SHA256 completeness, bytes, digests, and private markers."""

    release = release.resolve()
    manifest_path = release / "manifest.json"
    checksums_path = release / "SHA256SUMS"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    all_files = {
        path.relative_to(release).as_posix(): path
        for path in release.rglob("*")
        if path.is_file()
    }

    manifest_entries: dict[str, dict[str, object]] = {}
    for entry in manifest.get("artifacts", []):
        relative = str(entry.get("path", ""))
        if relative in manifest_entries:
            raise ValueError(f"duplicate manifest entry: {relative}")
        _safe_release_path(release, relative)
        manifest_entries[relative] = entry
    expected_manifest_paths = set(all_files) - {"manifest.json", "SHA256SUMS"}
    if set(manifest_entries) != expected_manifest_paths:
        missing = sorted(expected_manifest_paths - set(manifest_entries))
        unexpected = sorted(set(manifest_entries) - expected_manifest_paths)
        raise ValueError(
            f"manifest coverage mismatch; missing={missing}, unexpected={unexpected}"
        )
    for relative, entry in manifest_entries.items():
        path = all_files[relative]
        if entry.get("bytes") != path.stat().st_size:
            raise ValueError(f"manifest byte count mismatch: {relative}")
        if entry.get("sha256") != file_sha256(path):
            raise ValueError(f"manifest SHA-256 mismatch: {relative}")

    checksum_entries: dict[str, str] = {}
    for line_number, line in enumerate(
        checksums_path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        digest_value, separator, relative = line.partition("  ")
        if not separator or not SHA256_RE.fullmatch(digest_value):
            raise ValueError(f"invalid SHA256SUMS line {line_number}")
        if relative in checksum_entries:
            raise ValueError(f"duplicate SHA256SUMS entry: {relative}")
        _safe_release_path(release, relative)
        checksum_entries[relative] = digest_value
    expected_checksum_paths = set(all_files) - {"SHA256SUMS"}
    if set(checksum_entries) != expected_checksum_paths:
        missing = sorted(expected_checksum_paths - set(checksum_entries))
        unexpected = sorted(set(checksum_entries) - expected_checksum_paths)
        raise ValueError(
            f"SHA256SUMS coverage mismatch; missing={missing}, unexpected={unexpected}"
        )
    for relative, expected in checksum_entries.items():
        if file_sha256(all_files[relative]) != expected:
            raise ValueError(f"SHA256SUMS mismatch: {relative}")

    complete_attempts: int | None = None
    if require_complete_attempts:
        execution = json.loads(
            (release / "logs" / "execution.json").read_text(encoding="utf-8")
        )
        attempts = execution.get("attempts", [])
        incomplete = [
            str(attempt.get("id", "unknown"))
            for attempt in attempts
            if attempt.get("exit_code") != 0 or attempt.get("timed_out") is not False
        ]
        if incomplete:
            raise ValueError(
                "incomplete benchmark command attempts: " + ", ".join(incomplete)
            )
        complete_attempts = len(attempts)

    if sanitizer is not None:
        sanitizer.assert_tree_safe(release)
    return VerificationReport(
        files=len(all_files),
        manifest_entries=len(manifest_entries),
        sha256_entries=len(checksum_entries),
        complete_attempts=complete_attempts,
    )
