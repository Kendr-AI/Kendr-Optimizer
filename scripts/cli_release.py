#!/usr/bin/env python3
"""Build and verify Kendr Optimizer CLI release assets."""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[1]
BINARY_NAME = "kendr-opt"
SUPPORTED_TARGETS = {
    "aarch64-apple-darwin": ".tar.gz",
    "aarch64-unknown-linux-musl": ".tar.gz",
    "x86_64-apple-darwin": ".tar.gz",
    "x86_64-pc-windows-msvc": ".zip",
    "x86_64-unknown-linux-musl": ".tar.gz",
}
INSTALLER_ASSETS = {
    "kendr-opt-installer.ps1": ROOT / "install" / "kendr-opt-installer.ps1",
    "kendr-opt-installer.sh": ROOT / "install" / "kendr-opt-installer.sh",
}
PACKAGE_DOCUMENTS = {
    "CHANGELOG.md": ROOT / "CHANGELOG.md",
    "LICENSE": ROOT / "LICENSE",
    "NOTICE": ROOT / "crates" / "kendr-optimizer-cli" / "NOTICE",
    "README.md": ROOT / "README.md",
    "RUST_STDLIB_LICENSES.html": ROOT / "RUST_STDLIB_LICENSES.html",
    "THIRD_PARTY_LICENSES.html": ROOT / "THIRD_PARTY_LICENSES.html",
}
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$")
CHECKSUM_RE = re.compile(r"^([0-9a-f]{64})  ([A-Za-z0-9_.+-]+)$")
PROJECT_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))[
    "workspace"
]["package"]["version"]
NODE_ADAPTER_ASSETS = {
    f"kendr-optimizer-{name}-{PROJECT_VERSION}.tgz"
    for name in (
        "claude-channels",
        "claude-code",
        "openclaw",
        "opencode",
        "pi",
    )
}
HERMES_ADAPTER_ASSET = (
    f"kendr_optimizer_hermes-{PROJECT_VERSION}-py3-none-any.whl"
)
NANOCLAW_SKILL_ROOT = ROOT / "integrations" / "nanoclaw" / "skill"


def nanoclaw_asset_name(version: str) -> str:
    return f"kendr-optimizer-nanoclaw-{version}.tar.gz"


def adapter_assets(version: str = PROJECT_VERSION) -> set[str]:
    node_assets = {
        name.replace(f"-{PROJECT_VERSION}.tgz", f"-{version}.tgz")
        for name in NODE_ADAPTER_ASSETS
    }
    return {
        *node_assets,
        f"kendr_optimizer_hermes-{version}-py3-none-any.whl",
        nanoclaw_asset_name(version),
    }


def archive_name(target: str) -> str:
    try:
        suffix = SUPPORTED_TARGETS[target]
    except KeyError as error:
        raise ValueError(f"unsupported release target: {target}") from error
    return f"{BINARY_NAME}-{target}{suffix}"


def archive_root(version: str, target: str) -> str:
    return f"{BINARY_NAME}-{version}-{target}"


def binary_filename(target: str) -> str:
    return f"{BINARY_NAME}.exe" if target.endswith("-windows-msvc") else BINARY_NAME


def validate_version(version: str) -> None:
    if not VERSION_RE.fullmatch(version):
        raise ValueError(f"invalid release version: {version!r}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def run_binary_smoke(binary: Path, version: str) -> None:
    version_result = subprocess.run(
        [str(binary), "--version"],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if version_result.stdout.strip() != f"{BINARY_NAME} {version}":
        raise ValueError(
            "release binary version mismatch: "
            f"expected {BINARY_NAME} {version!s}, got {version_result.stdout.strip()!r}"
        )

    engines_result = subprocess.run(
        [str(binary), "engines", "--compact"],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    engines = json.loads(engines_result.stdout)
    if not isinstance(engines, list) or not engines:
        raise ValueError("release binary engine smoke returned no engine list")


def package_members(binary: Path, target: str) -> dict[str, tuple[Path, int]]:
    if not binary.is_file():
        raise FileNotFoundError(f"release binary does not exist: {binary}")
    members = {binary_filename(target): (binary, 0o755)}
    for name, source in PACKAGE_DOCUMENTS.items():
        if not source.is_file():
            raise FileNotFoundError(f"required package document is missing: {source}")
        members[name] = (source, 0o644)
    return members


def write_zip(
    output: Path,
    root: str,
    members: dict[str, tuple[Path, int]],
    epoch: int,
) -> None:
    timestamp = max(epoch, 315532800)  # ZIP timestamps begin at 1980-01-01.
    date_time = dt.datetime.fromtimestamp(timestamp, tz=dt.UTC).timetuple()[:6]
    with zipfile.ZipFile(
        output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for name in sorted(members):
            source, mode = members[name]
            info = zipfile.ZipInfo(f"{root}/{name}", date_time)
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (stat.S_IFREG | mode) << 16
            archive.writestr(info, source.read_bytes(), compresslevel=9)


def write_tar_gz(
    output: Path,
    root: str,
    members: dict[str, tuple[Path, int]],
    epoch: int,
) -> None:
    with output.open("wb") as raw_stream:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=raw_stream, mtime=epoch, compresslevel=9
        ) as gzip_stream:
            with tarfile.open(
                fileobj=gzip_stream, mode="w", format=tarfile.PAX_FORMAT
            ) as archive:
                for name in sorted(members):
                    source, mode = members[name]
                    data = source.read_bytes()
                    info = tarfile.TarInfo(f"{root}/{name}")
                    info.size = len(data)
                    info.mode = mode
                    info.mtime = epoch
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    with tempfile.SpooledTemporaryFile() as member_stream:
                        member_stream.write(data)
                        member_stream.seek(0)
                        archive.addfile(info, member_stream)


def package(
    binary: Path, target: str, version: str, output_dir: Path, epoch: int
) -> Path:
    validate_version(version)
    if epoch < 0:
        raise ValueError("source epoch cannot be negative")
    run_binary_smoke(binary, version)
    members = package_members(binary, target)
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / archive_name(target)
    root = archive_root(version, target)
    if output.suffix == ".zip":
        write_zip(output, root, members, epoch)
    else:
        write_tar_gz(output, root, members, epoch)
    return output


def expected_archive_members(version: str, target: str) -> dict[str, int]:
    root = archive_root(version, target)
    return {
        f"{root}/{binary_filename(target)}": 0o755,
        f"{root}/CHANGELOG.md": 0o644,
        f"{root}/LICENSE": 0o644,
        f"{root}/NOTICE": 0o644,
        f"{root}/README.md": 0o644,
        f"{root}/RUST_STDLIB_LICENSES.html": 0o644,
        f"{root}/THIRD_PARTY_LICENSES.html": 0o644,
    }


def verify_archive(path: Path, version: str, target: str) -> None:
    expected = expected_archive_members(version, target)
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(expected) or set(names) != set(expected):
                raise ValueError(
                    f"unexpected ZIP members in {path.name}: {sorted(names)}"
                )
            for info in archive.infolist():
                if info.is_dir():
                    raise ValueError(
                        f"unexpected directory entry in {path.name}: {info.filename}"
                    )
                unix_mode = info.external_attr >> 16
                if stat.S_IFMT(unix_mode) != stat.S_IFREG:
                    raise ValueError(
                        f"non-file ZIP member in {path.name}: {info.filename}"
                    )
                mode = unix_mode & 0o777
                if mode != expected[info.filename]:
                    raise ValueError(
                        f"unexpected mode for {info.filename}: {oct(mode)}"
                    )
                archive.read(info)
    else:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            names = [member.name for member in members]
            if len(names) != len(expected) or set(names) != set(expected):
                raise ValueError(
                    f"unexpected tar members in {path.name}: {sorted(names)}"
                )
            for member in members:
                pure_name = PurePosixPath(member.name)
                if pure_name.is_absolute() or ".." in pure_name.parts:
                    raise ValueError(
                        f"unsafe archive path in {path.name}: {member.name}"
                    )
                if not member.isfile():
                    raise ValueError(
                        f"non-file archive member in {path.name}: {member.name}"
                    )
                if member.mode != expected[member.name]:
                    raise ValueError(
                        f"unexpected mode for {member.name}: {oct(member.mode)}"
                    )
                extracted = archive.extractfile(member)
                if extracted is None:
                    raise ValueError(f"cannot read archive member: {member.name}")
                while extracted.read(1024 * 1024):
                    pass


def copy_installers(directory: Path) -> None:
    for name, source in INSTALLER_ASSETS.items():
        if not source.is_file():
            raise FileNotFoundError(f"required installer is missing: {source}")
        shutil.copyfile(source, directory / name)


def package_nanoclaw(directory: Path, version: str, epoch: int) -> Path:
    validate_version(version)
    if epoch < 0:
        raise ValueError("source epoch cannot be negative")
    members: dict[str, tuple[Path, int]] = {}
    for source in sorted(NANOCLAW_SKILL_ROOT.rglob("*")):
        if source.is_symlink():
            raise ValueError(f"NanoClaw skill cannot contain a symlink: {source}")
        if source.is_file():
            relative = source.relative_to(NANOCLAW_SKILL_ROOT).as_posix()
            members[relative] = (source, 0o644)
    if not members:
        raise ValueError("NanoClaw skill contains no files")
    output = directory / nanoclaw_asset_name(version)
    write_tar_gz(output, f"kendr-optimizer-nanoclaw-{version}", members, epoch)
    return output


def expected_release_assets(version: str = PROJECT_VERSION) -> set[str]:
    return {
        *(archive_name(target) for target in SUPPORTED_TARGETS),
        *INSTALLER_ASSETS,
        *adapter_assets(version),
        "SHA256SUMS",
    }


def write_checksums(directory: Path, version: str = PROJECT_VERSION) -> Path:
    checksummed = sorted(expected_release_assets(version) - {"SHA256SUMS"})
    missing = [name for name in checksummed if not (directory / name).is_file()]
    if missing:
        raise FileNotFoundError(f"release assets are missing: {', '.join(missing)}")
    output = directory / "SHA256SUMS"
    output.write_text(
        "".join(f"{sha256(directory / name)}  {name}\n" for name in checksummed),
        encoding="ascii",
        newline="\n",
    )
    return output


def read_checksums(path: Path) -> dict[str, str]:
    checksums: dict[str, str] = {}
    for line in path.read_text(encoding="ascii").splitlines():
        match = CHECKSUM_RE.fullmatch(line)
        if not match:
            raise ValueError(f"invalid checksum line: {line!r}")
        digest, name = match.groups()
        if name in checksums:
            raise ValueError(f"duplicate checksum entry: {name}")
        checksums[name] = digest
    return checksums


def verify_installer_versions(directory: Path, version: str) -> None:
    expected_tag = f"v{version}"
    for name in INSTALLER_ASSETS:
        text = (directory / name).read_text(encoding="utf-8")
        marker = (
            f'KENDR_DEFAULT_VERSION="{expected_tag}"'
            if name.endswith(".sh")
            else f"$DefaultVersion = '{expected_tag}'"
        )
        if marker not in text:
            raise ValueError(f"{name} does not default to {expected_tag}")


def verify_directory(directory: Path, version: str) -> None:
    validate_version(version)
    expected = expected_release_assets(version)
    entries = list(directory.iterdir())
    unsafe = [path.name for path in entries if path.is_symlink() or not path.is_file()]
    if unsafe:
        raise ValueError(
            f"release directory contains non-regular entries: {sorted(unsafe)}"
        )
    actual = {path.name for path in entries}
    if actual != expected:
        raise ValueError(
            f"release asset set mismatch; expected {sorted(expected)}, got {sorted(actual)}"
        )

    checksums = read_checksums(directory / "SHA256SUMS")
    expected_checksums = expected - {"SHA256SUMS"}
    if set(checksums) != expected_checksums:
        raise ValueError("SHA256SUMS does not cover the exact release asset set")
    for name, expected_digest in checksums.items():
        actual_digest = sha256(directory / name)
        if actual_digest != expected_digest:
            raise ValueError(f"checksum mismatch for {name}")

    for target in SUPPORTED_TARGETS:
        verify_archive(directory / archive_name(target), version, target)
    verify_nanoclaw_archive(directory / nanoclaw_asset_name(version), version)
    verify_installer_versions(directory, version)


def verify_nanoclaw_archive(path: Path, version: str) -> None:
    expected_root = f"kendr-optimizer-nanoclaw-{version}"
    expected = {
        f"{expected_root}/{source.relative_to(NANOCLAW_SKILL_ROOT).as_posix()}"
        for source in NANOCLAW_SKILL_ROOT.rglob("*")
        if source.is_file()
    }
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        names = {member.name for member in members}
        if len(members) != len(expected) or names != expected:
            raise ValueError(f"unexpected NanoClaw archive members in {path.name}")
        for member in members:
            pure_name = PurePosixPath(member.name)
            if pure_name.is_absolute() or ".." in pure_name.parts or not member.isfile():
                raise ValueError(f"unsafe NanoClaw archive member: {member.name}")
            if member.mode != 0o644:
                raise ValueError(f"unexpected mode for {member.name}: {oct(member.mode)}")
            extracted = archive.extractfile(member)
            if extracted is None:
                raise ValueError(f"cannot read NanoClaw archive member: {member.name}")
            while extracted.read(1024 * 1024):
                pass


def verify_github_assets(directory: Path, release_json: Path) -> None:
    payload = json.loads(release_json.read_text(encoding="utf-8"))
    assets = payload.get("assets")
    if not isinstance(assets, list):
        raise ValueError("GitHub release response does not contain an asset list")
    remote = {}
    for asset in assets:
        name = asset.get("name")
        digest = asset.get("digest")
        if not isinstance(name, str) or not isinstance(digest, str):
            raise ValueError("GitHub release asset is missing its name or digest")
        if name in remote:
            raise ValueError(f"duplicate GitHub release asset: {name}")
        if asset.get("state") != "uploaded":
            raise ValueError(f"GitHub release asset is not uploaded: {name}")
        size = asset.get("size")
        if not isinstance(size, int) or size <= 0:
            raise ValueError(f"GitHub release asset is empty: {name}")
        remote[name] = digest
    expected_names = expected_release_assets(PROJECT_VERSION)
    if set(remote) != expected_names:
        raise ValueError(
            "GitHub release asset set mismatch; "
            f"expected {sorted(expected_names)}, got {sorted(remote)}"
        )
    for name in expected_names:
        expected_digest = f"sha256:{sha256(directory / name)}"
        if remote[name].lower() != expected_digest:
            raise ValueError(f"GitHub release digest mismatch for {name}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    package_parser = subparsers.add_parser("package", help="build one archive")
    package_parser.add_argument("--binary", type=Path, required=True)
    package_parser.add_argument(
        "--target", choices=sorted(SUPPORTED_TARGETS), required=True
    )
    package_parser.add_argument("--version", required=True)
    package_parser.add_argument("--output-dir", type=Path, required=True)
    package_parser.add_argument(
        "--epoch", type=int, default=int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    )

    assemble_parser = subparsers.add_parser(
        "assemble", help="add installers and generate SHA256SUMS"
    )
    assemble_parser.add_argument("--directory", type=Path, required=True)
    assemble_parser.add_argument("--version", required=True)
    assemble_parser.add_argument(
        "--epoch", type=int, default=int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    )

    verify_parser = subparsers.add_parser("verify", help="verify a release directory")
    verify_parser.add_argument("--directory", type=Path, required=True)
    verify_parser.add_argument("--version", required=True)

    api_parser = subparsers.add_parser(
        "verify-github-assets", help="verify GitHub's recorded asset digests"
    )
    api_parser.add_argument("--directory", type=Path, required=True)
    api_parser.add_argument("--release-json", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "package":
        output = package(
            args.binary.resolve(),
            args.target,
            args.version,
            args.output_dir.resolve(),
            args.epoch,
        )
        verify_archive(output, args.version, args.target)
        print(output)
    elif args.command == "assemble":
        directory = args.directory.resolve()
        copy_installers(directory)
        package_nanoclaw(directory, args.version, args.epoch)
        write_checksums(directory, args.version)
        verify_directory(directory, args.version)
        print(directory)
    elif args.command == "verify":
        verify_directory(args.directory.resolve(), args.version)
        print(f"release assets verified: {args.directory}")
    elif args.command == "verify-github-assets":
        verify_github_assets(args.directory.resolve(), args.release_json.resolve())
        print("GitHub release asset digests verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
