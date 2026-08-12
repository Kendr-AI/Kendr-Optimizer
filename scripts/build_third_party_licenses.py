#!/usr/bin/env python3
"""Generate or verify the locked Rust dependency license bundle."""

from __future__ import annotations

import argparse
import hashlib
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "THIRD_PARTY_LICENSES.html"
RUST_OUTPUT = ROOT / "RUST_STDLIB_LICENSES.html"
CARGO_ABOUT_VERSION = "0.9.1"
RUST_TOOLCHAIN_VERSION = "1.88.0"
RUST_LICENSE_SHA256 = "3d3f60160f5214efa0a7fd804102d02ce9ea6af04b5249a19eeb243450246ae9"
SOURCE_PATHS = (
    ROOT / "Cargo.lock",
    ROOT / "Cargo.toml",
    ROOT / "about.toml",
    ROOT / "about.hbs",
    ROOT / "crates" / "kendr-optimizer-contracts" / "Cargo.toml",
    ROOT / "crates" / "kendr-optimizer-core" / "Cargo.toml",
    ROOT / "crates" / "kendr-optimizer-cli" / "Cargo.toml",
    Path(__file__).resolve(),
)
MARKER_RE = re.compile(r"<!-- Kendr license inputs SHA-256: ([0-9a-f]{64}) -->")
BODY_MARKER_RE = re.compile(r"<!-- Kendr generated body SHA-256: ([0-9a-f]{64}) -->")


def inputs_digest() -> str:
    digest = hashlib.sha256()
    digest.update(f"cargo-about {CARGO_ABOUT_VERSION}\0".encode())
    for path in SOURCE_PATHS:
        relative = path.relative_to(ROOT).as_posix()
        digest.update(relative.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def add_markers(rendered: str) -> str:
    lines = rendered.replace("\r\n", "\n").splitlines()
    if not lines:
        raise ValueError("cargo-about generated an empty document")
    body = "\n".join(lines) + "\n"
    input_marker = f"<!-- Kendr license inputs SHA-256: {inputs_digest()} -->"
    body_marker = (
        "<!-- Kendr generated body SHA-256: "
        f"{hashlib.sha256(body.encode()).hexdigest()} -->"
    )
    lines[1:1] = [input_marker, body_marker]
    return "\n".join(lines) + "\n"


def check() -> None:
    if not OUTPUT.is_file():
        raise FileNotFoundError(f"generated license bundle is missing: {OUTPUT}")
    content = OUTPUT.read_text(encoding="utf-8")
    matches = MARKER_RE.findall(content)
    if matches != [inputs_digest()]:
        raise ValueError(
            "THIRD_PARTY_LICENSES.html is stale; regenerate it with "
            "scripts/build_third_party_licenses.py"
        )
    body_matches = BODY_MARKER_RE.findall(content)
    if len(body_matches) != 1:
        raise ValueError("THIRD_PARTY_LICENSES.html has no unique body digest")
    body_lines = [
        line
        for line in content.replace("\r\n", "\n").splitlines()
        if MARKER_RE.fullmatch(line) is None and BODY_MARKER_RE.fullmatch(line) is None
    ]
    body = "\n".join(body_lines) + "\n"
    if hashlib.sha256(body.encode()).hexdigest() != body_matches[0]:
        raise ValueError("THIRD_PARTY_LICENSES.html content digest does not match")
    if not RUST_OUTPUT.is_file():
        raise FileNotFoundError(
            f"Rust standard-library license bundle is missing: {RUST_OUTPUT}"
        )
    rust_digest = hashlib.sha256(RUST_OUTPUT.read_bytes()).hexdigest()
    if rust_digest != RUST_LICENSE_SHA256:
        raise ValueError(
            "RUST_STDLIB_LICENSES.html does not match the reviewed Rust "
            f"{RUST_TOOLCHAIN_VERSION} license bundle"
        )


def generate(executable: Path) -> None:
    version = subprocess.run(
        [str(executable), "--version"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if version != f"cargo-about {CARGO_ABOUT_VERSION}":
        raise ValueError(
            f"cargo-about {CARGO_ABOUT_VERSION} is required, found {version!r}"
        )

    with tempfile.TemporaryDirectory(prefix="kendr-licenses-") as temporary:
        rendered_path = Path(temporary) / "licenses.html"
        subprocess.run(
            [
                str(executable),
                "generate",
                "--frozen",
                "--workspace",
                "--fail",
                "--config",
                str(ROOT / "about.toml"),
                "--output-file",
                str(rendered_path),
                str(ROOT / "about.hbs"),
            ],
            cwd=ROOT,
            check=True,
        )
        rendered = rendered_path.read_text(encoding="utf-8")

    OUTPUT.write_text(add_markers(rendered), encoding="utf-8", newline="\n")

    rust_version = subprocess.run(
        ["rustc", "--version"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if not rust_version.startswith(f"rustc {RUST_TOOLCHAIN_VERSION} "):
        raise ValueError(
            f"rustc {RUST_TOOLCHAIN_VERSION} is required, found {rust_version!r}"
        )
    sysroot = Path(
        subprocess.run(
            ["rustc", "--print", "sysroot"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    )
    rust_source = sysroot / "share" / "doc" / "rust" / "COPYRIGHT-library.html"
    rust_bytes = rust_source.read_bytes()
    rust_digest = hashlib.sha256(rust_bytes).hexdigest()
    if rust_digest != RUST_LICENSE_SHA256:
        raise ValueError(
            "installed Rust standard-library license bundle does not match "
            f"the reviewed {RUST_TOOLCHAIN_VERSION} digest"
        )
    RUST_OUTPUT.write_bytes(rust_bytes)
    check()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify that the committed bundle matches its generation inputs",
    )
    parser.add_argument(
        "--cargo-about",
        type=Path,
        help=f"path to cargo-about {CARGO_ABOUT_VERSION}",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.check:
        check()
        print("third-party license bundle is current")
        return 0

    executable = args.cargo_about
    if executable is None:
        resolved = shutil.which("cargo-about")
        if resolved is None:
            raise FileNotFoundError(
                f"cargo-about {CARGO_ABOUT_VERSION} is required to regenerate licenses"
            )
        executable = Path(resolved)
    generate(executable.resolve())
    print(OUTPUT)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
