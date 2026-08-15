#!/usr/bin/env python3
"""Exercise a native Kendr installer against a local release fixture."""

from __future__ import annotations

import argparse
import contextlib
import functools
import hashlib
import http.server
import json
import os
import shutil
import subprocess
import tempfile
import threading
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INSTALL_RECEIPT = ".kendr-opt-install.json"


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_install_receipt(path: Path, target: str, version: str) -> None:
    if not path.is_file() or path.is_symlink():
        raise ValueError("installer did not create a regular install receipt")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("install receipt is not valid UTF-8 JSON") from error
    expected = {
        "schema_version": "kendr.install/v1",
        "repository": "Kendr-AI/Kendr-Optimizer",
        "install_method": "github-release",
        "target": target,
        "version": version,
        "channel": "preview",
    }
    if payload != expected:
        raise ValueError(
            f"install receipt mismatch; expected={expected!r}, got={payload!r}"
        )


def verify_preserved_install(
    installed: Path,
    receipt: Path,
    installed_digest: str,
    receipt_digest: str,
    context: str,
) -> None:
    if digest(installed) != installed_digest:
        raise ValueError(f"{context} changed the installed binary")
    if digest(receipt) != receipt_digest:
        raise ValueError(f"{context} changed the install receipt")

    expected_entries = {installed.name, INSTALL_RECEIPT}
    actual_entries = {path.name for path in installed.parent.iterdir()}
    if actual_entries != expected_entries:
        raise ValueError(
            f"{context} left unexpected staging or rollback files: "
            f"{sorted(actual_entries - expected_entries)}"
        )


@contextlib.contextmanager
def windows_receipt_read_lock(path: Path):
    if os.name != "nt":
        raise RuntimeError("the receipt sharing-denial smoke is Windows-only")

    import ctypes
    from ctypes import wintypes

    create_file = ctypes.WinDLL("kernel32", use_last_error=True).CreateFileW
    create_file.argtypes = [
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.LPVOID,
        wintypes.DWORD,
        wintypes.DWORD,
        wintypes.HANDLE,
    ]
    create_file.restype = wintypes.HANDLE
    close_handle = ctypes.WinDLL("kernel32", use_last_error=True).CloseHandle
    close_handle.argtypes = [wintypes.HANDLE]
    close_handle.restype = wintypes.BOOL

    generic_read = 0x80000000
    file_share_read = 0x00000001
    open_existing = 3
    file_attribute_normal = 0x00000080
    handle = create_file(
        str(path),
        generic_read,
        file_share_read,
        None,
        open_existing,
        file_attribute_normal,
        None,
    )
    invalid_handle = ctypes.c_void_p(-1).value
    if handle == invalid_handle:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        yield
    finally:
        if not close_handle(handle):
            raise ctypes.WinError(ctypes.get_last_error())


def run(command: list[str], environment: dict[str, str], *, succeeds: bool) -> None:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
        timeout=60,
    )
    if (result.returncode == 0) != succeeds:
        raise RuntimeError(
            f"installer command returned {result.returncode}; "
            f"stdout={result.stdout!r}; stderr={result.stderr!r}"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--version", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    archive = args.archive.resolve()
    if not archive.is_file():
        raise FileNotFoundError(f"release archive is missing: {archive}")

    windows = args.target.endswith("-windows-msvc")
    if windows != (os.name == "nt"):
        raise ValueError("installer smoke target does not match the native runner")

    with tempfile.TemporaryDirectory(prefix="kendr-installer-smoke-") as temporary:
        root = Path(temporary)
        server_dir = root / "release assets"
        install_dir = root / "installed cli"
        temp_dir = root / "temporary files"
        server_dir.mkdir()
        temp_dir.mkdir()
        served_archive = server_dir / archive.name
        shutil.copyfile(archive, served_archive)
        (server_dir / "SHA256SUMS").write_text(
            f"{digest(served_archive)}  {served_archive.name}\n",
            encoding="ascii",
            newline="\n",
        )

        handler = functools.partial(QuietHandler, directory=str(server_dir))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            environment = os.environ.copy()
            environment.update(
                {
                    "KENDR_ALLOW_INSECURE": "1",
                    "KENDR_INSTALLER_TEST_MODE": "1",
                    "KENDR_DOWNLOAD_BASE_URL": (
                        f"http://127.0.0.1:{server.server_port}"
                    ),
                    "KENDR_INSTALL_DIR": str(install_dir),
                    "KENDR_NO_MODIFY_PATH": "1",
                    "KENDR_VERSION": f"v{args.version}",
                    "TMPDIR": str(temp_dir),
                }
            )
            if windows:
                command = [
                    "powershell",
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(ROOT / "install" / "kendr-opt-installer.ps1"),
                ]
                installed = install_dir / "kendr-opt.exe"
            else:
                command = ["sh", str(ROOT / "install" / "kendr-opt-installer.sh")]
                installed = install_dir / "kendr-opt"
            receipt = install_dir / INSTALL_RECEIPT

            untrusted_environment = environment.copy()
            untrusted_environment["KENDR_DOWNLOAD_BASE_URL"] = (
                "https://untrusted.invalid/releases"
            )
            untrusted_environment.pop("KENDR_INSTALLER_TEST_MODE")
            run(command, untrusted_environment, succeeds=False)
            if install_dir.exists() and any(install_dir.iterdir()):
                raise ValueError(
                    "rejected installer authority override changed the install directory"
                )

            run(command, environment, succeeds=True)
            verify_install_receipt(receipt, args.target, args.version)
            run(command, environment, succeeds=True)
            verify_install_receipt(receipt, args.target, args.version)
            installed_digest = digest(installed)
            receipt_digest = digest(receipt)

            if windows:
                with windows_receipt_read_lock(receipt):
                    run(command, environment, succeeds=False)
                verify_preserved_install(
                    installed,
                    receipt,
                    installed_digest,
                    receipt_digest,
                    "receipt sharing-denial rollback",
                )

            with served_archive.open("ab") as stream:
                stream.write(b"corrupted installer smoke fixture")
            run(command, environment, succeeds=False)
            verify_preserved_install(
                installed,
                receipt,
                installed_digest,
                receipt_digest,
                "corrupted-archive rejection",
            )

            version = subprocess.run(
                [str(installed), "--version"],
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            ).stdout.strip()
            if version != f"kendr-opt {args.version}":
                raise ValueError(f"installed version mismatch: {version!r}")
            engines = json.loads(
                subprocess.run(
                    [str(installed), "engines", "--compact"],
                    check=True,
                    capture_output=True,
                    text=True,
                    timeout=30,
                ).stdout
            )
            if not isinstance(engines, list) or not engines:
                raise ValueError("installed CLI returned no engine list")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    print(f"installer smoke passed for {args.target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
